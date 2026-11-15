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
    run_env(args, cwd, &[])
}

fn run_env(args: &[&str], cwd: &std::path::Path, envs: &[(&str, &str)]) -> (bool, String) {
    let mut cmd = Command::new("bash");
    cmd.arg(script()).args(args).current_dir(cwd);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("exécution du script");
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

/// Faux Roku : sert une réponse `<device-info>` sur un port libre de la boucle
/// locale. Permet de tester l'analyse XML et le choix de la MAC **sans réseau**
/// et sans toucher à un vrai téléviseur.
fn fake_roku(network_type: &str) -> u16 {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let device_info = format!(
        "<device-info>\
         <friendly-device-name>Salon</friendly-device-name>\
         <model-name>RTRU5528-CA</model-name>\
         <serial-number>X01200MPCLEW</serial-number>\
         <software-version>16.0.4</software-version>\
         <power-mode>PowerOn</power-mode>\
         <network-type>{network_type}</network-type>\
         <wifi-mac>18:ef:3a:21:a7:74</wifi-mac>\
         <ethernet-mac>ac:ac:e2:2a:fe:4c</ethernet-mac>\
         </device-info>"
    );
    let active_app =
        "<active-app><app id=\"837\" type=\"appl\" version=\"1.0\">YouTube</app></active-app>";
    // Plusieurs requêtes servies (`status` interroge device-info **et**
    // active-app) ; routage sur le chemin demandé.
    std::thread::spawn(move || {
        for _ in 0..6 {
            let Ok((mut sock, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 1024];
            let n = sock.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let body: &str = if req.contains("active-app") {
                active_app
            } else {
                &device_info
            };
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = sock.write_all(resp.as_bytes());
        }
    });
    port
}

/// Le script doit au minimum être **syntaxiquement valide** : une apostrophe
/// dans une chaîne simple l'a déjà cassé deux fois.
#[test]
fn the_script_parses() {
    if !bash_available() {
        return;
    }
    let out = Command::new("bash")
        .arg("-n")
        .arg(script())
        .output()
        .expect("bash -n");
    assert!(
        out.status.success(),
        "syntaxe invalide : {}",
        String::from_utf8_lossy(&out.stderr)
    );
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
fn wake_needs_a_mac_for_the_address() {
    if !bash_available() {
        return;
    }
    let d = tempdir("wol-nomac");
    std::fs::create_dir_all(d.join(".zer0")).unwrap();
    std::fs::write(d.join(".zer0/zer0do.allow"), "192.0.2.10\n").unwrap();
    let (ok, text) = run(&["wake", "192.0.2.10"], &d);
    assert!(!ok, "sans MAC, wake doit échouer : {text}");
    assert!(text.contains("aucune MAC"), "{text}");
    assert!(
        text.contains("zer0do.allow"),
        "il faut indiquer où l'ajouter : {text}"
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn wake_dry_run_builds_the_magic_packet_without_network() {
    if !bash_available() {
        return;
    }
    let d = tempdir("wol-dry");
    std::fs::create_dir_all(d.join(".zer0")).unwrap();
    std::fs::write(
        d.join(".zer0/zer0do.allow"),
        "# TV du salon\n192.0.2.10  A4:2B:8C:11:22:33\n",
    )
    .unwrap();
    let (ok, text) = run(&["wake", "192.0.2.10", "--dry-run"], &d);
    assert!(ok, "{text}");
    assert!(text.contains("A4:2B:8C:11:22:33"), "{text}");
    // 6 octets de 0xFF + 16 × 6 octets de MAC = 102.
    assert!(text.contains("102 octets"), "taille du paquet : {text}");
    assert!(text.contains("255.255.255.255"), "{text}");
    // Et via `power on`, qui doit router vers Wake-on-LAN.
    let (ok2, text2) = run(&["power", "192.0.2.10", "on", "--dry-run"], &d);
    assert!(ok2, "{text2}");
    assert!(text2.contains("Wake-on-LAN"), "{text2}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_invalid_mac_is_refused() {
    if !bash_available() {
        return;
    }
    let d = tempdir("wol-badmac");
    std::fs::create_dir_all(d.join(".zer0")).unwrap();
    std::fs::write(d.join(".zer0/zer0do.allow"), "192.0.2.10  pas-une-mac\n").unwrap();
    let (ok, text) = run(&["wake", "192.0.2.10", "--dry-run"], &d);
    assert!(!ok, "{text}");
    assert!(text.contains("MAC invalide"), "{text}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn launch_and_search_are_gated_by_the_allowlist() {
    if !bash_available() {
        return;
    }
    let d = tempdir("actions-gated");
    let (ok, text) = run(&["launch", "192.0.2.10", "Netflix"], &d);
    assert!(!ok, "{text}");
    assert!(text.contains("action refusée"), "{text}");
    let (ok2, text2) = run(&["search", "192.0.2.10", "météo"], &d);
    assert!(!ok2, "{text2}");
    assert!(text2.contains("action refusée"), "{text2}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn digits_in_key_names_are_accepted() {
    if !bash_available() {
        return;
    }
    let d = tempdir("key-digits");
    std::fs::create_dir_all(d.join(".zer0")).unwrap();
    std::fs::write(d.join(".zer0/zer0do.allow"), "192.0.2.10\n").unwrap();
    // `InputHDMI2` est une touche légitime : elle ne doit pas être refusée par
    // la validation (l'échec doit venir du réseau, pas du format).
    let (ok, text) = run(&["key", "192.0.2.10", "InputHDMI2"], &d);
    assert!(!ok, "aucun appareil ne doit répondre : {text}");
    assert!(!text.contains("touche invalide"), "{text}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn literal_key_typing_is_refused_with_an_alternative() {
    if !bash_available() {
        return;
    }
    let d = tempdir("key-lit");
    std::fs::create_dir_all(d.join(".zer0")).unwrap();
    std::fs::write(d.join(".zer0/zer0do.allow"), "192.0.2.10\n").unwrap();
    let (ok, text) = run(&["key", "192.0.2.10", "Lit_a"], &d);
    assert!(!ok, "{text}");
    assert!(text.contains("n'est pas gérée"), "{text}");
    assert!(
        text.contains("search"),
        "il faut proposer l'alternative : {text}"
    );
    // Régression : un message avec des backticks faisait exécuter du shell.
    assert!(
        !text.contains("command substitution") && !text.contains("syntax error"),
        "le message d'erreur s'exécute au lieu de s'afficher : {text}"
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn status_picks_the_mac_of_the_active_interface() {
    if !bash_available() {
        return;
    }
    let port = fake_roku("ethernet");
    let d = tempdir("mac-eth");
    let (ok, text) = run_env(
        &["status", "127.0.0.1"],
        &d,
        &[("ZER0DO_ECP_PORT", &port.to_string())],
    );
    assert!(ok, "{text}");
    assert!(text.contains("RTRU5528-CA"), "modèle absent : {text}");
    assert!(
        text.contains("ac:ac:e2:2a:fe:4c"),
        "en ethernet, c'est l'ethernet-mac qui sert au réveil : {text}"
    );
    assert!(
        !text.contains("18:ef:3a:21:a7:74"),
        "la MAC wifi ne doit pas être celle retenue en ethernet : {text}"
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn status_uses_the_wifi_mac_on_a_wifi_device() {
    if !bash_available() {
        return;
    }
    let port = fake_roku("wifi");
    let d = tempdir("mac-wifi");
    let (ok, text) = run_env(
        &["status", "127.0.0.1"],
        &d,
        &[("ZER0DO_ECP_PORT", &port.to_string())],
    );
    assert!(ok, "{text}");
    assert!(text.contains("18:ef:3a:21:a7:74"), "{text}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn power_on_does_not_toggle_a_device_that_is_already_on() {
    if !bash_available() {
        return;
    }
    // Le faux Roku répond `PowerOn` : `power on` ne doit envoyer **aucune**
    // touche (la touche Power est une bascule — l'envoyer éteindrait la TV).
    let port = fake_roku("ethernet");
    let d = tempdir("power-on-idempotent");
    std::fs::create_dir_all(d.join(".zer0")).unwrap();
    std::fs::write(d.join(".zer0/zer0do.allow"), "127.0.0.1\n").unwrap();
    let (ok, text) = run_env(
        &["power", "127.0.0.1", "on"],
        &d,
        &[("ZER0DO_ECP_PORT", &port.to_string())],
    );
    assert!(ok, "{text}");
    assert!(text.contains("déjà allumée"), "{text}");
    assert!(
        !text.contains("Touche"),
        "aucune touche ne devait partir : {text}"
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn wake_warns_when_the_allowlist_mac_is_the_wrong_interface() {
    if !bash_available() {
        return;
    }
    // Allowlist = MAC Wi-Fi, appareil en Ethernet : le mod doit le dire et
    // utiliser la MAC du réseau (sinon le réveil ne peut pas marcher).
    let port = fake_roku("ethernet");
    let d = tempdir("mac-mismatch");
    std::fs::create_dir_all(d.join(".zer0")).unwrap();
    std::fs::write(
        d.join(".zer0/zer0do.allow"),
        "127.0.0.1  18:ef:3a:21:a7:74\n",
    )
    .unwrap();
    let (ok, text) = run_env(
        &["wake", "127.0.0.1", "--dry-run"],
        &d,
        &[("ZER0DO_ECP_PORT", &port.to_string())],
    );
    assert!(ok, "{text}");
    assert!(text.contains("ne correspond pas"), "{text}");
    assert!(
        text.contains("ac:ac:e2:2a:fe:4c"),
        "la MAC du réseau doit être retenue : {text}"
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn status_and_active_show_the_app_id() {
    if !bash_available() {
        return;
    }
    let port = fake_roku("ethernet");
    let d = tempdir("app-id");
    let port_s = port.to_string();
    let envs = [("ZER0DO_ECP_PORT", port_s.as_str())];

    let (ok, text) = run_env(&["status", "127.0.0.1"], &d, &envs);
    assert!(ok, "{text}");
    assert!(
        text.contains("application: YouTube (id 837)"),
        "status doit donner l'id, utilisable tel quel dans `launch` : {text}"
    );

    let (ok2, text2) = run_env(&["active", "127.0.0.1"], &d, &envs);
    assert!(ok2, "{text2}");
    assert!(text2.contains("YouTube (id 837)"), "{text2}");
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
