use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

const MAX_BYTES: u64 = 8 * 1024 * 1024;
const DEFAULT_RETENTION_DAYS: u64 = 90;
const MAX_RESULT_CHARS: usize = 600;

fn audit_dir() -> PathBuf {
    crate::config::data_dir()
}

pub fn path() -> PathBuf {
    audit_dir().join("audit.jsonl")
}

pub fn record(tool: &str, args: &str, result: &str) {
    static PURGED: std::sync::Once = std::sync::Once::new();
    PURGED.call_once(|| {
        let days = retention_days_from(std::env::var("ZER0_AUDIT_RETENTION_DAYS").ok().as_deref());
        let _ = purge_older_than(&audit_dir(), days);
    });
    let line = format_line(tool, args, result);
    if let Err(e) = append_to(&audit_dir(), &line) {
        eprintln!("⚠ audit: écriture impossible ({e})");
    }
}

pub fn format_line(tool: &str, args: &str, result: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let summary: String = result.chars().take(MAX_RESULT_CHARS).collect();
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    serde_json::json!({
        "ts": ts,
        "tool": tool,
        "args": args,
        "cwd": cwd,
        "started_by": std::env::var("ZER0_CMD_NAME").unwrap_or_else(|_| "zer0".into()),
        "result": summary,
        "result_len": result.chars().count(),
    })
    .to_string()
}

pub fn append_to(dir: &Path, line: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let file = dir.join("audit.jsonl");
    if let Ok(meta) = std::fs::metadata(&file) {
        if meta.len() >= MAX_BYTES {
            let _ = std::fs::rename(&file, dir.join("audit.1.jsonl"));
        }
    }
    let mut f = OpenOptions::new().create(true).append(true).open(&file)?;
    writeln!(f, "{line}")?;
    f.flush()
}

pub fn retention_days_from(env: Option<&str>) -> u64 {
    match env.and_then(|v| v.trim().parse::<u64>().ok()) {
        Some(v) => v,
        None => DEFAULT_RETENTION_DAYS,
    }
}

pub fn purge_older_than(dir: &Path, days: u64) -> std::io::Result<usize> {
    if days == 0 {
        return Ok(0);
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let cutoff = now.saturating_sub(days.saturating_mul(24 * 3600));
    let mut removed = 0usize;
    for name in ["audit.jsonl", "audit.1.jsonl"] {
        let file = dir.join(name);
        let Ok(raw) = std::fs::read_to_string(&file) else {
            continue;
        };
        let kept: Vec<&str> = raw
            .lines()
            .filter(|l| match serde_json::from_str::<serde_json::Value>(l) {
                Ok(v) => v.get("ts").and_then(|t| t.as_u64()).unwrap_or(now) >= cutoff,
                Err(_) => true,
            })
            .collect();
        let before = raw.lines().count();
        if kept.len() == before {
            continue;
        }
        removed += before - kept.len();
        let tmp = dir.join(format!("{name}.tmp"));
        let mut out = String::new();
        for l in kept {
            out.push_str(l);
            out.push('\n');
        }
        std::fs::write(&tmp, out)?;
        std::fs::rename(&tmp, &file)?;
    }
    Ok(removed)
}

pub fn tail(n: usize) -> Vec<String> {
    let Ok(raw) = std::fs::read_to_string(path()) else {
        return Vec::new();
    };
    let lines: Vec<&str> = raw.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("zer0-audit-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn line_is_valid_jsonl_with_full_args() {
        let line = format_line("code_exec", r#"{"command":"rm -rf /tmp/x"}"#, "sortie");
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["tool"], "code_exec");
        assert!(v["args"].as_str().unwrap().contains("rm -rf"));
        assert_eq!(v["result"], "sortie");
    }

    #[test]
    fn long_results_are_summarized_but_counted() {
        let long = "x".repeat(5_000);
        let v: serde_json::Value = serde_json::from_str(&format_line("t", "{}", &long)).unwrap();
        assert_eq!(
            v["result"].as_str().unwrap().chars().count(),
            MAX_RESULT_CHARS
        );
        assert_eq!(v["result_len"], 5_000);
    }

    #[test]
    fn appends_in_order_and_rotates() {
        let dir = tmpdir("append");
        append_to(&dir, "{\"n\":1}").unwrap();
        append_to(&dir, "{\"n\":2}").unwrap();
        let raw = std::fs::read_to_string(dir.join("audit.jsonl")).unwrap();
        let ns: Vec<&str> = raw.lines().collect();
        assert_eq!(ns, vec!["{\"n\":1}", "{\"n\":2}"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn retention_parses_and_defaults() {
        assert_eq!(retention_days_from(None), DEFAULT_RETENTION_DAYS);
        assert_eq!(retention_days_from(Some("7")), 7);
        assert_eq!(retention_days_from(Some(" 0 ")), 0);
        assert_eq!(retention_days_from(Some("nope")), DEFAULT_RETENTION_DAYS);
    }

    #[test]
    fn retention_drops_old_lines_and_keeps_unparsable_ones() {
        let dir = tmpdir("retention");
        std::fs::create_dir_all(&dir).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let old = now - 200 * 24 * 3600;
        let body = format!(
            "{{\"ts\":{old},\"tool\":\"vieux\"}}\n\
             {{\"ts\":{now},\"tool\":\"recent\"}}\n\
             ligne illisible\n"
        );
        std::fs::write(dir.join("audit.jsonl"), body).unwrap();

        let removed = purge_older_than(&dir, 90).unwrap();
        assert_eq!(removed, 1);
        let kept = std::fs::read_to_string(dir.join("audit.jsonl")).unwrap();
        assert!(kept.contains("recent"));
        assert!(!kept.contains("vieux"));
        assert!(kept.contains("ligne illisible"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rotation_keeps_a_previous_file() {
        let dir = tmpdir("rotate");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("audit.jsonl"),
            vec![b'x'; (MAX_BYTES as usize) + 10],
        )
        .unwrap();
        append_to(&dir, "{\"n\":3}").unwrap();
        assert!(dir.join("audit.1.jsonl").exists(), "rotation absente");
        assert_eq!(
            std::fs::read_to_string(dir.join("audit.jsonl"))
                .unwrap()
                .trim(),
            "{\"n\":3}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
