use serde_json::json;

use super::ToolSpec;
use crate::memory;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "memory_save",
        description: "Persist a durable fact/decision/preference to long-term memory. \
                      Entries are stored as untrusted data. Add `trigger` (a future query) \
                      so the entry can be tested for retrievability later.",
        parameters: json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "What to remember." },
                "trigger": { "type": "string", "description": "A future query that should retrieve this entry." },
                "tags": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["text"]
        }),
    }
}

pub async fn run(args: &str) -> String {
    let v: serde_json::Value = match serde_json::from_str(args) {
        Ok(v) => v,
        Err(e) => return format!("Erreur: arguments JSON invalides ({e})"),
    };
    let text = v.get("text").and_then(|x| x.as_str()).unwrap_or("");
    let trigger = v
        .get("trigger")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    let tags = v
        .get("tags")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    match memory::save(text, trigger, tags).await {
        Ok(o) => format!(
            "Mémoire: entrée {} enregistrée (embedded={}, embedder={:?}, testable={}, trust={}, degraded={}{})",
            o.id,
            o.embedded,
            o.embedder,
            o.probeable,
            o.trust,
            o.degraded,
            o.degraded_reason
                .map(|r| format!(" — {r}"))
                .unwrap_or_default()
        ),
        Err(e) => format!("Erreur mémoire: {e}"),
    }
}
