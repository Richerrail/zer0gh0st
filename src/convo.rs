//! Historique de conversation persisté (JSONL).
//!
//! Une ligne par message (`role`, `content`, `tool_calls`…). Rechargé au
//! démarrage (`--resume`) ou par l'écran des conversations (F6).

use anyhow::Result;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use crate::agent::history::ChatMessage;
use crate::config;

pub fn path() -> PathBuf {
    config::data_dir().join("conversation.jsonl")
}

/// Charge les `max` derniers messages (0 = tout), **sans** réparation.
pub fn load(max: usize) -> Vec<ChatMessage> {
    load_from_path(&path(), max)
}

/// Charge et **répare** l'historique (à utiliser pour `--resume` / F6).
pub fn load_repaired(max: usize) -> Vec<ChatMessage> {
    let msgs = load_from_path(&path(), 0);
    take_last(repair(msgs), max)
}

/// Écrit l'historique actif en entier (atomique : tmp + rename).
///
/// Utilisé après une compaction : sinon la version résumée n'existait qu'en
/// mémoire, et un redémarrage rechargeait l'historique complet (contexte perdu
/// ou re-compacté à chaque lancement).
pub fn rewrite(msgs: &[ChatMessage]) -> Result<()> {
    let p = path();
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = p.with_extension("jsonl.tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        for m in msgs {
            writeln!(f, "{}", serde_json::to_string(m)?)?;
        }
        f.sync_all()?;
    }
    std::fs::rename(&tmp, &p)?;
    Ok(())
}

/// Ajoute un message à l'historique.
pub fn append(msg: &ChatMessage) -> Result<()> {
    let p = path();
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = OpenOptions::new().create(true).append(true).open(&p)?;
    writeln!(f, "{}", serde_json::to_string(msg)?)?;
    Ok(())
}

/// Charge les messages depuis un chemin arbitraire (0 = tout).
pub fn load_from_path(p: &std::path::Path, max: usize) -> Vec<ChatMessage> {
    let Ok(s) = std::fs::read_to_string(p) else {
        return Vec::new();
    };
    let all: Vec<ChatMessage> = s
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    take_last(all, max)
}

fn take_last(msgs: Vec<ChatMessage>, max: usize) -> Vec<ChatMessage> {
    if max == 0 || msgs.len() <= max {
        msgs
    } else {
        msgs[msgs.len() - max..].to_vec()
    }
}

/// Rend un historique **acceptable par un provider OpenAI-compatible**.
///
/// Deux règles, apprises à la dure : couper un JSONL « au milieu » produit des
/// historiques que la spec refuse (400, puis « tous les endpoints ont échoué »).
///
/// 1. l'historique commence sur un `user` (jamais sur un `tool` orphelin) ;
/// 2. un `tool` sans `tool_calls` parent est supprimé, et un `assistant` dont
///    les `tool_calls` n'ont pas (tous) leur résultat est supprimé.
pub fn repair(msgs: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let mut out: Vec<ChatMessage> = Vec::with_capacity(msgs.len());
    // Identifiants d'appels en attente de résultat.
    let mut pending: Vec<String> = Vec::new();

    for m in msgs {
        match m.role.as_str() {
            "tool" => {
                let Some(id) = m.tool_call_id.as_ref() else {
                    continue; // résultat sans identifiant : inutilisable
                };
                if pending.iter().any(|p| p == id) {
                    pending.retain(|p| p != id);
                    out.push(m);
                }
                // Sinon : orphelin (le parent a été coupé) → jeté.
            }
            "assistant" => {
                // Les appels précédents non résolus restent non résolus.
                pending.clear();
                if let Some(calls) = &m.tool_calls {
                    pending = calls.iter().map(|c| c.id.clone()).collect();
                }
                out.push(m);
            }
            _ => {
                pending.clear();
                out.push(m);
            }
        }
    }

    // Un dernier assistant dont les appels n'ont pas de résultat est invalide.
    while matches!(out.last().map(|m| m.role.as_str()), Some("assistant")) {
        let has_calls = out.last().and_then(|m| m.tool_calls.as_ref()).is_some();
        if has_calls {
            out.pop();
        } else {
            break;
        }
    }

    // 1) commencer sur un `user` : sinon le modèle répond à un contexte tronqué.
    if let Some(first_user) = out.iter().position(|m| m.role == "user") {
        out.drain(..first_user);
    } else {
        out.clear();
    }

    // 2) les `notice` ne sont jamais persistées, mais on purge par sécurité.
    out.retain(|m| m.role != "notice");
    out
}

