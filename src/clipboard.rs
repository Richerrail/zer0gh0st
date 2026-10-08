//! Copie dans le presse-papier : `wl-copy` (Wayland) sinon OSC 52.

use std::io::Write;
use std::process::{Command, Stdio};

fn has(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(cmd).exists()))
        .unwrap_or(false)
}

pub fn copy(text: &str) {
    if has("wl-copy") {
        if let Ok(mut c) = Command::new("wl-copy").stdin(Stdio::piped()).spawn() {
            if let Some(si) = c.stdin.as_mut() {
                let _ = si.write_all(text.as_bytes());
            }
            let _ = c.wait();
            return;
        }
    }
    // Repli OSC 52 (base64) — supporté par de nombreux terminaux.
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(text);
    let mut out = std::io::stdout();
    let _ = write!(out, "\x1b]52;c;{b64}\x07");
    let _ = out.flush();
}
