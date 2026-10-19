use crate::agent::history::ChatMessage;
use crate::agent::turn::run_turn;
use crate::event::UiEvent;
use crate::llm::llama::ChatOptions;
use crate::route::Endpoint;
use crate::tools::ToolSpec;
use tokio::sync::mpsc::UnboundedSender;

/// Events that a sub-agent can forward to the parent (for visibility).
#[derive(Debug, Clone)]
pub enum SubAgentEvent {
    /// The sub-agent switched to a different endpoint (failover).
    Fallback { from: String, to: String, reason: String },
    /// A tool was called.
    ToolCall { name: String, args: String },
    /// An error occurred (non-fatal).
    Error(String),
    /// Progress info.
    Info(String),
}

/// Run an isolated one-shot sub-agent and return its final text.
///
/// The sub-agent gets a fresh conversation (no shared history) and its own
/// event channel. Optionally forwards fallback/progress events via `event_tx`.
pub async fn run_subagent(
    endpoints: Vec<Endpoint>,
    system: String,
    task: String,
    tools: Vec<ToolSpec>,
    options: ChatOptions,
    event_tx: Option<UnboundedSender<SubAgentEvent>>,
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
    let mut last_endpoint: Option<String> = None;
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
                if let Some(tx) = &event_tx {
                    let _ = tx.send(SubAgentEvent::Error(e));
                }
                break;
            }
            UiEvent::Active(label) => {
                // Endpoint changed (failover) — forward to parent for visibility.
                if let Some(prev) = last_endpoint.replace(label.clone()) {
                    if prev != label {
                        if let Some(tx) = &event_tx {
                            let _ = tx.send(SubAgentEvent::Fallback {
                                from: prev,
                                to: label.clone(),
                                reason: "failover (rate limit, error, or empty response)".into(),
                            });
                        }
                    }
                } else {
                    last_endpoint = Some(label);
                }
            }
            UiEvent::ToolCall { name, args } => {
                if let Some(tx) = &event_tx {
                    let _ = tx.send(SubAgentEvent::ToolCall { name, args });
                }
            }
            UiEvent::Info(info) => {
                if let Some(tx) = &event_tx {
                    let _ = tx.send(SubAgentEvent::Info(info));
                }
            }
            _ => {}
        }
    }
    if result.trim().is_empty() {
        result = "(aucune réponse)".to_string();
    }
    result
}

/// Évalue la qualité d'un code via le rôle `review`.
///
/// Ne **tronque pas** : envoie tout le fichier. Au-delà de `MAX_WHOLE` caractères,
/// il découpe en parties (`CHUNK`) puis agrège les scores (moyenne).
/// Le découpage se fait **par caractères** (jamais par octets → pas de panic UTF-8).
pub async fn run_jev(
    endpoints: Vec<Endpoint>,
    system: String,
    path: &str,
    content: &str,
    options: ChatOptions,
    event_tx: Option<UnboundedSender<SubAgentEvent>>,
) -> String {
    const MAX_WHOLE: usize = 100_000;
    const CHUNK: usize = 60_000;

    let chars: Vec<char> = content.chars().collect();
    let total = chars.len();

    if total <= MAX_WHOLE {
        let task = jev_task(path, content, None);
        return run_subagent(endpoints, system, task, Vec::new(), options, event_tx).await;
    }

    let n = total.div_ceil(CHUNK);
    let mut out = String::new();
    let mut scores: Vec<f64> = Vec::new();
    for (i, part) in chars.chunks(CHUNK).enumerate() {
        let piece: String = part.iter().collect();
        let task = jev_task(path, &piece, Some((i + 1, n)));
        let res = run_subagent(
            endpoints.clone(),
            system.clone(),
            task,
            Vec::new(),
            options.clone(),
            event_tx.clone(),
        )
        .await;
        if let Some(s) = parse_score(&res) {
            scores.push(s);
        }
        out.push_str(&format!("— partie {}/{n} —\n{res}\n\n", i + 1));
    }
    if !scores.is_empty() {
        let avg = scores.iter().sum::<f64>() / scores.len() as f64;
        out.push_str(&format!(
            "SCORE GLOBAL (moyenne de {} parties) : {avg:.1}/3",
            scores.len()
        ));
    }
    out
}

fn jev_task(path: &str, code: &str, part: Option<(usize, usize)>) -> String {
    let ctx = match part {
        Some((i, n)) => format!(" (partie {i}/{n})"),
        None => String::new(),
    };
    format!(
        "Évalue la qualité de ce code sur 0-3 (0 faible, 1 moyen, 2 bon, 3 excellent). \
         Réponds EXACTEMENT :\nSCORE: <n>\nJUSTIFICATION: <2 phrases>\n\nFichier {path}{ctx}:\n\n{code}"
    )
}

fn parse_score(res: &str) -> Option<f64> {
    res.lines().find_map(|l| {
        let l = l.trim().strip_prefix("SCORE:")?.trim();
        let num: String = l
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
            .collect();
        num.replace(',', ".").parse::<f64>().ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_score_variants() {
        assert_eq!(parse_score("SCORE: 2\nJUSTIFICATION: ok"), Some(2.0));
        assert_eq!(parse_score("  SCORE: 3  "), Some(3.0));
        assert_eq!(parse_score("SCORE: 1,5"), Some(1.5));
        assert_eq!(parse_score("pas de score ici"), None);
    }
}
