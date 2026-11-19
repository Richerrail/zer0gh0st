//! Chargement et validation de la configuration email.
//! Les secrets proviennent uniquement de variables d'environnement.

use crate::email::error::EmailError;
use serde::Deserialize;
use std::env;

/// Configuration email chargée depuis l'environnement.
#[derive(Debug, Clone, Deserialize)]
pub struct EmailConfig {
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_user: String,
    pub imap_password: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_user: String,
    pub smtp_password: String,
    pub from_name: Option<String>,
    pub from_address: String,
    pub allow_read: bool,
    pub allow_write: bool,
    pub allow_manage: bool,
    pub allow_delete: bool,
}

impl EmailConfig {
    /// Charge la configuration depuis les variables d'environnement.
    /// Retourne une erreur si une variable requise est manquante.
    pub fn from_env() -> Result<Self, EmailError> {
        let get = |key: &str| -> Result<String, EmailError> {
            env::var(key).map_err(|_| EmailError::ConfigMissing(key.to_string()))
        };

        let imap_port: u16 = env::var("ZER0_EMAIL_IMAP_PORT")
            .unwrap_or_else(|_| "993".to_string())
            .parse()
            .map_err(|_| EmailError::ConfigInvalid("ZER0_EMAIL_IMAP_PORT".to_string()))?;

        let smtp_port: u16 = env::var("ZER0_EMAIL_SMTP_PORT")
            .unwrap_or_else(|_| "587".to_string())
            .parse()
            .map_err(|_| EmailError::ConfigInvalid("ZER0_EMAIL_SMTP_PORT".to_string()))?;

        let flag = |key: &str| -> bool {
            env::var(key)
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false)
        };

        Ok(Self {
            imap_host: get("ZER0_EMAIL_IMAP_HOST")?,
            imap_port,
            imap_user: get("ZER0_EMAIL_IMAP_USER")?,
            imap_password: get("ZER0_EMAIL_IMAP_PASSWORD")?,
            smtp_host: get("ZER0_EMAIL_SMTP_HOST")?,
            smtp_port,
            smtp_user: get("ZER0_EMAIL_SMTP_USER")?,
            smtp_password: get("ZER0_EMAIL_SMTP_PASSWORD")?,
            from_name: env::var("ZER0_EMAIL_FROM_NAME").ok(),
            from_address: get("ZER0_EMAIL_FROM_ADDRESS")?,
            allow_read: flag("ZER0_EMAIL_ALLOW_READ"),
            allow_write: flag("ZER0_EMAIL_ALLOW_WRITE"),
            allow_manage: flag("ZER0_EMAIL_ALLOW_MANAGE"),
            allow_delete: flag("ZER0_EMAIL_ALLOW_DELETE"),
        })
    }

    /// Valide la configuration au démarrage.
    pub fn validate(&self) -> Result<(), EmailError> {
        if self.imap_host.is_empty() || self.imap_user.is_empty() {
            return Err(EmailError::ConfigInvalid(
                "IMAP host/user requis".to_string(),
            ));
        }
        if self.smtp_host.is_empty() || self.smtp_user.is_empty() {
            return Err(EmailError::ConfigInvalid(
                "SMTP host/user requis".to_string(),
            ));
        }
        if self.from_address.is_empty() {
            return Err(EmailError::ConfigInvalid(
                "ZER0_EMAIL_FROM_ADDRESS requis".to_string(),
            ));
        }
        Ok(())
    }
}
