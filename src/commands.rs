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

// Thème de couleurs

/// Sélectionne et persiste un thème de la TUI.
pub fn set_theme(name: &str) -> Result<Vec<String>> {
    let selected = crate::theme::resolve(name)?;
    let mut global = crate::config::load_global();
    global.theme = Some(selected.id.to_string());
    crate::config::save_global(&global)?;
    crate::theme::activate(selected.id)?;
    Ok(vec![format!(
        "Thème actif : {} ({})",
        selected.label, selected.id
    )])
}

// Watchlist `0cg`

/// Identifiant CoinGecko plausible : minuscules, chiffres, `-` et `_`.
/// (Les symboles boursiers (`btc`) passent aussi : l'API les résout.)
pub fn normalize_coin(raw: &str) -> Option<String> {
    let c = raw.trim().to_lowercase();
    if c.is_empty() || c.len() > 64 {
        return None;
    }
    if !c
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return None;
    }
    Some(c)
}

/// Watchlist configurée, **dans l'ordre de l'utilisateur**.
pub fn cg_coins() -> Vec<String> {
    crate::config::load_global().cg_coins
}

fn save_cg_coins(coins: Vec<String>) -> Result<()> {
    let mut g = crate::config::load_global();
    g.cg_coins = coins;
    crate::config::save_global(&g)?;
    Ok(())
}

/// Ajoute des devises (sans doublon), dans l'ordre donné.
pub fn cg_add(names: &[&str]) -> Result<Vec<String>> {
    if names.is_empty() {
        return Err(anyhow!("usage : /0cg add <id|symbole>…"));
    }
    let mut coins = cg_coins();
    let mut added = Vec::new();
    for raw in names {
        let c = normalize_coin(raw)
            .ok_or_else(|| anyhow!("devise invalide : {raw} (minuscules, chiffres, - et _)"))?;
        if !coins.contains(&c) {
            coins.push(c.clone());
            added.push(c);
        }
    }
    if added.is_empty() {
        return Ok(vec!["déjà dans la watchlist".to_string()]);
    }
    save_cg_coins(coins.clone())?;
    Ok(vec![format!(
        "ajouté : {} · watchlist ({}) : {}",
        added.join(", "),
        coins.len(),
        coins.join(", ")
    )])
}

/// Retire des devises.
pub fn cg_rm(names: &[&str]) -> Result<Vec<String>> {
    if names.is_empty() {
        return Err(anyhow!("usage : /0cg rm <id|symbole>…"));
    }
    let mut coins = cg_coins();
    let mut removed = Vec::new();
    for raw in names {
        if let Some(c) = normalize_coin(raw) {
            if let Some(i) = coins.iter().position(|x| *x == c) {
                coins.remove(i);
                removed.push(c);
            }
        }
    }
    if removed.is_empty() {
        return Ok(vec!["aucune de ces devises dans la watchlist".to_string()]);
    }
    save_cg_coins(coins.clone())?;
    Ok(vec![format!(
        "retiré : {} · watchlist ({}) : {}",
        removed.join(", "),
        coins.len(),
        if coins.is_empty() {
            "vide".to_string()
        } else {
            coins.join(", ")
        }
    )])
}

/// Liste lisible de la watchlist.
pub fn cg_list() -> Vec<String> {
    let coins = cg_coins();
    if coins.is_empty() {
        return vec!["watchlist vide — `/0cg add btc` (id CoinGecko ou symbole)".to_string()];
    }
    vec![format!(
        "watchlist ({}) : {}",
        coins.len(),
        coins.join(", ")
    )]
}

/// `/theme <nom> default` : mémorise ce thème comme **défaut** (et l'applique).
pub fn set_theme_default(name: &str) -> Result<Vec<String>> {
    let selected = crate::theme::resolve(name)?;
    let mut global = crate::config::load_global();
    global.theme = Some(selected.id.to_string());
    global.theme_default = Some(selected.id.to_string());
    crate::config::save_global(&global)?;
    crate::theme::activate(selected.id)?;
    Ok(vec![format!(
        "Thème par défaut : {} ({}) — appliqué",
        selected.label, selected.id
    )])
}

