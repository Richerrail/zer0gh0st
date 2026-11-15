//! Tests du mod `zer0do` (appareils du réseau) — sans réseau, sans appareil.
//!
//! On vérifie le **contrat de sûreté** : aide lisible, commandes inconnues
//! refusées, et surtout **aucune action sans allowlist**. Le scan lui-même
//! dépend du réseau local : il n'est pas testé ici (et ne doit pas l'être —
//! un test qui sonde le LAN d'un runner CI serait absurde).

use std::path::PathBuf;
use std::process::Command;

fn script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mods/zer0do/zer0do")
}

/// `bash` est requis par le script ; sous Windows sans Git Bash, on saute.
fn bash_available() -> bool {
    Command::new("bash")
        .arg("-c")
        .arg("true")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn run(args: &[&str], cwd: &std::path::Path) -> (bool, String) {
    let out = Command::new("bash")
        .arg(script())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("exécution du script");
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

fn tempdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("z0do-{}-{tag}", std::process::id()));
    let _ = std::fs::create_dir_all(&d);
    d
}

#[test]
fn help_lists_the_commands_and_the_limits() {
    if !bash_available() {
        eprintln!("bash absent : test ignoré");
        return;
    }
    let d = tempdir("help");
    let (ok, text) = run(&["--help"], &d);
    assert!(ok, "{text}");
    for cmd in ["scan", "status", "key", "power"] {
        assert!(text.contains(cmd), "« {cmd} » absent de l'aide : {text}");
    }
    // L'aide doit dire ce que le mod NE fait pas.
    assert!(
        text.contains("Roku") && text.to_lowercase().contains("limite"),
        "l'aide doit annoncer ses limites : {text}"
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_unknown_command_fails_with_a_hint() {
    if !bash_available() {
        return;
    }
    let d = tempdir("unknown");
    let (ok, text) = run(&["explose"], &d);
    assert!(!ok, "une commande inconnue doit échouer");
    assert!(text.contains("inconnue"), "{text}");
    assert!(
        text.contains("--help"),
        "le message doit suggérer l'aide : {text}"
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn actions_are_refused_without_an_allowlist() {
    if !bash_available() {
        return;
    }
    let d = tempdir("noallow");
    // Aucun `.zer0/zer0do.allow` : l'action doit être refusée AVANT tout réseau.
    let (ok, text) = run(&["key", "192.0.2.10", "Home"], &d);
    assert!(!ok, "l'action aurait dû être refusée : {text}");
    assert!(text.contains("action refusée"), "{text}");
    assert!(
        text.contains("zer0do.allow"),
        "le chemin de l'allowlist doit être donné : {text}"
    );
    // Et avec `power off` aussi (aucun appareil ne doit être contacté).
    let (ok2, text2) = run(&["power", "192.0.2.10", "off"], &d);
    assert!(!ok2, "{text2}");
    assert!(text2.contains("action refusée"), "{text2}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_allowed_address_passes_the_allowlist_check() {
    if !bash_available() {
        return;
    }
    let d = tempdir("allow");
    std::fs::create_dir_all(d.join(".zer0")).unwrap();
    // 192.0.2.0/24 est le bloc de test RFC 5737 : rien ne répondra, mais la
    // vérification d'allowlist doit passer et l'échec venir du réseau (HTTP
    // vide), pas d'un refus de permission.
    std::fs::write(d.join(".zer0/zer0do.allow"), "# test\n192.0.2.10\n").unwrap();
    let (ok, text) = run(&["key", "192.0.2.10", "Home"], &d);
    assert!(!ok, "aucun appareil ne doit répondre : {text}");
    assert!(
        !text.contains("action refusée"),
        "l'adresse est autorisée, elle ne doit pas être refusée : {text}"
    );
    let (ok_list, listed) = run(&["list-allow"], &d);
    assert!(ok_list, "{listed}");
    assert!(listed.contains("192.0.2.10"), "{listed}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn invalid_key_names_are_refused() {
    if !bash_available() {
        return;
    }
    let d = tempdir("badkey");
    std::fs::create_dir_all(d.join(".zer0")).unwrap();
    std::fs::write(d.join(".zer0/zer0do.allow"), "192.0.2.10\n").unwrap();
    // Une touche contenant autre chose que des lettres ne doit jamais partir.
    let (ok, text) = run(&["key", "192.0.2.10", "Home;rm -rf /"], &d);
    assert!(!ok, "{text}");
    assert!(text.contains("touche invalide"), "{text}");
    let _ = std::fs::remove_dir_all(&d);
}
