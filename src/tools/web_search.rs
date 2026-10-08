use serde_json::json;

use super::ToolSpec;
use crate::search;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "web_search",
        description: "Search the web (DuckDuckGo) to verify facts or get recent info. \
                      Returns titles, snippets and URLs. No API key required.",
        parameters: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "What to search for." },
                "max": { "type": "integer", "description": "Max results (default 5)." }
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
    let max = v.get("max").and_then(|x| x.as_u64()).unwrap_or(5) as usize;
    match search::web_search(query, max).await {
        Ok(hits) => search::format_hits(&hits),
        Err(e) => format!("Erreur recherche web: {e}"),
    }
}
