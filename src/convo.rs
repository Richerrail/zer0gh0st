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

/// Efface l'historique.
pub fn clear() -> Result<()> {
    let _ = std::fs::remove_file(path());
    Ok(())
}
