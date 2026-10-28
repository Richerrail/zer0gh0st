use serde_json::json;

use super::ToolSpec;
use crate::knowledge;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "knowledge_delete",
        description: "Delete knowledge chunks whose `source` CONTAINS the given pattern \
                      (e.g. `note:projet` or `/home/me/docs/`). FTS index is rebuilt.",
        parameters: json!({
            "type": "object",
            "properties": {
                "source": { "type": "string", "description": "Substring of the source to remove." }
            },
            "required": ["source"]
        }),
    }
}

pub fn run(args: &str) -> String {
    let v: serde_json::Value = match serde_json::from_str(args) {
        Ok(v) => v,
        Err(e) => return format!("Erreur: arguments JSON invalides ({e})"),
    };
    let source = v
        .get("source")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim();
    if source.is_empty() {
        return "Erreur: `source` manquant".into();
    }
    match knowledge::drop_source(source) {
        Ok(0) => format!("Connaissance: aucune source ne contient `{source}`."),
        Ok(n) => format!("Connaissance: {n} chunk(s) supprimé(s) (source contenant `{source}`)."),
        Err(e) => format!("Erreur connaissance: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_is_well_formed() {
        let s = spec();
        assert_eq!(s.name, "knowledge_delete");
        assert_eq!(s.parameters["required"][0], "source");
    }
}
