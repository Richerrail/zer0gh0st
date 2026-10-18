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
///
/// L'environnement transmis au shell est **nettoyé des secrets** (`scrub_secrets`) :
/// un modèle qui demande une commande arbitraire ne doit jamais pouvoir faire
/// `echo $OPENROUTER_API_KEY` ni `env` pour récupérer une clé.
pub fn shell_command(script: &str) -> Command {
    let mut c = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(script);
        c
    } else {
        let mut c = Command::new("bash");
        c.arg("-lc").arg(script);
        c
    };
    scrub_secrets(&mut c);
    c
}

/// Retire de l'environnement d'un processus enfant toute variable qui **ressemble
/// à un secret** (`*API_KEY*`, `*TOKEN*`, `*SECRET*`, `*PASSPHRASE*`, `*PASSWORD*`,
/// `*CREDENTIAL*`, `*PRIVATE_KEY*`, `ZER0_VAULT*`).
///
/// Précaution supplémentaire (pas une garantie) : un `bash -lc` relit les profils
/// shell, qui peuvent ré-exporter des variables — ne mettez pas de clés dans
/// `~/.bashrc`. Le découpage fort reste `zer0-gw` (voir SAFETY.md §7).
pub fn scrub_secrets(cmd: &mut Command) {
    for (name, _) in std::env::vars_os() {
        if let Some(name) = name.to_str() {
            if is_secret_like(name) {
                cmd.env_remove(name);
            }
        }
    }
}

/// `true` si le nom de variable ressemble à un secret (comparaison insensible
/// à la casse, volontairement large — faux positifs inoffensifs).
pub fn is_secret_like(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("api_key")
        || n.contains("api-key")
        || n.contains("apikey")
        || n.contains("token")
        || n.contains("secret")
        || n.contains("passphrase")
        || n.contains("password")
        || n.contains("passwd")
        || n.contains("credential")
        || n.contains("private_key")
        || n.starts_with("zer0_vault")
}

/// Lit une image et renvoie `(data URL base64, taille en octets)`.
/// MIME déduit de l'extension (png par défaut). Utilisé par `/img` (TUI) et `--img` (CLI).
pub fn image_data_url(path: &str) -> Result<(String, usize), String> {
    use base64::Engine as _;
    let bytes = std::fs::read(path).map_err(|e| format!("lecture de {path} : {e}"))?;
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "image/png",
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok((format!("data:{mime};base64,{b64}"), bytes.len()))
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

/// Interpréteur Python. Priorité : `$ZER0_PYTHON`, puis `python3`, puis `python`.
///
/// `$ZER0_PYTHON` permet de pointer un **venv** (utile quand `faster-whisper`
/// n'est pas installé dans le Python système, ex. Ubuntu/WSL) :
/// `ZER0_PYTHON=~/.venvs/whisper/bin/python`.
pub fn python() -> String {
    if let Ok(p) = std::env::var("ZER0_PYTHON") {
        if !p.trim().is_empty() {
            return p;
        }
    }
    if has("python3") {
        "python3".to_string()
    } else if has("python") {
        "python".to_string()
    } else {
        "python3".to_string()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_like_matches_keys() {
        assert!(is_secret_like("OPENROUTER_API_KEY"));
        assert!(is_secret_like("nvidia_api_key"));
        assert!(is_secret_like("NVIDIA_APIKEY"));
        assert!(is_secret_like("GITHUB_TOKEN"));
        assert!(is_secret_like("ZER0_VAULT_PASSPHRASE"));
        assert!(is_secret_like("AWS_SECRET_ACCESS_KEY"));
        assert!(is_secret_like("SSH_PRIVATE_KEY"));
    }

    #[test]
    fn secret_like_ignores_harmless_vars() {
        assert!(!is_secret_like("PATH"));
        assert!(!is_secret_like("HOME"));
        assert!(!is_secret_like("ZER0_MODEL"));
        assert!(!is_secret_like("ZER0_MUSIC_DIR"));
    }

    #[test]
    fn scrub_removes_secrets_keeps_rest() {
        unsafe { std::env::set_var("TEST_SECRET_TOKEN", "x") };
        unsafe { std::env::set_var("TEST_PLAIN_VAR", "y") };
        let mut cmd = Command::new("true");
        scrub_secrets(&mut cmd);
        // env_remove est enregistré : on vérifie via get_envs (API stable depuis 1.57)
        let removed = cmd
            .get_envs()
            .any(|(k, _)| k == std::ffi::OsStr::new("TEST_SECRET_TOKEN"));
        let kept = cmd
            .get_envs()
            .any(|(k, _)| k == std::ffi::OsStr::new("TEST_PLAIN_VAR"));
        assert!(removed, "TEST_SECRET_TOKEN devait être retiré");
        assert!(!kept, "TEST_PLAIN_VAR ne devait pas être touché");
        unsafe { std::env::remove_var("TEST_SECRET_TOKEN") };
        unsafe { std::env::remove_var("TEST_PLAIN_VAR") };
    }
}
