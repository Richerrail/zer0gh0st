//! Parsing MIME et extraction du corps / pièces jointes.
//! Utilise `mail-parser` pour extraire texte, HTML et métadonnées des pièces jointes.

use crate::email::error::EmailError;
use crate::email::types::{AttachmentMeta, EmailMessage, EmailSummary};
use mail_parser::{Message as MailMessage, MessageParser, MimeHeaders};

/// Parse un email brut (octets) et retourne une `EmailMessage`.
pub fn parse_email(raw: &[u8]) -> Result<EmailMessage, EmailError> {
    let parsed = MessageParser::default()
        .parse(raw)
        .ok_or_else(|| EmailError::Parse("Impossible de parser l'email".to_string()))?;

    let summary = extract_summary(&parsed);
    let headers = extract_headers(&parsed);
    let text_body = extract_text_body(&parsed);
    let html_body = extract_html_body(&parsed);
    let attachments = extract_attachments(&parsed);

    Ok(EmailMessage {
        summary,
        headers,
        text_body,
        html_body,
        attachments,
    })
}

fn extract_summary(msg: &MailMessage) -> EmailSummary {
    let uid = 0; // UID assigné par l'IMAP, pas par le parser
    let mailbox = String::new(); // Assigné par l'IMAP
    let subject = msg.subject().map(|s| s.to_string());
    let from = msg
        .from()
        .and_then(|addr| addr.first())
        .and_then(|a| a.address.as_ref().map(|s| s.to_string()));
    let to = msg
        .to()
        .map(|addrs| {
            addrs
                .iter()
                .filter_map(|a| a.address.as_ref().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let date = msg.date().map(|d| d.to_string());
    let flags = Vec::new(); // Flags assignés par l'IMAP
    let snippet = msg.body_text(0).map(|s| s.chars().take(200).collect());

    EmailSummary {
        uid,
        mailbox,
        subject,
        from,
        to,
        date,
        flags,
        snippet,
    }
}

fn extract_headers(msg: &MailMessage) -> Vec<(String, String)> {
    msg.headers()
        .iter()
        .filter_map(|h| {
            let name = match &h.name {
                mail_parser::HeaderName::Subject => "Subject".to_string(),
                mail_parser::HeaderName::From => "From".to_string(),
                mail_parser::HeaderName::To => "To".to_string(),
                mail_parser::HeaderName::Cc => "Cc".to_string(),
                mail_parser::HeaderName::Date => "Date".to_string(),
                mail_parser::HeaderName::MessageId => "Message-ID".to_string(),
                mail_parser::HeaderName::Other(s) => s.to_string(),
                _ => format!("{:?}", h.name),
            };
            let value = match &h.value {
                mail_parser::HeaderValue::Text(s) => s.to_string(),
                mail_parser::HeaderValue::Address(mail_parser::Address::List(addrs)) => addrs
                    .iter()
                    .filter_map(|a| a.address.as_ref().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
                    .join(", "),
                mail_parser::HeaderValue::DateTime(dt) => dt.to_string(),
                mail_parser::HeaderValue::ContentType(ct) => ct.c_type.to_string(),
                _ => String::new(),
            };
            if name.is_empty() && value.is_empty() {
                None
            } else {
                Some((name, value))
            }
        })
        .collect()
}

fn extract_text_body(msg: &MailMessage) -> Option<String> {
    msg.body_text(0).map(|s| s.to_string())
}

fn extract_html_body(msg: &MailMessage) -> Option<String> {
    msg.body_html(0).map(|s| s.to_string())
}

fn extract_attachments(msg: &MailMessage) -> Vec<AttachmentMeta> {
    msg.attachments()
        .filter(|att| att.attachment_name().is_some())
        .map(|att| AttachmentMeta {
            filename: att.attachment_name().map(|s| s.to_string()),
            mime_type: att.content_type().map(|ct| ct.c_type.to_string()),
            size: Some(att.contents().len()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_EMAIL: &[u8] = b"From: alice@example.com\r\n\
        To: bob@example.com\r\n\
        Subject: Test Simple\r\n\
        Date: Mon, 1 Jan 2024 12:00:00 +0000\r\n\
        Content-Type: text/plain; charset=utf-8\r\n\
        \r\n\
        Bonjour Bob,\r\n\
        Ceci est un test.\r\n\
        Cordialement,\r\n\
        Alice";

    const HTML_EMAIL: &[u8] = b"From: carol@example.com\r\n\
        To: dave@example.com\r\n\
        Subject: Test HTML\r\n\
        Date: Tue, 2 Jan 2024 14:30:00 +0000\r\n\
        MIME-Version: 1.0\r\n\
        Content-Type: multipart/alternative; boundary=\"boundary123\"\r\n\
        \r\n\
        --boundary123\r\n\
        Content-Type: text/plain; charset=utf-8\r\n\
        \r\n\
        Version texte.\r\n\
        --boundary123\r\n\
        Content-Type: text/html; charset=utf-8\r\n\
        \r\n\
        <p>Version <b>HTML</b>.</p>\r\n\
        --boundary123--";

    const ATTACHMENT_EMAIL: &[u8] = b"From: eve@example.com\r\n\
        To: frank@example.com\r\n\
        Subject: Test Attachment\r\n\
        Date: Wed, 3 Jan 2024 09:15:00 +0000\r\n\
        MIME-Version: 1.0\r\n\
        Content-Type: multipart/mixed; boundary=\"attboundary\"\r\n\
        \r\n\
        --attboundary\r\n\
        Content-Type: text/plain; charset=utf-8\r\n\
        \r\n\
        Voir piece jointe.\r\n\
        --attboundary\r\n\
        Content-Type: application/octet-stream\r\n\
        Content-Disposition: attachment; filename=\"test.bin\"\r\n\
        \r\n\
        \x00\x01\x02\x03\x04\r\n\
        --attboundary--";

    #[test]
    fn test_parse_simple_email() {
        let msg = parse_email(SIMPLE_EMAIL).unwrap();
        assert_eq!(msg.summary.subject, Some("Test Simple".to_string()));
        assert_eq!(msg.summary.from, Some("alice@example.com".to_string()));
        assert_eq!(msg.summary.to, vec!["bob@example.com".to_string()]);
        assert!(msg.text_body.is_some());
        assert!(msg.text_body.unwrap().contains("Bonjour Bob"));
        // mail-parser peut retourner du HTML dérivé du texte
        assert!(msg.attachments.is_empty());
    }

    #[test]
    fn test_parse_html_email() {
        let msg = parse_email(HTML_EMAIL).unwrap();
        assert_eq!(msg.summary.subject, Some("Test HTML".to_string()));
        assert!(msg.text_body.is_some());
        assert!(msg.html_body.is_some());
        assert!(msg.html_body.unwrap().contains("<b>HTML</b>"));
    }

    #[test]
    fn test_parse_attachment_email() {
        let msg = parse_email(ATTACHMENT_EMAIL).unwrap();
        assert_eq!(msg.summary.subject, Some("Test Attachment".to_string()));
        assert_eq!(msg.attachments.len(), 1);
        let att = &msg.attachments[0];
        assert_eq!(att.filename, Some("test.bin".to_string()));
        // mail-parser peut retourner le type MIME complet ou juste le type
        assert!(att.mime_type.as_ref().unwrap().contains("application"));
        assert!(att.size.unwrap() > 0);
    }

    #[test]
    fn test_parse_invalid_email() {
        let result = parse_email(b"not an email");
        assert!(result.is_ok()); // mail-parser est tolérant
    }
}
