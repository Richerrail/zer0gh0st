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

const MAX_STREAM_CHARS: usize = 1_000_000;
fn stream_idle_timeout() -> Duration {
    parse_idle_secs(std::env::var("ZER0_STREAM_IDLE_SECS").ok().as_deref())
}

fn thermal_gate(options: &ChatOptions) -> Result<(), ChatError> {
    if !options.local {
        return Ok(());
    }
    match crate::thermal::check() {
        crate::thermal::Guard::Kill(t) => {
            let _ = crate::local::stop_if_owned();
            Err(ChatError::Setup {
                err: anyhow!(
                    "🌡 {t:.0} °C — surchauffe : serveur local arrêté (seuil ZER0_TEMP_KILL)"
                ),
                status: None,
            })
        }
        crate::thermal::Guard::Stop(t) => Err(ChatError::Setup {
            err: anyhow!(
                "🌡 {t:.0} °C — génération refusée pour protéger la machine \
                 (seuil ZER0_TEMP_STOP)"
            ),
            status: None,
        }),
        _ => Ok(()),
    }
}

fn request_timeout(options: &ChatOptions) -> Duration {
    if options.stream {
        Duration::from_secs(24 * 3600)
    } else {
        parse_total_secs(std::env::var("ZER0_HTTP_TIMEOUT_SECS").ok().as_deref())
    }
}

fn parse_total_secs(raw: Option<&str>) -> Duration {
    raw.and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|s| *s > 0)
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(600))
}

fn parse_idle_secs(raw: Option<&str>) -> Duration {
    raw.and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|s| *s > 0)
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(240))
}

const FREE_MAX_TOKENS: u32 = 2048;

