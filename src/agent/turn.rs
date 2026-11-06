use tokio::sync::mpsc::UnboundedSender;

use crate::agent::history::ChatMessage;
use crate::event::UiEvent;
use crate::llm::llama::{ApiClient, ChatError, ChatOptions, ToolsMode};
use crate::route::{self, Endpoint};
use crate::tools::{self, ToolSpec};

const MAX_ITERATIONS: usize = 8;
const MAX_TOOL_CALLS: usize = 24;
const MAX_CONTINUATIONS: u32 = 4;

fn should_failover(status: Option<u16>, err: &str) -> bool {
    match status {
        None => true,
        Some(400 | 422) => {
            let e = err.to_ascii_lowercase();
            e.contains("tool") || e.contains("function") || e.contains("not support")
        }
        Some(408 | 409 | 425 | 429) => true,
        Some(s) if (500..=599).contains(&s) => true,
        Some(_) => true,
    }
}

fn failure_message(last_err: &str, skipped: &[String]) -> String {
    if !last_err.trim().is_empty() {
        return format!("tous les endpoints ont échoué — dernier : {last_err}");
    }
    if skipped.is_empty() {
        return "aucun endpoint n'a répondu (chaîne vide ?)".to_string();
    }
    format!(
        "aucun endpoint disponible : {} marqué(s) en échec récent (nouvel essai dans ≤ 120 s). \
         Vérifie que le serveur local tourne (`scripts/serve.sh`) ou change de provider.",
        skipped.join(", ")
    )
}

