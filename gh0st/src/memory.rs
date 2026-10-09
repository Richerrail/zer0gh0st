use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config;
use crate::providers;

/// One persistent memory entry. Always stored as `untrusted`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub text: String,
    /// Optional query that makes this entry honestly testable later.
    #[serde(default)]
    pub trigger: Option<String>,
    /// Which embedder produced `vector` (never mixed silently).
    #[serde(default)]
    pub embedder: Option<String>,
    #[serde(default)]
    pub vector: Option<Vec<f32>>,
    pub trust: String,
    pub created_at: u64,
    #[serde(default)]
    pub tags: Vec<String>,
}

pub fn store_path() -> PathBuf {
    config::data_dir().join("memory.jsonl")
}

pub fn load() -> Vec<MemoryEntry> {
    let Ok(s) = std::fs::read_to_string(store_path()) else {
        return Vec::new();
    };
    s.lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

fn rewrite(entries: &[MemoryEntry]) -> Result<()> {
    let p = store_path();
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut out = String::new();
    for e in entries {
        out.push_str(&serde_json::to_string(e)?);
        out.push('\n');
    }
    std::fs::write(&p, out)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Delete one entry. Returns true if it existed.
pub fn forget(id: &str) -> Result<bool> {
    let entries = load();
    let before = entries.len();
    let kept: Vec<MemoryEntry> = entries.into_iter().filter(|e| e.id != id).collect();
    let removed = kept.len() != before;
    if removed {
        rewrite(&kept)?;
    }
    Ok(removed)
}

/// Delete all entries.
pub fn clear() -> Result<()> {
    rewrite(&[])
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn new_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("m{nanos:x}")
}

fn append(e: &MemoryEntry) -> Result<()> {
    let p = store_path();
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = OpenOptions::new().create(true).append(true).open(&p)?;
    writeln!(f, "{}", serde_json::to_string(e)?)?;
    Ok(())
}

// ---------------------------------------------------------------- embeddings

/// Configured embedder id, e.g. `local/bge-m3` — or `None` if unset.
pub fn embedder_id() -> Option<String> {
    let g = config::load_global();
    let e = g.embeddings?;
    let provider = e.provider?;
    let model = e.model?;
    Some(format!("{provider}/{model}"))
}

/// Call an OpenAI-compatible `/embeddings` endpoint.
pub async fn embed(text: &str) -> Result<Vec<f32>> {
    let g = config::load_global();
    let e = g
        .embeddings
        .ok_or_else(|| anyhow!("aucun embedder configuré"))?;
    let provider_id = e
        .provider
        .ok_or_else(|| anyhow!("provider d'embedding manquant"))?;
    let model = e.model.ok_or_else(|| anyhow!("modèle d'embedding manquant"))?;
    let provider = providers::find(&provider_id)
        .ok_or_else(|| anyhow!("provider inconnu: {provider_id}"))?;
    let key = config::key_for(&provider);

    let url = format!("{}/embeddings", provider.base_url.trim_end_matches('/'));
    let body = serde_json::json!({ "model": model, "input": text });
    let mut req = reqwest::Client::new().post(&url).json(&body);
    if let Some(k) = key {
        req = req.bearer_auth(k);
    }
    let resp = req.send().await?;
    if !resp.status().is_success() {
        let st = resp.status();
        let t = resp.text().await.unwrap_or_default();
        return Err(anyhow!("embeddings {st}: {}", t.chars().take(160).collect::<String>()));
    }
    let v: serde_json::Value = resp.json().await?;
    let arr = v["data"][0]["embedding"]
        .as_array()
        .ok_or_else(|| anyhow!("réponse d'embedding invalide"))?;
    Ok(arr.iter().filter_map(|x| x.as_f64().map(|f| f as f32)).collect())
}

// --------------------------------------------------------------------- save

#[derive(Debug, Clone, Serialize)]
pub struct SaveOutcome {
    pub id: String,
    pub embedded: bool,
    pub embedder: Option<String>,
    pub probeable: bool,
    pub trust: String,
    pub degraded: bool,
    pub degraded_reason: Option<String>,
}

pub async fn save(
    text: &str,
    trigger: Option<String>,
    tags: Vec<String>,
) -> Result<SaveOutcome> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(anyhow!("texte vide"));
    }

    let embedder = embedder_id();
    let mut degraded = true;
    let mut reason: Option<String>;

    let vector = match &embedder {
        Some(_) => match embed(&text).await {
            Ok(v) => {
                degraded = false;
                reason = None;
                Some(v)
            }
            Err(e) => {
                reason = Some(format!("embedding échoué: {e}"));
                None
            }
        },
        None => {
            reason = Some("aucun embedder configuré".into());
            None
        }
    };
    if vector.is_none() && reason.is_none() {
        reason = Some("vecteur absent".into());
    }

    let entry = MemoryEntry {
        id: new_id(),
        text,
        trigger: trigger.filter(|t| !t.trim().is_empty()),
        embedder: if vector.is_some() { embedder } else { None },
        vector,
        trust: "untrusted".into(),
        created_at: now(),
        tags,
    };
    append(&entry)?;

    Ok(SaveOutcome {
        id: entry.id,
        embedded: entry.vector.is_some(),
        embedder: entry.embedder,
        probeable: entry.trigger.is_some(),
        trust: entry.trust,
        degraded,
        degraded_reason: reason,
    })
}

