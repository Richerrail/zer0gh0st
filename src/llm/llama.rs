use anyhow::{anyhow, Result};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use reqwest::header::RETRY_AFTER;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

use crate::agent::history::{ChatMessage, FunctionCall, ToolCall};
use crate::event::UiEvent;
use crate::http_client::HTTP_CLIENT;
use crate::tools::ToolSpec;

/// Hard cap on a single streamed response (guards against runaway/looping models).
const MAX_STREAM_CHARS: usize = 1_000_000;
/// Abort a stream that sends nothing for this long.
const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(120);

/// Plafond de sortie pour les modèles `:free` : les providers comptent souvent
/// la sortie **réservée** contre le quota, et un `max_tokens` élevé rapproche
/// le 429 sans rien apporter sur une réponse normale.
const FREE_MAX_TOKENS: u32 = 2048;

/// `max_tokens` effectif (pur, testable).
fn effective_max_tokens(model: &str, requested: u32) -> u32 {
    if model.to_ascii_lowercase().contains(":free") {
        requested.min(FREE_MAX_TOKENS)
    } else {
        requested
    }
}

/// Comment les outils sont exposés au modèle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ToolsMode {
    /// API OpenAI `tools` + `tool_choice` (cloud, llama.cpp récent).
    #[default]
    Native,
    /// Protocole texte `<tool_call>{…}</tool_call>` : nécessaire quand le
    /// serveur local ne sait pas (ou plus) construire la grammaire des outils.
    Prompt,
}

/// Per-request generation options.
#[derive(Clone, Debug)]
pub struct ChatOptions {
    pub max_tokens: u32,
    pub thinking: bool,
    pub temperature: f32,
    pub tools_mode: ToolsMode,
}

impl Default for ChatOptions {
    fn default() -> Self {
        Self {
            max_tokens: 2048,
            thinking: false,
            temperature: 0.3,
            tools_mode: ToolsMode::Native,
        }
    }
}

/// Distinguishes failures that are safe to fail over from mid-stream failures.
#[derive(Debug)]
pub enum ChatError {
    /// Failed before any token was emitted (connect / status / request build).
    Setup {
        err: anyhow::Error,
        status: Option<u16>,
    },
    /// Failed after streaming started; tokens may already be visible.
    Mid(anyhow::Error),
    /// Quota définitif (journalier / crédits) : réessayer ou basculer vers un
    /// autre modèle `:free` du même compte ne sert à rien.
    Quota { detail: String },
}

impl std::fmt::Display for ChatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatError::Setup { err, .. } | ChatError::Mid(err) => write!(f, "{err:#}"),
            ChatError::Quota { detail } => write!(f, "quota épuisé : {detail}"),
        }
    }
}

impl std::error::Error for ChatError {}

/// Minimal OpenAI-compatible client for `llama-server`.
#[derive(Clone)]
pub struct ApiClient {
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    http: reqwest::Client,
}

