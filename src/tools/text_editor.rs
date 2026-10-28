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
                "offset": { "type": "integer", "description": "view: first line (1-based, default 1)." },
                "limit": { "type": "integer", "description": "view: max lines (default 400, max 2000)." },
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
    let command = v
        .get("command")
        .and_then(|x| x.as_str())
        .unwrap_or_default();
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
        "view" => view(
            path,
            v.get("offset").and_then(|x| x.as_u64()).map(|n| n as usize),
            v.get("limit").and_then(|x| x.as_u64()).map(|n| n as usize),
        ),
        "create" => create(path, content),
        "str_replace" => str_replace(
            path,
            v.get("old_str")
                .and_then(|x| x.as_str())
                .unwrap_or_default(),
            v.get("new_str")
                .and_then(|x| x.as_str())
                .unwrap_or_default(),
        ),
        "insert" => insert(
            path,
            v.get("insert_line").and_then(|x| x.as_i64()).unwrap_or(1) as usize,
            v.get("new_str")
                .and_then(|x| x.as_str())
                .unwrap_or_default(),
        ),
        "delete" | "remove" => delete(path),
        other => format!("Erreur: commande inconnue `{other}`"),
    }
}

/// Sauvegarde le fichier avant de l'écraser ou de le supprimer.
///
/// ZER0|GH0ST agit avec les droits de l'utilisateur, sans bac à sable. Un outil
/// qui réécrit un fichier sans garder de copie rend toute erreur du modèle
/// irréversible. La sauvegarde est posée **à côté** du fichier (`<nom>.zer0.bak`),
/// donc trouvable sans connaître l'état interne de l'agent.
fn backup(path: &Path) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() == 0 {
        return None;
    }
    let name = path.file_name()?.to_string_lossy().to_string();
    let dest = path.with_file_name(format!("{name}.zer0.bak"));
    fs::copy(path, &dest).ok()?;
    Some(dest.display().to_string())
}

fn backup_note(path: &Path) -> String {
    match backup(path) {
        Some(dest) => format!(" (sauvegarde : {dest})"),
        None => String::new(),
    }
}

fn stats(content: &str) -> String {
    format!(
        "{} octet(s), {} ligne(s)",
        content.len(),
        content.lines().count()
    )
}

