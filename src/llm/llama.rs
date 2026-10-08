use anyhow::{anyhow, Result};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

use crate::agent::history::{ChatMessage, FunctionCall, ToolCall};
use crate::event::UiEvent;
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
        Self { max_tokens: 2048, thinking: false, temperature: 0.3 }
    }
}

/// Distinguishes failures that are safe to fail over from mid-stream failures.
#[derive(Debug)]
pub enum ChatError {
    /// Failed before any token was emitted (connect / status / request build).
    Setup { err: anyhow::Error, status: Option<u16> },
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
            http: reqwest::Client::new(),
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
    ) -> Result<(String, Vec<ToolCall>), ChatError> {
        let mut msgs: Vec<serde_json::Value> = Vec::with_capacity(messages.len() + 1);
        msgs.push(json!({ "role": "system", "content": system }));
        for m in messages {
            // Defensive: never forward UI-only roles to the server.
            if !matches!(m.role.as_str(), "user" | "assistant" | "tool" | "system") {
                continue;
            }
            msgs.push(serde_json::to_value(m).map_err(|e| ChatError::Setup { err: e.into(), status: None })?);
        }

        let mut body = json!({
            "model": self.model,
            "messages": msgs,
            "stream": true,
            "temperature": options.temperature,
            "max_tokens": options.max_tokens,
        });
        body["chat_template_kwargs"] = json!({ "enable_thinking": options.thinking });
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
        let mut req = self.http.post(&url).json(&body);
        if self.base_url.contains("openrouter.ai") {
            req = req
                .header("HTTP-Referer", "https://github.com/omar0/ZER0v1")
                .header("X-Title", "Zer0");
        }
        if let Some(k) = &self.api_key {
            req = req.bearer_auth(k);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ChatError::Setup { err: e.into(), status: None })?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ChatError::Setup {
                err: anyhow!("{status}: {}", text.chars().take(200).collect::<String>()),
                status: Some(status.as_u16()),
            });
        }

        let mut stream = resp.bytes_stream().eventsource();
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut partial: Vec<PartialCall> = Vec::new();
        let mut finish_reason: Option<String> = None;
        let mut emitted: usize = 0;

        loop {
            let ev = match tokio::time::timeout(STREAM_IDLE_TIMEOUT, stream.next()).await {
                Ok(Some(ev)) => ev.map_err(|e| ChatError::Mid(e.into()))?,
                Ok(None) => break,
                Err(_) => {
                    let _ = ui.send(UiEvent::Info(
                        "⚠ flux interrompu (aucune donnée pendant 120 s)".into(),
                    ));
                    break;
                }
            };
            let data = ev.data.trim();
            if data.is_empty() {
                continue;
            }
            if data == "[DONE]" {
                break;
            }
            let chunk: StreamChunk = match serde_json::from_str(data) {
                Ok(c) => c,
                Err(_) => continue,
            };
            for choice in chunk.choices {
                if choice.finish_reason.is_some() {
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

        if finish_reason.as_deref() == Some("length") {
            let _ = ui.send(UiEvent::Info(
                "⚠ réponse tronquée : limite de tokens atteinte (augmente --max-tokens)".into(),
            ));
        }

        // Some providers return empty content in streaming mode (e.g. vLLM
        // diffusion models) while the non-streaming call works. Retry once
        // without streaming.
        if content.trim().is_empty() && partial.is_empty() {
            let mut body2 = body.clone();
            body2["stream"] = json!(false);
            let mut req2 = self.http.post(&url).json(&body2);
            if self.base_url.contains("openrouter.ai") {
                req2 = req2
                    .header("HTTP-Referer", "https://github.com/omar0/ZER0v1")
                    .header("X-Title", "Zer0");
            }
            if let Some(k) = &self.api_key {
                req2 = req2.bearer_auth(k);
            }
            if let Ok(resp2) = req2.send().await {
                if resp2.status().is_success() {
                    if let Ok(v) = resp2.json::<serde_json::Value>().await {
                        if let Some(msg) = v["choices"].get(0).and_then(|ch| ch.get("message")) {
                            let c = msg
                                .get("content")
                                .and_then(|x| x.as_str())
                                .unwrap_or("");
                            if !c.is_empty() {
                                content = c.to_string();
                                let _ = ui.send(UiEvent::Token(c.to_string()));
                            } else if let Some(r) = msg.get("reasoning_content").and_then(|x| x.as_str())
                            {
                                if !r.is_empty() {
                                    reasoning = r.to_string();
                                }
                            }
                        }
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
                id: if c.id.is_empty() { format!("call_{}", c.index) } else { c.id },
                kind: "function".into(),
                function: FunctionCall {
                    name: c.name,
                    arguments: if c.arguments.is_empty() { "{}".into() } else { c.arguments },
                },
            })
            .collect();

        Ok((content, calls))
    }
}

/// Retire les tokens spéciaux qui fuient (`<|open|>`, `<|close|>`, `<|sep|>`…).
fn sanitize_content(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '<' && it.peek() == Some(&'|') {
            it.next(); // consomme '|'
            while let Some(c2) = it.next() {
                if c2 == '|' && it.peek() == Some(&'>') {
                    it.next();
                    break;
                }
            }
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    // retire les espaces multiples créés par le nettoyage
    out.split_whitespace().collect::<Vec<_>>().join(" ")
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
        partial.push(PartialCall { index: partial.len(), ..Default::default() });
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
