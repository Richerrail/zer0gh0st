//! `zer0-gw` — passerelle de clés (sidecar ZER0│GH0ST).
//!
//! Problème : le coffre chiffre les clés **au repos**, mais tant que la clé est
//! résolue dans le processus agent (`config::key_for`), un agent compromis peut
//! la lire en mémoire et l'envoyer ailleurs. Solution : **la clé ne vit plus
//! dans l'agent**.
//!
//! - `zer0-gw` est le seul processus qui détient les clés (`/etc/zer0-gw`,
//!   lisible par l'utilisateur `zer0-gw` uniquement).
//! - L'agent appelle `http://127.0.0.1:8787/{provider}/v1/...` **sans aucune clé**.
//! - La passerelle injecte l'en-tête `Authorization` vers le vrai provider.
//!
//! Réduction de surface :
//! - loopback uniquement (rejet explicite de toute autre source) ;
//! - méthodes POST/GET seulement ;
//! - chemins en liste blanche (défaut : `/v1/chat/completions`, `/v1/models`,
//!   `/v1/embeddings` — ce dernier sert à la mémoire sémantique) ;
//! - rate-limit par provider (fenêtre fixe d'une minute) ;
//! - plafond de taille de requête, timeouts (connect + total) ;
//! - pas de suivi de redirections (pas de relais implicite) ;
//! - **journal JSON sans corps de requête et sans clé** — jamais.
//!
//! Ce que ça ne protège pas : l'abus du relais (l'agent peut demander un appel
//! provider et consommer du quota — d'où rate-limit + plafonds côté provider),
//! et tout ce qui reste listé dans SAFETY.md. Un processus qui exécute du code
//! arbitraire n'est pas devenu digne de confiance : sa clé est devenue
//! inatteignable, c'est différent.

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::Context as _;
use axum::{
    body::Body,
    extract::{ConnectInfo, DefaultBodyLimit, Path, State},
    http::{HeaderMap, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

// ─────────────────────────────────────────────────────────────────────────────
// Configuration (TOML, ex. /etc/zer0-gw/gw.toml, chmod 640 root:zer0-gw)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct GwConfig {
    #[serde(default = "default_listen")]
    listen: String,
    #[serde(default)]
    limits: Limits,
    #[serde(default)]
    providers: HashMap<String, ProviderCfg>,
    /// Fichier des clés déposées (géré par la passerelle ; défaut :
    /// `keys.managed` à côté de gw.toml).
    #[serde(default)]
    keys_file: Option<String>,
}

fn default_listen() -> String {
    "127.0.0.1:8787".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct Limits {
    /// Appels max par provider et par minute (l'agent partage ce plafond).
    requests_per_minute: u32,
    /// Plafond de taille du corps de requête (octets).
    max_body_bytes: usize,
    connect_timeout_secs: u64,
    /// Timeout total par appel (SSE long inclus).
    upstream_timeout_secs: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            requests_per_minute: 120,
            max_body_bytes: 4 * 1024 * 1024,
            connect_timeout_secs: 10,
            upstream_timeout_secs: 300,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ProviderCfg {
    /// Base du provider, sans le suffixe `/v1/...`.
    /// Ex. OpenRouter : `https://openrouter.ai/api` ; llama-server : `http://127.0.0.1:8080`.
    base_url: String,
    /// Clé en clair (déconseillé) — préférer `api_key_env`.
    #[serde(default)]
    api_key: Option<String>,
    /// Nom de la variable d'environnement contenant la clé (via `EnvironmentFile`).
    #[serde(default)]
    api_key_env: Option<String>,
    /// Chemins autorisés pour ce provider (défaut : chat/completions + models).
    #[serde(default)]
    allowed_paths: Option<Vec<String>>,
}

/// Résolution de la clé d'un provider (**pur** : testable sans muter
/// l'environnement — les tests Rust tournent en parallèle, y muter l'env
/// serait une course et rendait la CI instable).
/// Ordre : clé littérale → variable d'environnement.
fn key_from(literal: Option<&str>, env_value: Option<&str>) -> Option<String> {
    if let Some(k) = literal.map(str::trim).filter(|k| !k.is_empty()) {
        return Some(k.to_owned());
    }
    env_value
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

impl ProviderCfg {
    /// Résout la clé (variable d'env prioritaire dans les faits : `api_key` ne
    /// devrait servir qu'aux tests). Aucune clé n'est jamais journalisée.
    fn key(&self) -> Option<String> {
        let env_value = self
            .api_key_env
            .as_ref()
            .and_then(|e| std::env::var(e).ok());
        key_from(self.api_key.as_deref(), env_value.as_deref())
    }
}

impl GwConfig {
    fn load(path: &str) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("lecture de la configuration ({path})"))?;
        let cfg: GwConfig = toml::from_str(&raw).context("configuration invalide")?;
        anyhow::ensure!(!cfg.providers.is_empty(), "aucun [providers.*] défini");
        for (id, p) in &cfg.providers {
            anyhow::ensure!(
                p.base_url.starts_with("http://") || p.base_url.starts_with("https://"),
                "provider `{id}` : base_url doit commencer par http(s)://"
            );
        }
        Ok(cfg)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// État + rate-limit (fenêtre fixe d'une minute, par provider)
// ─────────────────────────────────────────────────────────────────────────────

struct AppState {
    cfg: GwConfig,
    http: reqwest::Client,
    windows: Mutex<HashMap<String, Window>>,
    /// Clés déposées via `/admin/keys` (id provider → clé). Jamais journalisées.
    keys: Mutex<HashMap<String, String>>,
    /// Fichier de persistance (ex. `/etc/zer0-gw/keys.env`).
    keys_path: String,
}

/// Résolution d'une clé, par ordre : dépôt admin → variable d'env → gw.toml.
fn resolve_key(keys: &HashMap<String, String>, id: &str, pcfg: &ProviderCfg) -> Option<String> {
    if let Some(k) = keys.get(id) {
        return Some(k.clone());
    }
    pcfg.key()
}

impl AppState {
    fn key_for(&self, id: &str, pcfg: &ProviderCfg) -> Option<String> {
        resolve_key(&self.keys.lock().expect("mutex"), id, pcfg)
    }
}

/// Charge le fichier des clés déposées : lignes `provider=clé` (commentaires `#`
/// ignorés). Fichier entièrement géré par la passerelle.
fn load_keys(path: &str) -> HashMap<String, String> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    raw.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .filter(|(_, v)| !v.is_empty())
        .collect()
}

/// Réécrit le fichier des clés déposées (fichier **entièrement géré**) puis
/// chmod 600. Écriture atomique (tmp + rename) : jamais d'état partiel exposé.
fn save_keys(path: &str, keys: &HashMap<String, String>) -> std::io::Result<()> {
    let mut out = String::from("# Clés déposées via /provider add — fichier géré par zer0-gw.\n");
    let mut entries: Vec<(&String, &String)> = keys.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    for (id, key) in entries {
        out.push_str(&format!("{id}={key}\n"));
    }
    let tmp = format!("{path}.tmp");
    std::fs::write(&tmp, out)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, path)
}

#[derive(Debug)]
struct Window {
    count: u32,
    start: Instant,
}

fn window_allow(windows: &mut HashMap<String, Window>, provider: &str, rpm: u32) -> bool {
    let win = windows
        .entry(provider.to_owned())
        .or_insert(Window { count: 0, start: Instant::now() });
    if win.start.elapsed() >= Duration::from_secs(60) {
        win.count = 0;
        win.start = Instant::now();
    }
    if win.count >= rpm {
        false
    } else {
        win.count += 1;
        true
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Routage / sécurité
// ─────────────────────────────────────────────────────────────────────────────

/// Découpe `/{provider}/v1/...` → `("provider", "v1/...")`.
fn split_provider(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix('/')?;
    let (provider, subpath) = rest.split_once('/')?;
    if provider.is_empty() || !subpath.starts_with("v1/") {
        return None;
    }
    Some((provider, subpath))
}

/// `true` si le dernier segment du chemin est une version d'API (`v1`, `v4`, `v1beta`…).
fn ends_with_api_version(base: &str) -> bool {
    matches!(base.rsplit('/').next(), Some(seg)
        if seg.len() > 1 && seg.as_bytes()[0] == b'v' && seg.as_bytes()[1].is_ascii_digit())
}

/// `v1/chat/completions` → `Some("chat/completions")` (retire le segment de version).
fn strip_version_prefix(sub: &str) -> Option<&str> {
    let (seg, rest) = sub.split_once('/')?;
    let b = seg.as_bytes();
    if b.len() > 1 && b[0] == b'v' && b[1].is_ascii_digit() {
        Some(rest)
    } else {
        None
    }
}

/// Construit l'URL upstream : `base_url` + `/` + `subpath` (+ query).
/// Le `/` est indispensable (`...com` + `v1/models` → `...comv1/models` sinon).
/// Si la base contient déjà une version (`…/paas/v4`) et que le sous-chemin
/// commence aussi par une version, on retire celle du sous-chemin : sinon on
/// obtient `…/paas/v4/v1/chat/completions` → 404 (cas z.ai).
fn upstream_url(base_url: &str, subpath: &str, query: Option<&str>) -> String {
    let base = base_url.trim_end_matches('/');
    let sub = if ends_with_api_version(base) {
        strip_version_prefix(subpath).unwrap_or(subpath)
    } else {
        subpath
    };
    let mut url = format!("{base}/{sub}");
    if let Some(q) = query {
        url.push('?');
        url.push_str(q);
    }
    url
}

/// Liste blanche de chemins : défaut conservateur, surchargeable par provider.
fn path_allowed(pcfg: &ProviderCfg, subpath: &str) -> bool {
    let want = format!("/{subpath}");
    match &pcfg.allowed_paths {
        Some(list) => list.iter().any(|p| p.trim_end_matches('/') == want),
        None => matches!(
            want.as_str(),
            "/v1/chat/completions" | "/v1/models" | "/v1/embeddings"
        ),
    }
}

fn err(status: StatusCode, msg: &str) -> Response {
    (status, format!("{msg}\n")).into_response()
}

/// Journal JSON sur stderr (journald) — jamais de corps, jamais de clé.
fn log_line(event: &str, fields: serde_json::Value) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut v = serde_json::json!({ "ts": ts, "event": event });
    if let (Some(obj), Some(extra)) = (v.as_object_mut(), fields.as_object()) {
        for (k, val) in extra {
            obj.insert(k.clone(), val.clone());
        }
    }
    eprintln!("{v}");
}

async fn proxy(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let started = Instant::now();

    // 1) Loopback uniquement — la passerelle n'existe que pour l'agent local.
    if !peer.ip().is_loopback() {
        return err(StatusCode::FORBIDDEN, "accès loopback uniquement");
    }

    // 2) Format de chemin et provider connu.
    let Some((provider_id, subpath)) = split_provider(uri.path()) else {
        return err(StatusCode::NOT_FOUND, "format attendu : /{provider}/v1/…");
    };
    let Some(pcfg) = state.cfg.providers.get(provider_id) else {
        return err(StatusCode::NOT_FOUND, "provider inconnu");
    };

    // 3) Méthode et chemin en liste blanche.
    if !matches!(method, Method::POST | Method::GET) {
        return err(StatusCode::METHOD_NOT_ALLOWED, "seules POST et GET sont autorisées");
    }
    if !path_allowed(pcfg, subpath) {
        return err(StatusCode::FORBIDDEN, "chemin non autorisé");
    }

    // 4) Rate-limit par provider.
    let rpm = state.cfg.limits.requests_per_minute;
    if !window_allow(&mut state.windows.lock().expect("mutex"), provider_id, rpm) {
        return err(StatusCode::TOO_MANY_REQUESTS, "rate-limit zer0-gw (réessayez plus tard)");
    }

    // 5) Construction de l'appel upstream : la clé est injectée ICI, jamais
    //    transmise par l'agent (l'en-tête Authorization entrant est ignoré).
    let model = serde_json::from_slice::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("model").and_then(serde_json::Value::as_str).map(str::to_owned));

    let url = upstream_url(&pcfg.base_url, subpath, uri.query());

    let Ok(rm) = reqwest::Method::from_bytes(method.as_str().as_bytes()) else {
        return err(StatusCode::METHOD_NOT_ALLOWED, "méthode invalide");
    };

    let mut req = state
        .http
        .request(rm, &url)
        .timeout(Duration::from_secs(state.cfg.limits.upstream_timeout_secs));
    for name in ["content-type", "accept"] {
        if let Some(v) = headers.get(name) {
            req = req.header(name, v);
        }
    }
    if let Some(key) = state.key_for(provider_id, pcfg) {
        req = req.bearer_auth(key);
    }

    let req_bytes = body.len();
    let upstream = match req.body(body).send().await {
        Ok(r) => r,
        Err(e) => {
            log_line(
                "upstream_error",
                serde_json::json!({ "provider": provider_id, "error": e.to_string() }),
            );
            return err(StatusCode::BAD_GATEWAY, "upstream injoignable");
        }
    };

    // 6) Réponse en flux (SSE inclus), seul content-type est recopié.
    let status = upstream.status();
    let ct = upstream
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    let code = StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut builder = Response::builder().status(code);
    if let Some(ct) = ct {
        builder = builder.header("content-type", ct);
    }

    let resp = match builder.body(Body::from_stream(upstream.bytes_stream())) {
        Ok(r) => r,
        Err(e) => {
            log_line(
                "response_error",
                serde_json::json!({ "provider": provider_id, "error": e.to_string() }),
            );
            return err(StatusCode::INTERNAL_SERVER_ERROR, "erreur de réponse");
        }
    };

    log_line(
        "proxy",
        serde_json::json!({
            "peer": peer.to_string(),
            "provider": provider_id,
            "path": subpath,
            "model": model,
            "status": status.as_u16(),
            "req_bytes": req_bytes,
            "duration_ms": started.elapsed().as_millis() as u64,
        }),
    );

    resp
}

// ────────────────────────────────────────────────
// Endpoints admin (loopback + écriture : déposer une clé, jamais la lire)
// ────────────────────────────────────────────────

fn loopback_or_403(peer: SocketAddr) -> Option<Response> {
    (!peer.ip().is_loopback()).then(|| err(StatusCode::FORBIDDEN, "accès loopback uniquement"))
}

/// `GET /health` — état de la passerelle + *présence* des clés (jamais leur valeur).
async fn health(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> Response {
    if let Some(r) = loopback_or_403(peer) {
        return r;
    }
    let keys = state.keys.lock().expect("mutex");
    let providers: serde_json::Map<String, serde_json::Value> = state
        .cfg
        .providers
        .iter()
        .map(|(id, p)| {
            (
                id.clone(),
                serde_json::json!({ "has_key": resolve_key(&keys, id, p).is_some() }),
            )
        })
        .collect();
    Json(serde_json::json!({ "ok": true, "providers": providers })).into_response()
}

/// `GET /admin/keys` — providers et présence d'une clé (booléen, jamais la valeur).
async fn admin_list(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> Response {
    if let Some(r) = loopback_or_403(peer) {
        return r;
    }
    let keys = state.keys.lock().expect("mutex");
    let providers: serde_json::Map<String, serde_json::Value> = state
        .cfg
        .providers
        .iter()
        .map(|(id, p)| {
            (
                id.clone(),
                serde_json::json!({
                    "has_key": resolve_key(&keys, id, p).is_some(),
                    "env_key": p.api_key_env,
                }),
            )
        })
        .collect();
    Json(serde_json::json!({ "ok": true, "keys": providers })).into_response()
}

/// `POST /admin/keys/:provider` — dépose une clé (`{"api_key": "…"}`).
/// Écriture seule : la valeur n'est jamais renvoyée ni journalisée.
async fn admin_set(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(provider): Path<String>,
    body: axum::body::Bytes,
) -> Response {
    if let Some(r) = loopback_or_403(peer) {
        return r;
    }
    let id = provider.trim().to_ascii_lowercase();
    if !state.cfg.providers.contains_key(&id) {
        return err(StatusCode::NOT_FOUND, "provider inconnu de zer0-gw");
    }
    let parsed: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return err(StatusCode::BAD_REQUEST, "corps JSON attendu : {\"api_key\":\"…\"}"),
    };
    let key = parsed
        .get("api_key")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|k| !k.is_empty() && k.len() <= 4096);
    let Some(key) = key else {
        return err(StatusCode::BAD_REQUEST, "api_key manquante ou invalide");
    };
    {
        let mut keys = state.keys.lock().expect("mutex");
        keys.insert(id.clone(), key.to_string());
        if let Err(e) = save_keys(&state.keys_path, &keys) {
            log_line(
                "admin_persist_error",
                serde_json::json!({ "provider": id, "error": e.to_string() }),
            );
            return err(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("échec d'écriture du fichier de clés : {e}"),
            );
        }
    }
    log_line("admin_key_set", serde_json::json!({ "provider": id }));
    Json(serde_json::json!({ "ok": true, "provider": id })).into_response()
}

/// `DELETE /admin/keys/:provider` — oublie une clé (idempotent).
async fn admin_delete(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(provider): Path<String>,
) -> Response {
    if let Some(r) = loopback_or_403(peer) {
        return r;
    }
    let id = provider.trim().to_ascii_lowercase();
    {
        let mut keys = state.keys.lock().expect("mutex");
        keys.remove(&id);
        if let Err(e) = save_keys(&state.keys_path, &keys) {
            log_line(
                "admin_persist_error",
                serde_json::json!({ "provider": id, "error": e.to_string() }),
            );
            return err(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("échec d'écriture du fichier de clés : {e}"),
            );
        }
    }
    log_line("admin_key_deleted", serde_json::json!({ "provider": id }));
    Json(serde_json::json!({ "ok": true, "provider": id })).into_response()
}

fn router(state: Arc<AppState>) -> Router {
    let max_body = state.cfg.limits.max_body_bytes;
    Router::new()
        .route("/health", get(health))
        .route("/admin/keys", get(admin_list))
        .route("/admin/keys/:provider", post(admin_set).delete(admin_delete))
        .fallback(proxy)
        .layer(DefaultBodyLimit::max(max_body))
        .with_state(state)
}

// ─────────────────────────────────────────────────────────────────────────────
// main
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/etc/zer0-gw/gw.toml".to_owned());
    let cfg = GwConfig::load(&path)?;

    if !cfg.listen.starts_with("127.0.0.1") && !cfg.listen.starts_with("[::1]") {
        log_line(
            "warn",
            serde_json::json!({
                "msg": "écoute non-loopback — zer0-gw est conçu pour 127.0.0.1",
                "listen": cfg.listen,
            }),
        );
    }

    let http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(cfg.limits.connect_timeout_secs))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("zer0-gw/", env!("CARGO_PKG_VERSION")))
        .build()?;

    // Fichier des clés déposées : `keys_file` de gw.toml, sinon keys.managed
    // à côté de gw.toml (keys.env reste réservé aux variables d'env systemd).
    let keys_path = cfg.keys_file.clone().unwrap_or_else(|| {
        std::path::Path::new(&path)
            .parent()
            .map(|d| d.join("keys.managed").to_string_lossy().into_owned())
            .unwrap_or_else(|| "keys.managed".to_string())
    });
    let mut keys = load_keys(&keys_path);
    // Ne gérer que les clés des providers connus : le reste (variables d'env
    // MAJUSCULES) est laissé au fichier et rechargé par systemd.
    keys.retain(|id, _| cfg.providers.contains_key(id));

    let state = Arc::new(AppState {
        cfg: cfg.clone(),
        http,
        windows: Mutex::new(HashMap::new()),
        keys: Mutex::new(keys.clone()),
        keys_path: keys_path.clone(),
    });

    let app = router(state);
    let addr: SocketAddr = cfg.listen.parse().context("listen invalide")?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    log_line(
        "start",
        serde_json::json!({
            "addr": addr.to_string(),
            "providers": cfg.providers.keys().cloned().collect::<Vec<_>>(),
            "keys_path": keys_path,
            "keys_loaded": keys.len(),
        }),
    );

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn pcfg(base: &str, allowed: Option<Vec<&str>>) -> ProviderCfg {
        ProviderCfg {
            base_url: base.to_owned(),
            api_key: None,
            api_key_env: None,
            allowed_paths: allowed.map(|l| l.into_iter().map(str::to_owned).collect()),
        }
    }

    #[test]
    fn split_provider_ok() {
        assert_eq!(
            split_provider("/openrouter/v1/chat/completions"),
            Some(("openrouter", "v1/chat/completions"))
        );
        assert_eq!(split_provider("/local/v1/models"), Some(("local", "v1/models")));
    }

    #[test]
    fn split_provider_rejects() {
        // pas de provider → rejet
        assert_eq!(split_provider("/v1/chat/completions"), None);
        // pas de suffixe /v1/ → rejet
        assert_eq!(split_provider("/openrouter/keys"), None);
        assert_eq!(split_provider("/openrouter"), None);
        assert_eq!(split_provider("/"), None);
    }

    #[test]
    fn default_allowlist_is_conservative() {
        let p = pcfg("https://x.example", None);
        assert!(path_allowed(&p, "v1/chat/completions"));
        assert!(path_allowed(&p, "v1/models"));
        assert!(path_allowed(&p, "v1/embeddings")); // mémoire sémantique
        assert!(!path_allowed(&p, "v1/keys"));
        assert!(!path_allowed(&p, "admin/v1/chat/completions"));
        assert!(!path_allowed(&p, "v1/chat/completions/../../secret"));
    }

    #[test]
    fn custom_allowlist_overrides() {
        let p = pcfg("https://x.example", Some(vec!["/v1/embeddings"]));
        assert!(path_allowed(&p, "v1/embeddings"));
        assert!(!path_allowed(&p, "v1/chat/completions"));
    }

    #[test]
    fn rate_window_limits_per_minute() {
        let mut w = HashMap::new();
        assert!(window_allow(&mut w, "openrouter", 2));
        assert!(window_allow(&mut w, "openrouter", 2));
        assert!(!window_allow(&mut w, "openrouter", 2));
        // l'autre provider a son propre compteur
        assert!(window_allow(&mut w, "nvidia", 2));
    }

    #[test]
    fn key_from_env_and_blank_ignored() {
        // Fonction **pure** : pas de mutation d'environnement (course en
        // parallèle → la CI devenait instable).
        // Littéral vide → ignoré.
        assert_eq!(key_from(Some("  "), None), None);
        // Littéral → il prime.
        assert_eq!(key_from(Some("sk-test"), Some("sk-env")), Some("sk-test".to_owned()));
        // Sinon la variable d'environnement.
        assert_eq!(key_from(None, Some("sk-env")), Some("sk-env".to_owned()));
        // Variable vide → ignorée.
        assert_eq!(key_from(None, Some("  ")), None);
    }

    #[test]
    fn config_rejects_bad_base_url() {
        let dir = std::env::temp_dir().join("zer0-gw-test-bad");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("gw.toml");
        std::fs::write(
            &path,
            "[providers.x]\nbase_url = \"ftp://non-http\"\n",
        )
        .unwrap();
        assert!(GwConfig::load(path.to_str().unwrap()).is_err());
    }

    #[test]
    fn upstream_url_joins_with_slash() {
        assert_eq!(
            upstream_url("https://integrate.api.nvidia.com", "v1/models", None),
            "https://integrate.api.nvidia.com/v1/models"
        );
        // base avec slash final → un seul slash
        assert_eq!(
            upstream_url("https://openrouter.ai/api/", "v1/chat/completions", Some("a=1")),
            "https://openrouter.ai/api/v1/chat/completions?a=1"
        );
        // base SANS version → version du sous-chemin conservée
        assert_eq!(
            upstream_url("https://openrouter.ai/api", "v1/models", None),
            "https://openrouter.ai/api/v1/models"
        );
        // base AVEC version (z.ai) → pas de double préfixe
        assert_eq!(
            upstream_url("https://api.z.ai/api/paas/v4", "v1/chat/completions", None),
            "https://api.z.ai/api/paas/v4/chat/completions"
        );
        assert_eq!(
            upstream_url("https://api.z.ai/api/paas/v4", "v1/models", None),
            "https://api.z.ai/api/paas/v4/models"
        );
    }

    #[test]
    fn resolve_key_order_store_env_literal() {
        let mut keys = HashMap::new();
        let mut p = pcfg("https://x.example", None);
        p.api_key = Some("literal".to_owned());
        p.api_key_env = Some("ZGW_UNSET_KEY".to_owned());
        // littéral seul
        assert_eq!(resolve_key(&keys, "x", &p), Some("literal".to_owned()));
        // dépôt admin prioritaire
        keys.insert("x".to_owned(), "deposed".to_owned());
        assert_eq!(resolve_key(&keys, "x", &p), Some("deposed".to_owned()));
    }

    #[test]
    fn save_and_load_keys_roundtrip_and_remove() {
        let dir = std::env::temp_dir().join("zer0-gw-test-keys");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("keys.managed");

        let mut keys = HashMap::new();
        keys.insert("openrouter".to_owned(), "sk-test".to_owned());
        keys.insert("nvidia".to_owned(), "nvapi-test".to_owned());
        save_keys(path.to_str().unwrap(), &keys).unwrap();

        let reloaded = load_keys(path.to_str().unwrap());
        assert_eq!(reloaded.get("openrouter").map(String::as_str), Some("sk-test"));
        assert_eq!(reloaded.get("nvidia").map(String::as_str), Some("nvapi-test"));

        // Retrait : la clé doit disparaître du fichier (fichier 100 % géré).
        keys.remove("openrouter");
        save_keys(path.to_str().unwrap(), &keys).unwrap();
        let reloaded = load_keys(path.to_str().unwrap());
        assert!(!reloaded.contains_key("openrouter"), "clé retirée encore présente");
        assert_eq!(reloaded.get("nvidia").map(String::as_str), Some("nvapi-test"));
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("# Clés déposées"));
    }
}
