//! Client SMTP (write).
//! Envoi via lettre : texte, HTML, pièces jointes.
//! Aucun secret n'est loggué.

use crate::email::config::EmailConfig;
use crate::email::error::EmailError;
use crate::email::types::{AttachmentInput, SendRequest};
use lettre::message::header::ContentType;
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{Address, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use tracing::debug;

/// Client SMTP avec transport pré-configuré.
pub struct SmtpClient {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from_name: Option<String>,
    from_address: String,
}

impl SmtpClient {
    /// Crée le transport SMTP (sans connexion réseau).
    /// Port 465 : TLS implicite (Wrapper). Autres ports : STARTTLS requis.
    pub fn new(config: &EmailConfig) -> Result<Self, EmailError> {
        let credentials = Credentials::new(config.smtp_user.clone(), config.smtp_password.clone());

        let tls_params = TlsParameters::new(config.smtp_host.clone())
            .map_err(|e| EmailError::Smtp(format!("Paramètres TLS échoués: {}", e)))?;

        let builder = if config.smtp_port == 465 {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.smtp_host)
                .port(config.smtp_port)
                .tls(Tls::Wrapper(tls_params))
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.smtp_host)
                .port(config.smtp_port)
                .tls(Tls::Required(tls_params))
        };

        let transport = builder.credentials(credentials).build();

        debug!(
            "Transport SMTP prêt pour {}:{}",
            config.smtp_host, config.smtp_port
        );
        Ok(Self {
            transport,
            from_name: config.from_name.clone(),
            from_address: config.from_address.clone(),
        })
    }

    /// Valide une adresse email et la convertit en Mailbox.
    pub fn parse_mailbox(addr: &str) -> Result<Mailbox, EmailError> {
        addr.parse::<Mailbox>()
            .map_err(|e| EmailError::InvalidAddress(format!("'{}': {}", addr, e)))
    }

    /// Valide une liste d'adresses.
    pub fn parse_addresses(addrs: &[String]) -> Result<Vec<Mailbox>, EmailError> {
        addrs.iter().map(|a| Self::parse_mailbox(a)).collect()
    }

    /// Ajoute les pièces jointes à un multipart mixte.
    fn with_attachments(
        mut mixed: MultiPart,
        attachments: &[AttachmentInput],
    ) -> Result<MultiPart, EmailError> {
        for att in attachments {
            if att.filename.is_empty() {
                return Err(EmailError::Smtp("Nom de fichier requis".to_string()));
            }
            let ct = ContentType::parse(&att.mime_type).map_err(|e| {
                EmailError::Smtp(format!("MIME type invalide '{}': {}", att.mime_type, e))
            })?;
            mixed =
                mixed.singlepart(Attachment::new(att.filename.clone()).body(att.bytes.clone(), ct));
        }
        Ok(mixed)
    }

    /// Construit le message lettre à partir d'une SendRequest (sans envoi).
    pub fn build_message(&self, request: &SendRequest) -> Result<Message, EmailError> {
        if request.to.is_empty() {
            return Err(EmailError::InvalidAddress(
                "Au moins un destinataire requis".to_string(),
            ));
        }
        if request.subject.trim().is_empty() {
            return Err(EmailError::Smtp("Sujet requis".to_string()));
        }

        let from_addr: Address = self.from_address.parse().map_err(|e| {
            EmailError::InvalidAddress(format!(
                "Adresse from invalide '{}': {}",
                self.from_address, e
            ))
        })?;
        let from_mailbox = Mailbox::new(self.from_name.clone(), from_addr);

        let mut builder = Message::builder()
            .from(from_mailbox)
            .subject(request.subject.as_str());

        for to in Self::parse_addresses(&request.to)? {
            builder = builder.to(to);
        }
        for cc in Self::parse_addresses(&request.cc)? {
            builder = builder.cc(cc);
        }
        for bcc in Self::parse_addresses(&request.bcc)? {
            builder = builder.bcc(bcc);
        }

        let text = request.text_body.clone().filter(|t| !t.is_empty());
        let html = request.html_body.clone().filter(|h| !h.is_empty());
        let has_attachments = !request.attachments.is_empty();

        if text.is_none() && html.is_none() {
            return Err(EmailError::Smtp("Corps requis (texte ou HTML)".to_string()));
        }

        match (text, html) {
            (Some(t), Some(h)) => {
                let alt = MultiPart::alternative_plain_html(t, h);
                if has_attachments {
                    let mixed = Self::with_attachments(
                        MultiPart::mixed().multipart(alt),
                        &request.attachments,
                    )?;
                    builder.multipart(mixed)
                } else {
                    builder.multipart(alt)
                }
            }
            (Some(t), None) => {
                let sp = SinglePart::plain(t);
                if has_attachments {
                    let mixed = Self::with_attachments(
                        MultiPart::mixed().singlepart(sp),
                        &request.attachments,
                    )?;
                    builder.multipart(mixed)
                } else {
                    builder.singlepart(sp)
                }
            }
            (None, Some(h)) => {
                let sp = SinglePart::html(h);
                if has_attachments {
                    let mixed = Self::with_attachments(
                        MultiPart::mixed().singlepart(sp),
                        &request.attachments,
                    )?;
                    builder.multipart(mixed)
                } else {
                    builder.singlepart(sp)
                }
            }
            (None, None) => unreachable!("corps vide déjà rejeté"),
        }
        .map_err(|e| EmailError::Smtp(format!("Construction du message échouée: {}", e)))
    }

    /// Envoie un email.
    pub async fn send(&self, request: &SendRequest) -> Result<(), EmailError> {
        let message = self.build_message(request)?;
        self.transport
            .send(message)
            .await
            .map_err(|e| EmailError::Smtp(format!("Envoi échoué: {}", e)))?;
        debug!("Email envoyé à {} destinataire(s)", request.to.len());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> EmailConfig {
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
            allow_read: true,
            allow_write: true,
            allow_manage: true,
            allow_delete: false,
        }
    }

    fn base_request() -> SendRequest {
        SendRequest {
            to: vec!["bob@example.com".into()],
            cc: vec![],
            bcc: vec![],
            subject: "Test".into(),
            text_body: Some("Bonjour".into()),
            html_body: None,
            attachments: vec![],
        }
    }

    #[test]
    fn test_build_text_message() {
        let client = SmtpClient::new(&test_config()).unwrap();
        let msg = client.build_message(&base_request()).unwrap();
        let formatted = String::from_utf8(msg.formatted()).unwrap();
        assert!(formatted.contains("Subject: Test"));
        assert!(formatted.contains("bob@example.com"));
        assert!(formatted.contains("Bonjour"));
    }

    #[test]
    fn test_build_html_message() {
        let client = SmtpClient::new(&test_config()).unwrap();
        let mut req = base_request();
        req.text_body = None;
        req.html_body = Some("<p>Bonjour</p>".into());
        let msg = client.build_message(&req).unwrap();
        let formatted = String::from_utf8(msg.formatted()).unwrap();
        assert!(formatted.contains("text/html"));
    }

    #[test]
    fn test_build_multipart_alternative() {
        let client = SmtpClient::new(&test_config()).unwrap();
        let mut req = base_request();
        req.html_body = Some("<p>Bonjour</p>".into());
        let msg = client.build_message(&req).unwrap();
        let formatted = String::from_utf8(msg.formatted()).unwrap();
        assert!(formatted.contains("multipart/alternative"));
    }

    #[test]
    fn test_build_attachment_message() {
        let client = SmtpClient::new(&test_config()).unwrap();
        let mut req = base_request();
        req.attachments.push(AttachmentInput {
            filename: "test.txt".into(),
            mime_type: "text/plain".into(),
            bytes: b"contenu".to_vec(),
        });
        let msg = client.build_message(&req).unwrap();
        let formatted = String::from_utf8(msg.formatted()).unwrap();
        assert!(formatted.contains("multipart/mixed"));
        assert!(formatted.contains("test.txt"));
    }

    #[test]
    fn test_reject_empty_recipients() {
        let client = SmtpClient::new(&test_config()).unwrap();
        let mut req = base_request();
        req.to = vec![];
        assert!(client.build_message(&req).is_err());
    }

    #[test]
    fn test_reject_empty_body() {
        let client = SmtpClient::new(&test_config()).unwrap();
        let mut req = base_request();
        req.text_body = None;
        assert!(client.build_message(&req).is_err());
    }

    #[test]
    fn test_reject_invalid_address() {
        let client = SmtpClient::new(&test_config()).unwrap();
        let mut req = base_request();
        req.to = vec!["pas-une-adresse".into()];
        assert!(client.build_message(&req).is_err());
    }
}
