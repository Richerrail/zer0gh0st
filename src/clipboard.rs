//! Copie dans le presse-papier. Ordre : `wl-copy` (Wayland) -> `xclip` / `xsel`
//! (X11) -> `pbcopy` (macOS) -> `clip` (Windows) -> **OSC 52** (repli terminal).
//! Lecture : `wl-paste` -> `xclip -o` / `xsel -o` -> `pbpaste` -> PowerShell `Get-Clipboard`.

use std::io::Write;
use std::process::{Command, Stdio};

fn has(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(cmd).exists()))
        .unwrap_or(false)
}

/// Envoie `text` sur l'entree standard de `cmd` avec `args`. `true` si succes.
fn pipe_to(cmd: &str, args: &[&str], text: &str) -> bool {
    let Ok(mut c) = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    if let Some(si) = c.stdin.as_mut() {
        let _ = si.write_all(text.as_bytes());
    }
    c.stdin.take(); // ferme stdin -> EOF pour le programme
    matches!(c.wait(), Ok(s) if s.success())
}

pub fn copy(text: &str) {
    // Wayland
    if has("wl-copy") && pipe_to("wl-copy", &[], text) {
        return;
    }
    // X11
    if has("xclip") && pipe_to("xclip", &["-selection", "clipboard"], text) {
        return;
    }
    if has("xsel") && pipe_to("xsel", &["--clipboard", "--input"], text) {
        return;
    }
    // macOS
    if has("pbcopy") && pipe_to("pbcopy", &[], text) {
        return;
    }
    // Windows: clip.exe (builtin cmd)
    if has("clip") && pipe_to("clip", &[], text) {
        return;
    }
    // Windows: PowerShell Set-Clipboard (marche meme si clip absent du PATH)
    // Methode 1: Fichier temporaire + [IO.File]::ReadAllText() | Set-Clipboard (robuste, UTF-8 garanti)
    if let Ok(tmp) = std::env::temp_dir().join(format!("zer0_clip_{}.txt", std::process::id())).into_os_string().into_string() {
        // Ecrire avec BOM UTF-8 pour forcer PowerShell a lire en UTF-8
        let mut bom_text = String::new();
        bom_text.push('\u{FEFF}'); // BOM UTF-8
        bom_text.push_str(text);
        if std::fs::write(&tmp, &bom_text).is_ok() {
            if let Ok(mut c) = Command::new("powershell")
                .args(["-NoProfile", "-Command", &format!("[IO.File]::ReadAllText('{}') | Set-Clipboard", tmp)])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                let _ = c.wait();
                let _ = std::fs::remove_file(&tmp);
                return;
            }
            let _ = std::fs::remove_file(&tmp);
        }
    }
    // Methode 2: $input | Set-Clipboard (stdin direct, marche dans Rust)
    if let Ok(mut c) = Command::new("powershell")
        .args(["-NoProfile", "-Command", "$input | Set-Clipboard"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        if let Some(si) = c.stdin.as_mut() {
            use std::io::Write;
            let _ = si.write_all(text.as_bytes());
        }
        c.stdin.take();
        if matches!(c.wait(), Ok(s) if s.success()) {
            return;
        }
    }
    // Repli OSC 52 (base64) -- supporté par de nombreux terminaux (SSH inclus).
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(text);
    let mut out = std::io::stdout();
    let _ = write!(out, "\x1b]52;c;{b64}\x07");
    let _ = out.flush();
}

/// Lit le presse-papier. Ordre : `wl-paste` -> `xclip -o` / `xsel -o`
/// -> `pbpaste` -> PowerShell `Get-Clipboard` (Windows).
/// Retourne `None` si aucun outil n'est dispo ou en cas d'erreur.
pub fn paste() -> Option<String> {
    // Wayland
    if let Ok(out) = Command::new("wl-paste").output() {
        if out.status.success() && !out.stdout.is_empty() {
            return String::from_utf8(out.stdout).ok();
        }
    }
    // X11: xclip
    if let Ok(out) = Command::new("xclip").args(["-selection", "clipboard", "-o"]).output() {
        if out.status.success() && !out.stdout.is_empty() {
            return String::from_utf8(out.stdout).ok();
        }
    }
    // X11: xsel
    if let Ok(out) = Command::new("xsel").args(["--clipboard", "--output"]).output() {
        if out.status.success() && !out.stdout.is_empty() {
            return String::from_utf8(out.stdout).ok();
        }
    }
    // macOS
    if let Ok(out) = Command::new("pbpaste").output() {
        if out.status.success() && !out.stdout.is_empty() {
            return String::from_utf8(out.stdout).ok();
        }
    }
    // Windows: .NET System.Windows.Forms.Clipboard (le plus fiable, UTF-8 natif)
    if let Ok(out) = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.Clipboard]::GetText()",
        ])
        .output()
    {
        if out.status.success() && !out.stdout.is_empty() {
            if let Ok(s) = String::from_utf8(out.stdout) {
                let trimmed = s.trim_end_matches(&['\r', '\n'][..]);
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    // Fallback: PowerShell Get-Clipboard (peut avoir des problemes d'encodage console)
    let ps_commands = vec![
        vec!["-NoProfile", "-Command", "Get-Clipboard -Raw"],
        vec!["-NoProfile", "-Command", "Get-Clipboard"],
        vec!["-sta", "-NoProfile", "-Command", "Get-Clipboard -Raw"],
        vec!["-sta", "-NoProfile", "-Command", "Get-Clipboard"],
    ];
    for cmd in ps_commands {
        if let Ok(out) = Command::new("powershell").args(cmd).output() {
            if out.status.success() && !out.stdout.is_empty() {
                if let Ok(s) = String::from_utf8(out.stdout) {
                    let trimmed = s.trim_end_matches(&['\r', '\n'][..]);
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_clipboard_roundtrip() {
        let test_text = "Test clipboard from zer0 - Japanese test";
        copy(test_text);
        thread::sleep(Duration::from_millis(500));
        let pasted = paste();
        assert!(pasted.is_some(), "paste() returned None");
        assert_eq!(pasted.unwrap(), test_text);
    }
}