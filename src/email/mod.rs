//! Module email pour zer0gh0st.
//! Opérations read / write / manage via IMAP et SMTP.
//! Aucun secret n'est jamais loggué ni stocké en clair.

pub mod config;
pub mod error;
pub mod imap_client;
pub mod parser;
pub mod service;
pub mod smtp_client;
pub mod types;

use crate::email::config::EmailConfig;
use crate::email::service::EmailService;

/// Point d'entrée du module email.
pub async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = EmailConfig::from_env()?;
    let _service = EmailService::new(config)?;
    Ok(())
}
