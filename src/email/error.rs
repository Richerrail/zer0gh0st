//! Erreurs typées du module email.
//! Aucun secret n'est inclus dans les variantes d'erreur.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum EmailError {
    #[error("Configuration manquante : {0}")]
    ConfigMissing(String),

    #[error("Configuration invalide : {0}")]
    ConfigInvalid(String),

    #[error("Permission refusée : {0}")]
    PermissionDenied(String),

    #[error("Erreur IMAP : {0}")]
    Imap(String),

    #[error("Erreur SMTP : {0}")]
    Smtp(String),

    #[error("Erreur de parsing MIME : {0}")]
    Parse(String),

    #[error("Erreur d'IO : {0}")]
    Io(#[from] std::io::Error),

    #[error("Erreur de sérialisation : {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Adresse email invalide : {0}")]
    InvalidAddress(String),

    #[error("Opération bloquée par un garde-fou : {0}")]
    Guardrail(String),

    #[error("Erreur interne : {0}")]
    Internal(String),
}
