use anyhow::{anyhow, Result};
use serde::Deserialize;

use crate::providers::Provider;

/// One model entry as returned by an OpenAI-compatible `/models` endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub name: Option<String>,
    #[serde(default)]
    pub context_length: Option<u64>,
    #[serde(default)]
    pub pricing: Option<Pricing>,
    #[serde(default)]
    pub benchmarks: Option<Benchmarks>,
    #[serde(default)]
    pub architecture: Option<Architecture>,
}

/// Capability indices published by OpenRouter (Artificial Analysis).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Benchmarks {
    #[serde(default)]
    pub artificial_analysis: Option<AaBench>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AaBench {
    #[serde(default)]
    pub intelligence_index: Option<f64>,
    #[serde(default)]
    pub coding_index: Option<f64>,
    #[serde(default)]
    pub agentic_index: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[allow(dead_code)]
pub struct Architecture {
    #[serde(default)]
    pub modality: Option<String>,
    #[serde(default)]
    pub input_modalities: Option<Vec<String>>,
    #[serde(default)]
    pub output_modalities: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Pricing {
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub completion: Option<String>,
}

#[derive(Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<ModelEntry>,
}

/// Query a provider's model list.
pub async fn list(
    http: &reqwest::Client,
    provider: &Provider,
    key: Option<&str>,
) -> Result<Vec<ModelEntry>> {
    let url = format!("{}/models", provider.base_url.trim_end_matches('/'));

    // Google's OpenAI-compat endpoint has no usable /models; use the native one.
    if provider.id == "google" {
        return list_google(http, key).await;
    }

    let mut req = http.get(&url);
    if provider.id == "openrouter" {
        req = req
            .header("HTTP-Referer", "https://github.com/omar0/ZER0v1")
            .header("X-Title", "Zer0");
    }
    if let Some(k) = key {
        req = req.bearer_auth(k);
    }
    let resp = req.send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("{} — {}", status, body.chars().take(200).collect::<String>()));
    }
    let parsed: ModelsResponse = resp.json().await?;
    Ok(parsed.data)
}

/// A model is considered free if its id says so or both prices are zero.
pub fn is_free(m: &ModelEntry) -> bool {
    if m.id.ends_with(":free") || m.id.ends_with("/free") {
        return true;
    }
    match &m.pricing {
        Some(p) => is_zero(&p.prompt) && is_zero(&p.completion),
        None => false,
    }
}

fn is_zero(v: &Option<String>) -> bool {
    matches!(
        v.as_deref(),
        Some("0") | Some("0.0") | Some("0.00") | Some("0.000000") | Some("0.0000")
    )
}

/// Google Gemini native model list (`/v1beta/models?key=…`).
async fn list_google(http: &reqwest::Client, key: Option<&str>) -> Result<Vec<ModelEntry>> {
    let key = key.ok_or_else(|| anyhow!("clé Google requise (GEMINI_API_KEY)"))?;
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models?key={key}&pageSize=200"
    );
    let resp = http.get(&url).send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow!("{status} — {}", body.chars().take(200).collect::<String>()));
    }

    #[derive(Deserialize)]
    struct GResp {
        #[serde(default)]
        models: Vec<GModel>,
    }
    #[derive(Deserialize)]
    struct GModel {
        name: String,
        #[serde(default)]
        display_name: Option<String>,
        #[serde(default)]
        input_token_limit: Option<u64>,
    }

    let parsed: GResp = resp.json().await?;
    Ok(parsed
        .models
        .into_iter()
        .map(|g| ModelEntry {
            id: g.name.strip_prefix("models/").unwrap_or(&g.name).to_string(),
            name: g.display_name,
            context_length: g.input_token_limit,
            pricing: None,
            benchmarks: None,
            architecture: None,
        })
        .collect())
}