impl ApiClient {
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: Option<String>,
    ) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            model: model.into(),
            api_key,
            http: HTTP_CLIENT.clone(),
        }
    }

    /// Stream a chat completion. Emits tokens through `ui` and returns the full
    /// assistant text plus any requested tool calls.
    pub async fn stream_chat(
        &self,
        system: &str,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
        options: &ChatOptions,
        ui: &UnboundedSender<UiEvent>,
    ) -> Result<(String, Vec<ToolCall>, Option<String>), ChatError> {
        let mut msgs: Vec<serde_json::Value> = Vec::with_capacity(messages.len() + 1);
        msgs.push(json!({ "role": "system", "content": system }));
        for m in messages {
            // Defensive: never forward UI-only roles to the server.
            if !matches!(m.role.as_str(), "user" | "assistant" | "tool" | "system") {
                continue;
            }
            msgs.push(m.to_wire_json());
        }

        let mut body = json!({
            "model": self.model,
            "messages": msgs,
            "stream": true,
            "temperature": options.temperature,
            "max_tokens": effective_max_tokens(&self.model, options.max_tokens),
        });
        // `chat_template_kwargs` est un paramètre llama.cpp/vLLM : l'endpoint OpenAI-compat
        // de Google le REJETTE (400 « Unknown name "chat_template_kwargs" ») — bug constaté
        // en débat 0box (rôle reasoning via gemma). On ne l'envoie pas à Google.
        if !self.base_url.contains("generativelanguage.googleapis.com") {
            body["chat_template_kwargs"] = json!({ "enable_thinking": options.thinking });
        }
        // OpenRouter's unified reasoning control (Qwen uses chat_template_kwargs).
        if self.base_url.contains("openrouter.ai") {
            body["reasoning"] = json!({ "enabled": options.thinking });
        }

        if options.tools_mode == ToolsMode::Native && !tools.is_empty() {
            let specs: Vec<serde_json::Value> = tools.iter().map(ToolSpec::to_openai).collect();
            body["tools"] = json!(specs);
            body["tool_choice"] = json!("auto");
        }

        let url = format!("{}/chat/completions", self.base_url);
        // Étiquette d'usage (suivi 200/429, voir `quota`) — identique aux labels de `route`.
        let quota_label = crate::quota::label_for(&self.base_url, &self.model);

        // Build request with proper headers
        let req_builder = || {
            let mut req = self.http.post(&url).json(&body);
            if self.base_url.contains("openrouter.ai") {
                req = attribution(req);
            }
            if let Some(k) = &self.api_key {
                req = req.bearer_auth(k);
            }
            req
        };

        // Retry loop for 429 (rate limit) with exponential backoff + Retry-After header
        let mut retries = 3;
        let resp = loop {
            if !crate::budget::try_consume() {
                return Err(ChatError::Setup {
                    err: anyhow!(
                        "budget d'appels LLM épuisé ({}) pour ce tour — réduis le périmètre \
                         (sous-agents 0box) ou augmente ZER0_MAX_LLM_CALLS",
                        crate::budget::max_calls()
                    ),
                    status: None,
                });
            }
            let req = req_builder();
            let resp = req.send().await.map_err(|e| ChatError::Setup {
                err: e.into(),
                status: None,
            })?;

            if resp.status() == 429 {
                // Retry-After doit être lu avant `text()` (qui consomme la réponse).
                let wait = resp
                    .headers()
                    .get(RETRY_AFTER)
                    .and_then(|h| h.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(2u64.pow(3 - retries));
                let text = resp.text().await.unwrap_or_default();
                crate::quota::record_rate_limited(&quota_label, &text);
                if let Some(detail) = definitive_quota(&text) {
                    // Inutile de réessayer : attendre la réinitialisation ou
                    // changer de modèle gratuit ne rendra pas de quota.
                    return Err(ChatError::Quota { detail });
                }
                if retries == 0 {
                    return Err(ChatError::Setup {
                        err: anyhow!("429: {}", text.chars().take(200).collect::<String>()),
                        status: Some(429),
                    });
                }
                let _ = ui.send(UiEvent::Info(format!(
                    "⏳ Rate limited (429), retrying in {}s...",
                    wait
                )));
                tokio::time::sleep(Duration::from_secs(wait)).await;
                retries -= 1;
                continue;
            }

            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                return Err(ChatError::Setup {
                    err: anyhow!("{status}: {}", text.chars().take(200).collect::<String>()),
                    status: Some(status.as_u16()),
                });
            }
            crate::quota::record_success(&quota_label);
            break resp;
        };

        let mut stream = resp.bytes_stream().eventsource();
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut partial: Vec<PartialCall> = Vec::new();
        let mut finish_reason: Option<String> = None;
        let mut emitted: usize = 0;
        // Un flux n'est valide que s'il annonce sa fin (`[DONE]` ou `finish_reason`).
        // Sinon on a peut-être perdu la fin d'un `tool_call` (JSON tronqué) :
        // c'était jusqu'ici retourné comme un succès, en silence.
        let mut terminal = false;

        loop {
            let ev = match tokio::time::timeout(STREAM_IDLE_TIMEOUT, stream.next()).await {
                Ok(Some(ev)) => ev.map_err(|e| ChatError::Mid(e.into()))?,
                Ok(None) => break,
                Err(_) => {
                    return Err(ChatError::Mid(anyhow!(
                        "flux inactif pendant {} s : connexion coupée en cours de réponse",
                        STREAM_IDLE_TIMEOUT.as_secs()
                    )));
                }
            };
            let data = ev.data.trim();
            if data.is_empty() {
                continue;
            }
            if data == "[DONE]" {
                terminal = true;
                break;
            }
            let chunk: StreamChunk = match serde_json::from_str(data) {
                Ok(c) => c,
                Err(_) => continue,
            };
            for choice in chunk.choices {
                if choice.finish_reason.is_some() {
                    terminal = true;
                    finish_reason = choice.finish_reason.clone();
                }
                if let Some(text) = choice.delta.content {
                    if !text.is_empty() {
                        emitted += text.len();
                        content.push_str(&text);
                        let _ = ui.send(UiEvent::Token(text));
                    }
                }
                if let Some(reason) = choice.delta.reasoning_content {
                    if !reason.is_empty() {
                        emitted += reason.len();
                        reasoning.push_str(&reason);
                        let _ = ui.send(UiEvent::Reasoning(reason));
                    }
                }
                if let Some(calls) = choice.delta.tool_calls {
                    for tc in calls {
                        merge_call(&mut partial, tc);
                    }
                }
            }
            if crate::cancel::is_cancelled() {
                let _ = ui.send(UiEvent::Info("⏹ génération interrompue".into()));
                break;
            }
            if emitted > MAX_STREAM_CHARS {
                let _ = ui.send(UiEvent::Info(
                    "⚠ réponse trop longue (limite 1 Mo) : flux coupé".into(),
                ));
                break;
            }
        }

        if !terminal && (!content.trim().is_empty() || !partial.is_empty()) {
            let _ = ui.send(UiEvent::Info(
                "⚠ flux clos sans marqueur de fin ([DONE]/finish_reason) — réponse \
                 possiblement incomplète"
                    .into(),
            ));
            if finish_reason.is_none() {
                finish_reason = Some("eof".into());
            }
        }

        if finish_reason.as_deref() == Some("length") {
            let _ = ui.send(UiEvent::Info(
                "⚠ réponse tronquée : limite de tokens atteinte (augmente --max-tokens)".into(),
            ));
        }

        // Some providers return empty content in streaming mode (e.g. vLLM
        // diffusion models) while the non-streaming call works. Retry once
        // without streaming (with retry on 429).
        if content.trim().is_empty() && partial.is_empty() {
            let mut body2 = body.clone();
            body2["stream"] = json!(false);

            let mut retries2 = 3;
            loop {
                if !crate::budget::try_consume() {
                    return Err(ChatError::Setup {
                        err: anyhow!("budget d'appels LLM épuisé pour ce tour"),
                        status: None,
                    });
                }
                let mut req2 = self.http.post(&url).json(&body2);
                if self.base_url.contains("openrouter.ai") {
                    req2 = attribution(req2);
                }
                if let Some(k) = &self.api_key {
                    req2 = req2.bearer_auth(k);
                }
                match req2.send().await {
                    Ok(resp2) if resp2.status() == 429 => {
                        let wait = resp2
                            .headers()
                            .get(RETRY_AFTER)
                            .and_then(|h| h.to_str().ok())
                            .and_then(|s| s.parse::<u64>().ok())
                            .unwrap_or(2u64.pow(3 - retries2));
                        // Limites éventuelles dans les en-têtes (avant le corps).
                        crate::quota::record_limits(
                            &quota_label,
                            crate::quota::header_u64(resp2.headers(), crate::quota::LIMIT_HEADERS),
                            crate::quota::header_u64(resp2.headers(), crate::quota::REMAIN_HEADERS),
                        );
                        let text = resp2.text().await.unwrap_or_default();
                        crate::quota::record_rate_limited(&quota_label, &text);
                        if retries2 == 0 {
                            break;
                        }
                        let _ = ui.send(UiEvent::Info(format!(
                            "⏳ Rate limited (429), retrying in {}s...",
                            wait
                        )));
                        tokio::time::sleep(Duration::from_secs(wait)).await;
                        retries2 -= 1;
                        continue;
                    }
                    Ok(resp2) if !resp2.status().is_success() => {
                        // Ne pas masquer la cause : un 401/404 ici doit remonter
                        // (turn.rs peut alors basculer d'endpoint).
                        let status = resp2.status();
                        let body = resp2.text().await.unwrap_or_default();
                        return Err(ChatError::Setup {
                            err: anyhow!(
                                "{status} (repli non-streaming) : {}",
                                body.chars().take(200).collect::<String>()
                            ),
                            status: Some(status.as_u16()),
                        });
                    }
                    Ok(resp2) => {
                        crate::quota::record_success(&quota_label);
                        crate::quota::record_limits(
                            &quota_label,
                            crate::quota::header_u64(resp2.headers(), crate::quota::LIMIT_HEADERS),
                            crate::quota::header_u64(resp2.headers(), crate::quota::REMAIN_HEADERS),
                        );
                        if let Ok(v) = resp2.json::<serde_json::Value>().await {
                            if let Some(msg) = v["choices"].get(0).and_then(|ch| ch.get("message"))
                            {
                                let c = msg.get("content").and_then(|x| x.as_str()).unwrap_or("");
                                if !c.is_empty() {
                                    content = c.to_string();
                                    let _ = ui.send(UiEvent::Token(c.to_string()));
                                } else if let Some(r) =
                                    msg.get("reasoning_content").and_then(|x| x.as_str())
                                {
                                    if !r.is_empty() {
                                        reasoning = r.to_string();
                                    }
                                }
                            }
                        }
                        break;
                    }
                    Err(e) => {
                        return Err(ChatError::Setup {
                            err: e.into(),
                            status: None,
                        });
                    }
                }
            }
        }

        // Last resort: some reasoning models put everything in reasoning_content and
        // leave content empty. Use the reasoning as the answer instead of failing over.
        if content.trim().is_empty() && partial.is_empty() && !reasoning.trim().is_empty() {
            let _ = ui.send(UiEvent::Info(
                "↪ réponse issue du raisonnement (`content` vide)".into(),
            ));
            content = reasoning.clone();
        }

        content = sanitize_content(&content);

        // Mode « prompt » : le modèle écrit <tool_call>{…}</tool_call> dans sa
        // réponse. On retire le bloc du texte et on synthétise les appels.
        let mut prompt_calls = Vec::new();
        if options.tools_mode == ToolsMode::Prompt {
            let (clean, parsed) = parse_prompt_tool_calls(&content);
            content = clean;
            prompt_calls = parsed;
        }

        let mut calls: Vec<ToolCall> = partial
            .into_iter()
            .filter(|c| !c.name.is_empty())
            .map(|c| ToolCall {
                id: if c.id.is_empty() {
                    format!("call_{}", c.index)
                } else {
                    c.id
                },
                kind: "function".into(),
                function: FunctionCall {
                    name: c.name,
                    arguments: if c.arguments.is_empty() {
                        "{}".into()
                    } else {
                        c.arguments
                    },
                },
            })
            .collect();
        calls.extend(prompt_calls);

        Ok((content, calls, finish_reason))
    }
}

