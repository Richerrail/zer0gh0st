use serde_json::json;

use super::ToolSpec;
use crate::knowledge;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "knowledge_add",
        description: "Store durable knowledge in the local base (chunked + FTS + embeddings, \
                      same pipeline as file ingestion). Same `source` replaces its previous \
                      chunks. Use it for facts you will need to retrieve later.",
        parameters: json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "Knowledge to store." },
                "source": { "type": "string", "description": "Stable id, e.g. `note:projet-x` (default: manual:<timestamp>)." },
                "sujet": { "type": "string", "description": "Short title/topic (default: source)." }
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
    if text.trim().is_empty() {
        return "Erreur: `text` manquant".into();
    }
    let source = v
        .get("source")
        .and_then(|x| x.as_str())
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            format!("manuel:{secs}")
        });
    let sujet = v
        .get("sujet")
        .and_then(|x| x.as_str())
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| source.clone());

    match knowledge::add(&source, &sujet, text, true).await {
        Ok((n, embedded)) => format!(
            "Connaissance: {n} chunk(s) indexé(s) — source `{source}`, embeddings={embedded}{}",
            if embedded {
                ""
            } else {
                " (aucun embedder : recherche lexicale seulement)"
            }
        ),
        Err(e) => format!("Erreur connaissance: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_is_well_formed() {
        let s = spec();
        assert_eq!(s.name, "knowledge_add");
        assert_eq!(s.parameters["required"][0], "text");
        assert!(s.parameters["properties"]["source"].is_object());
    }
}