fn view(path: &str, offset: Option<usize>, limit: Option<usize>) -> String {
    match fs::read_to_string(path) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let total = lines.len();
            let start = offset.unwrap_or(1).max(1);
            // 400 lignes par défaut : un README de 600 lignes ne doit plus être
            // avalé d'un coup (le modèle demande la suite avec `offset`).
            let count = limit.unwrap_or(400).clamp(1, 2_000);
            let from = (start - 1).min(total);
            let to = (from + count).min(total);
            let mut header = format!("{path} — {}", stats(&content));
            if from > 0 || to < total {
                header.push_str(&format!(
                    " — lignes {}-{} sur {total} (offset={}, limit={})",
                    from + 1,
                    to,
                    start,
                    count
                ));
            }
            let numbered: String = lines[from..to]
                .iter()
                .enumerate()
                .map(|(i, l)| format!("{:>5}  {l}\n", from + i + 1))
                .collect();
            let mut out = truncate(format!("{header}\n{numbered}"), 16_000);
            if to < total {
                out.push_str(&format!(
                    "… {} ligne(s) restante(s) — relance `view` avec offset={}",
                    total - to,
                    to + 1
                ));
            }
            out
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
    let note = backup_note(Path::new(path));
    match fs::write(path, content) {
        Ok(()) => {
            // Verify the write actually landed.
            match fs::metadata(path) {
                Ok(m) if m.len() > 0 => {
                    let first = content.lines().next().unwrap_or("");
                    format!(
                        "Fichier écrit: {path} ({}). Première ligne: {first}{note}",
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
    let note = backup_note(Path::new(path));
    match fs::write(path, &updated) {
        Ok(()) => format!(
            "Remplacement effectué dans {path} ({}){note}",
            stats(&updated)
        ),
        Err(e) => format!("Erreur d'écriture: {e}"),
    }
}

fn insert(path: &str, line: usize, text: &str) -> String {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => return format!("Erreur de lecture: {e}"),
    };
    // On préserve la présence (ou l'absence) de newline final : insérer une ligne
    // ne doit pas réécrire la fin du fichier.
    let had_trailing_newline = content.ends_with('\n');
    let mut lines: Vec<&str> = content.lines().collect();
    let idx = line.saturating_sub(1).min(lines.len());
    lines.insert(idx, text);
    let mut updated = lines.join("\n");
    if had_trailing_newline {
        updated.push('\n');
    }
    match fs::write(path, &updated) {
        Ok(()) => format!(
            "Insertion effectuée dans {path} à la ligne {line} ({})",
            stats(&updated)
        ),
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
    let note = backup_note(p);
    match fs::remove_file(p) {
        Ok(()) => format!("Fichier supprimé: {path}{note}"),
        Err(e) => format!("Erreur de suppression: {e}"),
    }
}

fn truncate(mut s: String, max: usize) -> String {
    // Troncature sûre : `String::truncate` panique au milieu d'un caractère
    // multi-octets (fichier français > 16 Ko → crash de la tâche de génération).
    if crate::util::truncate_utf8(&mut s, max) {
        s.push_str("\n… (tronqué)");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("zer0-te-{}-{name}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir.join(name).to_string_lossy().to_string()
    }

    #[test]
    fn empty_create_is_refused() {
        let p = tmp("vide.txt");
        let out = create(&p, "   \n");
        assert!(out.contains("PAS été écrit"));
        assert!(!Path::new(&p).exists());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn create_then_view_roundtrip() {
        let p = tmp("ok.txt");
        let out = create(&p, "ligne 1\nligne 2\n");
        assert!(out.contains("Fichier écrit"));
        let v = view(&p, None, None);
        assert!(v.contains("ligne 2"));
        assert!(v.contains("    1  ligne 1"), "got: {v}");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn view_paginates_with_offset_and_limit() {
        let p = tmp("pages.txt");
        let body: String = (1..=10).map(|i| format!("l{i}\n")).collect();
        create(&p, &body);
        let v = view(&p, Some(3), Some(2));
        assert!(v.contains("    3  l3"), "got: {v}");
        assert!(v.contains("    4  l4"));
        assert!(!v.contains("    2  l2"));
        assert!(!v.contains("    5  l5"));
        assert!(v.contains("lignes 3-4 sur 10"));
        assert!(v.contains("offset=5"), "pied de page manquant: {v}");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn str_replace_requires_unique_match() {
        let p = tmp("dup.txt");
        create(&p, "a\na\n");
        assert!(str_replace(&p, "a", "b").contains("2 fois"));
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn insert_preserves_missing_trailing_newline() {
        let p = tmp("nolf.txt");
        fs::write(&p, "un\ndeux").unwrap();
        insert(&p, 2, "milieu");
        assert_eq!(fs::read_to_string(&p).unwrap(), "un\nmilieu\ndeux");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn insert_preserves_trailing_newline() {
        let p = tmp("lf.txt");
        fs::write(&p, "un\ndeux\n").unwrap();
        insert(&p, 2, "milieu");
        assert_eq!(fs::read_to_string(&p).unwrap(), "un\nmilieu\ndeux\n");
        let _ = fs::remove_file(&p);
    }

    /// Régression : `view` sur un fichier français > 16 Ko faisait paniquer
    /// `String::truncate` (frontière UTF-8).
    #[test]
    fn view_multibyte_file_does_not_panic() {
        let p = tmp("fr.txt");
        let mut content = String::from("a");
        content.push_str(&"é".repeat(20_000));
        fs::write(&p, &content).unwrap();
        let v = view(&p, None, None);
        assert!(v.contains("tronqué"));
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn overwrite_keeps_a_backup() {
        let p = tmp("garde.txt");
        fs::write(&p, "version 1").unwrap();
        let out = create(&p, "version 2");
        assert!(out.contains("sauvegarde"), "{out}");
        let bak = format!("{p}.zer0.bak");
        assert_eq!(fs::read_to_string(&bak).unwrap(), "version 1");
        let _ = fs::remove_file(&p);
        let _ = fs::remove_file(&bak);
    }

    #[test]
    fn delete_keeps_a_backup() {
        let p = tmp("supprime.txt");
        fs::write(&p, "contenu").unwrap();
        let out = delete(&p);
        assert!(out.contains("sauvegarde"), "{out}");
        let bak = format!("{p}.zer0.bak");
        assert_eq!(fs::read_to_string(&bak).unwrap(), "contenu");
        assert!(!Path::new(&p).exists());
        let _ = fs::remove_file(&bak);
    }

    #[test]
    fn delete_reports_missing_file() {
        assert!(delete("/tmp/zer0-inexistant-xyz").contains("n'existe pas"));
    }
}
