use std::io::Write;
use std::process::{Command, Stdio};

fn has(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(cmd).exists()))
        .unwrap_or(false)
}

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
    c.stdin.take();
    matches!(c.wait(), Ok(s) if s.success())
}

pub fn copy(text: &str) -> bool {
    if has("wl-copy") && pipe_to("wl-copy", &[], text) {
        return true;
    }
    if has("xclip") && pipe_to("xclip", &["-selection", "clipboard"], text) {
        return true;
    }
    if has("xsel") && pipe_to("xsel", &["--clipboard", "--input"], text) {
        return true;
    }
    if has("pbcopy") && pipe_to("pbcopy", &[], text) {
        return true;
    }
    if has("clip") && pipe_to("clip", &[], text) {
        return true;
    }
    if let Ok(tmp) = std::env::temp_dir()
        .join(format!("zer0_clip_{}.txt", std::process::id()))
        .into_os_string()
        .into_string()
    {
        let mut bom_text = String::new();
        bom_text.push('\u{FEFF}');
        bom_text.push_str(text);
        if std::fs::write(&tmp, &bom_text).is_ok() {
            if let Ok(mut c) = Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    &format!("[IO.File]::ReadAllText('{}') | Set-Clipboard", tmp),
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                let _ = c.wait();
                let _ = std::fs::remove_file(&tmp);
                return true;
            }
            let _ = std::fs::remove_file(&tmp);
        }
    }
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
            return true;
        }
    }
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(text);
    let mut out = std::io::stdout();
    let _ = write!(out, "\x1b]52;c;{b64}\x07");
    let _ = out.flush();
    false
}

fn strip_one_trailing_newline(s: &str) -> &str {
    if let Some(rest) = s.strip_suffix('\n') {
        rest.strip_suffix('\r').unwrap_or(rest)
    } else {
        s
    }
}

pub fn paste() -> Option<String> {
    if let Ok(out) = Command::new("wl-paste").output() {
        if out.status.success() && !out.stdout.is_empty() {
            if let Ok(s) = String::from_utf8(out.stdout) {
                let s = strip_one_trailing_newline(&s);
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            }
        }
    }
    if let Ok(out) = Command::new("xclip")
        .args(["-selection", "clipboard", "-o"])
        .output()
    {
        if out.status.success() && !out.stdout.is_empty() {
            return String::from_utf8(out.stdout).ok();
        }
    }
    if let Ok(out) = Command::new("xsel")
        .args(["--clipboard", "--output"])
        .output()
    {
        if out.status.success() && !out.stdout.is_empty() {
            return String::from_utf8(out.stdout).ok();
        }
    }
    if let Ok(out) = Command::new("pbpaste").output() {
        if out.status.success() && !out.stdout.is_empty() {
            return String::from_utf8(out.stdout).ok();
        }
    }
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
        if !clipboard_tool_available() {
            eprintln!("test ignoré : aucun presse-papier disponible sur cette machine");
            return;
        }
        let test_text = "Test clipboard from zer0 - Japanese test";
        copy(test_text);
        thread::sleep(Duration::from_millis(500));
        let pasted = paste();
        assert!(pasted.is_some(), "paste() returned None");
        assert_eq!(pasted.unwrap(), test_text);
    }

    #[test]
    fn strip_one_trailing_newline_keeps_user_newline() {
        assert_eq!(strip_one_trailing_newline("abc\n"), "abc");
        assert_eq!(strip_one_trailing_newline("abc\n\n"), "abc\n");
        assert_eq!(strip_one_trailing_newline("abc\r\n"), "abc");
        assert_eq!(strip_one_trailing_newline("abc"), "abc");
    }

    fn clipboard_tool_available() -> bool {
        has("wl-copy")
            || has("xclip")
            || has("xsel")
            || has("pbcopy")
            || has("clip")
            || has("powershell")
    }
}