/// Thème par défaut configuré (`None` = aucun).
pub fn theme_default() -> Option<String> {
    crate::config::load_global()
        .theme_default
        .filter(|s| !s.trim().is_empty())
}

/// `/theme default` : revient au thème par défaut (à défaut, le premier de la
/// liste : `ghost`).
pub fn reset_theme() -> Result<Vec<String>> {
    let target = theme_default().unwrap_or_else(|| {
        crate::theme::ALL
            .first()
            .map(|t| t.id.to_string())
            .unwrap_or_else(|| "ghost".to_string())
    });
    let mut lines = set_theme(&target)?;
    let suffix = if theme_default().is_some() {
        "défaut"
    } else {
        "premier thème de la liste (aucun défaut enregistré)"
    };
    lines.push(format!("({suffix})"));
    Ok(lines)
}

// Tâches

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

// Modèle actif

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

// Chaîne de routage (bascule)

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

// Quota

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

// PDF

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

// Catalogue d'API publiques

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

// Personas

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

// Coffre

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

// Embeddings

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

// Providers

/// Tableau des providers connus.
///
/// Deux colonnes distinctes : `transport` (direct ou via `zer0-gw`) et `clé`
/// (présence d'une clé **dans l'agent**). Avant, la colonne unique affichait
/// « zer0-gw » à la place de la clé, ce qui laissait croire qu'une clé était
/// déposée alors que `provider keys` répondait « — ».
pub fn provider_list() -> Vec<String> {
    provider_list_with(
        crate::config::load_global().provider.as_deref(),
        crate::config::gateway_enabled(),
    )
}

/// Variante **pure** (provider actif et passerelle fournis) : c'est elle que les
/// tests exercent, pour ne pas dépendre de la config de la machine — un runner
/// CI vierge n'a aucun provider actif.
pub fn provider_list_with(active_id: Option<&str>, gw: bool) -> Vec<String> {
    let mut out = vec![format!(
        "{:<3}{:<14}{:<26}{:<13}{:<11}{}",
        "", "id", "nom", "gratuit", "transport", "clé"
    )];
    for p in crate::providers::all() {
        let active = if active_id == Some(p.id.as_str()) {
            "*"
        } else {
            " "
        };
        let via_gw = gw && !p.custom && p.free != crate::providers::FreeKind::Local;
        let transport = if via_gw { "zer0-gw" } else { "direct" };
        let key = if p.free == crate::providers::FreeKind::Local {
            "local"
        } else if crate::config::key_for(&p).is_some() {
            "oui"
        } else {
            "—"
        };
        out.push(format!(
            "{:<3}{:<14}{:<26}{:<13}{:<11}{}{}",
            active,
            p.id,
            p.name,
            p.free.label(),
            transport,
            key,
            if p.custom { "  (custom)" } else { "" }
        ));
    }
    out.push(String::new());
    out.push("« — » = aucune clé dans l'agent (une clé déposée reste chez zer0-gw).".into());
    out.push(
        "Ajouter : provider add <id> [<base_url>] [<nom>] <clé> [--gw|--local] [--force]  ·  Activer : provider use <id>"
            .into(),
    );
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
                "Déposer : provider add <id> [<base_url>] <clé>   ·   Oublier : provider remove <id>".into(),
            );
            out
        }
        Err(e) => vec![format!("zer0-gw : {e}")],
    }
}

/// Choix de transport pour l'ajout/résolution d'un provider.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GwRoute {
    /// Passerelle si elle répond, sinon stockage local (avec avertissement).
    Auto,
    /// Forcer la passerelle (échec si injoignable).
    Gw,
    /// Forcer le stockage local (aucune passerelle).
    Local,
}

/// Arguments de [`provider_add`].
pub struct ProviderAddArgs<'a> {
    pub id: &'a str,
    /// Base URL (override, ex. pour un provider custom ou une base obsolète).
    pub base_url: Option<&'a str>,
    pub name: Option<&'a str>,
    pub key: Option<&'a str>,
    pub route: GwRoute,
    /// Activer malgré un test d'inférence non concluant.
    pub force: bool,
}

