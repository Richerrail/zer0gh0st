use serde_json::json;
use std::process::Command;

use super::ToolSpec;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "code_exec",
        description: "Run a bash command on the host and return stdout/stderr. \
                      Use for shell, compilers, tests, git, listing files, etc.",
        parameters: json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The bash command to run." },
                "cwd": { "type": "string", "description": "Optional working directory." }
            },
            "required": ["command"]
        }),
    }
}

pub fn run(args: &str) -> String {
    let v: serde_json::Value = match serde_json::from_str(args) {
        Ok(v) => v,
        Err(e) => return format!("Erreur: arguments JSON invalides ({e})"),
    };
    let command = v.get("command").and_then(|x| x.as_str()).unwrap_or_default();
    if command.trim().is_empty() {
        return "Erreur: `command` manquant".into();
    }

    let mut cmd = Command::new("bash");
    cmd.arg("-lc").arg(command);
    if let Some(dir) = v.get("cwd").and_then(|x| x.as_str()) {
        cmd.current_dir(dir);
    }

    match cmd.output() {
        Ok(out) => {
            let mut text = String::new();
            if !out.stdout.is_empty() {
                text.push_str(&String::from_utf8_lossy(&out.stdout));
            }
            if !out.stderr.is_empty() {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str("[stderr]\n");
                text.push_str(&String::from_utf8_lossy(&out.stderr));
            }
            if text.trim().is_empty() {
                text = format!("(commande terminée, code {})", out.status.code().unwrap_or(-1));
            }
            truncate(text, 16_000)
        }
        Err(e) => format!("Erreur d'exécution: {e}"),
    }
}

fn truncate(mut s: String, max: usize) -> String {
    if s.len() > max {
        s.truncate(max);
        s.push_str("\n… (sortie tronquée)");
    }
    s
}
