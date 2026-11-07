//! `/upgrade scan` — audit du dépôt pour les développeurs.
//!
//! Aucune modification : lecture de git, des sources et de la base de
//! connaissances, puis écriture d'un rapport dans
//! `~/.local/share/zer0gh0st/upgrade/audit-<date>.{md,json}`.
//!
//! La commande est réservée aux checkouts de développement (`Cargo.toml` +
//! `.git`, ou `ZER0_DEV=1`) : un utilisateur du binaire release n'en a pas
//! l'usage et ne doit pas voir ce rapport.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Result};

/// Fichiers où un `unwrap()` est considéré comme sensible (chemins chauds).
const HOT_FILES: &[&str] = &[
    "local.rs",
    "vault.rs",
    "config.rs",
    "knowledge.rs",
    "bin/zer0-gw.rs",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    High,
    Medium,
    Low,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::High => "haute",
            Severity::Medium => "moyenne",
            Severity::Low => "basse",
        }
    }
    fn json(self) -> &'static str {
        match self {
            Severity::High => "high",
            Severity::Medium => "medium",
            Severity::Low => "low",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub id: String,
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    pub files: Vec<String>,
    pub effort: String,
}

#[derive(Debug, Clone)]
pub struct Report {
    pub date: String,
    pub version: String,
    pub branch: String,
    pub dirty: usize,
    pub since_tag: Option<String>,
    pub changed_files: Vec<String>,
    pub findings: Vec<Finding>,
    pub md_path: PathBuf,
    pub json_path: PathBuf,
}

/// La commande est-elle disponible ? (checkout de dev ou `ZER0_DEV=1`)
pub fn is_dev() -> bool {
    if let Ok(v) = std::env::var("ZER0_DEV") {
        let v = v.trim();
        return !matches!(v, "" | "0" | "false" | "off");
    }
    let root = crate::mods::project_root();
    root.join("Cargo.toml").is_file() && root.join(".git").exists()
}

/// Lance l'audit complet (lecture seule).
pub fn scan() -> Result<Report> {
    let root = crate::mods::project_root();
    if !root.join("Cargo.toml").is_file() || !root.join(".git").exists() {
        return Err(anyhow!(
            "pas un checkout de développement : {} (Cargo.toml + .git requis)",
            root.display()
        ));
    }
    let version = cargo_version(&root)?;
    let branch = git(&root, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_default();
    let dirty = git(&root, &["status", "--porcelain"])
        .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count())
        .unwrap_or(0);
    let date = git(&root, &["log", "-1", "--format=%cs"]).unwrap_or_else(|| "sans-date".into());

    let since_tag = git(
        &root,
        &["describe", "--tags", "--abbrev=0", "--match", "v*"],
    );
    let changed_files: Vec<String> = match &since_tag {
        Some(tag) => git(&root, &["diff", "--name-only", &format!("{tag}..HEAD")])
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect(),
        None => git(&root, &["log", "--name-only", "--pretty=format:", "-20"])
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(str::to_string)
            .collect(),
    };

    let mut findings = Vec::new();
    scan_sources(&root, &mut findings);
    scan_backups(&root, &mut findings);
    scan_dirty(&dirty, &mut findings);
    scan_kb(&mut findings);
    scan_since_tag(&changed_files, &mut findings);

    // Tri : haute d'abord, puis par titre (stable pour le futur `fix`).
    findings.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then_with(|| a.title.cmp(&b.title))
    });
    for (i, f) in findings.iter_mut().enumerate() {
        f.id = format!("U{}", i + 1);
    }

    let dir = crate::config::data_dir().join("upgrade");
    std::fs::create_dir_all(&dir)?;
    let md_path = dir.join(format!("audit-{date}.md"));
    let json_path = dir.join(format!("audit-{date}.json"));
    let report = Report {
        date,
        version,
        branch,
        dirty,
        since_tag,
        changed_files,
        findings,
        md_path,
        json_path,
    };
    std::fs::write(&report.md_path, render_md(&report))?;
    std::fs::write(&report.json_path, render_json(&report))?;
    Ok(report)
}