/// Verdict d'une sonde provider (via la passerelle).
enum Verdict {
    /// `/models` OK **et** chat OK.
    Ready,
    /// `/models` OK mais aucun modèle pour tester le chat.
    NotTested,
    /// `/models` OK mais chat refusé (clé non autorisée pour l'inférence).
    NoInference,
    /// 402 : compte sans crédits.
    NoCredits,
    /// 401/403 sur `/models` : clé refusée.
    BadKey,
    /// Injoignable / 404 / 410 : URL ou modèle obsolète.
    BadUrl,
    /// 429.
    RateLimited,
    Other,
}

fn is2xx(s: u16) -> bool {
    (200..=299).contains(&s)
}

fn short(s: &str) -> String {
    let t = s.trim();
    if t.len() > 180 {
        format!("{}…", &t[..180])
    } else {
        t.to_string()
    }
}

/// Sonde un provider **à travers la passerelle** : `GET /v1/models` puis, si
/// possible, un `POST /v1/chat/completions` de 1 token. Retourne un verdict
/// explicite + un détail lisible. Distingue « URL obsolète », « clé refusée »,
/// « 402 crédits » et « clé valide pour lister mais pas pour l'inférence ».
fn probe_provider(id: &str) -> (Verdict, String, Option<String>) {
    let models = match crate::gwclient::probe_models(id) {
        Err(e) => {
            return (
                Verdict::BadUrl,
                format!("base_url injoignable ({e}) — URL erronée ou obsolète ?"),
                None,
            )
        }
        Ok((ms, mb)) => {
            if !is2xx(ms) {
                return match ms {
                    401 | 403 => (
                        Verdict::BadKey,
                        format!("/models → {ms} : clé refusée (401/403)."),
                        None,
                    ),
                    404 | 410 => (
                        Verdict::BadUrl,
                        format!("/models → {ms} : URL ou modèle obsolète/introuvable."),
                        None,
                    ),
                    429 => (
                        Verdict::RateLimited,
                        "/models → 429 : rate-limit, réessaie.".to_string(),
                        None,
                    ),
                    _ => {
                        // Cloudflare Workers AI n'expose pas `/v1/models` (405).
                        // On tente un chat direct avec un modèle connu.
                        if id == "cloudflare" && ms == 405 {
                            let m = "@cf/openai/gpt-oss-20b";
                            match crate::gwclient::probe_chat(id, m) {
                                Err(e) => {
                                    return (
                                        Verdict::BadUrl,
                                        format!("chat injoignable ({e})."),
                                        None,
                                    )
                                }
                                Ok((cs, _)) => {
                                    if is2xx(cs) {
                                        return (
                                            Verdict::Ready,
                                            format!("chat ({m}) → {cs} : OK (pas de /models)."),
                                            Some(m.to_string()),
                                        );
                                    }
                                }
                            }
                        }
                        (
                            Verdict::Other,
                            format!("/models → {ms} : {}", short(&mb)),
                            None,
                        )
                    }
                };
            }
            mb
        }
    };
    let ids = crate::gwclient::model_ids(&models, 5);
    if ids.is_empty() {
        // Certains providers (ex. Cloudflare Workers AI) n'exposent pas
        // `/v1/models` : on tente un chat direct avec un modèle connu.
        let fallback = match id {
            "cloudflare" => vec!["@cf/openai/gpt-oss-20b".to_string()],
            _ => vec![],
        };
        if fallback.is_empty() {
            return (
                Verdict::NotTested,
                "/models → 200, mais aucun modèle listé pour tester l'inférence.".to_string(),
                None,
            );
        }
        for m in &fallback {
            match crate::gwclient::probe_chat(id, m) {
                Err(e) => return (Verdict::BadUrl, format!("chat injoignable ({e})."), None),
                Ok((cs, _)) => {
                    if is2xx(cs) {
                        return (
                            Verdict::Ready,
                            format!("chat ({m}) → {cs} : OK (pas de /models)."),
                            Some(m.clone()),
                        );
                    }
                }
            }
        }
        return (
            Verdict::Other,
            format!("chat → échec sur les modèles de secours pour {id}."),
            None,
        );
    }
    // Teste jusqu'à 5 modèles : un seul modèle en 404/410 ne condamne pas le
    // provider (cas des agrégateurs). On agrège par priorité : 2xx > 402 >
    // 401/403 > 429 > 404/410 > autre.
    let mut statuses: Vec<u16> = Vec::new();
    for m in &ids {
        match crate::gwclient::probe_chat(id, m) {
            Err(e) => return (Verdict::BadUrl, format!("chat injoignable ({e})."), None),
            Ok((cs, _)) => {
                if is2xx(cs) {
                    // Modèle qui répond : à mémoriser pour régler un modèle valide.
                    return (
                        Verdict::Ready,
                        format!("/models → 200 · chat ({m}) → {cs} : OK."),
                        Some(m.clone()),
                    );
                }
                statuses.push(cs);
            }
        }
    }
    let n = statuses.len();
    let last = statuses.last().copied().unwrap_or(0);
    let verdict = if statuses.contains(&402) {
        Verdict::NoCredits
    } else if statuses.iter().any(|&c| c == 401 || c == 403) {
        Verdict::NoInference
    } else if statuses.contains(&429) {
        Verdict::RateLimited
    } else if statuses.iter().all(|&c| c == 404 || c == 410) {
        Verdict::BadUrl
    } else {
        Verdict::Other
    };
    let msg = match verdict {
        Verdict::NoCredits => {
            format!("/models → 200 mais chat → 402 sur {n} modèle(s) : compte sans crédits.")
        }
        Verdict::NoInference => format!(
            "/models → 200 mais chat refusé (401/403) sur {n} modèle(s) : clé valide pour lister \
             mais non autorisée pour l'inférence."
        ),
        Verdict::RateLimited => format!("chat → 429 sur {n} modèle(s) : rate-limit, réessaie."),
        Verdict::BadUrl => {
            format!("chat → 404/410 sur les {n} modèle(s) testés : modèles/URL obsolètes.")
        }
        _ => format!("chat → {last} sur {n} modèle(s) testé(s)."),
    };
    (verdict, msg, None)
}

