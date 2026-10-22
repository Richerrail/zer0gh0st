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

/// Per-request generation options.
#[derive(Clone, Debug)]
pub struct ChatOptions {
    pub max_tokens: u32,
    pub thinking: bool,
    pub temperature: f32,
}

impl Default for ChatOptions {
    fn default() -> Self {
        Self {
            max_tokens: 2048,
            thinking: false,
            temperature: 0.3,
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
}

impl std::fmt::Display for ChatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatError::Setup { err, .. } | ChatError::Mid(err) => write!(f, "{err:#}"),
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
            "max_tokens": options.max_tokens,
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

        if !tools.is_empty() {
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

        let calls = partial
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

        Ok((content, calls, finish_reason))
    }
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
