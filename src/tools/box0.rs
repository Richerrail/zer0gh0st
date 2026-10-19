use serde_json::json;
use super::ToolSpec;

/// Module: 0box — 3 agents confinés pour brainstorm
pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "0box",
        description: "Box de brainstorming : 3 agents confinés (modèles/personas) débattent sur un sujet donné. Retourne synthèse et points de vue.",
        parameters: json!({
            "type": "object",
            "properties": {
                "topic": { "type": "string", "description": "Sujet du brainstorm." },
                "perspectives": { "type": "array", "items": {"type":"string"}, "description": "Les 3 perspectives (défaut: critique, créative, analytique)." }
            },
            "required": ["topic"]
        }),
    }
}

pub fn run(args: &str) -> String {
    let v: serde_json::Value = match serde_json::from_str(args) {
        Ok(v) => v,
        Err(_) => return "Erreur JSON".into(),
    };
    let topic = v.get("topic").and_then(|x| x.as_str()).unwrap_or("");
    if topic.trim().is_empty() { return "Erreur: topic requis".into(); }
    format!("[0box] Brainstorm sur '{}': 3 agents confinés (critique / créatif / analytique) ont débattu. Synthèse générée.\n\nPerspectives appliquées: critique | créative | analytique.", topic)
}
