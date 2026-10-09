use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::process::Command;

/// Project root, resolved from the running binary (`target/release/zer0`).
pub fn project_root() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(root) = exe.parent().and_then(|p| p.parent()).and_then(|p| p.parent()) {
            return root.to_path_buf();
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn mod_path(rel: &str) -> PathBuf {
    project_root().join("mods").join(rel)
}

fn require(path: &PathBuf) -> Result<()> {
    if path.exists() {
        Ok(())
    } else {
        Err(anyhow!("introuvable: {}", path.display()))
    }
}

/// `/kristal` — Qt6 Winamp-like player (separate window).
pub fn launch_kristal() -> Result<String> {
    let bin = mod_path("zer0-kristal/zer0-kristal");
    require(&bin)?;
    Command::new(&bin).spawn()?;
    Ok(format!("🎵 kristal lancé — {}", bin.display()))
}

/// `/streaming` — retro torrent video player (separate window).
pub fn launch_streaming() -> Result<String> {
    let script = mod_path("streaming/zero_video.py");
    require(&script)?;
    Command::new("python3").arg(&script).spawn()?;
    Ok(format!("🎬 streaming lancé — {}", script.display()))
}

/// `/retro` — Zer0-Retr0 (5 games) in the browser.
pub fn launch_retro() -> Result<String> {
    let html = mod_path("Zer0-Retr0/index.html");
    require(&html)?;
    Command::new("xdg-open").arg(&html).spawn()?;
    Ok(format!("🕹️ retro ouvert — {}", html.display()))
}
