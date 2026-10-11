//! Petites abstractions multiplateformes (Linux / macOS / Windows).
//!
//! Linux reste la cible principale ; macOS et Windows passent par des replis
//! (shell, presse-papier, ouverture de fichiers, enregistreur micro).

use std::process::Command;

/// `true` si `cmd` est trouvé dans le PATH (gère `.exe`/`.cmd` sur Windows).
pub fn has(cmd: &str) -> bool {
    let candidates: Vec<String> = if cfg!(windows) {
        vec![
            format!("{cmd}.exe"),
            format!("{cmd}.cmd"),
            format!("{cmd}.bat"),
            cmd.to_string(),
        ]
    } else {
        vec![cmd.to_string()]
    };
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| candidates.iter().any(|n| d.join(n).exists())))
        .unwrap_or(false)
}

/// Dossier personnel (`HOME` sur Unix, `USERPROFILE` sur Windows).
pub fn home() -> String {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| {
            if cfg!(windows) {
                "C:\\".to_string()
            } else {
                "/home".to_string()
            }
        })
}

/// Interpréteur pour `code_exec` : `bash -lc` (Unix) ou `cmd /C` (Windows).
pub fn shell_command(script: &str) -> Command {
    if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(script);
        c
    } else {
        let mut c = Command::new("bash");
        c.arg("-lc").arg(script);
        c
    }
}

/// Ouvre un fichier/URL dans l'application par défaut.
pub fn open(path: &str) -> std::io::Result<std::process::Child> {
    if cfg!(target_os = "macos") {
        Command::new("open").arg(path).spawn()
    } else if cfg!(windows) {
        Command::new("cmd").args(["/C", "start", "", path]).spawn()
    } else if has("xdg-open") {
        Command::new("xdg-open").arg(path).spawn()
    } else {
        Command::new("gio").args(["open", path]).spawn()
    }
}

/// Nom de l'interpréteur Python disponible.
pub fn python() -> &'static str {
    if has("python3") {
        "python3"
    } else if has("python") {
        "python"
    } else {
        "python3"
    }
}

/// Horodatage local `YYYY-MM-DD HH:MM:SS` (via `date` ou PowerShell).
pub fn now() -> String {
    let out = if cfg!(windows) {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Get-Date -Format 'yyyy-MM-dd HH:mm:ss'",
            ])
            .output()
    } else {
        Command::new("date").arg("+%Y-%m-%d %H:%M:%S").output()
    };
    out.ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}
