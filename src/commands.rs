//! Commandes du noyau — **une seule implémentation** pour le CLI et la TUI.
//!
//! Avant : chaque commande existait deux fois, une dans `cli.rs` (`println!`)
//! et une dans `tui/mod.rs` (`UiEvent::Info`). Elles avaient déjà divergé :
//! `select` gardait 8 modèles d'un côté, 5 de l'autre, avec deux avertissements
//! différents ; `task add` acceptait `--role` en CLI mais pas en TUI.
//!
//! Règle : ce module ne connaît **ni** `println!`, **ni** `ratatui`. Il renvoie
//! des lignes de texte et des erreurs ; chaque interface les rend à sa façon.
//! Toute nouvelle commande s'ajoute ici d'abord.

use anyhow::{anyhow, Result};

// ─────────────────────────────────────────────────────────────────────────────
// Tâches
// ─────────────────────────────────────────────────────────────────────────────

/// Commande `task`, indépendante de l'interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskCmd {
    List,
    Add { text: String, role: Option<String> },
    Done(String),
    Claim(String),
    Rm(String),
    Clear,
}

/// Parse les arguments d'une commande `task` (`["add", "texte", "--role", "code"]`).
///
/// La TUI (`/task …`) et le CLI s'en servent : c'est ce qui garantit qu'ils
/// acceptent exactement la même syntaxe. `usage` dans l'erreur est neutre
/// (pas de préfixe `/`, que le CLI ne connaît pas).
pub fn parse_task(args: &[&str]) -> Result<TaskCmd> {
    const USAGE: &str =
        "usage: task [list|add <texte> [--role <r>]|done <id>|claim <id>|rm <id>|clear]";
    let sub = args.first().copied().unwrap_or("list");
    match sub {
        "list" | "" => Ok(TaskCmd::List),
        "add" => {
            let mut text: Vec<&str> = Vec::new();
            let mut role: Option<String> = None;
            let mut it = args.iter().skip(1);
            while let Some(w) = it.next() {
                if *w == "--role" || *w == "-r" {
                    role = it.next().map(|s| (*s).to_string());
                } else {
                    text.push(w);
                }
            }
            let text = text.join(" ").trim().to_string();
            if text.is_empty() {
                return Err(anyhow!("{USAGE}"));
            }
            Ok(TaskCmd::Add { text, role })
        }
        "done" => Ok(TaskCmd::Done(task_id(args, "done")?)),
        "claim" | "start" => Ok(TaskCmd::Claim(task_id(args, "claim")?)),
        "rm" | "del" | "delete" => Ok(TaskCmd::Rm(task_id(args, "rm")?)),
        "clear" => Ok(TaskCmd::Clear),
        other => Err(anyhow!("sous-commande inconnue `{other}` — {USAGE}")),
    }
}

fn task_id(args: &[&str], sub: &str) -> Result<String> {
    match args.get(1).map(|s| s.trim()).filter(|s| !s.is_empty()) {
        Some(id) => Ok(id.to_string()),
        None => Err(anyhow!("usage: task {sub} <id>")),
    }
}

