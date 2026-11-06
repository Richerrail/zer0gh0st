use std::path::PathBuf;
use std::process::{Command, Stdio};

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

pub fn shell_command(script: &str) -> Command {
    #[cfg(windows)]
    let mut c = windows_shell(script);
    #[cfg(not(windows))]
    let mut c = {
        let mut c = Command::new("bash");
        c.arg("-lc").arg(script);
        c
    };
    scrub_secrets(&mut c);
    c
}

pub fn shell_label() -> String {
    #[cfg(windows)]
    {
        let ov = std::env::var("ZER0_SHELL").ok();
        let has_bash = find_in_path("bash").is_some();
        let has_pwsh = find_in_path("pwsh").is_some();
        let has_ps = find_in_path("powershell").is_some();
        let chosen = choose_windows_shell(
            ov.as_deref(),
            std::env::var_os("MSYSTEM").is_some(),
            std::env::var_os("PSModulePath").is_some(),
            has_bash,
            has_pwsh,
            has_ps,
        );
        match chosen {
            WinShell::Bash => "bash -lc (Git Bash)".to_string(),
            WinShell::PowerShell => "powershell -Command".to_string(),
            WinShell::Cmd => "cmd /C".to_string(),
        }
    }
    #[cfg(not(windows))]
    {
        "bash -lc".to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinShell {
    Bash,
    PowerShell,
    Cmd,
}

#[allow(clippy::too_many_arguments)]
pub fn choose_windows_shell(
    override_: Option<&str>,
    msys: bool,
    ps_module_path: bool,
    has_bash: bool,
    has_pwsh: bool,
    has_powershell: bool,
) -> WinShell {
    if override_.is_some() {
        return pick_windows_shell(override_, has_bash, has_pwsh, has_powershell);
    }
    if msys && has_bash {
        return WinShell::Bash;
    }
    if ps_module_path && (has_pwsh || has_powershell) {
        return WinShell::PowerShell;
    }
    pick_windows_shell(None, has_bash, has_pwsh, has_powershell)
}

pub fn pick_windows_shell(
    override_: Option<&str>,
    has_bash: bool,
    has_pwsh: bool,
    has_powershell: bool,
) -> WinShell {
    match override_.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("bash") | Some("gitbash") => return WinShell::Bash,
        Some("pwsh") | Some("powershell") | Some("ps") => return WinShell::PowerShell,
        Some("cmd") | Some("cmd.exe") => return WinShell::Cmd,
        _ => {}
    }
    if has_bash {
        WinShell::Bash
    } else if has_pwsh || has_powershell {
        WinShell::PowerShell
    } else {
        WinShell::Cmd
    }
}

#[cfg(windows)]
fn windows_shell(script: &str) -> Command {
    let ov = std::env::var("ZER0_SHELL").ok();
    let has_bash = find_in_path("bash").is_some();
    let has_pwsh = find_in_path("pwsh").is_some();
    let has_ps = find_in_path("powershell").is_some();
    let chosen = choose_windows_shell(
        ov.as_deref(),
        std::env::var_os("MSYSTEM").is_some(),
        std::env::var_os("PSModulePath").is_some(),
        has_bash,
        has_pwsh,
        has_ps,
    );
    match chosen {
        WinShell::Bash => {
            let mut c = Command::new("bash");
            c.arg("-lc").arg(script);
            c
        }
        WinShell::PowerShell => {
            let mut c = Command::new(if has_pwsh { "pwsh" } else { "powershell" });
            c.arg("-NoProfile").arg("-Command").arg(script);
            c
        }
        WinShell::Cmd => {
            let mut c = Command::new("cmd");
            c.arg("/C").arg(script);
            c
        }
    }
}

pub fn find_in_path(exe: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let direct = dir.join(exe);
        if direct.is_file() {
            return Some(direct);
        }
        #[cfg(windows)]
        for ext in ["exe", "cmd", "bat"] {
            let cand = dir.join(format!("{exe}.{ext}"));
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

pub fn scrub_secrets(cmd: &mut Command) {
    for name in
        secret_names(std::env::vars_os().filter_map(|(k, _)| k.to_str().map(str::to_string)))
    {
        cmd.env_remove(name);
    }
}

pub fn secret_names<I: IntoIterator<Item = String>>(names: I) -> Vec<String> {
    names.into_iter().filter(|n| is_secret_like(n)).collect()
}

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

pub fn open_url(url: &str) -> std::io::Result<std::process::Child> {
    if cfg!(target_os = "macos") {
        Command::new("open")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    } else if cfg!(windows) {
        Command::new("rundll32")
            .arg("url.dll,FileProtocolHandler")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    } else if has("xdg-open") {
        Command::new("xdg-open")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    } else {
        Command::new("gio")
            .args(["open", url])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    }
}

pub fn open_file(path: &str) -> std::io::Result<std::process::Child> {
    if cfg!(target_os = "macos") {
        Command::new("open").arg(path).spawn()
    } else if cfg!(windows) {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let mut c = Command::new("cmd");
            c.args(["/C", "start", "", path]);
            c.stdin(Stdio::null());
            c.creation_flags(CREATE_NO_WINDOW);
            c.spawn()
        }
        #[cfg(not(windows))]
        {
            unreachable!("bloc Windows")
        }
    } else if has("xdg-open") {
        Command::new("xdg-open")
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    } else {
        Command::new("gio")
            .args(["open", path])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    }
}

pub fn restore_mouse_capture() {
    #[cfg(windows)]
    {
        use crossterm::event::EnableMouseCapture;
        use crossterm::terminal::enable_raw_mode;
        let _ = enable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), EnableMouseCapture);
    }
    #[cfg(not(windows))]
    {
        use crossterm::event::EnableMouseCapture;
        let _ = crossterm::execute!(std::io::stdout(), EnableMouseCapture);
    }
}

