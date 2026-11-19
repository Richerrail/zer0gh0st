//! Façade haut niveau EmailService.
//! Combine IMAP (read/manage) et SMTP (write) avec permissions et audit log.
//! Aucun secret n'est loggué ; le contenu des messages n'apparaît jamais dans l'audit.

use crate::email::config::EmailConfig;
use crate::email::error::EmailError;
use crate::email::imap_client::ImapClient;
use crate::email::smtp_client::SmtpClient;
use crate::email::types::{EmailAction, EmailMessage, EmailSummary, SendRequest};
use chrono::Utc;
use serde_json::json;
use std::path::PathBuf;
use tracing::{debug, info};

/// Façade email : permissions + audit + clients IMAP/SMTP.
pub struct EmailService {
    config: EmailConfig,
    audit_log: PathBuf,
}

impl EmailService {
    /// Crée le service. Valide la config et prépare le fichier d'audit.
    pub fn new(config: EmailConfig) -> Result<Self, EmailError> {
        config.validate()?;
        let audit_dir = std::env::var("ZER0_EMAIL_AUDIT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"));
        std::fs::create_dir_all(&audit_dir)?;
        Ok(Self {
            config,
            audit_log: audit_dir.join("email_audit.log"),
        })
    }

    /// Accès à la config (pour les tests).
    pub fn config(&self) -> &EmailConfig {
        &self.config
    }

    /// Écrit une entrée d'audit (jamais de contenu de message ni de secret).
    fn audit(&self, action: &str, details: serde_json::Value) {
        let entry = json!({
            "timestamp": Utc::now().to_rfc3339(),
            "action": action,
            "details": details,
        });
        let line = entry.to_string();
        if let Err(e) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.audit_log)
            .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()))
        {
            debug!("Audit log échoué: {}", e);
        }
    }

    /// Vérifie une permission. Les refus sont journalisés dans l'audit.
    fn require(&self, perm: bool, action: &str) -> Result<(), EmailError> {
        if perm {
            Ok(())
        } else {
            self.audit("permission_denied", json!({"action": action}));
            Err(EmailError::PermissionDenied(action.to_string()))
        }
    }

    /// Ouvre une session IMAP.
    async fn imap(&self) -> Result<ImapClient, EmailError> {
        ImapClient::new(&self.config).await
    }

    /// Liste les dossiers (read).
    pub async fn list_folders(&self) -> Result<Vec<String>, EmailError> {
        self.require(self.config.allow_read, "list_folders")?;
        let mut imap = self.imap().await?;
        let folders = imap.list_folders().await?;
        imap.logout().await?;
        self.audit("list_folders", json!({"count": folders.len()}));
        Ok(folders)
    }

    /// Liste les messages d'un dossier (read).
    pub async fn list_messages(
        &self,
        mailbox: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<EmailSummary>, EmailError> {
        self.require(self.config.allow_read, "list_messages")?;
        let mut imap = self.imap().await?;
        let messages = imap.list_messages(mailbox, limit, offset).await?;
        imap.logout().await?;
        self.audit(
            "list_messages",
            json!({"mailbox": mailbox, "count": messages.len()}),
        );
        Ok(messages)
    }

    /// Récupère un message complet (read).
    pub async fn get_message(&self, mailbox: &str, uid: u32) -> Result<EmailMessage, EmailError> {
        self.require(self.config.allow_read, "get_message")?;
        let mut imap = self.imap().await?;
        let message = imap.get_message(mailbox, uid).await?;
        imap.logout().await?;
        self.audit("get_message", json!({"mailbox": mailbox, "uid": uid}));
        Ok(message)
    }

    /// Recherche des messages (read).
    pub async fn search(
        &self,
        mailbox: &str,
        query: &str,
        limit: u32,
    ) -> Result<Vec<EmailSummary>, EmailError> {
        self.require(self.config.allow_read, "search")?;
        let mut imap = self.imap().await?;
        let results = imap.search(mailbox, query, limit).await?;
        imap.logout().await?;
        self.audit(
            "search",
            json!({"mailbox": mailbox, "count": results.len()}),
        );
        Ok(results)
    }

    /// Envoie un email (write).
    pub async fn send(&self, request: SendRequest) -> Result<(), EmailError> {
        self.require(self.config.allow_write, "send")?;
        let smtp = SmtpClient::new(&self.config)?;
        smtp.send(&request).await?;
        self.audit(
            "send",
            json!({
                "to_count": request.to.len(),
                "cc_count": request.cc.len(),
                "bcc_count": request.bcc.len(),
                "attachments": request.attachments.len(),
            }),
        );
        info!("Email envoyé");
        Ok(())
    }

    /// Marque lu/non-lu (manage).
    pub async fn mark_read(&self, mailbox: &str, uid: u32, read: bool) -> Result<(), EmailError> {
        self.require(self.config.allow_manage, "mark_read")?;
        let mut imap = self.imap().await?;
        let flags = vec!["\\Seen".to_string()];
        imap.set_flags(mailbox, uid, flags, read).await?;
        imap.logout().await?;
        self.audit(
            "mark_read",
            json!({"mailbox": mailbox, "uid": uid, "read": read}),
        );
        Ok(())
    }

    /// Déplace un message (manage).
    pub async fn move_message(
        &self,
        mailbox: &str,
        uid: u32,
        target_mailbox: &str,
    ) -> Result<(), EmailError> {
        self.require(self.config.allow_manage, "move_message")?;
        let mut imap = self.imap().await?;
        imap.move_message(mailbox, uid, target_mailbox).await?;
        imap.logout().await?;
        self.audit(
            "move_message",
            json!({"mailbox": mailbox, "uid": uid, "target": target_mailbox}),
        );
        Ok(())
    }

    /// Supprime un message (delete). Expunge explicite uniquement.
    pub async fn delete_message(
        &self,
        mailbox: &str,
        uid: u32,
        expunge: bool,
    ) -> Result<(), EmailError> {
        self.require(self.config.allow_delete, "delete_message")?;
        let mut imap = self.imap().await?;
        imap.delete_message(mailbox, uid, expunge).await?;
        imap.logout().await?;
        self.audit(
            "delete_message",
            json!({"mailbox": mailbox, "uid": uid, "expunge": expunge}),
        );
        Ok(())
    }

    /// Exécute une action JSON de l'agent.
    pub async fn handle_action(
        &self,
        action: EmailAction,
    ) -> Result<serde_json::Value, EmailError> {
        match action {
            EmailAction::ListFolders => {
                let folders = self.list_folders().await?;
                Ok(json!({"folders": folders}))
            }
            EmailAction::ListMessages {
                mailbox,
                limit,
                offset,
            } => {
                let messages = self
                    .list_messages(&mailbox, limit.unwrap_or(20), offset.unwrap_or(0))
                    .await?;
                Ok(json!({"messages": messages}))
            }
            EmailAction::GetMessage { mailbox, uid } => {
                let message = self.get_message(&mailbox, uid).await?;
                Ok(json!({"message": message}))
            }
            EmailAction::Search {
                mailbox,
                query,
                limit,
            } => {
                let results = self.search(&mailbox, &query, limit.unwrap_or(20)).await?;
                Ok(json!({"results": results}))
            }
            EmailAction::Send { request } => {
                self.send(request).await?;
                Ok(json!({"status": "sent"}))
            }
            EmailAction::Reply { mailbox, uid, body } => {
                // Récupère le message original pour construire la réponse.
                let original = self.get_message(&mailbox, uid).await?;
                let to =
                    original.summary.from.clone().ok_or_else(|| {
                        EmailError::InvalidAddress("Pas d'expéditeur".to_string())
                    })?;
                let subject = format!("Re: {}", original.summary.subject.unwrap_or_default());
                let request = SendRequest {
                    to: vec![to],
                    cc: vec![],
                    bcc: vec![],
                    subject,
                    text_body: Some(body),
                    html_body: None,
                    attachments: vec![],
                };
                self.send(request).await?;
                Ok(json!({"status": "replied"}))
            }
            EmailAction::Forward {
                mailbox,
                uid,
                to,
                body,
            } => {
                let original = self.get_message(&mailbox, uid).await?;
                let subject = format!("Fwd: {}", original.summary.subject.unwrap_or_default());
                let mut text = String::new();
                if let Some(b) = body {
                    text.push_str(&b);
                    text.push_str("\n\n");
                }
                text.push_str("--- Message transféré ---\n");
                if let Some(from) = &original.summary.from {
                    text.push_str(&format!("De : {}\n", from));
                }
                if let Some(body) = &original.text_body {
                    text.push_str(body);
                }
                let request = SendRequest {
                    to,
                    cc: vec![],
                    bcc: vec![],
                    subject,
                    text_body: Some(text),
                    html_body: None,
                    attachments: vec![],
                };
                self.send(request).await?;
                Ok(json!({"status": "forwarded"}))
            }
            EmailAction::MarkRead { mailbox, uid, read } => {
                self.mark_read(&mailbox, uid, read).await?;
                Ok(json!({"status": "ok"}))
            }
            EmailAction::Move {
                mailbox,
                uid,
                target_mailbox,
            } => {
                self.move_message(&mailbox, uid, &target_mailbox).await?;
                Ok(json!({"status": "moved"}))
            }
            EmailAction::Delete {
                mailbox,
                uid,
                expunge,
            } => {
                self.delete_message(&mailbox, uid, expunge).await?;
                Ok(json!({"status": "deleted"}))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with(perms: (bool, bool, bool, bool)) -> EmailConfig {
        EmailConfig {
            imap_host: "imap.test".into(),
            imap_port: 993,
            imap_user: "user".into(),
            imap_password: "pass".into(),
            smtp_host: "smtp.test".into(),
            smtp_port: 587,
            smtp_user: "user".into(),
            smtp_password: "pass".into(),
            from_name: Some("Zer0".into()),
            from_address: "zer0@test.local".into(),
            allow_read: perms.0,
            allow_write: perms.1,
            allow_manage: perms.2,
            allow_delete: perms.3,
        }
    }

    #[tokio::test]
    async fn test_permission_denied_read() {
        let svc = EmailService::new(config_with((false, true, true, true))).unwrap();
        let err = svc.list_folders().await.unwrap_err();
        assert!(matches!(err, EmailError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn test_permission_denied_write() {
        let svc = EmailService::new(config_with((true, false, true, true))).unwrap();
        let req = SendRequest {
            to: vec!["a@b.c".into()],
            cc: vec![],
            bcc: vec![],
            subject: "s".into(),
            text_body: Some("b".into()),
            html_body: None,
            attachments: vec![],
        };
        let err = svc.send(req).await.unwrap_err();
        assert!(matches!(err, EmailError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn test_permission_denied_delete() {
        let svc = EmailService::new(config_with((true, true, true, false))).unwrap();
        let err = svc.delete_message("INBOX", 1, false).await.unwrap_err();
        assert!(matches!(err, EmailError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn test_audit_log_written() {
        let dir = std::env::temp_dir().join(format!("zer0_email_test_{}", std::process::id()));
        std::env::set_var("ZER0_EMAIL_AUDIT_DIR", &dir);
        let svc = EmailService::new(config_with((false, false, false, false))).unwrap();
        // Action refusée → audit quand même (via require avant connexion).
        let _ = svc.list_folders().await;
        let log = std::fs::read_to_string(dir.join("email_audit.log")).unwrap();
        assert!(log.contains("list_folders"));
        assert!(!log.contains("pass")); // jamais de secret
        std::env::remove_var("ZER0_EMAIL_AUDIT_DIR");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_config_validation() {
        let mut cfg = config_with((true, true, true, true));
        cfg.from_address = String::new();
        assert!(cfg.validate().is_err());
    }
}
