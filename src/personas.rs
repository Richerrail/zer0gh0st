use anyhow::Result;
use std::fs;
use std::path::PathBuf;

use crate::config;

/// Personas are system-prompt presets, one markdown file per persona.
pub fn dir() -> PathBuf {
    config::data_dir().join("personas")
}

pub fn list() -> Vec<String> {
    let Ok(rd) = fs::read_dir(dir()) else {
        return Vec::new();
    };
    let mut v: Vec<String> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| e.path().file_stem().map(|s| s.to_string_lossy().to_string()))
        .collect();
    v.sort();
    v
}

pub fn get(name: &str) -> Option<String> {
    fs::read_to_string(dir().join(format!("{name}.md"))).ok()
}

pub fn save(name: &str, content: &str) -> Result<()> {
    fs::create_dir_all(dir())?;
    fs::write(dir().join(format!("{name}.md")), content)?;
    Ok(())
}