/// Exécute une commande `task` et renvoie les lignes à afficher.
pub fn task(cmd: TaskCmd) -> Result<Vec<String>> {
    use crate::tasks::{self, Status, Update};

    match cmd {
        TaskCmd::List => Ok(vec![tasks::format_list()]),
        TaskCmd::Add { text, role } => {
            let t = tasks::add(&text, role)?;
            Ok(vec![format!("Tâche {} ajoutée : {}", t.id, t.text)])
        }
        TaskCmd::Done(id) => match tasks::set_status(&id, Status::Done)? {
            Update::Ok(_) => Ok(vec![format!("Tâche {id} → fait")]),
            Update::NotFound => Ok(vec![format!("Tâche {id} introuvable")]),
            Update::Refused(missing) => {
                let mut lines = vec![format!(
                    "⚠ Refusé : {id} ne peut pas être marquée « fait » — livrable(s) en défaut :"
                )];
                for (path, why) in missing {
                    lines.push(format!("  ✗ {path} ({why})"));
                }
                lines.push(
                    "« fait » exige le livrable sur disque, non vide — crée-le, ou retire le \
                     chemin du texte de la tâche (édition de .zer0/tasks.md)."
                        .to_string(),
                );
                Ok(lines)
            }
        },
        TaskCmd::Claim(id) => match tasks::set_status(&id, Status::Claimed)? {
            Update::Ok(s) => Ok(vec![format!("Tâche {id} → {}", s.label())]),
            Update::NotFound => Ok(vec![format!("Tâche {id} introuvable")]),
            Update::Refused(_) => Ok(vec![format!("Tâche {id} : changement refusé")]),
        },
        TaskCmd::Rm(id) => {
            if tasks::remove(&id)? {
                Ok(vec![format!("Tâche {id} supprimée")])
            } else {
                Ok(vec![format!("Tâche {id} introuvable")])
            }
        }
        TaskCmd::Clear => {
            let n = tasks::clear_done()?;
            Ok(vec![format!("{n} tâche(s) terminée(s) supprimée(s)")])
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Modèle actif
// ─────────────────────────────────────────────────────────────────────────────

/// Définit le modèle actif (persisté dans la config globale).
///
/// Refuse les modèles d'embedding/rerank : ils ne répondent que sur
/// `/embeddings`, donc les sélectionner casse le chat avec un 400 opaque.
/// Le CLI et la TUI partageaient déjà ce contrôle, mais avec deux textes
/// différents et deux façons de propager (ou d'avaler) l'erreur.
/// Retire un préfixe `{provider}/` redondant (voir `route::normalize_model`).
pub fn strip_route_prefix(provider_id: &str, id: &str) -> String {
    id.strip_prefix(&format!("{provider_id}/"))
        .filter(|rest| !rest.is_empty())
        .unwrap_or(id)
        .to_string()
}

pub fn set_model(id: &str) -> Result<Vec<String>> {
    let id = id.trim();
    if id.is_empty() {
        return Err(anyhow!("usage: model use <id>"));
    }
    // On stocke un identifiant **nu** : `openrouter/minimax/…` deviendrait un
    // modèle inexistant une fois préfixé par le provider au moment de l'appel.
    let active = crate::config::resolve().provider_id;
    let id = strip_route_prefix(&active, id);
    let id = id.as_str();
    if crate::select::is_embedding_or_rerank(id) {
        return Err(anyhow!(
            "'{id}' est un modèle d'embedding/rerank — inutilisable en chat \
             (endpoint /embeddings). Modèle inchangé."
        ));
    }
    let mut g = crate::config::load_global();
    g.model = Some(id.to_string());
    crate::config::save_global(&g)?;
    Ok(vec![format!("Modèle actif: {id}")])
}

// ─────────────────────────────────────────────────────────────────────────────
// Chaîne de routage (bascule)
// ─────────────────────────────────────────────────────────────────────────────

/// Chaîne de bascule effective, une ligne par endpoint.
pub fn route_lines(settings: &crate::config::Settings) -> Vec<String> {
    let chain = crate::route::build_chain(settings);
    let mut out = vec!["Chaîne de routage (ordre de bascule):".to_string()];
    if chain.is_empty() {
        out.push("  (vide — aucun endpoint configuré)".to_string());
    }
    for (i, ep) in chain.iter().enumerate() {
        let key = if ep.api_key.is_some() || ep.provider_id == "local" {
            "clé ok"
        } else {
            "sans clé"
        };
        out.push(format!("  {}. {}  [{}]", i + 1, ep.label, key));
    }
    out
}

pub fn route_add(r: &str) -> Result<Vec<String>> {
    let r = r.trim();
    if r.is_empty() || !r.contains('/') {
        return Err(anyhow!("usage: route add <provider>/<model>"));
    }
    crate::route::add_global_route(r)?;
    Ok(vec![format!("Route ajoutée: {r}")])
}

pub fn route_clear() -> Result<Vec<String>> {
    crate::route::clear_global_routes()?;
    Ok(vec!["Routes de bascule effacées.".to_string()])
}

/// Arguments de `route auto` : `[provider] [n]`, `[n]`, ou rien.
/// (`/route auto 4` et `/route auto openrouter 4` sont équivalents.)
pub fn parse_auto_args(args: &[&str]) -> (String, usize) {
    let mut provider = "openrouter".to_string();
    let mut n = 4usize;
    let mut it = args.iter();
    if let Some(first) = it.next() {
        match first.parse::<usize>() {
            Ok(v) => n = v,
            Err(_) => {
                provider = (*first).to_string();
                if let Some(second) = it.next() {
                    if let Ok(v) = second.parse::<usize>() {
                        n = v;
                    }
                }
            }
        }
    }
    (provider, n.clamp(1, 12))
}

/// Remplit la chaîne de bascule avec les modèles `:free` du provider.
pub async fn route_auto(provider: &str, n: usize) -> Result<Vec<String>> {
    let provider = if provider.trim().is_empty() {
        "openrouter"
    } else {
        provider.trim()
    };
    let n = n.clamp(1, 12);
    let eps = crate::route::auto_free_routes(provider, "raisonnement code général", n).await?;
    if eps.is_empty() {
        return Ok(vec![format!("Aucun modèle :free trouvé chez {provider}.")]);
    }
    let routes: Vec<String> = eps
        .iter()
        .map(|e| format!("{}/{}", e.provider_id, e.model))
        .collect();
    crate::route::set_global_routes(routes.clone())?;
    let mut out = vec![format!(
        "Chaîne de bascule remplie depuis {provider} ({} modèles):",
        routes.len()
    )];
    for (i, r) in routes.iter().enumerate() {
        out.push(format!("  {}. {}", i + 1, r));
    }
    Ok(out)
}

// ─────────────────────────────────────────────────────────────────────────────
// Quota
// ─────────────────────────────────────────────────────────────────────────────

pub async fn quota_report(provider: Option<&str>) -> Result<Vec<String>> {
    let settings = crate::config::resolve();
    let (pid, key) = match provider.map(str::trim).filter(|p| !p.is_empty()) {
        Some(id) => {
            let p = crate::providers::find(id).ok_or_else(|| anyhow!("provider inconnu: {id}"))?;
            (p.id.clone(), crate::config::key_for(&p))
        }
        None => (settings.provider_id.clone(), settings.api_key.clone()),
    };
    let mut out: Vec<String> = crate::quota::report()
        .lines()
        .map(|l| l.to_string())
        .collect();
    if let Some(live) = crate::quota::fetch_live(&pid, key.as_deref()).await {
        out.push(String::new());
        out.extend(live.lines().map(|l| l.to_string()));
    } else if pid != "openrouter" {
        out.push(String::new());
        out.push(format!(
            "{pid} ne publie aucun solde interrogable — seuls les appels observés et les \
             limites révélées dans les 429 sont comptés."
        ));
    }
    Ok(out)
}

// ─────────────────────────────────────────────────────────────────────────────
// PDF
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfCmd {
    Info(String),
    Text(String),
    Merge {
        output: String,
        inputs: Vec<String>,
    },
    Split {
        path: String,
        output: String,
        first: u32,
        last: u32,
    },
}

const PDF_USAGE: &str =
    "usage: pdf info <f> | text <f> | merge <sortie> <entrée…> | split <entrée> <sortie> <première> <dernière>";

/// Parse les arguments d'une commande `pdf` (mêmes règles pour CLI et TUI).
pub fn parse_pdf_args(args: &[&str]) -> Result<PdfCmd> {
    match args.first().copied().unwrap_or("") {
        "" => Err(anyhow!("{PDF_USAGE}")),
        "info" => args
            .get(1)
            .map(|p| PdfCmd::Info((*p).to_string()))
            .ok_or_else(|| anyhow!("{PDF_USAGE}")),
        "text" => args
            .get(1)
            .map(|p| PdfCmd::Text((*p).to_string()))
            .ok_or_else(|| anyhow!("{PDF_USAGE}")),
        "merge" => {
            let output = args.get(1).copied().unwrap_or("");
            let inputs: Vec<String> = args.iter().skip(2).map(|s| (*s).to_string()).collect();
            if output.is_empty() || inputs.is_empty() {
                return Err(anyhow!("{PDF_USAGE}"));
            }
            Ok(PdfCmd::Merge {
                output: output.to_string(),
                inputs,
            })
        }
        "split" => {
            let path = args.get(1).copied().unwrap_or("");
            let output = args.get(2).copied().unwrap_or("");
            let first = args.get(3).and_then(|s| s.parse::<u32>().ok());
            let last = args.get(4).and_then(|s| s.parse::<u32>().ok());
            match (path.is_empty(), output.is_empty(), first, last) {
                (false, false, Some(first), Some(last)) if first >= 1 && last >= first => {
                    Ok(PdfCmd::Split {
                        path: path.to_string(),
                        output: output.to_string(),
                        first,
                        last,
                    })
                }
                _ => Err(anyhow!("{PDF_USAGE}")),
            }
        }
        other => Err(anyhow!(
            "sous-commande pdf inconnue `{other}` — {PDF_USAGE}"
        )),
    }
}

pub fn pdf(cmd: PdfCmd) -> Result<Vec<String>> {
    let text = match cmd {
        PdfCmd::Info(p) => crate::pdf::info(&p)?,
        PdfCmd::Text(p) => crate::pdf::text(&p)?,
        PdfCmd::Merge { output, inputs } => crate::pdf::merge(&inputs, &output)?,
        PdfCmd::Split {
            path,
            output,
            first,
            last,
        } => crate::pdf::split(&path, &output, first, last)?,
    };
    Ok(text.lines().map(|l| l.to_string()).collect())
}

// ─────────────────────────────────────────────────────────────────────────────
// Catalogue d'API publiques
// ─────────────────────────────────────────────────────────────────────────────

/// Nombre de résultats par défaut (le CLI en affichait 10, la TUI 12).
pub const APIS_DEFAULT_K: usize = 10;

pub fn apis_stats() -> Result<Vec<String>> {
    Ok(crate::apis::stats()?
        .lines()
        .map(|l| l.to_string())
        .collect())
}

pub fn apis_search(query: &str, k: usize, keyless: bool) -> Result<Vec<String>> {
    let q = query.trim();
    if q.is_empty() {
        return Err(anyhow!("usage: apis search <besoin>"));
    }
    let entries = crate::apis::search_opts(q, k.clamp(1, 50), keyless)?;
    let meta = crate::apis::meta()?;
    Ok(crate::apis::format_entries(&entries, &meta)
        .lines()
        .map(|l| l.to_string())
        .collect())
}

pub fn apis_categories() -> Result<Vec<String>> {
    Ok(crate::apis::categories()?
        .into_iter()
        .map(|(c, n)| format!("  {c:<30} {n}"))
        .collect())
}

pub fn apis_import(path: &str) -> Result<Vec<String>> {
    let p = std::path::PathBuf::from(path);
    if !p.exists() {
        return Err(anyhow!("fichier introuvable: {}", p.display()));
    }
    let (n, meta) = crate::apis::import_tsv(&p)?;
    Ok(vec![format!(
        "Catalogue importé : {n} API(s) — source : {} — importé le {}",
        meta.source, meta.imported_at
    )])
}

// ─────────────────────────────────────────────────────────────────────────────
// Personas
// ─────────────────────────────────────────────────────────────────────────────

pub fn persona_list() -> Vec<String> {
    let list = crate::personas::list();
    let mut out = vec![format!("Dossier : {}", crate::personas::dir().display())];
    if list.is_empty() {
        out.push("  (aucune persona)".to_string());
    } else {
        out.push(format!("Personas ({}):", list.len()));
        for p in list {
            out.push(format!("  {p}"));
        }
    }
    out
}

pub fn persona_show(name: &str) -> Result<Vec<String>> {
    let name = name.trim();
    if name.is_empty() {
        return Err(anyhow!("usage: whoami show <nom>"));
    }
    match crate::personas::get(name) {
        Some(c) => Ok(c.lines().map(|l| l.to_string()).collect()),
        None => Err(anyhow!("persona '{name}' introuvable")),
    }
}

pub fn persona_add(name: &str, content: &str) -> Result<Vec<String>> {
    let name = name.trim();
    if name.is_empty() || content.trim().is_empty() {
        return Err(anyhow!("usage: whoami add <nom> <fichier|->"));
    }
    crate::personas::save(name, content)?;
    Ok(vec![format!(
        "Persona '{name}' enregistrée → {}",
        crate::personas::dir().display()
    )])
}

// ─────────────────────────────────────────────────────────────────────────────
// Coffre
// ─────────────────────────────────────────────────────────────────────────────

pub fn vault_status() -> Vec<String> {
    vec![
        format!(
            "Coffre      : {}",
            if crate::vault::enabled() {
                "activé"
            } else {
                "absent"
            }
        ),
        format!("Déverrouillé : {}", crate::vault::is_unlocked()),
        format!("Chemin      : {}", crate::vault::path().display()),
    ]
}

pub fn vault_lock() -> Vec<String> {
    crate::vault::lock();
    vec!["Coffre verrouillé.".to_string()]
}

// ─────────────────────────────────────────────────────────────────────────────
// Embeddings
// ─────────────────────────────────────────────────────────────────────────────

pub fn embeddings_show() -> Vec<String> {
    match crate::config::load_global().embeddings {
        Some(e) => vec![format!(
            "Embedder: {}/{} (mémoire sémantique active)",
            e.provider.unwrap_or_default(),
            e.model.unwrap_or_default()
        )],
        None => vec![
            "Aucun embedder — recherche par sous-chaîne (degraded=true).".to_string(),
            "embeddings set <provider> <model>  (ex. openrouter liquid/lfm-2.5-embedding-350m:free)"
                .to_string(),
        ],
    }
}

pub fn embeddings_set(provider: &str, model: &str) -> Result<Vec<String>> {
    let provider = provider.trim();
    let model = model.trim();
    if provider.is_empty() || model.is_empty() {
        return Err(anyhow!("usage: embeddings set <provider> <model>"));
    }
    let p =
        crate::providers::find(provider).ok_or_else(|| anyhow!("provider inconnu: {provider}"))?;
    let mut g = crate::config::load_global();
    g.embeddings = Some(crate::config::EmbeddingConfig {
        provider: Some(p.id.clone()),
        model: Some(model.to_string()),
    });
    crate::config::save_global(&g)?;
    let mut out = vec![format!("Embedder: {}/{model}", p.id)];
    if !crate::select::is_embedding_or_rerank(model) {
        out.push(
            "⚠ ce modèle ne ressemble pas à un embedder (nom sans « embedding ») — \
             vérifie qu'il accepte /embeddings."
                .to_string(),
        );
    }
    Ok(out)
}

pub fn embeddings_unset() -> Result<Vec<String>> {
    let mut g = crate::config::load_global();
    g.embeddings = None;
    crate::config::save_global(&g)?;
    Ok(vec![
        "Embedder retiré — recherche par sous-chaîne (degraded=true).".to_string(),
    ])
}

// ─────────────────────────────────────────────────────────────────────────────
// Providers
// ─────────────────────────────────────────────────────────────────────────────

/// Tableau des providers connus. `gateway` est fourni par l'appelant pour que
/// l'affichage reste cohérent avec ce que l'agent utilise réellement.
pub fn provider_list() -> Vec<String> {
    let g = crate::config::load_global();
    let gw = crate::config::gateway_enabled();
    let mut out = vec![format!(
        "{:<3}{:<14}{:<26}{:<12}{:<10}base_url",
        "", "id", "nom", "gratuit", "clé"
    )];
    for p in crate::providers::all() {
        let active = if g.provider.as_deref() == Some(p.id.as_str()) {
            "*"
        } else {
            " "
        };
        let key = if p.free == crate::providers::FreeKind::Local {
            "local".to_string()
        } else if gw && !p.custom {
            "zer0-gw".to_string()
        } else if crate::config::key_for(&p).is_some() {
            "oui".to_string()
        } else {
            "—".to_string()
        };
        out.push(format!(
            "{:<3}{:<14}{:<26}{:<12}{:<10}{}{}",
            active,
            p.id,
            p.name,
            p.free.label(),
            key,
            p.base_url,
            if p.custom { "  (custom)" } else { "" }
        ));
    }
    out.push(String::new());
    out.push(
        "Ajouter une clé : provider add <id> [clé]  ·  Provider actif : provider use <id>".into(),
    );
    out.push("Passerelle (clés hors de l'agent) : provider keys".into());
    out
}

pub fn provider_keys() -> Vec<String> {
    if !crate::gwclient::probe() {
        return vec![
            "zer0-gw injoignable — lance-le (sudo systemctl start zer0-gw) pour gérer les clés hors de l'agent."
                .to_string(),
        ];
    }
    match crate::gwclient::list_keys() {
        Ok(keys) => {
            let mut out = vec!["Clés détenues par zer0-gw (l'agent n'en a aucune) :".to_string()];
            for (id, has) in keys {
                out.push(format!(" {:<14} {}", id, if has { "clé ✓" } else { "—" }));
            }
            out.push(String::new());
            out.push(
                "Déposer : provider add <id> <clé>   ·   Oublier : provider remove <id>".into(),
            );
            out
        }
        Err(e) => vec![format!("zer0-gw : {e}")],
    }
}

/// Cas particulier `cline` : c'est un endpoint OpenAI-compatible, pas une clé.
/// `url` absent → Ollama local. La clé est optionnelle (stockée localement, ou
/// déposée sur la passerelle comme les autres providers).
pub fn provider_add_cline(url: Option<&str>, key: Option<&str>) -> Result<Vec<String>> {
    let url = url
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .unwrap_or("http://localhost:11434/v1");
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(anyhow!(
            "« provider add cline » attend une URL (http://…), pas une clé.\n\
             usage: provider add cline [http://hôte:port/v1] [clé]\n\
             Sans argument → Ollama local (http://localhost:11434/v1)."
        ));
    }
    let mut g = crate::config::load_global();
    g.custom.insert(
        "cline".to_string(),
        crate::config::CustomProvider {
            name: "cline".to_string(),
            base_url: url.to_string(),
            env_key: None,
        },
    );
    if let Some(k) = key.map(str::trim).filter(|k| !k.is_empty()) {
        g.keys.insert("cline".to_string(), k.to_string());
    }
    crate::config::save_global(&g)?;
    Ok(vec![format!(
        "Provider 'cline' ajouté → {url}. Ensuite : provider use cline, puis models."
    )])
}

/// Ajoute la clé d'un provider intégré : dépôt sur zer0-gw si elle répond,
/// sinon repli local **avec avertissement**.
pub fn provider_add(id: &str, key: Option<&str>) -> Result<Vec<String>> {
    if id.eq_ignore_ascii_case("cline") {
        return provider_add_cline(key, None);
    }
    let p = crate::providers::find(id).ok_or_else(|| anyhow!("provider inconnu: {id}"))?;
    let resolved = match key.map(str::trim).filter(|k| !k.is_empty()) {
        Some(k) => k.to_string(),
        None => {
            let from_env = if p.env_key.is_empty() {
                None
            } else {
                std::env::var(&p.env_key).ok().filter(|s| !s.is_empty())
            };
            match (from_env, p.env_key.is_empty()) {
                (Some(k), _) => k,
                (None, false) => {
                    return Err(anyhow!(
                        "clé manquante. Passe-la en argument, ou `export {}` puis relance.",
                        p.env_key
                    ))
                }
                (None, true) => {
                    return Err(anyhow!("clé manquante. usage: provider add {id} <clé>"))
                }
            }
        }
    };

    match crate::gwclient::add_key_flow(&p.id, &resolved) {
        Ok(crate::gwclient::AddRoute::ViaGateway { provider_id }) => {
            let mut g = crate::config::load_global();
            let (name, _) = crate::gwclient::activate_provider(&mut g, &provider_id);
            Ok(vec![
                "🔑 Clé déposée sur zer0-gw (écriture seule) — aucune clé stockée ici.".to_string(),
                format!("Provider actif : {name}"),
            ])
        }
        Ok(crate::gwclient::AddRoute::Local) => {
            let mut g = crate::config::load_global();
            g.keys.insert(p.id.clone(), resolved);
            crate::config::save_global(&g)?;
            Ok(vec![
                format!(
                    "⚠ zer0-gw injoignable — clé enregistrée localement pour {} → {}",
                    p.name,
                    crate::config::global_path().display()
                ),
                "Pour le durcissement : lance zer0-gw puis relance cette commande.".to_string(),
            ])
        }
        Err(e) => Err(anyhow!("zer0-gw : {e}")),
    }
}

pub fn provider_add_custom(
    id: &str,
    base_url: &str,
    name: Option<&str>,
    env: Option<&str>,
) -> Result<Vec<String>> {
    let id = id.trim().to_ascii_lowercase();
    let base_url = base_url.trim();
    if id.is_empty() {
        return Err(anyhow!("usage: provider add-custom <id> <base_url>"));
    }
    if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
        return Err(anyhow!(
            "base_url invalide : « {base_url} » — elle doit commencer par http:// ou https:// \
             (une clé API ne peut PAS servir de base_url)"
        ));
    }
    if crate::providers::builtins().iter().any(|p| p.id == id) {
        return Err(anyhow!(
            "'{id}' est déjà un provider intégré — pas besoin de add-custom."
        ));
    }
    let mut g = crate::config::load_global();
    g.custom.insert(
        id.clone(),
        crate::config::CustomProvider {
            name: name.map(str::to_string).unwrap_or_else(|| id.clone()),
            base_url: base_url.to_string(),
            env_key: env.map(str::to_string),
        },
    );
    crate::config::save_global(&g)?;
    Ok(vec![format!(
        "Provider personnalisé '{id}' enregistré. Ensuite : provider use {id}"
    )])
}

pub fn provider_use(id: &str) -> Result<Vec<String>> {
    let p = crate::providers::find(id).ok_or_else(|| anyhow!("provider inconnu: {id}"))?;
    let mut g = crate::config::load_global();
    g.provider = Some(p.id.to_string());
    if let Some(d) = crate::providers::default_model(&p.id) {
        g.model = Some(d.to_string());
    }
    crate::config::save_global(&g)?;
    Ok(vec![format!("Provider actif: {}", p.name)])
}

pub fn provider_remove(id: &str) -> Result<Vec<String>> {
    let p = crate::providers::find(id).ok_or_else(|| anyhow!("provider inconnu: {id}"))?;
    // Oublie aussi côté passerelle (si elle tourne) : sinon la clé survit là-bas.
    let _ = crate::gwclient::delete_key(&p.id);
    let mut g = crate::config::load_global();
    g.keys.remove(&p.id);
    let removed_custom = g.custom.remove(&p.id).is_some();
    crate::config::save_global(&g)?;
    Ok(vec![if removed_custom {
        format!("Provider personnalisé '{}' supprimé.", p.id)
    } else {
        format!("Clé supprimée pour {}", p.name)
    }])
}

// ─────────────────────────────────────────────────────────────────────────────
// Base de connaissance (SQLite + FTS5)
// ─────────────────────────────────────────────────────────────────────────────

pub fn kb_stats() -> Result<Vec<String>> {
    Ok(crate::knowledge::stats()?
        .lines()
        .map(|l| l.to_string())
        .collect())
}

pub async fn kb_search(query: &str, k: usize) -> Result<Vec<String>> {
    let q = query.trim();
    if q.is_empty() {
        return Err(anyhow!("usage: kb search <requête>"));
    }
    let (hits, degraded) = crate::knowledge::search(q, k.clamp(1, 20)).await;
    Ok(crate::knowledge::format_hits(&hits, &degraded)
        .lines()
        .map(|l| l.to_string())
        .collect())
}

/// Indexe un dossier (défaut : `base_connaissance`).
pub async fn kb_ingest(dir: Option<std::path::PathBuf>, embed: bool) -> Result<Vec<String>> {
    let d = dir.unwrap_or_else(crate::knowledge::default_dir);
    // Vérifié avant d'ouvrir la base : « dossier inexistant » ne doit pas créer
    // une base vide au passage.
    if !d.exists() {
        return Err(anyhow!("dossier introuvable: {}", d.display()));
    }
    let (files, chunks) = crate::knowledge::ingest_dir(&d, embed).await?;
    let mut out = vec![format!(
        "Indexé : {files} fichier(s), {chunks} chunk(s) depuis {}",
        d.display()
    )];
    if embed && crate::memory::embedder_id().is_none() {
        out.push("(aucun embedder configuré → vecteurs non calculés)".to_string());
    }
    out.push(format!("Base : {}", crate::knowledge::db_path().display()));
    Ok(out)
}

pub fn kb_rebuild() -> Result<Vec<String>> {
    crate::knowledge::rebuild_fts()?;
    Ok(vec!["Index FTS5 reconstruit.".to_string()])
}

pub fn kb_log(n: usize) -> Vec<String> {
    crate::knowledge::tail_log(n.clamp(1, 500))
        .lines()
        .map(|l| l.to_string())
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Mémoire persistante
// ─────────────────────────────────────────────────────────────────────────────

/// Liste les entrées (les plus récentes d'abord) + l'état de l'embedder.
pub fn memory_list(limit: usize) -> Vec<String> {
    let entries = crate::memory::load();
    let n = limit.clamp(1, 500);
    let mut out = vec![format!(
        "Mémoire: {} entrée(s) — {}",
        entries.len(),
        crate::memory::store_path().display()
    )];
    if entries.is_empty() {
        out.push("  (vide)".to_string());
    }
    for e in entries.iter().rev().take(n) {
        out.push(format!(
            "  {} [{}]{}{} {}",
            e.id,
            e.trust,
            if e.trigger.is_some() { " testable" } else { "" },
            if e.vector.is_some() { " emb" } else { "" },
            e.text.replace('\n', " ")
        ));
    }
    out.push(format!(
        "trust_enforced: false (toutes untrusted) · {}",
        match crate::memory::embedder_id() {
            Some(e) => format!("embedder: {e}"),
            None => "embedder: (aucun)".to_string(),
        }
    ));
    out
}

pub fn memory_stats() -> Vec<String> {
    let entries = crate::memory::load();
    let probeable = entries.iter().filter(|e| e.trigger.is_some()).count();
    let embedded = entries.iter().filter(|e| e.vector.is_some()).count();
    vec![
        format!("Store      : {}", crate::memory::store_path().display()),
        format!("Entrées    : {}", entries.len()),
        format!("Embeddées  : {embedded}"),
        format!("Testables  : {probeable}"),
        format!(
            "Embedder   : {}",
            crate::memory::embedder_id().unwrap_or_else(|| "(aucun)".into())
        ),
    ]
}

pub async fn memory_save(text: &str, trigger: Option<String>) -> Result<Vec<String>> {
    let text = text.trim();
    if text.is_empty() {
        return Err(anyhow!("usage: memory save <texte>"));
    }
    let o = crate::memory::save(text, trigger, Vec::new()).await?;
    Ok(vec![format!(
        "Enregistré: {} (embedded={}, testable={}, degraded={}{})",
        o.id,
        o.embedded,
        o.probeable,
        o.degraded,
        o.degraded_reason
            .map(|r| format!(" — {r}"))
            .unwrap_or_default()
    )])
}

pub async fn memory_search(query: &str, k: usize) -> Result<Vec<String>> {
    let query = query.trim();
    if query.is_empty() {
        return Err(anyhow!("usage: memory search <requête>"));
    }
    let o = crate::memory::search(query, k.clamp(1, 20)).await;
    Ok(crate::memory::format_outcome(&o)
        .lines()
        .map(|l| l.to_string())
        .collect())
}

/// Vide le magasin de mémoire (irréversible).
pub fn memory_clear() -> Result<Vec<String>> {
    crate::memory::clear()?;
    Ok(vec!["Mémoire vidée.".to_string()])
}

pub fn memory_forget(id: &str) -> Result<Vec<String>> {
    let id = id.trim();
    if id.is_empty() {
        return Err(anyhow!("usage: memory forget <id>"));
    }
    if crate::memory::forget(id)? {
        Ok(vec![format!("Entrée {id} supprimée.")])
    } else {
        Ok(vec![format!("Entrée {id} introuvable.")])
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Rôles
// ─────────────────────────────────────────────────────────────────────────────

/// Contenu effectif des rôles (données ; chaque interface met en forme).
pub fn roles_snapshot() -> Vec<(&'static str, Option<String>)> {
    let roles = crate::config::effective_roles();
    crate::route::KNOWN_ROLES
        .iter()
        .map(|r| (*r, roles.get(*r).cloned()))
        .collect()
}

pub fn roles_enabled() -> bool {
    crate::config::roles_enabled()
}

fn validate_role(role: &str) -> Result<()> {
    if crate::route::KNOWN_ROLES.contains(&role) {
        Ok(())
    } else {
        Err(anyhow!(
            "rôle inconnu `{role}` — connus : {}",
            crate::route::KNOWN_ROLES.join(", ")
        ))
    }
}

fn validate_route(route: &str) -> Result<()> {
    if crate::route::parse_route(route).is_some() {
        Ok(())
    } else {
        Err(anyhow!(
            "route invalide: {route} (attendu provider/model, \
             ex. openrouter/qwen/qwen3.8-27b:free)"
        ))
    }
}

/// Active/désactive le routage par rôle.
pub fn set_roles(on: bool) -> Result<Vec<String>> {
    crate::config::set_roles_enabled(on)?;
    Ok(vec![if on {
        "Rôles réactivés (routage par catégorie).".to_string()
    } else {
        "Rôles désactivés : un seul modèle (celui du provider actif) pour tout.".to_string()
    }])
}

pub fn role_set(role: &str, route: &str) -> Result<Vec<String>> {
    validate_role(role)?;
    validate_route(route)?;
    crate::config::set_role(role, route, false)?;
    Ok(vec![format!("Rôle {role} = {route}")])
}

pub fn role_unset(role: &str) -> Result<Vec<String>> {
    // Pas de validation ici : on doit pouvoir retirer un rôle devenu inconnu.
    crate::config::unset_role(role, false)?;
    Ok(vec![format!("Rôle {role} supprimé")])
}

/// Assigne chaque rôle au meilleur modèle `:free` du provider.
pub async fn role_auto(provider: &str) -> Result<Vec<String>> {
    let provider = if provider.trim().is_empty() {
        "openrouter"
    } else {
        provider.trim()
    };
    let assignments = crate::route::auto_role_assignments(provider).await?;
    if assignments.is_empty() {
        return Ok(vec![format!("Aucun modèle :free chez {provider}.")]);
    }
    let mut out = vec![format!("Rôles auto-assignés depuis {provider}:")];
    for (role, route) in assignments {
        crate::config::set_role(&role, &route, false)?;
        out.push(format!("  {role:<10} {route}"));
    }
    Ok(out)
}

// ─────────────────────────────────────────────────────────────────────────────
// Recherche web
// ─────────────────────────────────────────────────────────────────────────────

/// Résultats de recherche, formatés pour l'affichage (`max` borné).
pub async fn search(query: &str, max: usize) -> Result<Vec<String>> {
    let q = query.trim();
    if q.is_empty() {
        return Err(anyhow!("usage: search <requête>"));
    }
    let max = max.clamp(1, 20);
    let hits = crate::search::web_search(q, max).await?;
    Ok(vec![format!(
        "🔎 {q}\n{}",
        crate::search::format_hits(&hits)
    )])
}

// ─────────────────────────────────────────────────────────────────────────────
// Sélection de modèle
// ─────────────────────────────────────────────────────────────────────────────

/// Avertissement unique (les deux interfaces en affichaient deux versions
/// contradictoires : « benchmarks Artificial Analysis » vs « non benchmarké »).
pub const SELECT_DISCLAIMER: &str = "⚠ Classement heuristique : indices publiés quand ils \
     existent (Artificial Analysis), sinon mots-clés du nom — ce n'est pas une garantie de \
     qualité. Applique avec `model use <id>` (CLI : `zer0 model use <id>`).";

/// Classe les modèles `:free` pour un problème donné.
///
/// Source du catalogue : OpenRouter (liste publique, sans clé). Une seule
/// implémentation pour le CLI et la TUI, donc un seul nombre de résultats et
/// un seul texte d'avertissement.
pub async fn select_models(problem: &str, top: usize) -> Result<Vec<String>> {
    let problem = problem.trim();
    if problem.is_empty() {
        return Err(anyhow!("usage: select <problème>"));
    }
    let provider = crate::providers::find("openrouter")
        .ok_or_else(|| anyhow!("provider `openrouter` absent du catalogue"))?;
    let list = crate::models::list(&crate::http_client::HTTP_CLIENT, &provider, None).await?;
    let free: Vec<_> = list.into_iter().filter(crate::models::is_free).collect();
    if free.is_empty() {
        return Ok(vec![
            "Aucun modèle :free disponible pour le moment.".to_string()
        ]);
    }
    let (category, ranked) = crate::select::rank(problem, &free, top.clamp(1, 20));
    let mut lines = vec![
        format!("Problème → catégorie : {}", category.label()),
        format!("Modèles :free recommandés (top {}) :", ranked.len()),
    ];
    for (score, m) in ranked {
        lines.push(format!(
            "  [{score:>4}] {}{}  — {}",
            m.id,
            m.context_length
                .map(|c| format!("  (ctx {c})"))
                .unwrap_or_default(),
            crate::select::capability_note(&m)
        ));
    }
    lines.push(String::new());
    lines.push(SELECT_DISCLAIMER.to_string());
    Ok(lines)
}

// ─────────────────────────────────────────────────────────────────────────────
// Audit
// ─────────────────────────────────────────────────────────────────────────────

/// Les `n` dernières actions de l'agent, formatées en JSONL compact.
pub fn audit_tail(n: usize) -> Vec<String> {
    let lines = crate::audit::tail(n.clamp(1, 1000));
    if lines.is_empty() {
        return vec![format!(
            "Aucune action enregistrée pour le moment (journal : {}).",
            crate::audit::path().display()
        )];
    }
    let mut out = vec![format!("Journal : {}", crate::audit::path().display())];
    out.extend(lines);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_defaults_to_list() {
        assert_eq!(parse_task(&[]).unwrap(), TaskCmd::List);
        assert_eq!(parse_task(&["list"]).unwrap(), TaskCmd::List);
    }

    #[test]
    fn task_add_accepts_a_role() {
        // Divergence corrigée : le CLI acceptait `--role`, la TUI non.
        let cmd = parse_task(&["add", "écrire", "les", "tests", "--role", "code"]).unwrap();
        assert_eq!(
            cmd,
            TaskCmd::Add {
                text: "écrire les tests".into(),
                role: Some("code".into())
            }
        );
        let plain = parse_task(&["add", "juste", "du", "texte"]).unwrap();
        assert_eq!(
            plain,
            TaskCmd::Add {
                text: "juste du texte".into(),
                role: None
            }
        );
    }

    #[test]
    fn task_add_requires_text() {
        assert!(parse_task(&["add"]).is_err());
        assert!(parse_task(&["add", "   "]).is_err());
        assert!(parse_task(&["add", "--role", "code"]).is_err());
    }

    #[test]
    fn task_status_needs_an_id() {
        for sub in ["done", "claim", "rm"] {
            assert!(parse_task(&[sub]).is_err(), "{sub} sans id doit échouer");
            assert!(
                parse_task(&[sub, "  "]).is_err(),
                "{sub} id vide doit échouer"
            );
        }
    }

    #[test]
    fn task_aliases_are_equivalent() {
        assert_eq!(parse_task(&["rm", "T3"]).unwrap(), TaskCmd::Rm("T3".into()));
        assert_eq!(
            parse_task(&["del", "T3"]).unwrap(),
            TaskCmd::Rm("T3".into())
        );
        assert_eq!(
            parse_task(&["claim", "T3"]).unwrap(),
            TaskCmd::Claim("T3".into())
        );
        assert_eq!(
            parse_task(&["start", "T3"]).unwrap(),
            TaskCmd::Claim("T3".into())
        );
    }

    #[test]
    fn unknown_subcommand_is_an_error_with_usage() {
        let e = parse_task(&["frobnicate"]).unwrap_err().to_string();
        assert!(e.contains("usage: task"), "{e}");
    }

    #[test]
    fn pdf_args_are_validated_identically() {
        assert_eq!(
            parse_pdf_args(&["info", "a.pdf"]).unwrap(),
            PdfCmd::Info("a.pdf".into())
        );
        assert!(parse_pdf_args(&[]).is_err());
        assert!(parse_pdf_args(&["info"]).is_err());
        assert!(parse_pdf_args(&["frob"]).is_err());
        // merge : sortie + au moins une entrée
        assert!(parse_pdf_args(&["merge", "out.pdf"]).is_err());
        assert!(parse_pdf_args(&["merge", "out.pdf", "a.pdf", "b.pdf"]).is_ok());
        // split : bornes numériques cohérentes
        assert!(parse_pdf_args(&["split", "a.pdf", "out", "x", "3"]).is_err());
        assert!(parse_pdf_args(&["split", "a.pdf", "out", "5", "2"]).is_err());
        assert!(parse_pdf_args(&["split", "a.pdf", "out", "2", "5"]).is_ok());
    }

    #[test]
    fn quota_rejects_unknown_provider() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert!(rt.block_on(quota_report(Some("inconnu-xyz"))).is_err());
    }

    #[test]
    fn embeddings_and_personas_validate_input() {
        assert!(embeddings_set("", "m").is_err());
        assert!(embeddings_set("openrouter", "").is_err());
        assert!(embeddings_set("provider-inexistant", "m").is_err());
        assert!(persona_add("", "x").is_err());
        assert!(persona_add("nom", "   ").is_err());
        assert!(persona_show("  ").is_err());
    }

    #[test]
    fn provider_commands_validate_before_writing_config() {
        assert!(provider_add("provider-qui-nexiste-pas", Some("k")).is_err());
        assert!(provider_use("inconnu").is_err());
        assert!(provider_remove("inconnu").is_err());
        // base_url qui est en réalité une clé API
        assert!(provider_add_custom("monapi", "sk-abc123", None, None).is_err());
        assert!(provider_add_custom("openrouter", "https://x/v1", None, None).is_err());
        assert!(provider_add_custom("", "https://x/v1", None, None).is_err());
        assert!(provider_add_cline(Some("pas-une-url"), None).is_err());
    }

    #[test]
    fn provider_list_marks_the_active_provider() {
        let lines = provider_list();
        assert!(
            lines.iter().any(|l| l.starts_with("*")),
            "aucun provider actif marqué"
        );
        assert!(lines.iter().any(|l| l.contains("openrouter")));
    }

    #[test]
    fn kb_commands_validate_before_touching_the_database() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert!(rt.block_on(kb_search("   ", 5)).is_err());
        // Dossier inexistant : erreur avant ouverture de la base.
        let missing = std::path::PathBuf::from("/tmp/zer0-kb-inexistant-xyz");
        let e = rt
            .block_on(kb_ingest(Some(missing), false))
            .unwrap_err()
            .to_string();
        assert!(e.contains("introuvable"), "{e}");
    }

    #[test]
    fn memory_commands_validate_their_input() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert!(rt.block_on(memory_save("   ", None)).is_err());
        assert!(rt.block_on(memory_search("", 5)).is_err());
        assert!(memory_forget("  ").is_err());
    }

    #[test]
    fn role_validation_rejects_unknown_roles_and_routes() {
        // Erreurs pures : aucune écriture de config n'est atteinte.
        assert!(validate_role("code").is_ok());
        assert!(validate_role("inconnu").is_err());
        assert!(validate_route("openrouter/qwen/qwen3.8-27b:free").is_ok());
        assert!(validate_route("openrouter/").is_err());
        assert!(validate_route("sans-slash").is_err());
        assert!(role_set("inconnu", "openrouter/qwen/x").is_err());
        assert!(role_set("code", "sans-slash").is_err());
    }

    #[test]
    fn auto_args_accept_both_forms() {
        assert_eq!(parse_auto_args(&[]), ("openrouter".to_string(), 4));
        assert_eq!(parse_auto_args(&["6"]), ("openrouter".to_string(), 6));
        assert_eq!(parse_auto_args(&["moonshot"]), ("moonshot".to_string(), 4));
        assert_eq!(
            parse_auto_args(&["moonshot", "3"]),
            ("moonshot".to_string(), 3)
        );
        // bornes
        assert_eq!(parse_auto_args(&["99"]).1, 12);
        assert_eq!(parse_auto_args(&["0"]).1, 1);
    }

    #[test]
    fn route_add_validates_the_format() {
        // Validation seule : on ne touche pas à la vraie config dans un test.
        assert!(route_add("").is_err());
        assert!(route_add("   ").is_err());
        assert!(route_add("sans-slash").is_err());
        assert!(route_add("sans-slash")
            .unwrap_err()
            .to_string()
            .contains("usage"));
    }

    #[test]
    fn redundant_provider_prefix_is_stripped() {
        assert_eq!(
            strip_route_prefix("openrouter", "openrouter/minimax/x:free"),
            "minimax/x:free"
        );
        assert_eq!(
            strip_route_prefix("openrouter", "minimax/x:free"),
            "minimax/x:free"
        );
        assert_eq!(
            strip_route_prefix("nvidia", "nvidia/mistralai/foo"),
            "mistralai/foo"
        );
        // Un préfixe seul ne doit pas produire un identifiant vide.
        assert_eq!(
            strip_route_prefix("openrouter", "openrouter/"),
            "openrouter/"
        );
    }

    #[test]
    fn set_model_refuses_embeddings_and_empty_ids() {
        // Refus avant toute lecture/écriture de config : testable sans disque.
        let e = set_model("text-embedding-3-large").unwrap_err().to_string();
        assert!(e.contains("embedding"), "{e}");
        assert!(set_model("   ").unwrap_err().to_string().contains("usage"));
        assert!(set_model("bge-reranker-v2").is_err());
    }

    #[test]
    fn search_and_select_validate_their_input() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert!(rt.block_on(search("   ", 5)).is_err());
        assert!(rt.block_on(select_models(" ", 5)).is_err());
    }
}
