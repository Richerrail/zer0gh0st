use anyhow::Result;
use std::fs;
use std::path::PathBuf;

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

/// Markdown task list at `.zer0/tasks.md`.
pub fn path() -> PathBuf {
    PathBuf::from(".zer0").join("tasks.md")
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

pub fn set_status(id: &str, status: Status) -> Result<bool> {
    let mut tasks = load();
    let mut found = false;
    for t in &mut tasks {
        if t.id == id {
            t.status = status;
            found = true;
        }
    }
    if found {
        save(&tasks)?;
    }
    Ok(found)
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
        let role = t.role.as_deref().map(|r| format!(" [{r}]")).unwrap_or_default();
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
