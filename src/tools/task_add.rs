use serde_json::json;

use super::ToolSpec;
use crate::tasks;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "task_add",
        description: "Add a task to the user's markdown task list. \
                      Optional `role` assigns it to a model role (code, review, reasoning, …).",
        parameters: json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "What needs to be done." },
                "role": { "type": "string", "description": "Optional role for this task." },
                "due": { "type": "string", "description": "Optional due date: `+15m`, `+2h`, `+3d` or epoch seconds. Triggers a reminder in the TUI." }
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
    let due = match v.get("due").and_then(|x| x.as_str()) {
        Some(s) if !s.trim().is_empty() => match tasks::parse_due(s) {
            Some(d) => Some(d),
            None => {
                return format!(
                    "Erreur: échéance `{s}` invalide (formats : +15m, +2h, +3d, epoch)."
                )
            }
        },
        _ => None,
    };
    match tasks::add_with_due(text, role, due) {
        Ok(t) => format!(
            "Tâche {} ajoutée: {}{}",
            t.id,
            t.text,
            t.due
                .map(|d| format!(" (échéance epoch {d})"))
                .unwrap_or_default()
        ),
        Err(e) => format!("Erreur: {e}"),
    }
}
