//! Outil email : expose les actions IMAP/SMTP à l'agent via JSON.
//! Les permissions et l'audit sont gérés par `EmailService`.

use serde_json::json;

use super::ToolSpec;
use crate::email::service::EmailService;
use crate::email::types::EmailAction;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "email",
        description: "Read, search, send and manage emails via IMAP/SMTP. \
                      Actions: list_folders, list_messages, get_message, search, send, \
                      reply, forward, mark_read, move, delete. \
                      Requires ZER0_EMAIL_* environment variables and permissions.",
        parameters: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "One of: list_folders, list_messages, get_message, search, send, reply, forward, mark_read, move, delete",
                    "enum": ["list_folders", "list_messages", "get_message", "search",
                             "send", "reply", "forward", "mark_read", "move", "delete"]
                },
                "mailbox": { "type": "string", "description": "IMAP mailbox (e.g. INBOX)." },
                "uid": { "type": "integer", "description": "Message UID." },
                "limit": { "type": "integer", "description": "Max messages (default 20)." },
                "offset": { "type": "integer", "description": "Pagination offset (default 0)." },
                "query": { "type": "string", "description": "IMAP search query (e.g. FROM \"x\", UNSEEN)." },
                "to": { "type": "array", "items": { "type": "string" }, "description": "Recipients (send)." },
                "cc": { "type": "array", "items": { "type": "string" } },
                "bcc": { "type": "array", "items": { "type": "string" } },
                "subject": { "type": "string", "description": "Subject (send)." },
                "text_body": { "type": "string", "description": "Plain text body (send)." },
                "html_body": { "type": "string", "description": "HTML body (send)." },
                "attachments": {
                    "type": "array",
                    "description": "Attachments: [{filename, mime_type, bytes_base64}]",
                    "items": {
                        "type": "object",
                        "properties": {
                            "filename": { "type": "string" },
                            "mime_type": { "type": "string" },
                            "bytes_base64": { "type": "string" }
                        },
                        "required": ["filename", "mime_type", "bytes_base64"]
                    }
                },
                "body": { "type": "string", "description": "Body for reply/forward." },
                "target_mailbox": { "type": "string", "description": "Destination mailbox (move)." },
                "read": { "type": "boolean", "description": "Mark read (true) or unread (false)." },
                "expunge": { "type": "boolean", "description": "Permanently delete (delete only). Default false." }
            },
            "required": ["action"]
        }),
    }
}

/// Décodage base64 des pièces jointes.
fn decode_attachments(
    v: &serde_json::Value,
) -> Result<Vec<crate::email::types::AttachmentInput>, String> {
    use base64::Engine;
    let Some(arr) = v.get("attachments").and_then(|x| x.as_array()) else {
        return Ok(vec![]);
    };
    let mut out = Vec::new();
    for att in arr {
        let filename = att
            .get("filename")
            .and_then(|x| x.as_str())
            .ok_or("attachment: filename manquant")?
            .to_string();
        let mime_type = att
            .get("mime_type")
            .and_then(|x| x.as_str())
            .ok_or("attachment: mime_type manquant")?
            .to_string();
        let b64 = att
            .get("bytes_base64")
            .and_then(|x| x.as_str())
            .ok_or("attachment: bytes_base64 manquant")?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| format!("attachment: base64 invalide ({e})"))?;
        out.push(crate::email::types::AttachmentInput {
            filename,
            mime_type,
            bytes,
        });
    }
    Ok(out)
}

/// Construit un EmailAction depuis les arguments JSON de l'agent.
fn parse_action(v: &serde_json::Value) -> Result<EmailAction, String> {
    let action = v
        .get("action")
        .and_then(|x| x.as_str())
        .ok_or("champ `action` manquant")?;

    let mailbox = || {
        v.get("mailbox")
            .and_then(|x| x.as_str())
            .unwrap_or("INBOX")
            .to_string()
    };
    let uid = || v.get("uid").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
    let opt_limit = || v.get("limit").and_then(|x| x.as_u64()).map(|x| x as u32);
    let str_list = |key: &str| -> Vec<String> {
        v.get(key)
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    };

    match action {
        "list_folders" => Ok(EmailAction::ListFolders),
        "list_messages" => Ok(EmailAction::ListMessages {
            mailbox: mailbox(),
            limit: opt_limit(),
            offset: v.get("offset").and_then(|x| x.as_u64()).map(|x| x as u32),
        }),
        "get_message" => Ok(EmailAction::GetMessage {
            mailbox: mailbox(),
            uid: uid(),
        }),
        "search" => Ok(EmailAction::Search {
            mailbox: mailbox(),
            query: v
                .get("query")
                .and_then(|x| x.as_str())
                .unwrap_or("ALL")
                .to_string(),
            limit: opt_limit(),
        }),
        "send" => {
            let attachments = decode_attachments(v).map_err(|e| e.to_string())?;
            Ok(EmailAction::Send {
                request: crate::email::types::SendRequest {
                    to: str_list("to"),
                    cc: str_list("cc"),
                    bcc: str_list("bcc"),
                    subject: v
                        .get("subject")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    text_body: v
                        .get("text_body")
                        .and_then(|x| x.as_str())
                        .map(String::from),
                    html_body: v
                        .get("html_body")
                        .and_then(|x| x.as_str())
                        .map(String::from),
                    attachments,
                },
            })
        }
        "reply" => Ok(EmailAction::Reply {
            mailbox: mailbox(),
            uid: uid(),
            body: v
                .get("body")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
        }),
        "forward" => Ok(EmailAction::Forward {
            mailbox: mailbox(),
            uid: uid(),
            to: str_list("to"),
            body: v.get("body").and_then(|x| x.as_str()).map(String::from),
        }),
        "mark_read" => Ok(EmailAction::MarkRead {
            mailbox: mailbox(),
            uid: uid(),
            read: v.get("read").and_then(|x| x.as_bool()).unwrap_or(true),
        }),
        "move" => Ok(EmailAction::Move {
            mailbox: mailbox(),
            uid: uid(),
            target_mailbox: v
                .get("target_mailbox")
                .and_then(|x| x.as_str())
                .unwrap_or("Archive")
                .to_string(),
        }),
        "delete" => Ok(EmailAction::Delete {
            mailbox: mailbox(),
            uid: uid(),
            expunge: v.get("expunge").and_then(|x| x.as_bool()).unwrap_or(false),
        }),
        other => Err(format!("action inconnue `{other}`")),
    }
}

pub async fn run(args: &str) -> String {
    let v: serde_json::Value = match serde_json::from_str(args) {
        Ok(v) => v,
        Err(e) => return format!("Erreur: arguments JSON invalides ({e})"),
    };

    let action = match parse_action(&v) {
        Ok(a) => a,
        Err(e) => return format!("Erreur: {e}"),
    };

    let config = match crate::email::config::EmailConfig::from_env() {
        Ok(c) => c,
        Err(e) => return format!("Erreur: configuration email absente ({e})"),
    };

    let service = match EmailService::new(config) {
        Ok(s) => s,
        Err(e) => return format!("Erreur: service email indisponible ({e})"),
    };

    match service.handle_action(action).await {
        Ok(result) => result.to_string(),
        Err(e) => format!("Erreur: {e}"),
    }
}
