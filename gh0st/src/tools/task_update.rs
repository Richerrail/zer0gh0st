use serde_json::json;

use super::ToolSpec;
use crate::tasks::{self, Status};

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "task_update",
        description: "Update a task's status: `claim` (en cours) or `done` (fait) or `pending`.",
        parameters: json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Task id, e.g. t1." },
                "status": { "type": "string", "enum": ["claim", "done", "pending"] }
            },
            "required": ["id", "status"]
        }),
    }
}

pub fn run(args: &str) -> String {
    let v: serde_json::Value = match serde_json::from_str(args) {
        Ok(v) => v,
        Err(e) => return format!("Erreur: arguments JSON invalides ({e})"),
    };
    let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("");
    let status = match v.get("status").and_then(|x| x.as_str()).unwrap_or("") {
        "claim" => Status::Claimed,
        "done" => Status::Done,
        "pending" => Status::Pending,
        other => return format!("Erreur: statut inconnu `{other}` (claim|done|pending)"),
    };
    if id.is_empty() {
        return "Erreur: `id` manquant".into();
    }
    match tasks::set_status(id, status) {
        Ok(true) => format!("Tâche {id} → {}", status.label()),
        Ok(false) => format!("Tâche {id} introuvable"),
        Err(e) => format!("Erreur: {e}"),
    }
}
