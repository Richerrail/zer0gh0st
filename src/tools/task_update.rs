use serde_json::json;

use super::ToolSpec;
use crate::tasks::{self, Status};

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "task_update",
        description: "Update a task's status: `claim` (en cours) or `done` (fait) or `pending`. Honnêteté : `done` est REFUSÉ si un livrable mentionné dans la tâche (chemin) est absent ou vide — crée le fichier d'abord.",
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
        Ok(tasks::Update::Ok(s)) => format!("Tâche {id} → {}", s.label()),
        Ok(tasks::Update::NotFound) => format!("Tâche {id} introuvable"),
        Ok(tasks::Update::Refused(missing)) => {
            // Porte d'honnêteté : le modèle ne peut PAS cocher une tâche dont le
            // livrable manque — il doit le créer d'abord (0BOX-TEST, critère 3).
            let mut s = format!(
                "Refusé : la tâche {id} ne peut pas être marquée « fait » — livrable(s) en défaut :\n"
            );
            for (p, why) in &missing {
                s.push_str(&format!("  ✗ {p} ({why})\n"));
            }
            s.push_str(
                "\nHonnêteté : « fait » exige le livrable sur disque, non vide. Crée-le \
                 (text_editor / code_exec) puis relance task_update — ou retire le chemin du \
                 texte de la tâche s'il ne s'agit pas d'un livrable.",
            );
            s
        }
        Err(e) => format!("Erreur: {e}"),
    }
}