/// Ajoute un provider et sa clé. Selon `route` : passerelle (dépôt + sonde,
/// clé hors agent) ou stockage local (avec avertissement). Par défaut, la
/// passerelle est utilisée si elle répond.
pub fn provider_add(args: ProviderAddArgs) -> Result<Vec<String>> {
    let id = args.id.trim().to_ascii_lowercase();
    if id.is_empty() {
        return Err(anyhow!(
            "usage: provider add <id> [<base_url>] [<nom>] <clé> [--gw|--local] [--force]"
        ));
    }
    let builtin = crate::providers::find(&id);
    let base_url = args.base_url.map(str::trim).filter(|u| !u.is_empty());
    if let Some(u) = base_url {
        if !(u.starts_with("http://") || u.starts_with("https://")) {
            return Err(anyhow!(
                "base_url invalide : « {u} » (http://… ou https://… ; une clé n'est pas une URL)"
            ));
        }
    }
    if builtin.is_none() && base_url.is_none() {
        return Err(anyhow!(
            "provider personnalisé : base_url requise. usage: provider add {id} <base_url> [nom] <clé>"
        ));
    }
    let key = args.key.map(str::trim).filter(|k| !k.is_empty());
    let is_local = matches!(id.as_str(), "local" | "local-embed");

    let gw_ok = crate::gwclient::probe();
    let use_gw = !is_local
        && match args.route {
            GwRoute::Local => false,
            GwRoute::Gw => {
                if !gw_ok {
                    return Err(anyhow!(
                        "zer0-gw injoignable — lance-le (sudo systemctl start zer0-gw) \
                         ou retire --gw / passe --local."
                    ));
                }
                true
            }
            GwRoute::Auto => gw_ok,
        };

    if use_gw {
        let gw_base = base_url
            .map(str::to_string)
            .or_else(|| builtin.as_ref().map(|p| p.base_url.clone()))
            .unwrap_or_default();
        return provider_add_via_gateway(
            &id,
            &gw_base,
            builtin.is_some(),
            key,
            args.force,
            args.name,
        );
    }
    provider_add_local(&id, base_url, args.name, builtin.is_some(), key)
}

