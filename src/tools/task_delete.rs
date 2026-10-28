use serde_json::json;

use super::ToolSpec;
use crate::tasks;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "task_delete",
        description: "Delete a task from the markdown task list by id (t1, t2…).",
        parameters: json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Task id, e.g. t3." }
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
    match tasks::remove(id) {
        Ok(true) => format!("Tâche {id} supprimée."),
        Ok(false) => format!("Tâche {id} introuvable."),
        Err(e) => format!("Erreur: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_is_well_formed() {
        let s = spec();
        assert_eq!(s.name, "task_delete");
        assert!(s.description.contains("id"));
        assert_eq!(s.parameters["required"][0], "id");
    }
}
