//! Résolution de la racine du projet.
//!
//! Utilisée par la base de connaissance (dossier source `base_connaissance`) et
//! par la migration des données historiques. Les lanceurs `/kristal`,
//! `/streaming` et `/retro` ont été retirés : leurs sources n'étaient pas
//! versionnées, donc les commandes étaient mortes après un clone.

use std::path::PathBuf;

/// Dossier (courant ou parent) qui contient `.zer0/project.toml`.
pub fn project_config_dir() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    cwd.ancestors()
        .find(|d| d.join(".zer0/project.toml").is_file())
        .map(|d| d.to_path_buf())
}

/// Racine du projet : dossier courant (ou parent) contenant `.zer0/project.toml`
/// ou `.git`, sinon checkout de l'exécutable, sinon dossier courant.
///
/// Repli : `ZER0_ROOT`, puis le dossier courant. Une installation hors checkout
/// n'a pas de racine de projet — les appelants testent la présence de
/// `Cargo.toml` avant de s'en servir.
pub fn project_root() -> PathBuf {
    if let Ok(root) = std::env::var("ZER0_ROOT") {
        let root = root.trim();
        if !root.is_empty() {
            return PathBuf::from(root);
        }
    }
    // 1) Le vrai projet est celui où l'on travaille : on remonte depuis le
    // dossier courant jusqu'à `.zer0/project.toml` ou `.git`.
    if let Ok(cwd) = std::env::current_dir() {
        for dir in cwd.ancestors() {
            if dir.join(".zer0/project.toml").is_file() || dir.join(".git").exists() {
                return dir.to_path_buf();
            }
        }
        // 2) Sinon, un binaire lancé depuis un checkout : on remonte tant que
        // le dossier contient un Cargo.toml (évite `/usr` pour /usr/bin/zer0).
        if let Ok(exe) = std::env::current_exe() {
            for dir in exe.ancestors().skip(1) {
                if dir.join("Cargo.toml").is_file() {
                    return dir.to_path_buf();
                }
            }
        }
        return cwd;
    }
    PathBuf::from(".")
}