fn effective_max_tokens(model: &str, requested: u32) -> u32 {
    if model.to_ascii_lowercase().contains(":free") {
        requested.min(FREE_MAX_TOKENS)
    } else {
        requested
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ToolsMode {
    #[default]
    Native,
    Prompt,
}

#[derive(Clone, Debug)]
pub struct ChatOptions {
    pub max_tokens: u32,
    pub thinking: bool,
    pub temperature: f32,
    pub tools_mode: ToolsMode,
    pub stream: bool,
    pub local: bool,
}

impl Default for ChatOptions {
    fn default() -> Self {
        Self {
            max_tokens: 2048,
            thinking: false,
            temperature: 0.3,
            tools_mode: ToolsMode::Native,
            stream: true,
            local: false,
        }
    }
}

#[derive(Debug)]
pub enum ChatError {
    Setup {
        err: anyhow::Error,
        status: Option<u16>,
    },
    Mid(anyhow::Error),
    Quota {
        detail: String,
    },
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
            if !matches!(m.role.as_str(), "user" | "assistant" | "tool" | "system") {
                continue;
            }
            msgs.push(m.to_wire_json());
        }

        let mut body = json!({
            "model": self.model,
            "messages": msgs,
            "stream": options.stream,
            "temperature": options.temperature,
            "max_tokens": effective_max_tokens(&self.model, options.max_tokens),
        });
        if !self.base_url.contains("generativelanguage.googleapis.com") {
            body["chat_template_kwargs"] = json!({ "enable_thinking": options.thinking });
        }
        if self.base_url.contains("openrouter.ai") {
            body["reasoning"] = json!({ "enabled": options.thinking });
        }

        if options.tools_mode == ToolsMode::Native && !tools.is_empty() {
            let specs: Vec<serde_json::Value> = tools.iter().map(ToolSpec::to_openai).collect();
            body["tools"] = json!(specs);
            body["tool_choice"] = json!("auto");
        }

        thermal_gate(options)?;

        let url = format!("{}/chat/completions", self.base_url);
        let quota_label = crate::quota::label_for(&self.base_url, &self.model);

        let req_builder = || {
            let mut req = self
                .http
                .post(&url)
                .json(&body)
                .timeout(request_timeout(options));
            if self.base_url.contains("openrouter.ai") {
                req = attribution(req);
            }
            if let Some(k) = &self.api_key {
                req = req.bearer_auth(k);
            }
            req
        };

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
                let wait = resp
                    .headers()
                    .get(RETRY_AFTER)
                    .and_then(|h| h.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(2u64.pow(3 - retries));
                let text = resp.text().await.unwrap_or_default();
                crate::quota::record_rate_limited(&quota_label, &text);
                if let Some(detail) = definitive_quota(&text) {
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
                let mut msg = format!("{status}: {}", text.chars().take(200).collect::<String>());
                if matches!(status.as_u16(), 401 | 403) {
                    msg.push_str(&format!(
                        "\n— clé absente ou refusée. Vérifie `zer0 provider list`, que le coffre \
                         est déverrouillé, et la config : {}",
                        crate::config::global_path().display()
                    ));
                }
                return Err(ChatError::Setup {
                    err: anyhow!(msg),
                    status: Some(status.as_u16()),
                });
            }
            crate::quota::record_success(&quota_label);
            break resp;
        };

        if !options.stream {
            let raw = resp.text().await.map_err(|e| ChatError::Mid(e.into()))?;
            return parse_completion(&raw, options, ui);
        }

        let mut stream = resp.bytes_stream().eventsource();
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut partial: Vec<PartialCall> = Vec::new();
        let mut finish_reason: Option<String> = None;
        let mut emitted: usize = 0;
        let mut last_thermal = std::time::Instant::now();
        let mut warned_thermal = false;
        let mut terminal = false;

        loop {
            let idle = stream_idle_timeout();
            let ev = match tokio::time::timeout(idle, stream.next()).await {
                Ok(Some(ev)) => ev.map_err(|e| ChatError::Mid(e.into()))?,
                Ok(None) => break,
                Err(_) => {
                    return Err(ChatError::Mid(anyhow!(
                        "flux inactif pendant {} s : connexion coupée en cours de réponse",
                        idle.as_secs()
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
            if options.local && last_thermal.elapsed() >= Duration::from_secs(2) {
                last_thermal = std::time::Instant::now();
                match crate::thermal::check() {
                    crate::thermal::Guard::Kill(t) => {
                        let _ = crate::local::stop_if_owned();
                        return Err(ChatError::Mid(anyhow!(
                            "🌡 {t:.0} °C — surchauffe : génération coupée et serveur local arrêté \
                             (seuil ZER0_TEMP_KILL)"
                        )));
                    }
                    crate::thermal::Guard::Stop(t) => {
                        return Err(ChatError::Mid(anyhow!(
                            "🌡 {t:.0} °C — génération interrompue pour protéger la machine \
                             (seuil ZER0_TEMP_STOP, réglable par ZER0_TEMP_STOP)"
                        )));
                    }
                    crate::thermal::Guard::Warn(t) if !warned_thermal => {
                        warned_thermal = true;
                        let _ = ui.send(UiEvent::Info(format!(
                            "🌡 {t:.0} °C — ça chauffe (seuil d'arrêt : ZER0_TEMP_STOP)"
                        )));
                    }
                    _ => {}
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

        if content.trim().is_empty() && partial.is_empty() && !reasoning.trim().is_empty() {
            let _ = ui.send(UiEvent::Info(
                "↪ réponse issue du raisonnement (`content` vide)".into(),
            ));
            content = reasoning.clone();
        }

        let (clean, prompt_calls) = finalize_content(content, options);
        content = clean;

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

fn finalize_content(raw: String, options: &ChatOptions) -> (String, Vec<ToolCall>) {
    let content = sanitize_content(&raw);
    if options.tools_mode == ToolsMode::Prompt {
        parse_prompt_tool_calls(&content)
    } else {
        (content, Vec::new())
    }
}

#[derive(serde::Deserialize)]
struct ChatCompletion {
    #[serde(default)]
    choices: Vec<CompletionChoice>,
}

#[derive(serde::Deserialize)]
struct CompletionChoice {
    message: CompletionMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(serde::Deserialize)]
struct CompletionMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCall>>,
}

fn emit_in_chunks(text: &str, ui: &UnboundedSender<UiEvent>, reasoning: bool) {
    let chars: Vec<char> = text.chars().collect();
    for part in chars.chunks(64) {
        let s: String = part.iter().collect();
        let _ = if reasoning {
            ui.send(UiEvent::Reasoning(s))
        } else {
            ui.send(UiEvent::Token(s))
        };
    }
}

fn parse_completion(
    raw: &str,
    options: &ChatOptions,
    ui: &UnboundedSender<UiEvent>,
) -> Result<(String, Vec<ToolCall>, Option<String>), ChatError> {
    let parsed: ChatCompletion = serde_json::from_str(raw).map_err(|e| {
        ChatError::Mid(anyhow!(
            "réponse JSON invalide ({e}) : {}",
            raw.chars().take(200).collect::<String>()
        ))
    })?;
    let Some(choice) = parsed.choices.into_iter().next() else {
        return Err(ChatError::Mid(anyhow!(
            "réponse vide du serveur : {}",
            raw.chars().take(200).collect::<String>()
        )));
    };
    if let Some(r) = choice.message.reasoning_content.as_deref() {
        if !r.is_empty() {
            emit_in_chunks(r, ui, true);
        }
    }
    let raw_content = choice.message.content.unwrap_or_default();
    if !raw_content.is_empty() {
        emit_in_chunks(&raw_content, ui, false);
    }
    let (content, mut calls) = finalize_content(raw_content, options);
    calls.extend(choice.message.tool_calls.unwrap_or_default());
    Ok((content, calls, choice.finish_reason))
}

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

fn attribution(req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    req.header(
        "HTTP-Referer",
        format!("https://github.com/{}", crate::update::REPO),
    )
    .header("X-Title", concat!("ZER0|GH0ST/", env!("CARGO_PKG_VERSION")))
}

fn sanitize_content(s: &str) -> String {
    collapse_blank_lines(&strip_special_tokens(s))
}

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
    fn non_stream_total_timeout_defaults_to_600s() {
        assert_eq!(parse_total_secs(None).as_secs(), 600);
        assert_eq!(parse_total_secs(Some("900")).as_secs(), 900);
        assert_eq!(parse_total_secs(Some("-1")).as_secs(), 600);
        let non_stream = ChatOptions {
            stream: false,
            ..ChatOptions::default()
        };
        assert_eq!(request_timeout(&non_stream).as_secs(), 600);
        let streaming = ChatOptions::default();
        assert!(request_timeout(&streaming).as_secs() > 600);
    }

    #[test]
    fn stream_idle_defaults_to_240s_and_honours_env() {
        assert_eq!(parse_idle_secs(None).as_secs(), 240);
        assert_eq!(parse_idle_secs(Some("600")).as_secs(), 600);
        assert_eq!(parse_idle_secs(Some("0")).as_secs(), 240);
        assert_eq!(parse_idle_secs(Some("nope")).as_secs(), 240);
    }

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
