use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pending,
    Claimed,
    Done,
}

impl Status {
    fn mark(&self) -> &'static str {
        match self {
            Status::Pending => "[ ]",
            Status::Claimed => "[~]",
            Status::Done => "[x]",
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Status::Pending => "à faire",
            Status::Claimed => "en cours",
            Status::Done => "fait",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub text: String,
    pub status: Status,
    pub role: Option<String>,
}

/// Fichier de la liste de tâches (Markdown), **global à l'utilisateur**.
///
/// Avant : `.zer0/tasks.md` **relatif au dossier courant**. Les tâches
/// changeaient donc selon l'endroit d'où l'agent était lancé (`~`, le dépôt, un
/// projet quelconque), ce qui rendait `/task` incohérent — et créait des
/// `.zer0/` un peu partout.
///
/// Désormais dans le dossier de données (XDG), comme la mémoire et les
/// conversations. Surchargeable par `ZERO_TASKS_FILE` (utile pour une liste
/// propre à un projet).
pub fn path() -> PathBuf {
    static P: OnceLock<PathBuf> = OnceLock::new();
    P.get_or_init(|| {
        let p = task_file_from(
            std::env::var("ZERO_TASKS_FILE").ok().as_deref(),
            &crate::config::data_dir(),
        );
        if let Some(legacy) = migrate_legacy(&p) {
            eprintln!("🗂️  tâches migrées depuis {}", legacy.display());
        }
        p
    })
    .clone()
}

/// Résolution pure du chemin (testable sans toucher l'environnement).
pub fn task_file_from(env: Option<&str>, data_dir: &Path) -> PathBuf {
    env.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir.join("tasks.md"))
}