// ------------------------------------------------------------------- search

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub id: String,
    pub text: String,
    pub score: f32,
    pub embedder: Option<String>,
    pub trigger: Option<String>,
    pub trust: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchOutcome {
    pub hits: Vec<Hit>,
    pub total: usize,
    pub probeable: usize,
    pub probed: usize,
    pub degraded: bool,
    pub degraded_reason: Option<String>,
    pub ranking_meaningful: bool,
    pub trust_enforced: bool,
    pub embedders_seen: Vec<String>,
}

pub async fn search(query: &str, k: usize) -> SearchOutcome {
    let entries = load();
    let total = entries.len();
    let probeable = entries.iter().filter(|e| e.trigger.is_some()).count();

    let mut embedders_seen: Vec<String> =
        entries.iter().filter_map(|e| e.embedder.clone()).collect();
    embedders_seen.sort();
    embedders_seen.dedup();

    let query_embedder = embedder_id();
    let mut degraded = false;
    let mut reason: Option<String> = None;
    let mut ranking_meaningful = false;
    let mut hits: Vec<Hit> = Vec::new();
    let mut used_vectors = false;

    if let Some(qe) = &query_embedder {
        match embed(query).await {
            Ok(qv) => {
                used_vectors = true;
                let mut scored: Vec<(f32, &MemoryEntry)> = entries
                    .iter()
                    .filter(|e| {
                        e.embedder.as_deref() == Some(qe.as_str()) && e.vector.is_some()
                    })
                    .map(|e| (cosine(&qv, e.vector.as_ref().unwrap()), e))
                    .collect();
                scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
                hits = scored
                    .into_iter()
                    .take(k)
                    .map(|(s, e)| Hit {
                        id: e.id.clone(),
                        text: e.text.clone(),
                        score: s,
                        embedder: e.embedder.clone(),
                        trigger: e.trigger.clone(),
                        trust: e.trust.clone(),
                    })
                    .collect();

                // Ranking is only meaningful within one embedding space.
                let mismatched = entries
                    .iter()
                    .any(|e| e.vector.is_some() && e.embedder.as_deref() != Some(qe.as_str()));
                ranking_meaningful = !mismatched;
                if mismatched {
                    degraded = true;
                    reason = Some(
                        "vecteurs de plusieurs modèles d'embedding → classement inter-espaces non significatif"
                            .into(),
                    );
                }
            }
            Err(e) => {
                degraded = true;
                reason = Some(format!("embedding de la requête échoué: {e}"));
            }
        }
    } else {
        degraded = true;
        reason = Some("aucun embedder configuré → recherche par sous-chaîne".into());
    }

    if !used_vectors {
        let q = query.to_ascii_lowercase();
        let mut scored: Vec<(f32, &MemoryEntry)> = entries
            .iter()
            .filter(|e| e.text.to_ascii_lowercase().contains(&q))
            .map(|e| (1.0, e))
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        hits = scored
            .into_iter()
            .take(k)
            .map(|(s, e)| Hit {
                id: e.id.clone(),
                text: e.text.clone(),
                score: s,
                embedder: e.embedder.clone(),
                trigger: e.trigger.clone(),
                trust: e.trust.clone(),
            })
            .collect();
        ranking_meaningful = false;
    }

    SearchOutcome {
        hits,
        total,
        probeable,
        probed: 0,
        degraded,
        degraded_reason: reason,
        ranking_meaningful,
        trust_enforced: false,
        embedders_seen,
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0;
    let mut na = 0.0;
    let mut nb = 0.0;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na.sqrt() * nb.sqrt())
    }
}

/// Human-readable honesty header for a search result.
pub fn format_outcome(o: &SearchOutcome) -> String {
    let mut s = format!(
        "Mémoire — {} résultat(s) / {} entrée(s)\n",
        o.hits.len(),
        o.total
    );
    s.push_str(&format!(
        "  degraded            : {}{}\n",
        o.degraded,
        o.degraded_reason
            .as_ref()
            .map(|r| format!(" ({r})"))
            .unwrap_or_default()
    ));
    s.push_str(&format!(
        "  ranking_meaningful  : {}\n",
        o.ranking_meaningful
    ));
    s.push_str(&format!(
        "  trust_enforced      : {} (toutes les entrées = untrusted)\n",
        o.trust_enforced
    ));
    s.push_str(&format!(
        "  testables/probées/total : {}/{}/{}\n",
        o.probeable, o.probed, o.total
    ));
    if !o.embedders_seen.is_empty() {
        s.push_str(&format!(
            "  embedders           : {}\n",
            o.embedders_seen.join(", ")
        ));
    }
    if o.hits.is_empty() {
        s.push_str("  (aucun résultat)");
    }
    for h in &o.hits {
        let trig = if h.trigger.is_some() { " · testable" } else { "" };
        s.push_str(&format!(
            "\n  [{:.2}] {}{}  · {}\n      {}\n",
            h.score, h.id, trig, h.trust,
            h.text.replace('\n', " ")
        ));
    }
    s
}
