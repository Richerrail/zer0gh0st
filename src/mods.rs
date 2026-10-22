//! Résolution de la racine du projet.
//!
//! Utilisée par la base de connaissance (dossier source `base_connaissance`) et
//! par la migration des données historiques. Les lanceurs `/kristal`,
//! `/streaming` et `/retro` ont été retirés : leurs sources n'étaient pas
//! versionnées, donc les commandes étaient mortes après un clone.

use std::path::PathBuf;

/// Racine du projet, déduite de l'exécutable (`target/release/zer0` → projet).
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
    if let Ok(exe) = std::env::current_exe() {
        if let Some(root) = exe
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
        {
            return root.to_path_buf();
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}