/// Anciens emplacements possibles de la liste (ordre d'exploration).
fn legacy_candidates() -> Vec<PathBuf> {
    let mut out = vec![PathBuf::from(".zer0/tasks.md")];
    for p in [
        crate::mods::project_root().join(".zer0/tasks.md"),
        PathBuf::from(crate::platform::home()).join(".zer0/tasks.md"),
    ] {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// Importe l'ancienne liste si la nouvelle n'existe pas encore.
pub fn migrate_legacy(target: &Path) -> Option<PathBuf> {
    migrate_legacy_from(target, &legacy_candidates())
}

/// Variante pure : choisit le candidat **le plus récent** et le copie.
pub(crate) fn migrate_legacy_from(target: &Path, candidates: &[PathBuf]) -> Option<PathBuf> {
    if target.exists() {
        return None;
    }
    let newest = candidates
        .iter()
        .filter(|p| p.is_file())
        .max_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok())?;
    if let Some(dir) = target.parent() {
        let _ = fs::create_dir_all(dir);
    }
    fs::copy(newest, target).ok()?;
    Some(newest.clone())
}

pub fn load() -> Vec<Task> {
    let Ok(s) = fs::read_to_string(path()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in s.lines() {
        let line = line.trim_start();
        if !line.starts_with("- [") {
            continue;
        }
        let status = match line.chars().nth(3) {
            Some('x') | Some('X') => Status::Done,
            Some('~') => Status::Claimed,
            _ => Status::Pending,
        };
        let rest = line.get(6..).unwrap_or("").trim();
        let mut it = rest.splitn(3, '|').map(|x| x.trim());
        let id = it.next().unwrap_or("").to_string();
        let role = it.next().unwrap_or("-").to_string();
        let text = it.next().unwrap_or("").to_string();
        if id.is_empty() {
            continue;
        }
        out.push(Task {
            id,
            text,
            status,
            role: if role.is_empty() || role == "-" {
                None
            } else {
                Some(role)
            },
        });
    }
    out
}

pub fn save(tasks: &[Task]) -> Result<()> {
    let p = path();
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut s = String::from("# Tâches Zer0\n\n");
    for t in tasks {
        s.push_str(&format!(
            "- {} {} | {} | {}\n",
            t.status.mark(),
            t.id,
            t.role.clone().unwrap_or_else(|| "-".to_string()),
            t.text
        ));
    }
    fs::write(&p, s)?;
    Ok(())
}

pub fn add(text: &str, role: Option<String>) -> Result<Task> {
    let mut tasks = load();
    let n = tasks
        .iter()
        .filter_map(|t| t.id.strip_prefix('t').and_then(|x| x.parse::<usize>().ok()))
        .max()
        .unwrap_or(0)
        + 1;
    let task = Task {
        id: format!("t{n}"),
        text: text.trim().to_string(),
        status: Status::Pending,
        role: role.filter(|r| !r.trim().is_empty()),
    };
    tasks.push(task.clone());
    save(&tasks)?;
    Ok(task)
}

/// Issue d'une mise à jour de statut.
#[derive(Debug, PartialEq)]
pub enum Update {
    /// Appliqué (statut résultant).
    Ok(Status),
    /// Id inconnu.
    NotFound,
    /// `done` refusé : livrables absents/vides — (chemin, problème).
    Refused(Vec<(String, String)>),
}

pub fn set_status(id: &str, status: Status) -> Result<Update> {
    let mut tasks = load();
    let mut found = false;
    // Porte d'honnêteté : marquer `done` exige que les livrables mentionnés
    // dans le texte existent réellement sur disque (le test 0BOX-TEST a révélé
    // des tâches [x] sans livrable — « j'ai écrit le fichier » non vérifié).
    // Contournement assumé : éditer `.zer0/tasks.md` à la main.
    if status == Status::Done {
        let mut missing = Vec::new();
        for t in &tasks {
            if t.id == id {
                found = true;
                let paths = extract_paths(&t.text);
                missing.extend(verify_deliverables(&paths));
            }
        }
        if found && !missing.is_empty() {
            return Ok(Update::Refused(missing));
        }
    }
    for t in &mut tasks {
        if t.id == id {
            t.status = status;
            found = true;
        }
    }
    if found {
        save(&tasks)?;
        Ok(Update::Ok(status))
    } else {
        Ok(Update::NotFound)
    }
}

/// Chemins candidats dans le texte d'une tâche. L'heuristique est volontairement
/// **prudente** : seuls les chemins ancrés (`~/…`, `/…`, `X:\…`, `./…`, `../…`)
/// sont retenus — jamais un relatif ambigu (cwd variable), ni une notation
/// d'extension (`.txt/.jsonl`), ni une URL (« et/ou » est déjà écarté : pas de
/// point dans le dernier segment). Objectif : ne JAMAIS bloquer une tâche sur un
/// faux positif ; la vérification vise les livrables en chemin complet.
pub fn extract_paths(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for tok in text.split_whitespace() {
        let tok = tok.trim_matches(|c: char| {
            matches!(
                c,
                '`' | '"' | '\'' | '(' | ')' | ',' | ';' | ']' | '[' | '{' | '}' | '«' | '»'
            )
        });
        if tok.is_empty() {
            continue;
        }
        let low = tok.to_ascii_lowercase();
        if low.starts_with("http://") || low.starts_with("https://") || low.starts_with("ftp://") {
            continue; // URL, pas un livrable local
        }
        if !tok.contains('/') && !tok.contains('\\') {
            continue;
        }
        let rooted = tok.starts_with("~/")
            || tok.starts_with("~\\")
            || tok.starts_with('/')
            || tok.starts_with("./")
            || tok.starts_with(".\\")
            || tok.starts_with("../")
            || tok.starts_with("..\\")
            || (tok.len() > 1 && tok.as_bytes()[1] == b':'); // C:\ …
        if !rooted {
            continue; // relatif ambigu ou notation d'extension (`.txt/.jsonl`)
        }
        out.push(expand_home(tok));
    }
    out
}

/// `~/x` → dossier personnel. Autres chemins inchangés.
fn expand_home(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/").or_else(|| p.strip_prefix("~\\")) {
        format!(
            "{}{}",
            crate::platform::home().trim_end_matches(['/', '\\']),
            rest
        )
    } else {
        p.to_string()
    }
}

/// Vérifie les livrables : chaque chemin doit exister ; un fichier doit être
/// non vide. Retourne les défauts (chemin, problème) — vide = tout va bien.
fn verify_deliverables(paths: &[String]) -> Vec<(String, String)> {
    paths
        .iter()
        .filter_map(|p| match fs::metadata(p) {
            Err(_) => Some((p.clone(), "introuvable".to_string())),
            Ok(m) if m.is_dir() => None,
            Ok(m) if m.len() == 0 => Some((p.clone(), "vide (0 octet)".to_string())),
            Ok(_) => None,
        })
        .collect()
}

/// Supprime une tâche par id. `true` si trouvée.
pub fn remove(id: &str) -> Result<bool> {
    let mut tasks = load();
    let before = tasks.len();
    tasks.retain(|t| t.id != id);
    let removed = tasks.len() != before;
    if removed {
        save(&tasks)?;
    }
    Ok(removed)
}

/// Supprime toutes les tâches terminées (`[x]`). Retourne le nombre supprimé.
pub fn clear_done() -> Result<usize> {
    let mut tasks = load();
    let before = tasks.len();
    tasks.retain(|t| t.status != Status::Done);
    let n = before - tasks.len();
    if n > 0 {
        save(&tasks)?;
    }
    Ok(n)
}

/// First pending task (for the work loop).
pub fn next_pending() -> Option<Task> {
    load().into_iter().find(|t| t.status == Status::Pending)
}

pub fn format_list() -> String {
    let tasks = load();
    if tasks.is_empty() {
        return "Aucune tâche. `/task add <texte>` pour en créer.".to_string();
    }
    let mut s = format!("Tâches ({}):\n", tasks.len());
    for t in &tasks {
        let role = t
            .role
            .as_deref()
            .map(|r| format!(" [{r}]"))
            .unwrap_or_default();
        s.push_str(&format!(
            "  {} {} {}{}\n",
            t.status.mark(),
            t.id,
            t.text,
            role
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_file_resolution_is_explicit() {
        let data = Path::new("/data/zer0");
        assert_eq!(
            task_file_from(None, data),
            PathBuf::from("/data/zer0/tasks.md")
        );
        assert_eq!(
            task_file_from(Some("   "), data),
            PathBuf::from("/data/zer0/tasks.md")
        );
        assert_eq!(
            task_file_from(Some("/tmp/projet-tasks.md"), data),
            PathBuf::from("/tmp/projet-tasks.md")
        );
    }

    #[test]
    fn newest_legacy_list_is_migrated_once() {
        let dir = std::env::temp_dir().join(format!("zer0-tasks-mig-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("a")).unwrap();
        fs::create_dir_all(dir.join("b")).unwrap();
        let old = dir.join("a/tasks.md");
        let recent = dir.join("b/tasks.md");
        fs::write(&old, "- [ ] ancienne").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        fs::write(&recent, "- [ ] récente").unwrap();
        let target = dir.join("cible/tasks.md");

        let src = migrate_legacy_from(&target, &[old.clone(), recent.clone()]);
        assert_eq!(src.as_deref(), Some(recent.as_path()));
        assert_eq!(fs::read_to_string(&target).unwrap(), "- [ ] récente");

        // Deuxième passe : la cible existe, on ne réécrase plus.
        assert!(migrate_legacy_from(&target, &[old]).is_none());
        assert_eq!(fs::read_to_string(&target).unwrap(), "- [ ] récente");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_paths_finds_real_ones() {
        let ps = extract_paths(
            "Coordination → D:\\x\\0box-test\\RAPPORT.md, intégrer dans D:\\x\\0box-test\\",
        );
        assert!(ps.contains(&"D:\\x\\0box-test\\RAPPORT.md".to_string()));
        assert!(ps.contains(&"D:\\x\\0box-test\\".to_string()));

        // ~ expandu vers le dossier personnel
        let ps = extract_paths("Générer ~/0box-test/sample.jsonl (20 lignes)");
        assert_eq!(ps.len(), 1);
        assert!(ps[0].ends_with("0box-test/sample.jsonl"));
        assert!(!ps[0].contains('~'));
    }

    #[test]
    fn extract_paths_rejects_false_positives() {
        assert!(extract_paths("choisir A et/ou B puis valider").is_empty());
        assert!(extract_paths("voir https://exemple.com/page/1 pour plus").is_empty());
        assert!(extract_paths("réponds directement dans le chat").is_empty());
        // nom de fichier sans répertoire : non détecté (limite assumée)
        assert!(extract_paths("écris RAPPORT.md puis arrête").is_empty());
        // notation d'extensions : pas un chemin
        assert!(extract_paths("entrées .txt/.jsonl, format txt|json").is_empty());
        // chemin relatif ambigu : non vérifiable honnêtement (cwd variable)
        assert!(extract_paths("crée 0box-test/RAPPORT.md ici").is_empty());
    }

    #[test]
    fn verify_flags_missing_and_empty() {
        let dir = std::env::temp_dir().join("zer0_tasks_test");
        let _ = fs::create_dir_all(&dir);
        let full = dir.join("plein.txt");
        fs::write(&full, "contenu").unwrap();
        let empty = dir.join("vide.txt");
        fs::write(&empty, "").unwrap();
        let missing = dir.join("absent.md");
        let _ = fs::remove_file(&missing);

        let fails = verify_deliverables(&[
            full.to_str().unwrap().to_string(),
            empty.to_str().unwrap().to_string(),
            missing.to_str().unwrap().to_string(),
            dir.to_str().unwrap().to_string(), // dossier existant = OK
        ]);
        assert_eq!(fails.len(), 2);
        assert!(fails.iter().any(|(p, _)| p.ends_with("vide.txt")));
        assert!(fails.iter().any(|(p, _)| p.ends_with("absent.md")));
        let _ = fs::remove_dir_all(&dir);
    }
}
