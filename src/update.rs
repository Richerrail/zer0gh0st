//! Mise à jour automatique depuis les Releases GitHub de zer0gh0st.
//!
//! Principes :
//! - on télécharge le **binaire brut** de la plateforme (pas d'archive à extraire,
//!   donc aucune dépendance tar/gzip) ;
//! - on **vérifie le SHA256** quand la release publie un `SHA256SUMS` (sinon on
//!   le dit franchement : l'intégrité n'est pas vérifiée) ;
//! - remplacement **atomique** du binaire courant (`rename`) ;
//! - limites assumées : l'API GitHub non authentifiée est plafonnée (60 req/h),
//!   et un SHA256 pris dans le même dépôt protège d'une **corruption**, pas d'un
//!   dépôt compromis (aucune signature).

use anyhow::{anyhow, Context, Result};

/// Dépôt d'origine des releases.
pub const REPO: &str = "Richerrail/zer0gh0st";

/// Version compilée dans ce binaire.
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Cible de compilation, telle qu'utilisée dans les noms d'assets de la CI.
pub fn target_triple() -> &'static str {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "aarch64-unknown-linux-gnu"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "x86_64-apple-darwin"
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "x86_64-pc-windows-msvc"
    } else {
        "inconnue"
    }
}

/// Nom de l'asset à télécharger pour cette plateforme.
pub fn asset_name() -> String {
    let t = target_triple();
    if cfg!(windows) {
        format!("zer0-{t}.exe")
    } else {
        format!("zer0-{t}")
    }
}

/// `true` si `latest` est plus récent que `current` (comparaison numérique par
/// composants ; `v` initial et suffixes ignorés).
pub fn is_newer(latest: &str, current: &str) -> bool {
    fn nums(s: &str) -> Vec<u64> {
        s.trim()
            .trim_start_matches(['v', 'V'])
            .split(|c: char| !c.is_ascii_digit())
            .filter(|p| !p.is_empty())
            .filter_map(|p| p.parse().ok())
            .collect()
    }
    let (a, b) = (nums(latest), nums(current));
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

/// Extrait le SHA256 attendu pour `asset` depuis un fichier `SHA256SUMS`.
/// Formes acceptées : `<hash>  <nom>` et `<hash> *<nom>` (mode binaire).
pub fn sha256_for(sums: &str, asset: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let hash = it.next()?;
        let name = it.next()?.trim_start_matches('*');
        (name == asset).then(|| hash.to_ascii_lowercase())
    })
}

/// `true` si le binaire vient d'un build Cargo (`.../target/<profil>/…`).
/// Dans ce cas `zer0 update` ne doit **pas** remplacer le binaire : pour un dev,
/// c'est `cargo build` qui fait foi (et un prochain build écraserait la mise à jour).
pub fn is_dev_build(path: &str) -> bool {
    path.replace('\\', "/").contains("/target/")
}

/// SHA256 hexadécimal d'un bloc d'octets.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// Release GitHub (champs utiles).
pub struct Release {
    pub tag: String,
    pub url: String,
    pub notes: String,
    pub assets: Vec<(String, String)>,
}

fn http() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(format!("zer0/{}", current_version()))
        .build()?)
}

async fn get_bytes(url: &str) -> Result<Vec<u8>> {
    let r = http()?
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?;
    Ok(r.bytes().await?.to_vec())
}

/// Dernière release publiée.
pub async fn fetch_latest() -> Result<Release> {
    let v: serde_json::Value = serde_json::from_slice(
        &get_bytes(&format!(
            "https://api.github.com/repos/{REPO}/releases/latest"
        ))
        .await?,
    )?;
    let tag = v["tag_name"].as_str().unwrap_or_default().to_string();
    if tag.is_empty() {
        return Err(anyhow!("réponse GitHub inattendue (tag absent)"));
    }
    let assets = v["assets"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    Some((
                        x["name"].as_str()?.to_string(),
                        x["browser_download_url"].as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Release {
        tag,
        url: v["html_url"].as_str().unwrap_or_default().to_string(),
        notes: v["body"].as_str().unwrap_or_default().to_string(),
        assets,
    })
}

/// Remplace le binaire en cours d'exécution.
/// Unix : `rename` atomique (l'ancien inode reste valide pour le process courant).
/// Windows : impossible d'écraser un `.exe` en cours → on renomme l'ancien puis
/// on installe le nouveau ; l'ancien reste verrouillé jusqu'à la fin du
/// processus (Windows refuse sa suppression) → on le signale, il est nettoyé au
/// lancement suivant.
/// Retourne une note à afficher (None si rien à signaler).
pub fn replace_current_exe(bytes: &[u8]) -> Result<Option<String>> {
    let exe = std::env::current_exe().context("chemin de l'exécutable")?;
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow!("dossier de l'exécutable introuvable"))?;
    let stem = exe
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let new = dir.join(format!(".{stem}.new"));
    std::fs::write(&new, bytes).with_context(|| format!("écriture de {}", new.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755))?;
    }

    #[cfg(windows)]
    {
        let old = dir.join(format!("{stem}.old"));
        // Reliquat d'une mise à jour précédente : le processus qui le tenait
        // est quitté, il est donc supprimable.
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&exe, &old).context("renommage de l'ancien binaire")?;
        std::fs::rename(&new, &exe).context("installation du nouveau binaire")?;
        // L'ancien binaire reste chargé par le processus courant → Windows
        // refuse de le supprimer : on le dit plutôt que de l'ignorer.
        match std::fs::remove_file(&old) {
            Ok(()) => Ok(None),
            Err(e) => Ok(Some(format!(
                "l'ancien binaire est resté en {} ({e}) — il sera supprimé au prochain lancement",
                old.display()
            ))),
        }
    }

    #[cfg(not(windows))]
    {
        std::fs::rename(&new, &exe)
            .with_context(|| format!("remplacement de {}", exe.display()))?;
        Ok(None)
    }
}

