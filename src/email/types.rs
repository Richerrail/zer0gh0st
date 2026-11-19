//! Structures de données principales du module email.

use serde::{Deserialize, Serialize};

/// Résumé d'un message (listage, recherche).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailSummary {
    pub uid: u32,
    pub mailbox: String,
    pub subject: Option<String>,
    pub from: Option<String>,
    pub to: Vec<String>,
    pub date: Option<String>,
    pub flags: Vec<String>,
    pub snippet: Option<String>,
}

/// Message complet avec corps et métadonnées des pièces jointes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMessage {
    pub summary: EmailSummary,
    pub headers: Vec<(String, String)>,
    pub text_body: Option<String>,
    pub html_body: Option<String>,
    pub attachments: Vec<AttachmentMeta>,
}

/// Métadonnées d'une pièce jointe (sans le contenu binaire par défaut).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentMeta {
    pub filename: Option<String>,
    pub mime_type: Option<String>,
    pub size: Option<usize>,
}

/// Requête d'envoi d'email.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendRequest {
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub text_body: Option<String>,
    pub html_body: Option<String>,
    pub attachments: Vec<AttachmentInput>,
}

/// Pièce jointe à envoyer (contenu binaire inclus).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentInput {
    pub filename: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

/// Actions exposées à l'agent via JSON.
#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum EmailAction {
    ListFolders,
    ListMessages {
        mailbox: String,
        limit: Option<u32>,
        offset: Option<u32>,
    },
    GetMessage {
        mailbox: String,
        uid: u32,
    },
    Search {
        mailbox: String,
        query: String,
        limit: Option<u32>,
    },
    Send {
        request: SendRequest,
    },
    Reply {
        mailbox: String,
        uid: u32,
        body: String,
    },
    Forward {
        mailbox: String,
        uid: u32,
        to: Vec<String>,
        body: Option<String>,
    },
    MarkRead {
        mailbox: String,
        uid: u32,
        read: bool,
    },
    Move {
        mailbox: String,
        uid: u32,
        target_mailbox: String,
    },
    Delete {
        mailbox: String,
        uid: u32,
        expunge: bool,
    },
}