fn cargo_version(root: &Path) -> Result<String> {
    let raw = std::fs::read_to_string(root.join("Cargo.toml"))?;
    raw.lines()
        .find_map(|l| l.strip_prefix("version = \""))
        .and_then(|v| v.split('"').next())
        .map(str::to_string)
        .ok_or_else(|| anyhow!("version introuvable dans Cargo.toml"))
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn scan_sources(root: &Path, out: &mut Vec<Finding>) {
    let mut files: Vec<PathBuf> = Vec::new();
    walk(&root.join("src"), &mut files);
    let mut unwrap_total = 0usize;
    let mut todos = 0usize;
    let mut todo_files: Vec<String> = Vec::new();
    let mut unsafe_files: Vec<String> = Vec::new();
    let mut big_files: Vec<String> = Vec::new();
    let mut commented: Vec<String> = Vec::new();
    let mut unwrap_files: Vec<(String, usize)> = Vec::new();

    for path in files
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
    {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();
        // Les tests gonflent artificiellement unwrap/expect/TODO : on ne
        // mesure que le code de production (tout ce qui précède le module de
        // tests). Le nombre de lignes, lui, reste celui du fichier entier.
        let production = text.split("\n#[cfg(test)]").next().unwrap_or(&text);
        // Le scanner se lit lui-même : ses chaînes `"TODO"`, `"unsafe {"`… ne
        // sont pas des findings. On ignore ses auto-références.
        let is_scanner = rel == "src/upgrade.rs";
        let lines = text.lines().count();
        let unwrap =
            count_occurrences(production, ".unwrap()") + count_occurrences(production, ".expect(");
        unwrap_total += unwrap;
        if unwrap > 0 {
            unwrap_files.push((rel.clone(), unwrap));
        }
        // TODO/FIXME/XXX : uniquement dans les commentaires (sinon une chaîne
        // de code qui contient « TODO » se compte elle-même).
        let todo = if is_scanner {
            0
        } else {
            production
                .lines()
                .filter(|l| l.trim_start().starts_with("//"))
                .map(|l| {
                    count_occurrences(l, "TODO")
                        + count_occurrences(l, "FIXME")
                        + count_occurrences(l, "XXX")
                })
                .sum::<usize>()
        };
        if todo > 0 {
            todos += todo;
            todo_files.push(format!("{rel} ({todo})"));
        }
        // `unsafe` réel : bloc/fonction/impl, pas le mot dans une chaîne.
        if !is_scanner
            && (production.contains("unsafe {")
                || production.contains("unsafe fn ")
                || production.contains("unsafe impl "))
        {
            unsafe_files.push(rel.clone());
        }
        for (n, line) in production.lines().enumerate() {
            if let Some(code) = commented_code(line) {
                commented.push(format!("{rel}:{} — {code}", n + 1));
            }
        }
        if lines > 900 {
            big_files.push(format!("{rel} ({lines} lignes)"));
        }
        // Chemin chaud : unwrap/expect concentrés.
        if HOT_FILES.iter().any(|h| rel.ends_with(*h)) && unwrap >= 5 {
            out.push(Finding {
                id: String::new(),
                severity: Severity::Medium,
                title: format!("{} unwrap()/expect() dans {}", unwrap, rel),
                detail: format!(
                    "{} occurrences dans un chemin chaud : un I/O raté (permission, \
                     disque plein, JSON corrompu) provoque un panic au lieu d'une erreur.",
                    unwrap
                ),
                files: vec![rel.clone()],
                effort: "faible".into(),
            });
        }
    }

    if unwrap_total > 0 {
        unwrap_files.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out.push(Finding {
            id: String::new(),
            severity: Severity::Low,
            title: format!("{unwrap_total} unwrap()/expect() dans src/ (hors tests)"),
            detail: "À traiter en priorité sur les chemins chauds ; les tests ne sont pas comptés."
                .into(),
            files: unwrap_files
                .iter()
                .take(8)
                .map(|(f, n)| format!("{f} ({n})"))
                .collect(),
            effort: "moyen".into(),
        });
    }
    if todos > 0 {
        out.push(Finding {
            id: String::new(),
            severity: Severity::Low,
            title: format!("{todos} TODO/FIXME/XXX"),
            detail: format!("Répartis dans {} fichier(s).", todo_files.len()),
            files: todo_files.into_iter().take(12).collect(),
            effort: "faible".into(),
        });
    }
    if !unsafe_files.is_empty() {
        out.push(Finding {
            id: String::new(),
            severity: Severity::High,
            title: format!("{} fichier(s) avec `unsafe`", unsafe_files.len()),
            detail: "Chaque bloc `unsafe` doit être justifié par un commentaire `// SAFETY:`."
                .into(),
            files: unsafe_files,
            effort: "moyen".into(),
        });
    }
    if !commented.is_empty() {
        let severity = if commented.len() >= 20 {
            Severity::Medium
        } else {
            Severity::Low
        };
        out.push(Finding {
            id: String::new(),
            severity,
            title: format!("{} ligne(s) de code commenté", commented.len()),
            detail: "Code désactivé (pas de la documentation) : à supprimer s'il est mort, \
                     à réactiver s'il est utile. Les `///` et `//!` ne sont pas comptés."
                .into(),
            files: commented.into_iter().take(12).collect(),
            effort: "faible".into(),
        });
    }
    if !big_files.is_empty() {
        out.push(Finding {
            id: String::new(),
            severity: Severity::Low,
            title: format!("{} fichier(s) > 900 lignes", big_files.len()),
            detail: "Candidats à un découpage (lisibilité, revue, conflits git).".into(),
            files: big_files.into_iter().take(8).collect(),
            effort: "moyen".into(),
        });
    }
}

/// Une ligne `// …` ressemble-t-elle à du **code désactivé** ?
///
/// Heuristique stricte : on ne veut pas classer la prose en « code mort »
/// (les commentaires de décision/pièges restent intacts). Les commentaires
/// de doc (`///`, `//!`) ne sont jamais concernés.
pub fn commented_code(line: &str) -> Option<&str> {
    let t = line.trim_start();
    let rest = t.strip_prefix("//")?;
    if rest.starts_with('/') || rest.starts_with('!') {
        return None; // doc
    }
    let code = rest.trim();
    if code.is_empty() {
        return None;
    }
    const KEYWORDS: &[&str] = &[
        "let ",
        "fn ",
        "pub ",
        "use ",
        "if ",
        "else ",
        "for ",
        "while ",
        "loop ",
        "match ",
        "impl ",
        "struct ",
        "enum ",
        "mod ",
        "return ",
        "break;",
        "continue;",
        "const ",
        "static ",
        "trait ",
        "type ",
        "unsafe ",
        "async fn ",
        "} else",
    ];
    if KEYWORDS.iter().any(|k| code.starts_with(k)) {
        return Some(code);
    }
    if code == "{" || code == "}" || code == "};" {
        return Some(code);
    }
    // Ligne terminée par `;` (sans espace avant) avec une syntaxe de code et
    // peu de mots : évite « … renommé en canonique ; » (phrase française).
    let words = code.split_whitespace().count();
    if code.ends_with(';')
        && !code.ends_with(" ;")
        && words <= 10
        && (code.contains('(') || code.contains('=') || code.contains("::") || code.contains('!'))
    {
        return Some(code);
    }
    None
}

fn count_occurrences(haystack: &str, needle: &str) -> usize {
    haystack.match_indices(needle).count()
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn scan_backups(root: &Path, out: &mut Vec<Finding>) {
    let mut files = Vec::new();
    walk(root, &mut files);
    let baks: Vec<String> = files
        .iter()
        .filter(|p| p.to_string_lossy().ends_with(".zer0.bak"))
        .map(|p| {
            p.strip_prefix(root)
                .unwrap_or(p)
                .to_string_lossy()
                .to_string()
        })
        .collect();
    if !baks.is_empty() {
        out.push(Finding {
            id: String::new(),
            severity: Severity::Low,
            title: format!("{} sauvegarde(s) .zer0.bak", baks.len()),
            detail: "Fichiers de sauvegarde laissés par text_editor : à supprimer ou .gitignore."
                .into(),
            files: baks.into_iter().take(10).collect(),
            effort: "trivial".into(),
        });
    }
}

fn scan_dirty(dirty: &usize, out: &mut Vec<Finding>) {
    if *dirty > 0 {
        out.push(Finding {
            id: String::new(),
            severity: Severity::Low,
            title: format!("{dirty} fichier(s) modifié(s) non commité(s)"),
            detail: "Un scan sur un arbre sale peut attribuer un bug au mauvais commit.".into(),
            files: Vec::new(),
            effort: "trivial".into(),
        });
    }
}

fn scan_kb(out: &mut Vec<Finding>) {
    let Ok(c) = crate::knowledge::open() else {
        return;
    };
    let Ok(mut stmt) = c.prepare("SELECT source, COUNT(*) FROM knowledge GROUP BY source") else {
        return;
    };
    let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))) else {
        return;
    };
    let mut groups: BTreeMap<String, Vec<(String, i64)>> = BTreeMap::new();
    for row in rows.flatten() {
        groups
            .entry(crate::knowledge::canonical_source(&row.0))
            .or_default()
            .push(row);
    }
    let dup: Vec<String> = groups
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|(canon, v)| {
            let detail: Vec<String> = v.iter().map(|(s, n)| format!("{s} ({n})")).collect();
            format!("{canon} ← {}", detail.join(" + "))
        })
        .collect();
    if !dup.is_empty() {
        out.push(Finding {
            id: String::new(),
            severity: Severity::High,
            title: format!("{} source(s) de connaissance en double", dup.len()),
            detail: "Même fichier indexé sous plusieurs formes : `zer0 kb dedupe` les fusionne."
                .into(),
            files: dup.into_iter().take(6).collect(),
            effort: "trivial".into(),
        });
    }
}

