//! Tests de la couche réseau/agent contre un **faux serveur local**.
//!
//! C'était le trou principal : `llm/llama.rs` (parseur SSE, assemblage des
//! `tool_calls`) et `agent/turn.rs` (bascule, containment) n'avaient aucun test.
//! Or c'est là que vivent les régressions invisibles : flux tronqué accepté,
//! arguments d'outil coupés en deux, backoff qui ne bascule jamais.
//!
//! Aucune dépendance ajoutée : le serveur est un `TcpListener` tokio qui répond
//! du HTTP/1.1 brut.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::unbounded_channel;

use zer0::agent::history::ChatMessage;
use zer0::agent::turn::run_turn;
use zer0::event::UiEvent;
use zer0::llm::llama::{ApiClient, ChatOptions};
use zer0::route::Endpoint;

// ─────────────────────────────────────────────────────────────────────────────
// Faux serveur : une réponse brute par connexion, dans l'ordre.
// ─────────────────────────────────────────────────────────────────────────────

struct Mock {
    base: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl Mock {
    /// Sert `responses` dans l'ordre (une par connexion acceptée).
    async fn start(responses: Vec<String>) -> Mock {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = requests.clone();
        tokio::spawn(async move {
            for resp in responses {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                if let Some(req) = read_request(&mut sock).await {
                    log.lock().unwrap().push(req);
                }
                let _ = sock.write_all(resp.as_bytes()).await;
                let _ = sock.flush().await;
                let _ = sock.shutdown().await;
            }
        });
        Mock {
            base: format!("http://{addr}/v1"),
            requests,
        }
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

/// Lit en-têtes + corps (Content-Length) d'une requête HTTP.
async fn read_request(sock: &mut TcpStream) -> Option<String> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = sock.read(&mut chunk).await.ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        let text = String::from_utf8_lossy(&buf).to_string();
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let len = head
                .lines()
                .find_map(|l| {
                    let l = l.to_ascii_lowercase();
                    l.strip_prefix("content-length:")
                        .and_then(|v| v.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            if body.len() >= len {
                return Some(text);
            }
        }
    }
    Some(String::from_utf8_lossy(&buf).to_string())
}

fn sse(events: &[&str]) -> String {
    let mut s = String::from(
        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n",
    );
    for e in events {
        s.push_str("data: ");
        s.push_str(e);
        s.push_str("\n\n");
    }
    s
}

fn status(code: &str, extra_headers: &str) -> String {
    format!("HTTP/1.1 {code}\r\n{extra_headers}content-length: 0\r\nconnection: close\r\n\r\n")
}

// ─────────────────────────────────────────────────────────────────────────────
// Client
// ─────────────────────────────────────────────────────────────────────────────

async fn chat(base: &str) -> (String, Vec<zer0::agent::history::ToolCall>, Option<String>) {
    let client = ApiClient::new(base, "test-model", None);
    let (tx, _rx) = unbounded_channel::<UiEvent>();
    client
        .stream_chat(
            "system",
            &[ChatMessage::user("salut")],
            &[],
            &ChatOptions::default(),
            &tx,
        )
        .await
        .expect("le flux doit réussir")
}

#[tokio::test]
async fn streams_content_and_finish_reason() {
    let mock = Mock::start(vec![sse(&[
        r#"{"choices":[{"delta":{"content":"Bonjour"}}]}"#,
        r#"{"choices":[{"delta":{"content":" monde"}}]}"#,
        r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#,
        "[DONE]",
    ])])
    .await;

    let (content, calls, finish) = chat(&mock.base).await;
    assert_eq!(content, "Bonjour monde");
    assert!(calls.is_empty());
    assert_eq!(finish.as_deref(), Some("stop"));
}

/// Le cas qui casse en vrai : un `tool_call` dont le nom et les arguments
/// arrivent en plusieurs morceaux. Le parseur doit recoller sans perdre d'octet.
#[tokio::test]
async fn assembles_a_fragmented_tool_call() {
    let mock = Mock::start(vec![sse(&[
        r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"code_ex","arguments":"{\"comm"}}]}}]}"#,
        r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"name":"ec","arguments":"and\":\"ls -la\"}"}}]}}]}"#,
        r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#,
        "[DONE]",
    ])])
    .await;

    let (_, calls, finish) = chat(&mock.base).await;
    assert_eq!(calls.len(), 1, "un seul appel attendu");
    assert_eq!(calls[0].id, "call_1");
    assert_eq!(calls[0].function.name, "code_exec");
    assert_eq!(calls[0].function.arguments, r#"{"command":"ls -la"}"#);
    assert_eq!(finish.as_deref(), Some("tool_calls"));
}

/// Un flux qui se ferme sans `[DONE]` ni `finish_reason` ne doit **pas** passer
/// pour une réponse complète : on le signale (`finish_reason = "eof"`).
#[tokio::test]
async fn unterminated_stream_is_flagged_not_silent() {
    let mock = Mock::start(vec![sse(&[
        r#"{"choices":[{"delta":{"content":"réponse coup"}}]}"#,
    ])])
    .await;

    let (content, _, finish) = chat(&mock.base).await;
    assert_eq!(content, "réponse coup");
    assert_eq!(
        finish.as_deref(),
        Some("eof"),
        "un flux sans terminaison doit être marqué, pas accepté en silence"
    );
}

