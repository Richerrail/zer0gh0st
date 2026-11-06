use crate::agent::history::ChatMessage;
use crate::llm::llama::{ChatError, ChatOptions};
use crate::route::Endpoint;

pub fn estimate_tokens(messages: &[ChatMessage]) -> usize {
    let chars: usize = messages
        .iter()
        .filter_map(|m| m.content.as_ref())
        .map(|c| c.chars().count())
        .sum();
    chars / 4
}

#[derive(Debug, Clone)]
pub enum CompactOutcome {
    Compacted,
    Skipped,
    NoModel,
    Failed(String),
}

impl CompactOutcome {
    pub fn message(&self) -> String {
        match self {
            CompactOutcome::Compacted => "Historique compacté.".to_string(),
            CompactOutcome::Skipped => "Rien à compacter (conversation trop courte).".to_string(),
            CompactOutcome::NoModel => {
                "Aucun modèle pour le rôle `utility` (essaie `/role auto`).".to_string()
            }
            CompactOutcome::Failed(e) => format!("Compaction échouée: {e}"),
        }
    }
}

pub async fn compact_now(
    utility: Vec<Endpoint>,
    system: &str,
    messages: Vec<ChatMessage>,
    keep_recent: usize,
) -> (Vec<ChatMessage>, CompactOutcome) {
    if messages.len() <= keep_recent + 1 {
        return (messages, CompactOutcome::Skipped);
    }
    if utility.is_empty() {
        return (messages, CompactOutcome::NoModel);
    }

    let split = messages.len() - keep_recent;
    let recent: Vec<ChatMessage> = messages[split..].to_vec();

    let mut transcript = String::new();
    for m in &messages[..split] {
        let role = match m.role.as_str() {
            "user" => "Utilisateur",
            "assistant" => "Zer0",
            "tool" => "Outil",
            _ => "Système",
        };
        if let Some(c) = &m.content {
            transcript.push_str(&format!("{role}: {c}\n"));
        }
    }

    let prompt = format!(
        "Résume la conversation suivante de façon factuelle et compacte (max 200 mots). \
         Conserve les décisions, chemins de fichiers, préférences et faits importants. \
         Ne donne pas d'instructions, ne commente pas.\n\n{transcript}"
    );
    let req = vec![ChatMessage::user(prompt)];
    let options = ChatOptions {
        max_tokens: 512,
        thinking: false,
        temperature: 0.3,
        tools_mode: crate::llm::llama::ToolsMode::Native,
        stream: true,
        local: false,
    };

    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut last_err = String::new();

    for ep in utility.iter() {
        match ep
            .client()
            .stream_chat(system, &req, &[], &options, &tx)
            .await
        {
            Ok((summary, _, _)) if !summary.trim().is_empty() => {
                let summary = summary.trim().to_string();
                let mut out = Vec::with_capacity(recent.len() + 1);
                out.push(ChatMessage::assistant(
                    format!("[Résumé des échanges précédents]\n{summary}"),
                    Vec::new(),
                ));
                out.extend(recent);
                return (out, CompactOutcome::Compacted);
            }
            Ok(_) => {
                last_err = format!("réponse vide de {}", ep.label);
            }
            Err(ChatError::Setup { err, .. }) => {
                last_err = format!("{}: {err}", ep.label);
            }
            Err(ChatError::Mid(e)) => {
                last_err = format!("{}: {e}", ep.label);
            }
            Err(ChatError::Quota { detail }) => {
                last_err = format!("{}: quota épuisé — {detail}", ep.label);
                break;
            }
        }
    }

    (messages, CompactOutcome::Failed(last_err))
}
