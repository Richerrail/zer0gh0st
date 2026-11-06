use crate::config::{self, Settings};
use crate::http_client;
use crate::llm::llama::ApiClient;
use crate::models;
use crate::providers;
use crate::select::{self, Category};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

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
        ApiClient::new(
            self.base_url.clone(),
            self.model.clone(),
            self.api_key.clone(),
        )
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

fn normalize_model(provider_id: &str, model: &str) -> String {
    if let Some(rest) = model.strip_prefix(&format!("{provider_id}/")) {
        if !rest.is_empty() {
            return rest.to_string();
        }
    }
    model.to_string()
}

pub fn parse_route(route: &str) -> Option<Endpoint> {
    let (provider_id, model) = route.split_once('/')?;
    let provider = providers::find(provider_id)?;
    if model.is_empty() {
        return None;
    }
    let model = normalize_model(&provider.id, model);
    let (base_url, api_key) = config::endpoint_for(&provider);
    Some(Endpoint {
        provider_id: provider.id.to_string(),
        label: format!("{}/{}", provider.id, model),
        base_url,
        api_key,
        model,
    })
}

pub const KNOWN_ROLES: &[&str] = &[
    "main",
    "code",
    "reasoning",
    "writing",
    "vision",
    "fast",
    "utility",
    "review",
];

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

pub fn auto_role(text: &str) -> &'static str {
    role_for_category(select::classify(text))
}

pub fn chain_for_role(settings: &Settings, role: &str) -> Vec<Endpoint> {
    chain_for_role_with(settings, role, config::roles_enabled())
}

fn chain_for_role_with(settings: &Settings, role: &str, roles_on: bool) -> Vec<Endpoint> {
    if !roles_on {
        return vec![from_settings(settings)];
    }
    let roles = config::effective_roles();
    let route = roles.get(role).or_else(|| roles.get("main")).cloned();

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

pub fn chain_for_0box_role(settings: &Settings, role: &str) -> Vec<Endpoint> {
    let mut chain = Vec::new();

    if let Some(fallbacks) = config::load_global().zero_box_fallbacks {
        for fb in fallbacks {
            if let Some(ep) = parse_route(&fb) {
                let dup = chain
                    .iter()
                    .any(|e: &Endpoint| e.base_url == ep.base_url && e.model == ep.model);
                if !dup {
                    chain.push(ep);
                }
            }
        }
    }

    let mut role_chain = Vec::new();

    let roles = config::effective_roles();
    let route = roles.get(role).or_else(|| roles.get("main")).cloned();
    if let Some(r) = route {
        if let Some(ep) = parse_route(&r) {
            role_chain.push(ep);
        }
    }

    for ep in build_chain(settings) {
        let dup = role_chain
            .iter()
            .any(|e| e.base_url == ep.base_url && e.model == ep.model);
        if !dup {
            role_chain.push(ep);
        }
    }

    for ep in role_chain {
        let dup = chain
            .iter()
            .any(|e: &Endpoint| e.base_url == ep.base_url && e.model == ep.model);
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

const FAILURE_TTL: Duration = Duration::from_secs(120);

fn failures() -> &'static Mutex<HashMap<String, Instant>> {
    static F: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    F.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn mark_failed(label: &str) {
    if let Ok(mut m) = failures().lock() {
        m.insert(label.to_string(), Instant::now());
    }
}

pub fn clear_failed(label: &str) {
    if let Ok(mut m) = failures().lock() {
        m.remove(label);
    }
}

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

pub fn set_global_routes(routes: Vec<String>) -> anyhow::Result<()> {
    let mut g = config::load_global();
    g.routes = routes;
    config::save_global(&g)?;
    Ok(())
}

pub async fn auto_role_assignments(provider_id: &str) -> anyhow::Result<Vec<(String, String)>> {
    let provider = providers::find(provider_id)
        .ok_or_else(|| anyhow::anyhow!("provider inconnu: {provider_id}"))?;
    let key = config::key_for(&provider);
    let list = models::list(&http_client::HTTP_CLIENT, &provider, key.as_deref()).await?;
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

pub async fn auto_free_routes(
    provider_id: &str,
    problem: &str,
    top: usize,
) -> anyhow::Result<Vec<Endpoint>> {
    let provider = providers::find(provider_id)
        .ok_or_else(|| anyhow::anyhow!("provider inconnu: {provider_id}"))?;
    let key = config::key_for(&provider);
    let list = models::list(&http_client::HTTP_CLIENT, &provider, key.as_deref()).await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_route_rejects_bad() {
        assert!(parse_route("no-slash").is_none());
        assert!(parse_route("unknownprovider/foo").is_none());
        assert!(parse_route("openrouter/").is_none());
    }

    #[test]
    fn provider_prefix_is_not_doubled() {
        let s = Settings {
            provider_id: "openrouter".to_string(),
            provider_name: "OpenRouter".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            model: "openrouter/minimax/minimax-m3.1-flash:free".to_string(),
            api_key: None,
        };
        let ep = from_settings(&s);
        assert_eq!(ep.model, "minimax/minimax-m3.1-flash:free");
        assert_eq!(ep.label, "openrouter/minimax/minimax-m3.1-flash:free");
    }

    #[test]
    fn a_bare_model_id_is_untouched() {
        let s = Settings {
            provider_id: "nvidia".to_string(),
            provider_name: "NVIDIA".to_string(),
            base_url: "https://integrate.api.nvidia.com/v1".to_string(),
            model: "mistralai/mixtral-8x22b-v0.1".to_string(),
            api_key: None,
        };
        assert_eq!(from_settings(&s).model, "mistralai/mixtral-8x22b-v0.1");
    }

    #[test]
    fn roles_off_gives_single_endpoint_without_failover() {
        let s = Settings {
            provider_id: "nvidia".to_string(),
            provider_name: "NVIDIA".to_string(),
            base_url: "https://integrate.api.nvidia.com/v1".to_string(),
            model: "mistralai/mixtral-8x22b-v0.1".to_string(),
            api_key: None,
        };
        let chain = chain_for_role_with(&s, "main", false);
        assert_eq!(
            chain.len(),
            1,
            "rôles off ⇒ un seul endpoint (pas de bascule)"
        );
        assert_eq!(chain[0].model, "mistralai/mixtral-8x22b-v0.1");
        assert_eq!(chain[0].base_url, s.base_url);
    }
}