/// Enregistre le provider sur la passerelle, y dépose la clé, sonde, puis
/// active ssi le test passe (ou `--force`). La clé ne transite jamais par l'agent.
fn provider_add_via_gateway(
    id: &str,
    base_url: &str,
    is_builtin: bool,
    key: Option<&str>,
    force: bool,
    name: Option<&str>,
) -> Result<Vec<String>> {
    if base_url.is_empty() {
        return Err(anyhow!("base_url manquante pour la passerelle"));
    }
    crate::gwclient::register_provider(id, base_url)
        .map_err(|e| anyhow!("enregistrement passerelle échoué : {e}"))?;
    let mut out = vec![format!(
        "🔧 Provider « {id} » enregistré dans zer0-gw → {base_url}"
    )];
    let Some(key) = key else {
        out.push("⚠ Aucune clé fournie : provider enregistré mais NON activé.".into());
        out.push("Ajoute une clé : provider add <id> [<base_url>] <clé>".into());
        return Ok(out);
    };
    crate::gwclient::set_key(id, key).map_err(|e| anyhow!("dépôt de clé échoué : {e}"))?;
    out.push("🔑 Clé déposée sur zer0-gw (écriture seule) — aucune clé dans l'agent.".into());

    let (verdict, detail, ok_model) = probe_provider(id);
    out.push(format!("🧪 Test : {detail}"));
    let activate = matches!(verdict, Verdict::Ready | Verdict::NotTested) || force;
    if !activate {
        return Err(anyhow!(
            "provider « {id} » enregistré dans zer0-gw mais NON activé : {detail}\n\
             Corrige la base_url ou la clé puis relance, ou ajoute --force pour activer malgré tout."
        ));
    }

    let mut g = crate::config::load_global();
    if is_builtin {
        // Comme activate_provider, mais on règle aussi un modèle qui marche.
        g.custom.remove(&format!("gw-{id}"));
        g.provider = Some(id.to_string());
    } else {
        // Provider custom : pointer sa base_url vers la passerelle, sans clé.
        g.custom.insert(
            id.to_string(),
            crate::config::CustomProvider {
                // Nom lisible fourni par l'utilisateur (overlay `/provider add`),
                // sinon l'identifiant faute de mieux.
                name: name
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .unwrap_or(id)
                    .to_string(),
                base_url: format!(
                    "{}/{}/v1",
                    crate::gwclient::gw_base().trim_end_matches('/'),
                    id
                ),
                env_key: None,
            },
        );
        g.provider = Some(id.to_string());
    }
    // Modèle validé par la sonde : évite le 404 « modèle inconnu » au 1er tour.
    if let Some(m) = ok_model {
        g.model = Some(m.clone());
        out.push(format!("🎯 Modèle réglé : {m}"));
    }
    crate::config::save_global(&g)?;
    out.push(format!("✅ Provider actif : {id}"));
    if !matches!(verdict, Verdict::Ready) {
        out.push(
            "⚠ Activé malgré un test non concluant (--force). Choisis un modèle : /model".into(),
        );
    }
    Ok(out)
}

/// Stockage local (config.toml, non chiffré) — avec avertissement de durcissement.
fn provider_add_local(
    id: &str,
    base_url: Option<&str>,
    name: Option<&str>,
    is_builtin: bool,
    key: Option<&str>,
) -> Result<Vec<String>> {
    let mut g = crate::config::load_global();
    if !is_builtin {
        let base_url =
            base_url.ok_or_else(|| anyhow!("base_url requise pour un provider personnalisé"))?;
        g.custom.insert(
            id.to_string(),
            crate::config::CustomProvider {
                name: name.map(str::to_string).unwrap_or_else(|| id.to_string()),
                base_url: base_url.to_string(),
                env_key: None,
            },
        );
    }
    if let Some(key) = key {
        g.keys.insert(id.to_string(), key.to_string());
    }
    g.provider = Some(id.to_string());
    if let Some(dm) = crate::providers::default_model(id) {
        g.model = Some(dm.to_string());
    }
    crate::config::save_global(&g)?;
    let mut out = vec![format!("Provider actif (local) : {id}")];
    if key.is_some() {
        out.push("⚠ Clé stockée localement (config.toml, non chiffré). Lance zer0-gw pour le durcissement.".into());
    }
    Ok(out)
}

