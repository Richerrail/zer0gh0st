use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

use crate::agent::history::ChatMessage;
use crate::event::UiEvent;
use crate::llm::llama::{ApiClient, ChatError, ChatOptions};
use crate::route::{self, Endpoint};
use crate::tools::{self, ToolSpec};

const MAX_ITERATIONS: usize = 8;
const MAX_429_RETRIES: u32 = 2;
/// Hard ceiling on tool calls within a single turn.
const MAX_TOOL_CALLS: usize = 24;

/// Run one user turn over an ordered endpoint chain.
///
/// The first model call fails over across endpoints (setup failures only),
/// with exponential backoff on HTTP 429. Once an endpoint answers, the rest
/// of the turn sticks to it.
pub async fn run_turn(
    endpoints: Vec<Endpoint>,
    system: String,
    mut messages: Vec<ChatMessage>,
    tools: Vec<ToolSpec>,
    options: ChatOptions,
    ui: UnboundedSender<UiEvent>,
) {
    if endpoints.is_empty() {
        let _ = ui.send(UiEvent::Error("aucun endpoint configuré".into()));
        return;
    }

    crate::cancel::clear();

    // --- first call, with failover across the ordered chain ---
    let mut chosen: Option<(ApiClient, String, Vec<crate::agent::history::ToolCall>)> = None;
    let mut last_err = String::new();

    'endpoints: for ep in endpoints.iter() {
        if route::is_failed(&ep.label) {
            let _ = ui.send(UiEvent::Info(format!(
                "⏭ {} ignoré (échec récent)",
                ep.label
            )));
            continue;
        }

        let client = ep.client();
        let mut attempt: u32 = 0;

        loop {
            match client.stream_chat(&system, &messages, &tools, &options, &ui).await {
                Ok((content, calls)) if content.trim().is_empty() && calls.is_empty() => {
                    // Model produced nothing usable (e.g. reasoning-only) → fail over.
                    route::mark_failed(&ep.label);
                    let _ = ui.send(UiEvent::Info(format!(
                        "⚠ {} a renvoyé une réponse vide → endpoint suivant",
                        ep.label
                    )));
                    last_err = format!("réponse vide de {}", ep.label);
                    continue 'endpoints;
                }
                Ok((content, calls)) => {
                    route::clear_failed(&ep.label);
                    let _ = ui.send(UiEvent::Active(ep.label.clone()));
                    chosen = Some((client.clone(), content, calls));
                    break 'endpoints;
                }
                Err(ChatError::Setup { err, status }) => {
                    // rate limit → back off and retry the same endpoint
                    if status == Some(429) && attempt < MAX_429_RETRIES {
                        attempt += 1;
                        let wait = 2u64.pow(attempt);
                        let _ = ui.send(UiEvent::Info(format!(
                            "⏳ {} limité (429), nouvel essai dans {wait}s…",
                            ep.label
                        )));
                        tokio::time::sleep(Duration::from_secs(wait)).await;
                        continue;
                    }
                    route::mark_failed(&ep.label);
                    let hint = match status {
                        Some(401) => " — clé API invalide ou absente",
                        Some(402) => " — crédit épuisé",
                        Some(403) => " — accès/modèle refusé par le provider",
                        Some(404) => " — modèle ou endpoint introuvable",
                        _ => "",
                    };
                    let _ = ui.send(UiEvent::Info(format!(
                        "⚠ {} indisponible{hint} ({err}) → endpoint suivant",
                        ep.label
                    )));
                    last_err = format!("{err}");
                    continue 'endpoints;
                }
                Err(ChatError::Mid(e)) => {
                    let _ = ui.send(UiEvent::Error(format!(
                        "erreur en cours de flux ({}): {e}",
                        ep.label
                    )));
                    return;
                }
            }
        }
    }

    let Some((client, mut content, mut calls)) = chosen else {
        let _ = ui.send(UiEvent::Error(format!(
            "tous les endpoints ont échoué. dernier: {last_err}"
        )));
        return;
    };

    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut total_calls: usize = 0;

    for _ in 0..MAX_ITERATIONS {
        let assistant = ChatMessage::assistant(content.clone(), calls.clone());
        messages.push(assistant.clone());

        if calls.is_empty() {
            break;
        }

        for call in &calls {
            if crate::cancel::is_cancelled() {
                let _ = ui.send(UiEvent::Error("⏹ interrompu par /stop".into()));
                return;
            }
            // Loop containment: repeated identical calls or a global budget.
            total_calls += 1;
            let fp = format!("{}|{}", call.function.name, call.function.arguments);
            let n = seen.entry(fp).or_insert(0);
            *n += 1;
            if *n >= 3 {
                let _ = ui.send(UiEvent::Error(format!(
                    "boucle détectée : `{}` appelé {} fois avec les mêmes arguments → arrêt",
                    call.function.name, *n
                )));
                return;
            }
            if total_calls > MAX_TOOL_CALLS {
                let _ = ui.send(UiEvent::Error(format!(
                    "budget d'appels d'outils dépassé ({MAX_TOOL_CALLS}) → arrêt"
                )));
                return;
            }

            let _ = ui.send(UiEvent::ToolCall {
                name: call.function.name.clone(),
                args: call.function.arguments.clone(),
            });
            let result = tools::execute(&call.function.name, &call.function.arguments).await;
            let _ = ui.send(UiEvent::ToolResult(result.clone()));
            let tool_msg = ChatMessage::tool_result(call.id.clone(), result);
            messages.push(tool_msg);
        }

        // subsequent calls stay on the chosen endpoint
        match client.stream_chat(&system, &messages, &tools, &options, &ui).await {
            Ok((c, cs)) => {
                content = c;
                calls = cs;
            }
            Err(e) => {
                let _ = ui.send(UiEvent::Error(format!("{e}")));
                return;
            }
        }
    }

    let _ = ui.send(UiEvent::TurnComplete(messages));
}