/// Nettoie le reliquat d'une mise à jour Windows (`zer0.old`) : le processus
/// qui le tenait est quitté, il est donc supprimable. Sans effet ailleurs.
pub fn cleanup_leftover() {
    #[cfg(windows)]
    {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let stem = exe
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let _ = std::fs::remove_file(dir.join(format!("{stem}.old")));
            }
        }
    }
}

/// Flux complet. Retourne un rapport lisible (aussi utilisé par la TUI).
pub async fn run(check_only: bool, force: bool) -> Result<String> {
    cleanup_leftover();
    let rel = fetch_latest().await?;
    let cur = current_version();
    let newer = is_newer(&rel.tag, cur);
    let mut out = format!(
        "version actuelle {cur} · dernière {} — {}\n",
        rel.tag, rel.url
    );
    if !rel.notes.is_empty() {
        let excerpt: Vec<&str> = rel
            .notes
            .lines()
            .filter(|l| !l.trim().is_empty())
            .take(3)
            .collect();
        out.push_str(&format!("notes : {}\n", excerpt.join(" · ")));
    }
    if !newer && !force {
        out.push_str("Déjà à jour. (‑‑force pour réinstaller)");
        return Ok(out);
    }
    let dev = std::env::current_exe()
        .map(|p| is_dev_build(&p.display().to_string()))
        .unwrap_or(false);
    if dev {
        out.push_str(
            "ℹ binaire de développement (target/) — `cargo build --release` fait foi ici.\n",
        );
    }
    if check_only {
        out.push_str(if newer {
            "→ mise à jour disponible"
        } else {
            "→ réinstallation forcée demandée"
        });
        return Ok(out);
    }
    if dev && !force {
        return Err(anyhow!(
            "binaire de développement détecté (target/) : mets à jour les sources \
             (git pull && cargo build --release), ou --force pour installer la release"
        ));
    }

    let asset = asset_name();
    let url = rel
        .assets
        .iter()
        .find(|(n, _)| n == &asset)
        .map(|(_, u)| u.clone())
        .ok_or_else(|| {
            anyhow!(
                "aucun binaire « {asset} » dans la release {} — cible {} non publiée : compile depuis les sources",
                rel.tag,
                target_triple()
            )
        })?;

    out.push_str(&format!("téléchargement de {asset}…\n"));
    let bytes = get_bytes(&url).await?;

    match rel.assets.iter().find(|(n, _)| n == "SHA256SUMS") {
        Some((_, sums_url)) => {
            let sums = String::from_utf8_lossy(&get_bytes(sums_url).await?).to_string();
            match sha256_for(&sums, &asset) {
                Some(expected) => {
                    let got = sha256_hex(&bytes);
                    if got != expected {
                        return Err(anyhow!(
                            "SHA256 invalide pour {asset} (attendu {expected}, obtenu {got}) — abandon"
                        ));
                    }
                    out.push_str("SHA256 vérifié ✓\n");
                }
                None => out.push_str("⚠ SHA256SUMS publié mais sans entrée pour cet asset\n"),
            }
        }
        None => out.push_str("⚠ aucun SHA256SUMS publié — intégrité non vérifiée\n"),
    }

    if let Some(note) = replace_current_exe(&bytes)? {
        out.push_str(&format!("⚠ {note}\n"));
    }
    out.push_str("binaire remplacé ✓ — relance zer0 pour utiliser la nouvelle version.");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison_is_numeric() {
        assert!(is_newer("v0.6.0", "0.5.4"));
        assert!(is_newer("0.10.0", "0.9.9")); // 10 > 9, pas de comparaison texte
        assert!(!is_newer("v0.5.4", "0.5.4"));
        assert!(!is_newer("0.5.3", "0.5.4"));
        assert!(is_newer("1.0", "0.99.99"));
    }

    #[test]
    fn sha256sums_parsing_variants() {
        let sums = "\
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  zer0-x86_64-unknown-linux-gnu
ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789 *zer0-aarch64-apple-darwin
";
        assert_eq!(
            sha256_for(sums, "zer0-x86_64-unknown-linux-gnu").as_deref(),
            Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
        // mode binaire `*nom` + normalisation en minuscules
        assert_eq!(
            sha256_for(sums, "zer0-aarch64-apple-darwin").as_deref(),
            Some("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
        );
        assert_eq!(sha256_for(sums, "inconnu"), None);
    }

    #[test]
    fn sha256_of_empty_is_the_known_vector() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn asset_name_matches_ci_convention() {
        let a = asset_name();
        assert!(a.starts_with("zer0-"), "{a}");
        assert!(a.contains(target_triple()), "{a}");
        if cfg!(windows) {
            assert!(a.ends_with(".exe"));
        } else {
            assert!(!a.ends_with(".exe"));
        }
    }

    #[test]
    fn dev_builds_are_detected() {
        assert!(is_dev_build("/home/dev/projet/target/release/zer0"));
        assert!(is_dev_build("/home/dev/projet/target/debug/zer0"));
        assert!(is_dev_build("C:\\dev\\projet\\target\\release\\zer0.exe"));
        assert!(!is_dev_build("/usr/local/bin/zer0"));
        assert!(!is_dev_build("/home/dev/.local/bin/zer0gh0st"));
        // pas de faux positif sur un dossier nommé target ailleurs
        assert!(!is_dev_build("/opt/targets/zer0"));
    }
}
