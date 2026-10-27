//! Petites abstractions multiplateformes (Linux / macOS / Windows).
//!
//! Linux reste la cible principale ; macOS et Windows passent par des replis
//! (shell, presse-papier, ouverture de fichiers, enregistreur micro).

use std::path::PathBuf;
use std::process::{Command, Stdio};

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
    for name in
        secret_names(std::env::vars_os().filter_map(|(k, _)| k.to_str().map(str::to_string)))
    {
        cmd.env_remove(name);
    }
}

/// Noms de variables « à secret » dans une liste donnée (fonction **pure** :
/// les tests peuvent la tester sans muter l'environnement, ce qui serait une
/// course — les tests Rust tournent en parallèle).
pub fn secret_names<I: IntoIterator<Item = String>>(names: I) -> Vec<String> {
    names.into_iter().filter(|n| is_secret_like(n)).collect()
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

/// Ouvre une **URL** dans le navigateur par défaut.
///
/// **Windows** : on passe par `rundll32 url.dll,FileProtocolHandler` plutôt
/// que par `cmd /C start`. Raison : `cmd` **hérite de la console** du TUI —
/// or c'est le mode de la console qui porte la **capture souris** de crossterm.
/// Un `cmd` enfant reprenait ce mode à sa sortie : après un simple clic sur le
/// 👍, le terminal ne recevait **plus aucun événement souris** (menu compris).
/// Mesuré : `mouse=true` avant → `mouse=false` après `cmd /C start`.
/// `rundll32` n'ouvre aucune console annexe : le terminal garde la main.
///
/// Dans tous les cas, les flux stdio sont détachés (`Stdio::null()`) : un
/// enfant qui écrit dans le stdout du TUI casserait l'affichage.
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
        // `gio open` : repli GNOME (xdg-open absent).
        Command::new("gio")
            .args(["open", url])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    }
}

/// Ouvre un **fichier local** dans l'application par défaut.
///
/// Même principe que [`open_url`], mais le lanceur doit gérer un chemin de
/// fichier : sous Windows, `cmd /C start "" <chemin>` est le lanceur fiable.
///
/// **Windows** : on ajoute `CREATE_NO_WINDOW` — sans ce drapeau, `cmd`
/// **hérite de la console** du TUI et lui reprend le mode « capture souris »
/// (même symptôme que pour les URLs : plus aucun clic n'atteint le TUI).
/// `CREATE_NO_WINDOW` empêche explicitement cet héritage.
pub fn open_file(path: &str) -> std::io::Result<std::process::Child> {
    if cfg!(target_os = "macos") {
        Command::new("open").arg(path).spawn()
    } else if cfg!(windows) {
        // `CommandExt` n'existe que sous Windows : l'import doit être dans le
        // bloc `cfg(windows)`, sinon la compilation Linux/macOS échoue.
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

/// Réactive la capture souris du terminal.
///
/// Utile après avoir lancé un programme externe : celui-ci a pu hériter du
/// terminal (console Windows) ou du TTY (Unix) et lui reprendre la main —
/// auquel cas le TUI ne recevrait plus **aucun** clic.
///
/// - **Windows** : la capture souris est portée par le *mode de la console*
///   (ENABLE_MOUSE_INPUT). Un process enfant qui hérite de la console peut
///   rendre ce mode au terminal : on le réactive donc explicitement.
/// - **Linux / macOS** : la capture souris est portée par les séquences ANSI
///   émises au démarrage (`?1000h`, `?1006h`…). Elles restent actives, mais un
///   enfant qui a hérité du TTY a pu désorganiser le terminal ; on les
///   réémet pour être sûr.
pub fn restore_mouse_capture() {
    #[cfg(windows)]
    {
        use crossterm::event::EnableMouseCapture;
        use crossterm::terminal::enable_raw_mode;
        // Mode brut d'abord, puis capture souris : un programme tiers ayant
        // hérité de la console a pu modifier l'un ou l'autre.
        let _ = enable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), EnableMouseCapture);
    }
    #[cfg(not(windows))]
    {
        use crossterm::event::EnableMouseCapture;
        // Sur Unix, EnableMouseCapture écrit les séquences ANSI de suivi
        // souris. On les renvoie : inoffensif si elles sont déjà actives.
        let _ = crossterm::execute!(std::io::stdout(), EnableMouseCapture);
    }
}

/// Interpréteur Python. Priorité : `$ZER0_PYTHON`, puis le venv STT
/// `~/.venvs/zer0-whisper`, puis `python3`, puis `python`.
///
/// `$ZER0_PYTHON` permet de pointer un autre **venv** (utile quand
/// `faster-whisper` n'est pas installé dans le Python système, ex. Ubuntu/WSL) :
/// `ZER0_PYTHON=~/.venvs/whisper/bin/python`.
pub fn python() -> String {
    if let Ok(p) = std::env::var("ZER0_PYTHON") {
        if !p.trim().is_empty() {
            return p;
        }
    }
    // Convention Zero-chan : un venv STT dédié dans le dossier utilisateur.
    // La détection automatique évite de dépendre d'un terminal déjà ouvert
    // avant la définition de `$ZER0_PYTHON`.
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

/// Carte graphique détectée : faits bruts du bus PCI, sans supposer l'usage.
#[derive(Debug, Clone, PartialEq)]
pub struct Gpu {
    pub vendor: String,
    pub pci_id: String,
    pub driver: Option<String>,
}

/// Cartes DRM présentes (Linux). Ailleurs : liste vide (pas d'API portable).
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
            // `card0`, `card1`… mais pas les connecteurs `card0-DP-1`.
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

/// Extrait vendeur / id PCI / pilote d'un `uevent` de carte DRM.
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
        // Test sur la fonction **pure** : pas de mutation d'environnement
        // (les tests tournent en parallèle → la CI devenait instable).
        let names = secret_names([
            "TEST_SECRET_TOKEN".to_string(),
            "TEST_PLAIN_VAR".to_string(),
        ]);
        assert_eq!(names, vec!["TEST_SECRET_TOKEN".to_string()]);
        assert!(!names.contains(&"TEST_PLAIN_VAR".to_string()));
    }
}
