use serde_json::json;

use super::ToolSpec;
use crate::memory;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "memory_delete",
        description: "Delete one long-term memory entry by id (ids are shown by memory_search). \
                      Use it to remove a wrong or obsolete fact instead of letting it poison search.",
        parameters: json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Memory entry id, e.g. m18f2c…" }
            },
            "required": ["id"]
        }),
    }
}

pub fn run(args: &str) -> String {
    let v: serde_json::Value = match serde_json::from_str(args) {
        Ok(v) => v,
        Err(e) => return format!("Erreur: arguments JSON invalides ({e})"),
    };
    let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
    if id.is_empty() {
        return "Erreur: `id` manquant".into();
    }
    match memory::forget(id) {
        Ok(true) => format!("Mémoire: entrée {id} supprimée."),
        Ok(false) => format!("Mémoire: entrée {id} introuvable."),
        Err(e) => format!("Erreur mémoire: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_is_well_formed() {
        let s = spec();
        assert_eq!(s.name, "memory_delete");
        assert!(s.description.contains("id"));
        assert_eq!(s.parameters["required"][0], "id");
    }
}
