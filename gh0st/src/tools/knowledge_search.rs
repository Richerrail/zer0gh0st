use serde_json::json;

use super::ToolSpec;
use crate::knowledge;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "knowledge_search",
        description: "Search the local knowledge base (SQLite + FTS5, hybrid with embeddings \
                      when configured). Use it for reference material before answering.",
        parameters: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "What to look for." },
                "k": { "type": "integer", "description": "Max results (default 5)." }
            },
            "required": ["query"]
        }),
    }
}

pub async fn run(args: &str) -> String {
    let v: serde_json::Value = match serde_json::from_str(args) {
        Ok(v) => v,
        Err(e) => return format!("Erreur: arguments JSON invalides ({e})"),
    };
    let query = v.get("query").and_then(|x| x.as_str()).unwrap_or("");
    if query.trim().is_empty() {
        return "Erreur: `query` manquant".into();
    }
    let k = v.get("k").and_then(|x| x.as_u64()).unwrap_or(5) as usize;
    let (hits, degraded) = knowledge::search(query, k).await;
    knowledge::format_hits(&hits, &degraded)
}
