use serde_json::json;
use std::fs;
use std::path::Path;

use super::ToolSpec;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "text_editor",
        description: "View, create, edit or delete text files. Commands: view, create, str_replace, insert, delete. \
                      For `create`, `file_text` is REQUIRED and must contain the FULL file content — \
                      an empty `file_text` writes nothing and is reported as an error. \
                      Use `delete` to clean up temporary/test files after verification.",
        parameters: json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "enum": ["view", "create", "str_replace", "insert", "delete"],
                    "description": "The action to perform."
                },
                "path": { "type": "string", "description": "Absolute file path." },
                "file_text": { "type": "string", "description": "FULL content for `create` (required)." },
                "content": { "type": "string", "description": "Alias for `file_text`." },
                "old_str": { "type": "string", "description": "Text to replace for `str_replace`." },
                "new_str": { "type": "string", "description": "Replacement / inserted text." },
                "insert_line": { "type": "integer", "description": "1-based line for `insert`." }
            },
            "required": ["command", "path"]
        }),
    }
}

pub fn run(args: &str) -> String {
    let v: serde_json::Value = match serde_json::from_str(args) {
        Ok(v) => v,
        Err(e) => return format!("Erreur: arguments JSON invalides ({e})"),
    };
    let command = v.get("command").and_then(|x| x.as_str()).unwrap_or_default();
    let path = v.get("path").and_then(|x| x.as_str()).unwrap_or_default();
    if path.is_empty() {
        return "Erreur: `path` manquant".into();
    }

    let content = v
        .get("file_text")
        .and_then(|x| x.as_str())
        .or_else(|| v.get("content").and_then(|x| x.as_str()))
        .or_else(|| v.get("text").and_then(|x| x.as_str()))
        .unwrap_or("");

    match command {
        "view" => view(path),
        "create" => create(path, content),
        "str_replace" => str_replace(
            path,
            v.get("old_str").and_then(|x| x.as_str()).unwrap_or_default(),
            v.get("new_str").and_then(|x| x.as_str()).unwrap_or_default(),
        ),
        "insert" => insert(
            path,
            v.get("insert_line").and_then(|x| x.as_i64()).unwrap_or(1) as usize,
            v.get("new_str").and_then(|x| x.as_str()).unwrap_or_default(),
        ),
        "delete" | "remove" => delete(path),
        other => format!("Erreur: commande inconnue `{other}`"),
    }
}

fn stats(content: &str) -> String {
    format!("{} octet(s), {} ligne(s)", content.len(), content.lines().count())
}

fn view(path: &str) -> String {
    match fs::read_to_string(path) {
        Ok(content) => {
            let header = format!("{path} — {}\n", stats(&content));
            let numbered: String = content
                .lines()
                .enumerate()
                .map(|(i, l)| format!("{:>5}  {l}\n", i + 1))
                .collect();
            truncate(format!("{header}{numbered}"), 16_000)
        }
        Err(e) => format!("Erreur de lecture: {e}"),
    }
}

fn create(path: &str, content: &str) -> String {
    // Guard: never silently write an empty file.
    if content.trim().is_empty() {
        return format!(
            "Erreur: contenu vide — `{path}` n'a PAS été écrit. \
             Fournis le contenu complet dans `file_text` (ou `content`). \
             Si le contenu est en plusieurs fois, écris d'abord la première partie puis utilise `insert`/`str_replace`."
        );
    }
    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = fs::create_dir_all(parent) {
                return format!("Erreur: {e}");
            }
        }
    }
    match fs::write(path, content) {
        Ok(()) => {
            // Verify the write actually landed.
            match fs::metadata(path) {
                Ok(m) if m.len() > 0 => {
                    let first = content.lines().next().unwrap_or("");
                    format!(
                        "Fichier écrit: {path} ({}). Première ligne: {first}",
                        stats(content)
                    )
                }
                _ => format!("Erreur: {path} est vide après écriture."),
            }
        }
        Err(e) => format!("Erreur d'écriture: {e}"),
    }
}

fn str_replace(path: &str, old: &str, new: &str) -> String {
    if old.is_empty() {
        return format!("Erreur: `old_str` vide pour {path}");
    }
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => return format!("Erreur de lecture: {e}"),
    };
    let count = content.matches(old).count();
    if count == 0 {
        return format!("Erreur: `old_str` introuvable dans {path}");
    }
    if count > 1 {
        return format!("Erreur: `old_str` apparaît {count} fois, rends-le unique");
    }
    let updated = content.replacen(old, new, 1);
    match fs::write(path, &updated) {
        Ok(()) => format!("Remplacement effectué dans {path} ({})", stats(&updated)),
        Err(e) => format!("Erreur d'écriture: {e}"),
    }
}

fn insert(path: &str, line: usize, text: &str) -> String {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => return format!("Erreur de lecture: {e}"),
    };
    let mut lines: Vec<&str> = content.lines().collect();
    let idx = line.saturating_sub(1).min(lines.len());
    lines.insert(idx, text);
    let mut updated = lines.join("\n");
    updated.push('\n');
    match fs::write(path, &updated) {
        Ok(()) => format!("Insertion effectuée dans {path} à la ligne {line} ({})", stats(&updated)),
        Err(e) => format!("Erreur d'écriture: {e}"),
    }
}

fn delete(path: &str) -> String {
    let p = Path::new(path);
    if !p.exists() {
        return format!("{path} n'existe pas (rien à supprimer)");
    }
    if p.is_dir() {
        return format!("Erreur: {path} est un répertoire, supprime son contenu d'abord");
    }
    match fs::remove_file(p) {
        Ok(()) => format!("Fichier supprimé: {path}"),
        Err(e) => format!("Erreur de suppression: {e}"),
    }
}

fn truncate(mut s: String, max: usize) -> String {
    if s.len() > max {
        s.truncate(max);
        s.push_str("\n… (tronqué)");
    }
    s
}