/// Détecte un 429 **définitif** (quota journalier, crédits épuisés) d'après le
/// corps renvoyé par le provider. Retourne un message court, ou `None` si la
/// limite est probablement passagère (rafale par minute).
fn definitive_quota(body: &str) -> Option<String> {
    let b = body.to_ascii_lowercase();
    let daily = b.contains("free-models-per-day")
        || b.contains("per-day")
        || b.contains("per day")
        || b.contains("daily limit")
        || b.contains("daily quota")
        || b.contains("quota exceeded")
        || b.contains("insufficient credits")
        || b.contains("not enough credits");
    if !daily {
        return None;
    }
    let msg: String = body.chars().filter(|c| !c.is_control()).take(220).collect();
    Some(msg.trim().to_string())
}

/// Extrait les appels d'outils d'un texte au format `<tool_call>…</tool_call>`.
///
/// Tolérant sur la forme : `{"name":…,"arguments":{…}}` ou
/// `{"tool":…,"args":{…}}`, et arguments en chaîne JSON ou en objet.
/// Retourne le texte nettoyé et les appels (ids synthétiques `local_N`).
pub fn parse_prompt_tool_calls(content: &str) -> (String, Vec<ToolCall>) {
    let mut clean = String::with_capacity(content.len());
    let mut calls = Vec::new();
    let mut rest = content;
    loop {
        let Some(start) = rest.find("<tool_call>") else {
            clean.push_str(rest);
            break;
        };
        clean.push_str(&rest[..start]);
        let after = &rest[start + "<tool_call>".len()..];
        let Some(end) = after.find("</tool_call>") else {
            // Bloc non fermé : on garde le texte tel quel (le modèle a été coupé).
            clean.push_str(&rest[start..]);
            break;
        };
        let block = after[..end].trim();
        let json = block
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(json) {
            let name = v
                .get("name")
                .or_else(|| v.get("tool"))
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let args = v
                .get("arguments")
                .or_else(|| v.get("args"))
                .or_else(|| v.get("parameters"))
                .cloned()
                .unwrap_or_else(|| json!({}));
            if !name.is_empty() {
                let arguments = match args {
                    serde_json::Value::String(s) => s,
                    other => other.to_string(),
                };
                calls.push(ToolCall {
                    id: format!("local_{}", calls.len() + 1),
                    kind: "function".into(),
                    function: FunctionCall { name, arguments },
                });
            }
        }
        rest = &after[end + "</tool_call>".len()..];
    }
    (clean.trim().to_string(), calls)
}