/// Archive et efface l'historique actif.
pub fn clear() -> Result<()> {
    let p = path();
    if p.exists() {
        let msgs = load(0);
        if !msgs.is_empty() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let archive = p.with_file_name(format!("conversation_{now}.jsonl"));
            let _ = std::fs::rename(&p, &archive);
        } else {
            let _ = std::fs::remove_file(&p);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::history::{FunctionCall, ToolCall};

    fn user(t: &str) -> ChatMessage {
        ChatMessage::user(t)
    }

    fn assistant_calls() -> ChatMessage {
        ChatMessage::assistant(
            String::new(),
            vec![ToolCall {
                id: "call_1".into(),
                kind: "function".into(),
                function: FunctionCall {
                    name: "code_exec".into(),
                    arguments: "{}".into(),
                },
            }],
        )
    }

    #[test]
    fn repair_drops_orphan_tool_result() {
        // La coupe a mangé l'assistant qui demandait l'outil.
        let msgs = vec![
            ChatMessage::tool_result("call_1", "sortie".into()),
            user("bonjour"),
        ];
        let out = repair(msgs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, "user");
    }

    #[test]
    fn repair_keeps_a_complete_exchange() {
        let msgs = vec![
            user("fais-le"),
            assistant_calls(),
            ChatMessage::tool_result("call_1", "ok".into()),
            ChatMessage::assistant("c'est fait".into(), vec![]),
        ];
        let out = repair(msgs);
        assert_eq!(out.len(), 4);
        assert_eq!(out[0].role, "user");
    }

    #[test]
    fn repair_drops_assistant_with_unanswered_calls() {
        let msgs = vec![user("fais-le"), assistant_calls()];
        let out = repair(msgs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, "user");
    }

    #[test]
    fn repair_starts_on_user_even_after_truncation() {
        let msgs = vec![
            ChatMessage::assistant("réponse précédente".into(), vec![]),
            user("nouvelle question"),
        ];
        let out = repair(msgs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, "user");
    }

    #[test]
    fn repair_without_user_is_empty() {
        let msgs = vec![ChatMessage::assistant("seul".into(), vec![])];
        assert!(repair(msgs).is_empty());
    }

    #[test]
    fn take_last_bounds_the_window() {
        let msgs: Vec<_> = (0..10).map(|i| user(&format!("m{i}"))).collect();
        assert_eq!(take_last(msgs.clone(), 3).len(), 3);
        assert_eq!(take_last(msgs.clone(), 0).len(), 10);
        assert_eq!(take_last(msgs, 50).len(), 10);
    }

    #[test]
    fn rewrite_roundtrips_through_disk() {
        let dir = std::env::temp_dir().join(format!("zer0-convo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("c.jsonl");
        let msgs = vec![user("un"), ChatMessage::assistant("deux".into(), vec![])];
        // On réutilise la même sérialisation que `rewrite`.
        let mut s = String::new();
        for m in &msgs {
            s.push_str(&serde_json::to_string(m).unwrap());
            s.push('\n');
        }
        std::fs::write(&p, s).unwrap();
        let back = load_from_path(&p, 0);
        assert_eq!(back.len(), 2);
        assert_eq!(back[1].content.as_deref(), Some("deux"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
