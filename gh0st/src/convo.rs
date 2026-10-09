//! Historique de conversation persisté (JSONL).
//!
//! Une ligne par message (`role`, `content`, `tool_calls`…). Rechargé au
//! démarrage pour retrouver la conversation, effacé par `/new`.

use anyhow::Result;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use crate::agent::history::ChatMessage;
use crate::config;

pub fn path() -> PathBuf {
    config::data_dir().join("conversation.jsonl")
}

/// Charge les `max` derniers messages (0 = tout).
pub fn load(max: usize) -> Vec<ChatMessage> {
    let Ok(s) = std::fs::read_to_string(path()) else {
        return Vec::new();
    };
    let all: Vec<ChatMessage> = s
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    if max == 0 || all.len() <= max {
        all
    } else {
        all[all.len() - max..].to_vec()
    }
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

/// Charge les messages depuis un chemin arbitraire.
pub fn load_from_path(p: &std::path::Path) -> Vec<ChatMessage> {
    let Ok(s) = std::fs::read_to_string(p) else {
        return Vec::new();
    };
    s.lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
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