/// En-têtes d'attribution envoyés à OpenRouter (tableau de bord + `HTTP-Referer`).
///
/// La version précédente avait `zer0gh0st/0.5.5` et `github.com/richerrail/...`
/// écrits en dur à deux endroits : version fausse dès la 0.5.6, et mauvaise
/// casse de l'organisation. Une seule source : `CARGO_PKG_VERSION` + `update::REPO`.
fn attribution(req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    req.header(
        "HTTP-Referer",
        format!("https://github.com/{}", crate::update::REPO),
    )
    .header("X-Title", concat!("ZER0|GH0ST/", env!("CARGO_PKG_VERSION")))
}

/// Retire les tokens spéciaux qui fuient (`<|im_end|>`, `<|eot_id|>`…).
///
/// **Ne touche pas aux blancs.** La version précédente terminait par
/// `out.split_whitespace().collect::<Vec<_>>().join(" ")`, ce qui écrasait les
/// retours à la ligne et l'indentation de *chaque* réponse assistant : le code
/// relu (et renvoyé au modèle au tour suivant) arrivait aplati sur une ligne.
/// On se contente désormais de supprimer les lignes vides excédentaires.
fn sanitize_content(s: &str) -> String {
    collapse_blank_lines(&strip_special_tokens(s))
}

