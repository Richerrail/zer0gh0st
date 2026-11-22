//! Contexte d'exécution d'un module fun.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::ModuleConfig;

/// Contexte fourni par zer0gh0st à chaque module.
///
/// Contient les arguments, la configuration, et des flags de contrôle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    /// Arguments passés au module (ex: `/pomodoro start 25` → ["start", "25"])
    pub args: Vec<String>,

    /// Répertoire de données persistantes du module.
    ///
    /// Chemin: `~/.local/share/zer0gh0st/fun/<module_name>/`
    /// Créé automatiquement si inexistant.
    pub data_dir: PathBuf,

    /// Configuration du module (depuis `config.toml` de zer0gh0st).
    pub config: ModuleConfig,

    /// Demander la fermeture du module.
    ///
    /// Mettre à `true` pour quitter proprement.
    pub exit: bool,

    /// Forcer un redraw au prochain tick.
    ///
    /// Mettre à `true` après une modification d'état.
    pub redraw: bool,

    /// Largeur du terminal (en caractères).
    pub term_width: u16,

    /// Hauteur du terminal (en caractères).
    pub term_height: u16,

    /// Mode debug activé.
    pub debug: bool,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            args: Vec::new(),
            data_dir: PathBuf::from("."),
            config: ModuleConfig::default(),
            exit: false,
            redraw: true,
            term_width: 80,
            term_height: 24,
            debug: false,
        }
    }
}

impl Context {
    /// Crée un nouveau contexte avec les arguments donnés.
    pub fn new(args: Vec<String>) -> Self {
        Self {
            args,
            ..Default::default()
        }
    }

    /// Retourne le premier argument, ou une valeur par défaut.
    pub fn arg(&self, index: usize) -> Option<&str> {
        self.args.get(index).map(|s| s.as_str())
    }

    /// Retourne le premier argument parsé en nombre, ou une erreur.
    pub fn arg_parse<T: std::str::FromStr>(&self, index: usize) -> crate::Result<T> {
        self.arg(index)
            .ok_or_else(|| crate::Error::Config(format!("argument {} manquant", index)))?
            .parse()
            .map_err(|_| crate::Error::Config(format!("argument {} invalide", index)))
    }

    /// Chemin vers un fichier de données du module.
    pub fn data_file(&self, name: &str) -> PathBuf {
        self.data_dir.join(name)
    }

    /// Lit un fichier de données JSON.
    pub fn read_json<T: serde::de::DeserializeOwned>(&self, name: &str) -> crate::Result<T> {
        let path = self.data_file(name);
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content)
            .map_err(|e| crate::Error::Config(format!("JSON invalide: {}", e)))
    }

    /// Écrit un fichier de données JSON.
    pub fn write_json<T: serde::Serialize>(&self, name: &str, value: &T) -> crate::Result<()> {
        let path = self.data_file(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(value)
            .map_err(|e| crate::Error::Config(format!("sérialisation JSON: {}", e)))?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Demande la fermeture du module.
    pub fn quit(&mut self) {
        self.exit = true;
    }

    /// Demande un redraw.
    pub fn request_redraw(&mut self) {
        self.redraw = true;
    }

    /// Vérifie si le terminal est assez grand.
    pub fn check_size(&self, min_width: u16, min_height: u16) -> bool {
        self.term_width >= min_width && self.term_height >= min_height
    }
}