pub async fn run_turn(
    endpoints: Vec<Endpoint>,
    system: String,
    messages: Vec<ChatMessage>,
    tools: Vec<ToolSpec>,
    options: ChatOptions,
    ui: UnboundedSender<UiEvent>,
) {
    run_turn_budgeted(
        endpoints,
        system,
        messages,
        tools,
        options,
        ui,
        MAX_ITERATIONS,
        MAX_TOOL_CALLS,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn run_turn_budgeted(
    endpoints: Vec<Endpoint>,
    system: String,
    mut messages: Vec<ChatMessage>,
    tools: Vec<ToolSpec>,
    options: ChatOptions,
    ui: UnboundedSender<UiEvent>,
    max_iterations: usize,
    max_tool_calls: usize,
) {
    if endpoints.is_empty() {
        let _ = ui.send(UiEvent::Error("aucun endpoint configuré".into()));
        return;
    }

    let free_chain = endpoints.iter().any(|e| e.model.contains(":free"));
    let max_iterations = if free_chain {
        max_iterations.min(4)
    } else {
        max_iterations
    };

    crate::cancel::clear();
    let _budget = crate::budget::TurnGuard::enter();

    let mut chosen: Option<(
        ApiClient,
        String,
        Vec<crate::agent::history::ToolCall>,
        Option<String>,
    )> = None;
    let mut last_err = String::new();
    let mut skipped: Vec<String> = Vec::new();

    'endpoints: for ep in endpoints.iter() {
        if route::is_failed(&ep.label) {
            skipped.push(ep.label.clone());
            let _ = ui.send(UiEvent::Info(format!(
                "⏭ {} ignoré (échec récent)",
                ep.label
            )));
            continue;
        }

        let client = ep.client();

        match client
            .stream_chat(&system, &messages, &tools, &options, &ui)
            .await
        {
            Ok((content, calls, _)) if content.trim().is_empty() && calls.is_empty() => {
                route::mark_failed(&ep.label);
                let _ = ui.send(UiEvent::Info(format!(
                    "⚠ {} a renvoyé une réponse vide → endpoint suivant",
                    ep.label
                )));
                last_err = format!("réponse vide de {}", ep.label);
                continue 'endpoints;
            }
            Ok((content, calls, finish)) => {
                route::clear_failed(&ep.label);
                let _ = ui.send(UiEvent::Active(ep.label.clone()));
                chosen = Some((client.clone(), content, calls, finish));
                break 'endpoints;
            }
            Err(ChatError::Setup { err, status }) => {
                if status == Some(429) && ep.model.contains(":free") {
                    route::mark_failed(&ep.label);
                    let _ = ui.send(UiEvent::Error(format!(
                        "limite atteinte sur {} ({err})\n\
                         Les modèles `:free` partagent le quota du compte : pas de bascule.",
                        ep.label
                    )));
                    return;
                }
                route::mark_failed(&ep.label);
                let hint = match status {
                    Some(401) => " — clé API invalide ou absente",
                    Some(402) => " — crédit épuisé",
                    Some(403) => " — accès/modèle refusé par le provider",
                    Some(404) => " — modèle ou endpoint introuvable",
                    _ => "",
                };
                if !should_failover(status, &err.to_string()) {
                    let _ = ui.send(UiEvent::Error(format!(
                        "{} : erreur déterministe{hint} ({err}) — pas de bascule",
                        ep.label
                    )));
                    return;
                }
                let _ = ui.send(UiEvent::Info(format!(
                    "⚠ {} indisponible{hint} ({err}) → endpoint suivant",
                    ep.label
                )));
                last_err = format!("{err}");
                continue 'endpoints;
            }
            Err(ChatError::Quota { detail }) => {
                route::mark_failed(&ep.label);
                let _ = ui.send(UiEvent::Error(format!(
                    "quota épuisé sur {} — {detail}\n\
                     Attends la réinitialisation, ajoute du crédit chez le provider, \
                     ou passe en local (`/provider use local`).",
                    ep.label
                )));
                return;
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

    let Some((client, mut content, mut calls, mut finish)) = chosen else {
        let _ = ui.send(UiEvent::Error(failure_message(&last_err, &skipped)));
        return;
    };

    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut total_calls: usize = 0;
    let mut continuations: u32 = 0;

    for _ in 0..max_iterations {
        let assistant = if options.tools_mode == ToolsMode::Prompt && !calls.is_empty() {
            let mut text = content.clone();
            for c in &calls {
                text.push_str(&format!(
                    "\n<tool_call>{{\"name\":\"{}\",\"arguments\":{}}}</tool_call>",
                    c.function.name, c.function.arguments
                ));
            }
            ChatMessage::assistant(text, Vec::new())
        } else {
            ChatMessage::assistant(content.clone(), calls.clone())
        };
        messages.push(assistant.clone());

        if calls.is_empty() {
            if finish.as_deref() == Some("length") && continuations < MAX_CONTINUATIONS {
                continuations += 1;
                messages.push(ChatMessage::user(
                    "Continue EXACTEMENT là où tu t'es arrêté, sans répéter ce qui précède.",
                ));
                let _ = ui.send(UiEvent::Info(format!(
                    "↪ réponse tronquée → reprise automatique ({continuations}/{MAX_CONTINUATIONS})"
                )));
                match client
                    .stream_chat(&system, &messages, &tools, &options, &ui)
                    .await
                {
                    Ok((c, cs, f)) => {
                        content = c;
                        calls = cs;
                        finish = f;
                        continue;
                    }
                    Err(e) => {
                        let _ = ui.send(UiEvent::Error(format!("{e}")));
                        return;
                    }
                }
            }
            break;
        }

        for call in &calls {
            if crate::cancel::is_cancelled() {
                let _ = ui.send(UiEvent::Error("⏹ interrompu par /stop".into()));
                return;
            }
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
            if total_calls > max_tool_calls {
                let _ = ui.send(UiEvent::Error(format!(
                    "budget d'appels d'outils dépassé ({max_tool_calls}) → arrêt"
                )));
                return;
            }

            let _ = ui.send(UiEvent::ToolCall {
                name: call.function.name.clone(),
                args: call.function.arguments.clone(),
            });
            let result = tools::execute_boxed(&call.function.name, &call.function.arguments).await;
            let _ = ui.send(UiEvent::ToolResult(result.clone()));
            let tool_msg = if options.tools_mode == ToolsMode::Prompt {
                ChatMessage::user(format!(
                    "Résultat de l'outil `{}` :\n{}",
                    call.function.name, result
                ))
            } else {
                ChatMessage::tool_result(call.id.clone(), result)
            };
            messages.push(tool_msg);
        }

        match client
            .stream_chat(&system, &messages, &tools, &options, &ui)
            .await
        {
            Ok((c, cs, f)) => {
                content = c;
                calls = cs;
                finish = f;
            }
            Err(e) => {
                let _ = ui.send(UiEvent::Error(format!("{e}")));
                return;
            }
        }
    }

    let _ = ui.send(UiEvent::TurnComplete(messages));
}

#[cfg(test)]
mod tests {
    use super::{failure_message, should_failover};

    #[test]
    fn failure_message_is_never_empty() {
        let msg = failure_message("", &["local/qwen".to_string()]);
        assert!(msg.contains("local/qwen"));
        assert!(msg.contains("aucun endpoint"));
        assert!(!failure_message("", &[]).trim().is_empty());
        let with_err = failure_message("connexion refusée", &[]);
        assert!(with_err.contains("connexion refusée"));
    }

    #[test]
    fn failover_is_selective() {
        assert!(should_failover(None, "connexion refusée"));
        for s in [401u16, 402, 403, 404, 408, 409, 429, 500, 502, 503] {
            assert!(should_failover(Some(s), "n'importe quoi"), "statut {s}");
        }
        assert!(!should_failover(Some(400), "invalid request: bad json"));
        assert!(!should_failover(Some(422), "unprocessable entity"));
        assert!(should_failover(Some(400), "model does not support tools"));
        assert!(should_failover(Some(400), "function calling unsupported"));
    }
}