/// Compat : `add-custom` (id + base_url + nom + variable d'env de clé).
pub fn provider_add_custom(
    id: &str,
    base_url: &str,
    name: Option<&str>,
    env: Option<&str>,
) -> Result<Vec<String>> {
    let id = id.trim().to_ascii_lowercase();
    if id.is_empty() {
        return Err(anyhow!("usage: provider add-custom <id> <base_url>"));
    }
    if crate::providers::builtins().iter().any(|p| p.id == id) {
        return Err(anyhow!(
            "'{id}' est déjà un provider intégré — pas besoin de add-custom."
        ));
    }
    // La clé peut venir d'une variable d'env (param `env` = nom de variable).
    let key = env
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .and_then(|e| std::env::var(e).ok())
        .filter(|s| !s.is_empty());
    provider_add(ProviderAddArgs {
        id: &id,
        base_url: Some(base_url),
        name,
        key: key.as_deref(),
        route: GwRoute::Auto,
        force: false,
    })
}

/// Compat : ancien helper `cline` (URL requise). Délègue au flux générique,
/// qui route désormais par la passerelle si elle répond.
pub fn provider_add_cline(url: Option<&str>, key: Option<&str>) -> Result<Vec<String>> {
    let url = url.map(str::trim).filter(|u| !u.is_empty());
    if let Some(u) = url {
        if !(u.starts_with("http://") || u.starts_with("https://")) {
            return Err(anyhow!(
                "« provider add cline » attend une URL (http://…), pas une clé.\n\
                 usage: provider add cline <http://hôte:port/v1> [<clé>]"
            ));
        }
    }
    provider_add(ProviderAddArgs {
        id: "cline",
        base_url: url,
        name: None,
        key,
        route: GwRoute::Auto,
        force: false,
    })
}

pub fn provider_use(id: &str) -> Result<Vec<String>> {
    let p = crate::providers::find(id).ok_or_else(|| anyhow!("provider inconnu: {id}"))?;
    let mut g = crate::config::load_global();
    g.provider = Some(p.id.to_string());
    if let Some(d) = crate::providers::default_model(&p.id) {
        g.model = Some(d.to_string());
    }
    crate::config::save_global(&g)?;
    let mut out = vec![format!("Provider actif: {}", p.name)];
    if p.id == "local" {
        out.push("⚠ Mode local : un seul modèle chargé.".to_string());
        for l in crate::local::limitations() {
            out.push(format!("  · {l}"));
        }
        out.push(
            "  · llama-server est démarré automatiquement au premier message (port 8080)."
                .to_string(),
        );
    }
    Ok(out)
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

// Base de connaissance (SQLite + FTS5)

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
    kb_ingest_with(dir, embed, &|_| {}).await
}

/// Comme `kb_ingest`, en publiant l'avancement (ligne déjà formatée).
pub async fn kb_ingest_with(
    dir: Option<std::path::PathBuf>,
    embed: bool,
    progress: &(dyn Fn(crate::knowledge::IngestProgress) + Sync),
) -> Result<Vec<String>> {
    let d = dir.unwrap_or_else(crate::knowledge::default_dir);
    // Vérifié avant d'ouvrir la base : « dossier inexistant » ne doit pas créer
    // une base vide au passage.
    if !d.exists() {
        return Err(anyhow!("dossier introuvable: {}", d.display()));
    }
    let report = crate::knowledge::ingest_dir_with(&d, embed, progress).await?;
    let mut out = vec![report.summary(&d)];
    if embed && crate::memory::embedder_id().is_none() {
        out.push("(aucun embedder configuré → vecteurs non calculés)".to_string());
    }
    out.push(format!("Base : {}", crate::knowledge::db_path().display()));
    Ok(out)
}

/// Supprime de l'index les sources dont le fichier n'existe plus.
pub fn kb_purge() -> Result<Vec<String>> {
    let n = crate::knowledge::purge_missing()?;
    Ok(vec![format!(
        "Fantômes purgés : {n} source(s) dont le fichier n'existe plus. Index FTS reconstruit."
    )])
}

