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
    pub due: Option<u64>,
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn parse_due(raw: &str) -> Option<u64> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Some(rest) = raw.strip_prefix('+') {
        let (num, unit) = rest.split_at(rest.find(|c: char| !c.is_ascii_digit())?);
        let n: u64 = num.parse().ok()?;
        let secs = match unit.trim().to_ascii_lowercase().as_str() {
            "m" | "min" | "mins" => n * 60,
            "h" | "hr" | "hrs" => n * 3600,
            "d" | "j" | "jour" | "jours" => n * 86_400,
            _ => return None,
        };
        return Some(now_secs() + secs);
    }
    raw.parse::<u64>().ok().map(|v| v.max(now_secs()))
}

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

pub fn task_file_from(env: Option<&str>, data_dir: &Path) -> PathBuf {
    env.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir.join("tasks.md"))
}

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

pub fn migrate_legacy(target: &Path) -> Option<PathBuf> {
    migrate_legacy_from(target, &legacy_candidates())
}

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
        let mut it = rest.splitn(4, '|').map(|x| x.trim());
        let id = it.next().unwrap_or("").to_string();
        let role = it.next().unwrap_or("-").to_string();
        let text = it.next().unwrap_or("").to_string();
        let due = it
            .next()
            .and_then(|f| f.strip_prefix("due="))
            .and_then(|v| v.parse::<u64>().ok());
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
            due,
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
            "- {} {} | {} | {}",
            t.status.mark(),
            t.id,
            t.role.clone().unwrap_or_else(|| "-".to_string()),
            t.text
        ));
        if let Some(due) = t.due {
            s.push_str(&format!(" | due={due}"));
        }
        s.push('\n');
    }
    fs::write(&p, s)?;
    Ok(())
}

pub fn add(text: &str, role: Option<String>) -> Result<Task> {
    add_with_due(text, role, None)
}

pub fn add_with_due(text: &str, role: Option<String>, due: Option<u64>) -> Result<Task> {
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
        due,
    };
    tasks.push(task.clone());
    save(&tasks)?;
    Ok(task)
}

#[derive(Debug, PartialEq)]
pub enum Update {
    Ok(Status),
    NotFound,
    Refused(Vec<(String, String)>),
}

pub fn set_status(id: &str, status: Status) -> Result<Update> {
    let mut tasks = load();
    let mut found = false;
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
            continue;
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
            || (tok.len() > 1 && tok.as_bytes()[1] == b':');
        if !rooted {
            continue;
        }
        out.push(expand_home(tok));
    }
    out
}

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

pub fn next_pending() -> Option<Task> {
    load().into_iter().find(|t| t.status == Status::Pending)
}

pub const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

fn mark_for(status: Status, phase: Option<usize>) -> String {
    match (status, phase) {
        (Status::Claimed, Some(p)) => format!("[{}]", SPINNER[p % SPINNER.len()]),
        _ => status.mark().to_string(),
    }
}

pub fn format_tasks(tasks: &[Task], phase: Option<usize>) -> String {
    if tasks.is_empty() {
        return "Aucune tâche. `/task add <texte>` pour en créer.".to_string();
    }
    let mut s = format!("Tâches ({}):\n", tasks.len());
    for t in tasks {
        let role = t
            .role
            .as_deref()
            .map(|r| format!(" [{r}]"))
            .unwrap_or_default();
        s.push_str(&format!(
            "  {} {} {}{}\n",
            mark_for(t.status, phase),
            t.id,
            t.text,
            role
        ));
    }
    s
}

pub fn format_list() -> String {
    format_tasks(&load(), None)
}

pub fn format_list_animated(phase: usize) -> String {
    format_tasks(&load(), Some(phase))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_due_accepts_offsets_and_epoch() {
        let now = now_secs();
        let h = parse_due("+2h").expect("+2h");
        assert!(h >= now + 7000 && h <= now + 7500, "{h} vs {now}");
        let m = parse_due("+30m").expect("+30m");
        assert!(m >= now + 1700 && m <= now + 1900);
        assert_eq!(parse_due("nope"), None);
        assert_eq!(parse_due(""), None);
        assert!(parse_due("1").unwrap() >= now);
    }

    #[test]
    fn spinner_frames_are_one_column_wide() {
        use unicode_width::UnicodeWidthChar;
        for c in SPINNER {
            assert_eq!(
                UnicodeWidthChar::width(*c),
                Some(1),
                "frame {c:?} n'occupe pas exactement une colonne"
            );
        }
    }

    #[test]
    fn in_progress_task_animates_between_brackets() {
        let tasks = vec![
            Task {
                id: "t1".into(),
                text: "faite".into(),
                status: Status::Done,
                role: None,
                due: None,
            },
            Task {
                id: "t2".into(),
                text: "en cours".into(),
                status: Status::Claimed,
                role: None,
                due: None,
            },
            Task {
                id: "t3".into(),
                text: "à faire".into(),
                status: Status::Pending,
                role: None,
                due: None,
            },
        ];
        let plain = format_tasks(&tasks, None);
        assert!(plain.contains("[x] t1"));
        assert!(plain.contains("[~] t2"));
        assert!(plain.contains("[ ] t3"));

        let a = format_tasks(&tasks, Some(0));
        let b = format_tasks(&tasks, Some(1));
        assert!(a.contains(&format!("[{}] t2", SPINNER[0])));
        assert!(b.contains(&format!("[{}] t2", SPINNER[1])));
        assert!(a.contains("[x] t1"));
        assert!(a.contains("[ ] t3"));
        let wrapped = format_tasks(&tasks, Some(SPINNER.len() + 2));
        assert!(wrapped.contains(&format!("[{}] t2", SPINNER[2])));
    }

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
        assert!(extract_paths("écris RAPPORT.md puis arrête").is_empty());
        assert!(extract_paths("entrées .txt/.jsonl, format txt|json").is_empty());
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
            dir.to_str().unwrap().to_string(),
        ]);
        assert_eq!(fails.len(), 2);
        assert!(fails.iter().any(|(p, _)| p.ends_with("vide.txt")));
        assert!(fails.iter().any(|(p, _)| p.ends_with("absent.md")));
        let _ = fs::remove_dir_all(&dir);
    }
}
