use crate::config::{self, Settings};
use crate::llm::llama::ApiClient;
use crate::models;
use crate::providers;
use crate::select::{self, Category};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// One concrete place to send a request.
#[derive(Debug, Clone)]
pub struct Endpoint {
    pub provider_id: String,
    pub label: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
}

impl Endpoint {
    pub fn client(&self) -> ApiClient {
        ApiClient::new(self.base_url.clone(), self.model.clone(), self.api_key.clone())
    }
}

pub fn from_settings(s: &Settings) -> Endpoint {
    let model = normalize_model(&s.provider_id, &s.model);
    Endpoint {
        provider_id: s.provider_id.clone(),
        label: format!("{}/{}", s.provider_id, model),
        base_url: s.base_url.clone(),
        api_key: s.api_key.clone(),
        model,
    }
}

/// Users often paste OpenRouter-style slugs (`nvidia/...`) into another provider's
/// `model`. Native APIs expect the bare id. Strip a leading `{provider}/` unless the
/// provider is OpenRouter (whose namespace is meaningful).
fn normalize_model(provider_id: &str, model: &str) -> String {
    if provider_id != "openrouter" {
        if let Some(rest) = model.strip_prefix(&format!("{provider_id}/")) {
            if !rest.is_empty() {
                return rest.to_string();
            }
        }
    }
    model.to_string()
}

/// Parse a `provider/model` route string into an endpoint.
pub fn parse_route(route: &str) -> Option<Endpoint> {
    let (provider_id, model) = route.split_once('/')?;
    let provider = providers::find(provider_id)?;
    if model.is_empty() {
        return None;
    }
    let model = normalize_model(&provider.id, model);
    Some(Endpoint {
        provider_id: provider.id.to_string(),
        label: format!("{}/{}", provider.id, model),
        base_url: provider.base_url.to_string(),
        api_key: config::key_for(&provider),
        model,
    })
}

/// Roles Zer0 knows about.
pub const KNOWN_ROLES: &[&str] = &[
    "main", "code", "reasoning", "writing", "vision", "fast", "utility", "review",
];

/// Default role for a detected task category.
pub fn role_for_category(cat: Category) -> &'static str {
    match cat {
        Category::Code => "code",
        Category::Reasoning | Category::Math => "reasoning",
        Category::Writing => "writing",
        Category::Vision => "vision",
        Category::Fast => "fast",
        Category::General => "main",
    }
}

/// Pick a role automatically from the user's text.
pub fn auto_role(text: &str) -> &'static str {
    role_for_category(select::classify(text))
}

/// Endpoint chain for a role: the role's model first, then the normal chain.
/// Falls back to the `main` role, then to the active endpoint.
pub fn chain_for_role(settings: &Settings, role: &str) -> Vec<Endpoint> {
    let roles = config::effective_roles();
    let route = roles
        .get(role)
        .or_else(|| roles.get("main"))
        .cloned();

    let mut chain: Vec<Endpoint> = Vec::new();
    if let Some(r) = route {
        if let Some(ep) = parse_route(&r) {
            chain.push(ep);
        }
    }
    for ep in build_chain(settings) {
        let dup = chain
            .iter()
            .any(|e| e.base_url == ep.base_url && e.model == ep.model);
        if !dup {
            chain.push(ep);
        }
    }

    let alive: Vec<Endpoint> = chain
        .iter()
        .filter(|e| !is_failed(&e.label))
        .cloned()
        .collect();
    if alive.is_empty() {
        chain
    } else {
        alive
    }
}

/// Ordered chain: active endpoint first, then configured failover routes (deduped).
/// Recently-failed endpoints are skipped (unless that would leave nothing).
pub fn build_chain(settings: &Settings) -> Vec<Endpoint> {
    let mut chain = vec![from_settings(settings)];
    for r in config::effective_routes() {
        if let Some(ep) = parse_route(&r) {
            let dup = chain
                .iter()
                .any(|e| e.base_url == ep.base_url && e.model == ep.model);
            if !dup {
                chain.push(ep);
            }
        }
    }
    let alive: Vec<Endpoint> = chain
        .iter()
        .filter(|e| !is_failed(&e.label))
        .cloned()
        .collect();
    if alive.is_empty() {
        chain
    } else {
        alive
    }
}