pub fn python() -> String {
    if let Ok(p) = std::env::var("ZER0_PYTHON") {
        if !p.trim().is_empty() {
            return p;
        }
    }
    let venv = if cfg!(windows) {
        PathBuf::from(home())
            .join(".venvs")
            .join("zer0-whisper")
            .join("Scripts")
            .join("python.exe")
    } else {
        PathBuf::from(home())
            .join(".venvs")
            .join("zer0-whisper")
            .join("bin")
            .join("python")
    };
    if venv.is_file() {
        return venv.to_string_lossy().into_owned();
    }
    if has("python3") {
        "python3".to_string()
    } else if has("python") {
        "python".to_string()
    } else {
        "python3".to_string()
    }
}

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

#[derive(Debug, Clone, PartialEq)]
pub struct Gpu {
    pub vendor: String,
    pub pci_id: String,
    pub driver: Option<String>,
}

pub fn gpus() -> Vec<Gpu> {
    #[cfg(target_os = "linux")]
    {
        let mut out: Vec<Gpu> = Vec::new();
        let Ok(dir) = std::fs::read_dir("/sys/class/drm") else {
            return out;
        };
        for entry in dir.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with("card") || name.contains('-') {
                continue;
            }
            let Ok(txt) = std::fs::read_to_string(entry.path().join("device/uevent")) else {
                continue;
            };
            if let Some(g) = parse_uevent(&txt) {
                if !out.iter().any(|x| x.pci_id == g.pci_id) {
                    out.push(g);
                }
            }
        }
        out.sort_by(|a, b| a.pci_id.cmp(&b.pci_id));
        out
    }
    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}

pub fn parse_uevent(text: &str) -> Option<Gpu> {
    let mut driver = None;
    let mut pci = None;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("DRIVER=") {
            driver = Some(v.trim().to_string());
        }
        if let Some(v) = line.strip_prefix("PCI_ID=") {
            pci = Some(v.trim().to_ascii_uppercase());
        }
    }
    let pci_id = pci?;
    let vendor = match pci_id.split(':').next().unwrap_or("") {
        "1002" => "AMD/ATI",
        "10DE" => "NVIDIA",
        "8086" => "Intel",
        "1AF4" => "virtio",
        _ => "inconnu",
    }
    .to_string();
    Some(Gpu {
        vendor,
        pci_id,
        driver,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_uevent_extracts_vendor_and_driver() {
        let g = parse_uevent("DRIVER=radeon\nPCI_ID=1002:6779\n").unwrap();
        assert_eq!(g.vendor, "AMD/ATI");
        assert_eq!(g.pci_id, "1002:6779");
        assert_eq!(g.driver.as_deref(), Some("radeon"));
        assert!(parse_uevent("DRIVER=amdgpu\n").is_none());
    }

    #[test]
    fn inherited_shell_wins_over_the_heuristic() {
        assert_eq!(
            choose_windows_shell(None, false, true, true, true, true),
            WinShell::PowerShell
        );
        assert_eq!(
            choose_windows_shell(None, true, false, true, true, true),
            WinShell::Bash
        );
        assert_eq!(
            choose_windows_shell(None, false, false, true, true, true),
            WinShell::Bash
        );
        assert_eq!(
            choose_windows_shell(Some("cmd"), true, true, true, true, true),
            WinShell::Cmd
        );
        assert_eq!(
            choose_windows_shell(None, false, true, false, false, false),
            WinShell::Cmd
        );
    }

    #[test]
    fn windows_shell_prefers_bash_then_powershell() {
        assert_eq!(pick_windows_shell(None, true, true, true), WinShell::Bash);
        assert_eq!(
            pick_windows_shell(None, false, true, true),
            WinShell::PowerShell
        );
        assert_eq!(
            pick_windows_shell(None, false, false, true),
            WinShell::PowerShell
        );
        assert_eq!(pick_windows_shell(None, false, false, false), WinShell::Cmd);
        assert_eq!(
            pick_windows_shell(Some("cmd"), true, true, true),
            WinShell::Cmd
        );
        assert_eq!(
            pick_windows_shell(Some("POWERSHELL"), true, false, true),
            WinShell::PowerShell
        );
    }

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
        let names = secret_names([
            "TEST_SECRET_TOKEN".to_string(),
            "TEST_PLAIN_VAR".to_string(),
        ]);
        assert_eq!(names, vec!["TEST_SECRET_TOKEN".to_string()]);
        assert!(!names.contains(&"TEST_PLAIN_VAR".to_string()));
    }
}