/// 429 puis succès : le client honore `Retry-After` et réessaie une fois.
#[tokio::test]
async fn retries_once_on_429_then_succeeds() {
    let mock = Mock::start(vec![
        status("429 Too Many Requests", "retry-after: 0\r\n"),
        sse(&[
            r#"{"choices":[{"delta":{"content":"ok"}}]}"#,
            r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#,
            "[DONE]",
        ]),
    ])
    .await;

    let (content, _, finish) = chat(&mock.base).await;
    assert_eq!(content, "ok");
    assert_eq!(finish.as_deref(), Some("stop"));
    assert_eq!(
        mock.requests().len(),
        2,
        "le 429 doit provoquer un second appel"
    );
}

/// Une erreur de setup (500) n'est pas avalée : elle remonte avec son statut.
#[tokio::test]
async fn setup_error_carries_the_status() {
    let mock = Mock::start(vec![status("500 Internal Server Error", "")]).await;
    let client = ApiClient::new(&mock.base, "m", None);
    let (tx, _rx) = unbounded_channel::<UiEvent>();
    let err = client
        .stream_chat(
            "s",
            &[ChatMessage::user("x")],
            &[],
            &ChatOptions::default(),
            &tx,
        )
        .await
        .expect_err("un 500 doit échouer");
    match err {
        zer0::llm::llama::ChatError::Setup { status, .. } => {
            assert_eq!(status, Some(500));
        }
        other => panic!("attendu Setup, obtenu {other:?}"),
    }
}

/// La bascule d'endpoint : le premier répond 500, le second répond. C'est la
/// logique de `run_turn` qui n'était couverte par aucun test.
#[tokio::test]
async fn run_turn_fails_over_to_the_next_endpoint() {
    let bad = Mock::start(vec![status("503 Service Unavailable", "")]).await;
    let good = Mock::start(vec![sse(&[
        r#"{"choices":[{"delta":{"content":"depuis le second"}}]}"#,
        r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#,
        "[DONE]",
    ])])
    .await;

    // Libellés uniques : `route::mark_failed` est un état global partagé.
    let suffix = std::process::id();
    let endpoints = vec![
        Endpoint {
            provider_id: "bad".into(),
            label: format!("bad-{suffix}"),
            base_url: bad.base.clone(),
            api_key: None,
            model: "m".into(),
        },
        Endpoint {
            provider_id: "good".into(),
            label: format!("good-{suffix}"),
            base_url: good.base.clone(),
            api_key: None,
            model: "m".into(),
        },
    ];

    let (tx, mut rx) = unbounded_channel::<UiEvent>();
    let handle = tokio::spawn(async move {
        run_turn(
            endpoints,
            "system".into(),
            vec![ChatMessage::user("salut")],
            vec![],
            ChatOptions::default(),
            tx,
        )
        .await;
    });

    let mut completed = None;
    let timeout = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(ev) = rx.recv().await {
            if let UiEvent::TurnComplete(msgs) = ev {
                completed = Some(msgs);
                break;
            }
        }
    });
    timeout
        .await
        .expect("pas de TurnComplete (bascule bloquée ?)");
    handle.await.unwrap();

    let msgs = completed.expect("aucune conversation complétée");
    let last = msgs.last().expect("historique vide");
    assert_eq!(last.role, "assistant");
    assert_eq!(last.content.as_deref(), Some("depuis le second"));
    assert_eq!(
        bad.requests().len(),
        1,
        "le premier endpoint doit être appelé"
    );
    assert_eq!(good.requests().len(), 1, "le second doit prendre le relais");
}

/// Les outils déclarés partent bien dans le corps de la requête (sinon le
/// modèle n'a aucun moyen de les appeler).
/// Mode prompt : le modèle écrit `<tool_call>…</tool_call>` dans le contenu.
/// On doit extraire l'appel, retirer le bloc du texte, et **ne pas** envoyer
/// le paramètre `tools` (c'est tout l'intérêt pour un serveur local bridé).
#[tokio::test]
async fn prompt_mode_parses_text_tool_calls() {
    let mock = Mock::start(vec![sse(&[
        r#"{"choices":[{"delta":{"content":"Je regarde. <tool_call>{\"name\":\"code_exec\",\"arguments\":{\"command\":\"ls\"}}</tool_call>"}}]}"#,
        r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#,
        "[DONE]",
    ])])
    .await;
    let client = ApiClient::new(&mock.base, "local", None);
    let (tx, _rx) = unbounded_channel::<UiEvent>();
    let opts = ChatOptions {
        tools_mode: zer0::llm::llama::ToolsMode::Prompt,
        ..ChatOptions::default()
    };
    let (content, calls, _) = client
        .stream_chat(
            "s",
            &[ChatMessage::user("x")],
            &zer0::tools::all(),
            &opts,
            &tx,
        )
        .await
        .unwrap();
    assert!(
        !content.contains("<tool_call>"),
        "bloc non retiré: {content}"
    );
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].function.name, "code_exec");
    let body = mock.requests()[0]
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_string())
        .unwrap_or_default();
    assert!(!body.contains("\"tools\":"), "tools envoyés en mode prompt");
}

#[tokio::test]
async fn tool_specs_are_sent_to_the_provider() {
    let mock = Mock::start(vec![sse(&[
        r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#,
        "[DONE]",
    ])])
    .await;

    let client = ApiClient::new(&mock.base, "m", None);
    let (tx, _rx) = unbounded_channel::<UiEvent>();
    let tools = zer0::tools::all();
    let _ = client
        .stream_chat(
            "s",
            &[ChatMessage::user("x")],
            &tools,
            &ChatOptions::default(),
            &tx,
        )
        .await;

    let req = &mock.requests()[0];
    let body = req.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
    assert!(body.contains(r#""tool_choice":"auto""#), "{body}");
    assert!(body.contains(r#""name":"code_exec""#), "{body}");
}