fn scan_since_tag(changed: &[String], out: &mut Vec<Finding>) {
    if changed.is_empty() {
        return;
    }
    out.push(Finding {
        id: String::new(),
        severity: Severity::Low,
        title: format!(
            "{} fichier(s) modifiés depuis la dernière release",
            changed.len()
        ),
        detail: "Périmètre prioritaire pour la revue (le reste n'a pas bougé).".into(),
        files: changed.iter().take(15).cloned().collect(),
        effort: "trivial".into(),
    });
}

fn render_md(r: &Report) -> String {
    let mut s = format!(
        "# Audit `/upgrade scan` — {}\n\n\
         - Version : {}\n- Branche : {}\n- Fichiers modifiés (non commités) : {}\n",
        r.date, r.version, r.branch, r.dirty
    );
    if let Some(tag) = &r.since_tag {
        s.push_str(&format!(
            "- Depuis `{tag}` : {} fichier(s) modifié(s)\n",
            r.changed_files.len()
        ));
    } else {
        s.push_str("- Aucun tag `v*` : périmètre = 20 derniers commits\n");
    }
    s.push_str("\n## Findings\n\n");
    if r.findings.is_empty() {
        s.push_str("Aucun finding.\n");
    }
    for f in &r.findings {
        s.push_str(&format!(
            "### {} — {} ({})\n\n{}\n\n",
            f.id,
            f.title,
            f.severity.label(),
            f.detail
        ));
        if !f.files.is_empty() {
            s.push_str("Fichiers :\n");
            for file in &f.files {
                s.push_str(&format!("- `{file}`\n"));
            }
            s.push('\n');
        }
        s.push_str(&format!("Effort : {}\n\n", f.effort));
    }
    s.push_str("\n_Prochaine étape : `/upgrade fix` (à venir)._\n");
    s
}

