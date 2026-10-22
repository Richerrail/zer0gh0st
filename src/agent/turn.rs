use tokio::sync::mpsc::UnboundedSender;

use crate::agent::history::ChatMessage;
use crate::event::UiEvent;
use crate::llm::llama::{ApiClient, ChatError, ChatOptions};
use crate::route::{self, Endpoint};
use crate::tools::{self, ToolSpec};

const MAX_ITERATIONS: usize = 8;
/// Hard ceiling on tool calls within a single turn.
const MAX_TOOL_CALLS: usize = 24;
/// Reprises automatiques après une réponse tronquée (`finish_reason = length`).
const MAX_CONTINUATIONS: u32 = 4;

/// Run one user turn over an ordered endpoint chain.
///
/// The first model call fails over across endpoints (setup failures only),
/// with exponential backoff on HTTP 429. Once an endpoint answers, the rest
/// of the turn sticks to it.
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

/// Comme `run_turn`, mais avec un budget d'endiguement explicite. Le débat 0box
/// (MAIN exécute un plan multi-étapes : vérifier + créer + marquer) a montré que
/// le budget par défaut (8 itérations) ne laisse même pas place aux `task_update`
/// après vérification des livrables — les tours 0box passent donc un budget élargi,
/// toujours borné (principe d'endiguement respecté : jamais illimité).
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

    crate::cancel::clear();
    // Budget partagé par tous les appels LLM de ce tour, sous-agents `0box`
    // compris. Le compteur n'est remis à zéro qu'au premier niveau.
    let _budget = crate::budget::TurnGuard::enter();

    // --- first call, with failover across the ordered chain ---
    let mut chosen: Option<(
        ApiClient,
        String,
        Vec<crate::agent::history::ToolCall>,
        Option<String>,
    )> = None;
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

        match client
            .stream_chat(&system, &messages, &tools, &options, &ui)
            .await
        {
            Ok((content, calls, _)) if content.trim().is_empty() && calls.is_empty() => {
                // Model produced nothing usable (e.g. reasoning-only) → fail over.
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
                // Le backoff 429 est fait **une seule fois**, dans le client
                // (`llm::llama`, qui honore `Retry-After`). Ici, un 429 signifie
                // donc « essais épuisés » : on bascule d'endpoint au lieu
                // d'empiler un second backoff.
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

    let Some((client, mut content, mut calls, mut finish)) = chosen else {
        let _ = ui.send(UiEvent::Error(format!(
            "tous les endpoints ont échoué. dernier: {last_err}"
        )));
        return;
    };

    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut total_calls: usize = 0;
    let mut continuations: u32 = 0;

    for _ in 0..max_iterations {
        let assistant = ChatMessage::assistant(content.clone(), calls.clone());
        messages.push(assistant.clone());

        if calls.is_empty() {
            // Réponse tronquée (limite de tokens) → on REPREND automatiquement
            // au lieu de laisser l'utilisateur appuyer sur F7.
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
            // `execute_boxed` (effacement de type) : casse le cycle de types
            // async (l'outil `0box` → débat → sous-agent → outils → …).
            let result = tools::execute_boxed(&call.function.name, &call.function.arguments).await;
            let _ = ui.send(UiEvent::ToolResult(result.clone()));
            let tool_msg = ChatMessage::tool_result(call.id.clone(), result);
            messages.push(tool_msg);
        }

        // subsequent calls stay on the chosen endpoint
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
