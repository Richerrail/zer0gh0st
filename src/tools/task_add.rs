use serde_json::json;

use super::ToolSpec;
use crate::tasks;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "task_add",
        description: "Add a task to the project's markdown task list (`.zer0/tasks.md`). \
                      Optional `role` assigns it to a model role (code, review, reasoning, …).",
        parameters: json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "What needs to be done." },
                "role": { "type": "string", "description": "Optional role for this task." }
            },
            "required": ["text"]
        }),
    }
}

pub fn run(args: &str) -> String {
    let v: serde_json::Value = match serde_json::from_str(args) {
        Ok(v) => v,
        Err(e) => return format!("Erreur: arguments JSON invalides ({e})"),
    };
    let text = v.get("text").and_then(|x| x.as_str()).unwrap_or("");
    if text.trim().is_empty() {
        return "Erreur: `text` manquant".into();
    }
    let role = v
        .get("role")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    match tasks::add(text, role) {
        Ok(t) => format!("Tâche {} ajoutée: {}", t.id, t.text),
        Err(e) => format!("Erreur: {e}"),
    }
}