// --------------------------------------------------- model failure memory

/// How long an endpoint stays blacklisted after a failure.
const FAILURE_TTL: Duration = Duration::from_secs(120);

fn failures() -> &'static Mutex<HashMap<String, Instant>> {
    static F: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    F.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Remember that an endpoint just failed (gated model, rate limit, …).
pub fn mark_failed(label: &str) {
    if let Ok(mut m) = failures().lock() {
        m.insert(label.to_string(), Instant::now());
    }
}

/// Clear the failure mark after a success.
pub fn clear_failed(label: &str) {
    if let Ok(mut m) = failures().lock() {
        m.remove(label);
    }
}

/// True while the endpoint is blacklisted (and not expired).
pub fn is_failed(label: &str) -> bool {
    let Ok(mut m) = failures().lock() else {
        return false;
    };
    match m.get(label) {
        Some(t) if t.elapsed() < FAILURE_TTL => true,
        Some(_) => {
            m.remove(label);
            false
        }
        None => false,
    }
}

/// Append a route to the global config (no-op if already present).
pub fn add_global_route(route: &str) -> anyhow::Result<()> {
    let mut g = config::load_global();
    if !g.routes.iter().any(|r| r == route) {
        g.routes.push(route.to_string());
        config::save_global(&g)?;
    }
    Ok(())
}

pub fn clear_global_routes() -> anyhow::Result<()> {
    let mut g = config::load_global();
    g.routes.clear();
    config::save_global(&g)?;
    Ok(())
}

/// Overwrite the global failover chain.
pub fn set_global_routes(routes: Vec<String>) -> anyhow::Result<()> {
    let mut g = config::load_global();
    g.routes = routes;
    config::save_global(&g)?;
    Ok(())
}

/// Auto-assign each role to the best free model for its category.
pub async fn auto_role_assignments(provider_id: &str) -> anyhow::Result<Vec<(String, String)>> {
    let provider = providers::find(provider_id)
        .ok_or_else(|| anyhow::anyhow!("provider inconnu: {provider_id}"))?;
    let key = config::key_for(&provider);
    let http = reqwest::Client::new();
    let list = models::list(&http, &provider, key.as_deref()).await?;
    let free: Vec<_> = list.into_iter().filter(models::is_free).collect();

    let plan: &[(&str, &str)] = &[
        ("main", "général raisonnement"),
        ("code", "code programmation"),
        ("reasoning", "raisonnement analyse"),
        ("writing", "rédaction rédige"),
        ("vision", "image vision"),
        ("fast", "rapide simple"),
        ("utility", "rapide simple"),
        ("review", "code revue correction"),
    ];

    let mut out = Vec::new();
    for (role, problem) in plan {
        let (_, ranked) = select::rank(problem, &free, 1);
        if let Some((_, m)) = ranked.first() {
            out.push((role.to_string(), format!("{}/{}", provider.id, m.id)));
        }
    }
    Ok(out)
}

/// Build a failover chain from the provider's free models, ranked for `problem`.
pub async fn auto_free_routes(
    provider_id: &str,
    problem: &str,
    top: usize,
) -> anyhow::Result<Vec<Endpoint>> {
    let provider = providers::find(provider_id)
        .ok_or_else(|| anyhow::anyhow!("provider inconnu: {provider_id}"))?;
    let key = config::key_for(&provider);
    let http = reqwest::Client::new();
    let list = models::list(&http, &provider, key.as_deref()).await?;
    let free: Vec<_> = list.into_iter().filter(models::is_free).collect();
    let (_, ranked) = select::rank(problem, &free, top);
    let key = config::key_for(&provider);
    Ok(ranked
        .into_iter()
        .map(|(_, m)| Endpoint {
            provider_id: provider.id.clone(),
            label: format!("{}/{}", provider.id, m.id),
            base_url: provider.base_url.clone(),
            api_key: key.clone(),
            model: m.id,
        })
        .collect())
}
