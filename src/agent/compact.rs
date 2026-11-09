use crate::agent::history::ChatMessage;
use crate::llm::llama::{ChatError, ChatOptions};
use crate::route::Endpoint;

/// Rough token estimate (chars / 4).
pub fn estimate_tokens(messages: &[ChatMessage]) -> usize {
    let chars: usize = messages
        .iter()
        .filter_map(|m| m.content.as_ref())
        .map(|c| c.chars().count())
        .sum();
    chars / 4
}

/// Seuil de compaction pour une fenêtre de contexte donnée.
///
/// 60 % de la fenêtre **réelle** du modèle, avec un plancher de 4 000 tokens.
/// Avant, ce calcul était dupliqué (TUI ×2) et `telegram.rs` utilisait 4 000
/// en dur : sur un modèle 200 k il compactait 50× trop tôt, sur un 8 k trop
/// tard. Une seule définition, un seul comportement.
pub fn budget_for(context_window: u64) -> usize {
    ((context_window as f64 * 0.6) as usize).max(4_000)
}

/// Why a compaction did or did not happen.
#[derive(Debug, Clone)]
pub enum CompactOutcome {
    Compacted,
    /// Not enough messages to bother.
    Skipped,
    /// No utility endpoint available.
    NoModel,
    Failed(String),
}

impl CompactOutcome {
    /// Human-readable message for the UI.
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

/// Summarize the older part of the conversation, keeping `keep_recent` messages.
///
/// Tries each endpoint of the utility chain in order (failover).
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

    // Quiet channel: compaction tokens never reach the chat.
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
            // Quota de compte : passer au modèle suivant ne changerait rien.
            Err(ChatError::Quota { detail }) => {
                last_err = format!("{}: quota épuisé — {detail}", ep.label);
                break;
            }
        }
    }

    (messages, CompactOutcome::Failed(last_err))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// B6 : seuil unique = 60 % de la fenêtre, plancher 4 000.
    #[test]
    fn budget_for_follows_the_window_with_a_floor() {
        // Fenêtre inconnue (0) : plancher.
        assert_eq!(budget_for(0), 4_000);
        // Petite fenêtre : le plancher domine.
        assert_eq!(budget_for(4_000), 4_000);
        assert_eq!(budget_for(6_000), 4_000);
        // 8 k -> 4 800 (au-dessus du plancher).
        assert_eq!(budget_for(8_000), 4_800);
        // 128 k -> 76 800.
        assert_eq!(budget_for(128_000), 76_800);
        // 200 k -> 120 000 : c'est le cas que le 4 000 en dur de Telegram
        // compactait 30× trop tôt.
        assert_eq!(budget_for(200_000), 120_000);
        assert_eq!(budget_for(1_000_000), 600_000);
    }

    /// B6 : le seuil doit croître avec la fenêtre, jamais régresser.
    #[test]
    fn budget_for_is_monotonic() {
        let windows = [
            0u64, 1_000, 4_000, 8_000, 32_000, 128_000, 200_000, 1_000_000,
        ];
        let budgets: Vec<usize> = windows.iter().map(|w| budget_for(*w)).collect();
        for pair in budgets.windows(2) {
            assert!(pair[1] >= pair[0], "seuil non monotone : {budgets:?}");
        }
    }
}
