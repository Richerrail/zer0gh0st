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
pub fn set_model(id: &str) -> Result<Vec<String>> {
    let id = id.trim();
    if id.is_empty() {
        return Err(anyhow!("usage: model use <id>"));
    }
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
