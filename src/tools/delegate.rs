use serde_json::json;

use super::ToolSpec;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "delegate",
        description: "Delegate ONE focused task to an isolated sub-agent (fresh context, no \
                      access to this conversation) and get back a short synthesis. Use it for \
                      exploration or long side-work that must not pollute the main thread.",
        parameters: json!({
            "type": "object",
            "properties": {
                "task": { "type": "string", "description": "Precise, self-contained instruction." },
                "role": { "type": "string", "description": "Optional role for routing (code, review, reasoning… default: reasoning)." }
            },
            "required": ["task"]
        }),
    }
}

pub async fn run(args: &str) -> String {
    let v: serde_json::Value = match serde_json::from_str(args) {
        Ok(v) => v,
        Err(e) => return format!("Erreur: arguments JSON invalides ({e})"),
    };
    let task = v.get("task").and_then(|x| x.as_str()).unwrap_or("").trim();
    if task.is_empty() {
        return "Erreur: `task` manquant".into();
    }
    let role = v
        .get("role")
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|r| crate::route::KNOWN_ROLES.contains(r))
        .unwrap_or("reasoning");

    let Some(rt) = super::runtime() else {
        return "Erreur delegate : contexte d'exécution absent (agent non initialisé).".into();
    };

    let endpoints = crate::route::chain_for_role(&rt.settings, role);
    if endpoints.is_empty() {
        return format!("Erreur delegate : aucun endpoint pour le rôle `{role}`.");
    }
    // Pas de récursion : le sous-agent ne peut ni déléguer ni relancer 0box.
    let tools: Vec<ToolSpec> = rt
        .all_tools
        .iter()
        .filter(|t| t.name != "delegate" && t.name != "0box")
        .cloned()
        .collect();
    let system = "Tu es un sous-agent isolé de ZER0|GH0ST. Tu ne vois PAS la conversation \
                  principale : la tâche ci-dessous doit se suffire à elle-même. Exécute-la \
                  (outils si nécessaire) et rends une synthèse courte et factuelle : résultats, \
                  chemins, commandes utiles. Pas de préambule, pas de politesses."
        .to_string();
    let mut options = rt.options.clone();
    options.max_tokens = options.max_tokens.min(1_200);

    let out = crate::agent::subagent::run_subagent(
        endpoints,
        system,
        task.to_string(),
        tools,
        options,
        None,
    )
    .await;
    let out = out.trim();
    let shown: String = out.chars().take(8_000).collect();
    format!("Sous-agent `{role}` (contexte isolé) :\n{shown}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_is_well_formed() {
        let s = spec();
        assert_eq!(s.name, "delegate");
        assert_eq!(s.parameters["required"][0], "task");
    }
}
