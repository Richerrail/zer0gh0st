//! Copie dans le presse-papier. Ordre : `wl-copy` (Wayland) → `xclip` / `xsel`
//! (X11) → `pbcopy` (macOS) → `clip` (Windows) → **OSC 52** (repli terminal).

use std::io::Write;
use std::process::{Command, Stdio};

fn has(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(cmd).exists()))
        .unwrap_or(false)
}

/// Envoie `text` sur l'entrée standard de `cmd` avec `args`. `true` si succès.
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
    c.stdin.take(); // ferme stdin → EOF pour le programme
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
    // macOS / Windows
    if has("pbcopy") && pipe_to("pbcopy", &[], text) {
        return;
    }
    if has("clip") && pipe_to("clip", &[], text) {
        return;
    }
    // Repli OSC 52 (base64) — supporté par de nombreux terminaux (SSH inclus).
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(text);
    let mut out = std::io::stdout();
    let _ = write!(out, "\x1b]52;c;{b64}\x07");
    let _ = out.flush();
}