fn render_json(r: &Report) -> String {
    let findings: Vec<serde_json::Value> = r
        .findings
        .iter()
        .map(|f| {
            serde_json::json!({
                "id": &f.id,
                "severity": f.severity.json(),
                "title": &f.title,
                "detail": &f.detail,
                "files": &f.files,
                "effort": &f.effort,
            })
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "date": r.date,
        "version": r.version,
        "branch": r.branch,
        "dirty": r.dirty,
        "since_tag": &r.since_tag,
        "changed_files": &r.changed_files,
        "findings": findings,
    }))
    .unwrap_or_else(|_| "{}".to_string())
}

/// Résumé court pour le chat.
pub fn summary(r: &Report) -> String {
    let high = r
        .findings
        .iter()
        .filter(|f| f.severity == Severity::High)
        .count();
    let med = r
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Medium)
        .count();
    let low = r
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Low)
        .count();
    let mut s = format!(
        "🔧 Audit {} — {} finding(s) : {} haute, {} moyenne, {} basse\n",
        r.version,
        r.findings.len(),
        high,
        med,
        low
    );
    for f in &r.findings {
        s.push_str(&format!(
            "  {} [{}] {}\n",
            f.id,
            f.severity.label(),
            f.title
        ));
    }
    s.push_str(&format!(
        "\nRapport : {}\nJSON : {}",
        r.md_path.display(),
        r.json_path.display()
    ));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn occurrence_counting_is_literal() {
        assert_eq!(count_occurrences("a.unwrap().unwrap()", ".unwrap()"), 2);
        assert_eq!(count_occurrences("TODO: x\nFIXME", "TODO"), 1);
    }

    #[test]
    fn commented_code_detects_disabled_code_only() {
        assert!(commented_code("// let x = 1;").is_some());
        assert!(commented_code("  // }").is_some());
        assert!(commented_code("// pub fn f() {").is_some());
        assert!(commented_code("// foo();").is_some());
        // Documentation et prose : jamais « code commenté ».
        assert!(commented_code("/// doc publique").is_none());
        assert!(commented_code("//! doc de module").is_none());
        assert!(commented_code("// le serveur fige, ne pas utiliser").is_none());
        assert!(commented_code("// async (l'outil `0box` → débat → sous-agent).").is_none());
        assert!(commented_code(
            "// Le groupe le plus récent (relatif) est gardé, renommé en canonique ;"
        )
        .is_none());
        assert!(commented_code("// ─── section ───").is_none());
        assert!(commented_code("let x = 1;").is_none());
    }

    #[test]
    #[ignore = "écrit un rapport dans le dossier de données ; lancer à la main"]
    fn scan_smoke() {
        let r = scan().expect("scan");
        println!("{}", summary(&r));
    }

    #[test]
    fn report_renders_ids_and_paths() {
        let r = Report {
            date: "2026-11-05".into(),
            version: "0.7.32".into(),
            branch: "main".into(),
            dirty: 0,
            since_tag: Some("v0.7.31".into()),
            changed_files: vec!["src/a.rs".into()],
            findings: vec![Finding {
                id: "U1".into(),
                severity: Severity::High,
                title: "test".into(),
                detail: "détail".into(),
                files: vec!["src/a.rs".into()],
                effort: "faible".into(),
            }],
            md_path: PathBuf::from("/tmp/a.md"),
            json_path: PathBuf::from("/tmp/a.json"),
        };
        assert!(render_md(&r).contains("### U1"));
        assert!(render_json(&r).contains("\"severity\": \"high\""));
        assert!(summary(&r).contains("U1"));
    }
}
