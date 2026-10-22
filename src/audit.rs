//! Journal d'audit des actions de l'agent.
//!
//! ZER0|GH0ST n'a **pas** de bac à sable : il agit avec les droits de
//! l'utilisateur. Le corollaire non négociable, c'est que ses actions doivent
//! être **relisibles après coup**. Avant ce module, une commande exécutée ou un
//! fichier écrasé ne laissait aucune trace : impossible de reconstituer ce que
//! l'agent avait fait, ni quand.
//!
//! Format : un objet JSON par ligne (JSONL), ajout seul, jamais réécrit.
//! Emplacement : `<data>/audit.jsonl` (rotation à 8 Mio → `audit.1.jsonl`).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Taille au-delà de laquelle le journal est tourné.
const MAX_BYTES: u64 = 8 * 1024 * 1024;
/// Longueur maximale du résumé de résultat conservé (les arguments, eux, sont
/// conservés en entier : c'est ce qui permet de vérifier une action).
const MAX_RESULT_CHARS: usize = 600;

fn audit_dir() -> PathBuf {
    crate::config::data_dir()
}

/// Chemin du journal actif.
pub fn path() -> PathBuf {
    audit_dir().join("audit.jsonl")
}

/// Enregistre une action d'outil. Ne panique jamais, ne bloque jamais un tour :
/// une erreur d'écriture est signalée sur stderr et on continue.
pub fn record(tool: &str, args: &str, result: &str) {
    let line = format_line(tool, args, result);
    if let Err(e) = append_to(&audit_dir(), &line) {
        eprintln!("⚠ audit: écriture impossible ({e})");
    }
}

/// Construit la ligne JSONL (fonction pure, testable).
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

/// Ajoute une ligne au journal de `dir`, avec rotation.
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
    // Pas de buffer : une ligne = un `write`, lisible même si le process meurt.
    f.flush()
}

/// Les `n` dernières actions (les plus récentes en dernier).
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
        // Les arguments sont conservés en entier : c'est la trace vérifiable.
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
    fn rotation_keeps_a_previous_file() {
        let dir = tmpdir("rotate");
        std::fs::create_dir_all(&dir).unwrap();
        // Un fichier déjà au-delà du seuil déclenche la rotation.
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