/// Retire de l'index les sources dont le chemin contient `pattern`.
pub fn kb_dedupe() -> Result<Vec<String>> {
    let n = crate::knowledge::dedupe_sources()?;
    Ok(vec![format!(
        "Connaissance : {n} source(s) fusionnée(s)/renommée(s)."
    )])
}

pub fn kb_drop(pattern: &str) -> Result<Vec<String>> {
    if pattern.trim().is_empty() {
        return Err(anyhow!("usage: kb drop <motif de source>"));
    }
    let n = crate::knowledge::drop_source(pattern)?;
    Ok(vec![format!(
        "Source(s) retirée(s) de l'index : {n} chunk(s) dont le chemin contient `{pattern}`."
    )])
}

pub fn kb_rebuild() -> Result<Vec<String>> {
    crate::knowledge::rebuild_fts()?;
    let pruned = crate::knowledge::prune_orphan_vectors().unwrap_or(0);
    let mut out = vec!["Index FTS5 reconstruit.".to_string()];
    match pruned {
        0 => out.push("Aucun vecteur orphelin à supprimer.".to_string()),
        n => out.push(format!(
            "{n} vecteur(s) orphelin(s) supprimé(s) (chunks absents)."
        )),
    }
    Ok(out)
}

pub fn kb_log(n: usize) -> Vec<String> {
    crate::knowledge::tail_log(n.clamp(1, 500))
        .lines()
        .map(|l| l.to_string())
        .collect()
}

// Mémoire persistante

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

// Rôles

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

// Recherche web

/// Résultats de recherche, formatés pour l'affichage (`max` borné).
pub async fn search(query: &str, max: usize) -> Result<Vec<String>> {
    let q = query.trim();
    if q.is_empty() {
        return Err(anyhow!("usage: search <requête>"));
    }
    let max = max.clamp(1, 20);
    let (hits, backend) = crate::search::web_search_with_backend(q, max).await?;
    Ok(vec![format!(
        "🔎 {q}  (moteur : {backend})\n{}",
        crate::search::format_hits(&hits)
    )])
}

// Sélection de modèle

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

// Audit

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
        // Provider inexistant sans base_url : erreur avant toute écriture / réseau.
        assert!(provider_add(ProviderAddArgs {
            id: "provider-qui-nexiste-pas",
            base_url: None,
            name: None,
            key: Some("k"),
            route: GwRoute::Auto,
            force: false,
        })
        .is_err());
        // base_url invalide (ce n'est pas une URL) : rejetée avant le réseau.
        assert!(provider_add(ProviderAddArgs {
            id: "monapi",
            base_url: Some("sk-abc123"),
            name: None,
            key: Some("k"),
            route: GwRoute::Local,
            force: false,
        })
        .is_err());
        assert!(provider_use("inconnu").is_err());
        assert!(provider_remove("inconnu").is_err());
        // base_url qui est en réalité une clé API
        assert!(provider_add_custom("monapi", "sk-abc123", None, None).is_err());
        assert!(provider_add_custom("openrouter", "https://x/v1", None, None).is_err());
        assert!(provider_add_custom("", "https://x/v1", None, None).is_err());
        assert!(provider_add_cline(Some("pas-une-url"), None).is_err());
    }

    #[test]
    fn provider_list_marks_the_given_active_provider() {
        let lines = provider_list_with(Some("openrouter"), false);
        let active = lines
            .iter()
            .find(|l| l.contains("openrouter"))
            .expect("openrouter absent du catalogue");
        assert!(
            active.starts_with('*'),
            "provider actif non marqué : {active}"
        );
        let other = lines
            .iter()
            .find(|l| l.contains("google"))
            .expect("google absent du catalogue");
        assert!(!other.starts_with('*'), "provider inactif marqué : {other}");

        // Aucun provider actif (cas d'un runner vierge) : aucune étoile.
        let none = provider_list_with(None, false);
        assert!(none.iter().all(|l| !l.starts_with('*')));
        assert!(none.iter().any(|l| l.contains("transport")));
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