/// Supprime les séquences `<|…|>` dont le contenu est un identifiant de token
/// (`alnum` + `_`). Tout le reste est laissé intact : on ne mange pas de texte
/// arbitraire au motif qu'il commence par `<|`.
fn strip_special_tokens(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0usize;
    while i < s.len() {
        if s[i..].starts_with("<|") {
            if let Some(rel) = s[i + 2..].find("|>") {
                let inner = &s[i + 2..i + 2 + rel];
                let is_token = !inner.is_empty()
                    && inner.len() <= 40
                    && inner.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                if is_token {
                    i += 2 + rel + 2;
                    continue;
                }
            }
        }
        let ch = s[i..].chars().next().expect("frontière UTF-8");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Réduit les suites de lignes vides (`\n\n\n…` → `\n\n`), sans toucher à
/// l'indentation ni aux espaces significatifs.
fn collapse_blank_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut newlines = 0usize;
    for c in s.chars() {
        if c == '\n' {
            newlines += 1;
            if newlines <= 2 {
                out.push(c);
            }
        } else {
            newlines = 0;
            out.push(c);
        }
    }
    out
}

#[derive(Default)]
struct PartialCall {
    index: usize,
    id: String,
    name: String,
    arguments: String,
}

fn merge_call(partial: &mut Vec<PartialCall>, delta: ToolCallDelta) {
    let idx = delta.index;
    while partial.len() <= idx {
        partial.push(PartialCall {
            index: partial.len(),
            ..Default::default()
        });
    }
    let slot = &mut partial[idx];
    if let Some(id) = delta.id {
        slot.id.push_str(&id);
    }
    if let Some(f) = delta.function {
        if let Some(name) = f.name {
            slot.name.push_str(&name);
        }
        if let Some(args) = f.arguments {
            slot.arguments.push_str(&args);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_tool_calls_are_parsed_and_removed_from_text() {
        let text = "Je vérifie.\n<tool_call>{\"name\":\"code_exec\",\"arguments\":{\"command\":\"ls\"}}</tool_call>";
        let (clean, calls) = parse_prompt_tool_calls(text);
        assert_eq!(clean, "Je vérifie.");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "code_exec");
        assert_eq!(calls[0].function.arguments, r#"{"command":"ls"}"#);
    }

    #[test]
    fn prompt_tool_calls_accept_string_args_and_fences() {
        let text = "<tool_call>```json\n{\"tool\":\"task_add\",\"args\":\"{\\\"text\\\":\\\"x\\\"}\"}\n```</tool_call>";
        let (clean, calls) = parse_prompt_tool_calls(text);
        assert!(clean.is_empty());
        assert_eq!(calls[0].function.name, "task_add");
        assert!(calls[0].function.arguments.contains("text"));
    }

    #[test]
    fn unterminated_prompt_tool_call_is_left_as_text() {
        let (clean, calls) = parse_prompt_tool_calls("avant <tool_call>{\"name\":\"x\"");
        assert!(calls.is_empty());
        assert!(clean.contains("avant"));
    }

    #[test]
    fn free_models_get_a_lower_output_cap() {
        assert_eq!(effective_max_tokens("vendor/x:free", 8192), 2048);
        assert_eq!(effective_max_tokens("VENDOR/X:FREE", 1000), 1000);
        assert_eq!(effective_max_tokens("vendor/x", 8192), 8192);
        assert_eq!(effective_max_tokens("vendor/x:free", 512), 512);
    }

    #[test]
    fn definitive_quota_is_recognised() {
        assert!(definitive_quota(
            r#"{"error":{"message":"Rate limit exceeded: free-models-per-day"}}"#
        )
        .is_some());
        assert!(definitive_quota("daily limit reached").is_some());
        assert!(definitive_quota("Insufficient credits").is_some());
        // Une rafale par minute n'est PAS définitive : on réessaie.
        assert!(definitive_quota("rate limit exceeded, retry in 3s").is_none());
        assert!(definitive_quota("").is_none());
    }

    #[test]
    fn markdown_and_indentation_are_preserved() {
        let md = "Voici le code:\n\n```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n\n- item 1\n- item 2\n";
        assert_eq!(sanitize_content(md), md);
    }

    #[test]
    fn table_alignment_is_preserved() {
        let t = "| a | b |\n|---|---|\n| 1 | 2 |\n";
        assert_eq!(sanitize_content(t), t);
    }

    #[test]
    fn leaked_special_tokens_are_removed() {
        assert_eq!(sanitize_content("bonjour<|im_end|>"), "bonjour");
        assert_eq!(sanitize_content("a<|eot_id|>b<|end_of_text|>c"), "abc");
    }

    #[test]
    fn non_token_angle_pipes_are_kept() {
        // Pas un identifiant de token : on ne détruit rien.
        let s = "comparaison a <| b et c |> d";
        assert_eq!(sanitize_content(s), s);
        let s2 = "code: `<|pas un token|>`";
        assert_eq!(sanitize_content(s2), s2);
    }

    #[test]
    fn excessive_blank_lines_are_collapsed() {
        assert_eq!(sanitize_content("a\n\n\n\n\nb"), "a\n\nb");
        assert_eq!(sanitize_content("a\n\nb"), "a\n\nb");
    }

    #[test]
    fn attribution_uses_a_single_source_of_truth() {
        let title = concat!("ZER0|GH0ST/", env!("CARGO_PKG_VERSION"));
        assert!(title.starts_with("ZER0|GH0ST/"));
        assert_eq!(crate::update::REPO, "Richerrail/zer0gh0st");
    }

    #[test]
    fn tool_call_arguments_are_not_altered() {
        let args = "{\n  \"path\": \"/tmp/a b\",\n  \"command\": \"create\"\n}";
        assert_eq!(sanitize_content(args), args);
    }
}

#[derive(Deserialize)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
}

#[derive(Deserialize)]
struct StreamChoice {
    #[serde(default)]
    delta: Delta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
struct Delta {
    content: Option<String>,
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCallDelta>>,
}

#[derive(Deserialize)]
struct ToolCallDelta {
    #[serde(default)]
    index: usize,
    id: Option<String>,
    function: Option<FunctionDelta>,
}

#[derive(Deserialize)]
struct FunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}
