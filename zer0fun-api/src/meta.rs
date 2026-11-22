//! Métadonnées d'un module fun.

use serde::{Deserialize, Serialize};

/// Métadonnées descriptives d'un module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    /// Nom unique du module (utilisé pour la commande slash).
    ///
    /// Doit être en minuscules, sans espaces: `^[a-z0-9_-]+$`
    pub name: String,

    /// Version sémantique (ex: "1.0.0").
    pub version: String,

    /// Description courte (une ligne).
    pub description: String,

    /// Auteur(s).
    pub authors: Vec<String>,

    /// Licence (ex: "MIT", "GPL-3.0").
    pub license: String,

    /// Site web ou repository.
    pub homepage: Option<String>,

    /// Mots-clés pour la recherche.
    pub keywords: Vec<String>,

    /// Catégorie du module.
    pub category: Category,

    /// Permissions requises.
    pub permissions: Permissions,

    /// Configuration par défaut du module.
    #[serde(default)]
    pub default_config: serde_json::Value,
}

impl Meta {
    /// Crée des métadonnées minimales.
    pub fn new(name: &str, version: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
            description: description.to_string(),
            authors: Vec::new(),
            license: "MIT".to_string(),
            homepage: None,
            keywords: Vec::new(),
            category: Category::Other,
            permissions: Permissions::default(),
            default_config: serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    /// Ajoute un auteur.
    pub fn author(mut self, author: &str) -> Self {
        self.authors.push(author.to_string());
        self
    }

    /// Définit la licence.
    pub fn license(mut self, license: &str) -> Self {
        self.license = license.to_string();
        self
    }

    /// Définit la catégorie.
    pub fn category(mut self, category: Category) -> Self {
        self.category = category;
        self
    }

    /// Ajoute un mot-clé.
    pub fn keyword(mut self, keyword: &str) -> Self {
        self.keywords.push(keyword.to_string());
        self
    }

    /// Définit les permissions.
    pub fn permissions(mut self, permissions: Permissions) -> Self {
        self.permissions = permissions;
        self
    }

    /// Vérifie que le nom est valide.
    pub fn validate(&self) -> crate::Result<()> {
        if self.name.is_empty() {
            return Err(crate::Error::Init("nom de module vide".into()));
        }
        if !self
            .name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        {
            return Err(crate::Error::Init(format!(
                "nom de module invalide: '{}' (doit être ^[a-z0-9_-]+$)",
                self.name
            )));
        }
        Ok(())
    }
}

/// Catégorie de module fun.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    /// Jeu interactif.
    Game,

    /// Visualisation/animation.
    Visual,

    /// Outil/utilitaire.
    Tool,

    /// Audio/musique.
    Audio,

    /// Réseau/communication.
    Network,

    /// Productivité.
    Productivity,

    /// Autre.
    Other,
}

impl Default for Category {
    fn default() -> Self {
        Self::Other
    }
}

/// Permissions d'un module.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Permissions {
    /// Accès réseau (HTTP, WebSocket, etc.).
    pub network: bool,

    /// Lecture de fichiers hors du répertoire de données.
    pub fs_read: bool,

    /// Écriture de fichiers hors du répertoire de données.
    pub fs_write: bool,

    /// Exécution de commandes système.
    pub process: bool,

    /// Accès au presse-papiers.
    pub clipboard: bool,

    /// Notifications système.
    pub notifications: bool,

    /// Audio (lecture/son).
    pub audio: bool,
}

impl Permissions {
    /// Aucune permission (module sandboxé).
    pub fn none() -> Self {
        Self::default()
    }

    /// Toutes les permissions (module de confiance).
    pub fn all() -> Self {
        Self {
            network: true,
            fs_read: true,
            fs_write: true,
            process: true,
            clipboard: true,
            notifications: true,
            audio: true,
        }
    }

    /// Permissions pour un module réseau.
    pub fn network_only() -> Self {
        Self {
            network: true,
            ..Default::default()
        }
    }
}
