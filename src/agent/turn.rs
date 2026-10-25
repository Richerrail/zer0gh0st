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
/// Une erreur de *setup* justifie-t-elle de basculer vers l'endpoint suivant ?
///
/// Oui pour le réseau et les statuts qui peuvent dépendre du provider
/// (clé, crédit, accès, modèle, indisponibilité). Non pour une erreur
/// **déterministe** (400/422) — sauf si elle parle d'outils, auquel cas un autre
/// modèle peut les supporter.
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

/// Message final quand aucun endpoint n'a répond, jamais vide.
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

    // Les modèles `:free` ont un quota de requêtes serré : un tour ne doit pas
    // consommer 8 appels. On plafonne à 4 itérations.
    let free_chain = endpoints.iter().any(|e| e.model.contains(":free"));
    let max_iterations = if free_chain {
        max_iterations.min(4)
    } else {
        max_iterations
    };

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
    // Endpoints sautés parce qu'ils sont marqués en échec récent : sans cette
    // liste, si TOUS sont sautés, le message final était vide.
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
                // Un 429 sur un modèle gratuit est un plafond de compte : la
                // chaîne de bascule est faite des mêmes modèles `:free`.
                if status == Some(429) && ep.model.contains(":free") {
                    route::mark_failed(&ep.label);
                    let _ = ui.send(UiEvent::Error(format!(
                        "limite atteinte sur {} ({err})\n\
                         Les modèles `:free` partagent le quota du compte : pas de bascule.",
                        ep.label
                    )));
                    return;
                }
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
                if !should_failover(status, &err.to_string()) {
                    // Erreur déterministe : elle se reproduira à l'identique sur
                    // tous les endpoints. Basculer ne ferait que consommer du
                    // quota et du temps.
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
                // Quota de compte : basculer vers un autre `:free` ne rend pas
                // de quota, et réessayer ne fait que retarder le message.
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
        // réseau : on bascule
        assert!(should_failover(None, "connexion refusée"));
        // dépend du provider : on bascule
        for s in [401u16, 402, 403, 404, 408, 409, 429, 500, 502, 503] {
            assert!(should_failover(Some(s), "n'importe quoi"), "statut {s}");
        }
        // déterministe : on ne bascule pas
        assert!(!should_failover(Some(400), "invalid request: bad json"));
        assert!(!should_failover(Some(422), "unprocessable entity"));
        // … sauf si un autre modèle peut supporter les outils
        assert!(should_failover(Some(400), "model does not support tools"));
        assert!(should_failover(Some(400), "function calling unsupported"));
    }
}
