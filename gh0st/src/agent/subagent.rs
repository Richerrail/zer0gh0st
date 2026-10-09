use crate::agent::history::ChatMessage;
use crate::agent::turn::run_turn;
use crate::event::UiEvent;
use crate::llm::llama::ChatOptions;
use crate::route::Endpoint;
use crate::tools::ToolSpec;

/// Run an isolated one-shot sub-agent and return its final text.
///
/// The sub-agent gets a fresh conversation (no shared history) and its own
/// event channel, so it never pollutes the main chat.
pub async fn run_subagent(
    endpoints: Vec<Endpoint>,
    system: String,
    task: String,
    tools: Vec<ToolSpec>,
    options: ChatOptions,
) -> String {
    if endpoints.is_empty() {
        return "Aucun endpoint disponible pour ce rôle.".to_string();
    }

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<UiEvent>();
    let messages = vec![ChatMessage::user(task)];
    tokio::spawn(async move {
        run_turn(endpoints, system, messages, tools, options, tx).await;
    });

    let mut result = String::new();
    while let Some(ev) = rx.recv().await {
        match ev {
            UiEvent::TurnComplete(full) => {
                result = full
                    .iter()
                    .rev()
                    .find(|m| m.role == "assistant")
                    .and_then(|m| m.content.clone())
                    .unwrap_or_default();
                break;
            }
            UiEvent::Error(e) => {
                result = format!("⚠ {e}");
                break;
            }
            _ => {}
        }
    }
    if result.trim().is_empty() {
        result = "(aucune réponse)".to_string();
    }
    result
}
