//! `/zer0push` — tests, push, pull request.
//!
//! Enchaîne, dans l'ordre : vérifications git → tests (`cargo test`) →
//! `git push -u` → `gh pr create`. Garde-fous : jamais de `--force`, jamais
//! de push direct sur `main`/`master` sans `--allow-main`, arbre sale refusé
//! sauf `--commit` (commit explicite, jamais de `git add -A` aveugle).
//!
//! Logique pure (`parse_args`, `validate`) séparée de l'exécution
//! (`run`) : testable sans réseau ni dépôt.

use std::path::Path;
use std::process::Command;

use anyhow::{anyhow, Result};

/// Options parsées de `/zer0push` (et de `zer0 zer0push`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushOptions {
    /// Titre de la PR (défaut : branche courante).
    pub title: Option<String>,
    /// Corps de la PR.
    pub body: Option<String>,
    /// Branche de base (défaut : branche par défaut du remote, sinon `main`).
    pub base: Option<String>,
    /// PR en brouillon.
    pub draft: bool,
    /// Ne pas lancer les tests avant le push.
    pub no_tests: bool,
    /// Afficher le plan sans rien exécuter.
    pub dry_run: bool,
    /// Committer l'arbre sale avant le push (message = titre ou défaut).
    pub commit: bool,
    /// Autoriser le push sur `main`/`master` (déconseillé).
    pub allow_main: bool,
    /// Remote à utiliser (défaut : `origin`).
    pub remote: String,
}

impl Default for PushOptions {
    fn default() -> Self {
        Self {
            title: None,
            body: None,
            base: None,
            draft: false,
            no_tests: false,
            dry_run: false,
            commit: false,
            allow_main: false,
            remote: "origin".to_string(),
        }
    }
}

const USAGE: &str = "usage: zer0push [titre…] [--base <branche>] [--body <texte>] [--remote <nom>] \
[--draft] [--no-tests] [--commit] [--allow-main] [--dry-run]";

/// Parse les arguments (même syntaxe en TUI `/zer0push …` et CLI `zer0 zer0push …`).
pub fn parse_args(args: &[&str]) -> Result<PushOptions> {
    let mut o = PushOptions::default();
    let mut title: Vec<&str> = Vec::new();
    let mut it = args.iter();
    while let Some(w) = it.next() {
        match *w {
            "--draft" => o.draft = true,
            "--no-tests" | "--skip-tests" => o.no_tests = true,
            "--dry-run" | "--dryrun" => o.dry_run = true,
            "--commit" => o.commit = true,
            "--allow-main" => o.allow_main = true,
            "--base" | "-B" => {
                o.base = Some(it.next().map(|s| s.to_string()).ok_or_else(|| anyhow!("{USAGE}"))?)
            }
            "--body" | "-b" => {
                o.body = Some(it.next().map(|s| s.to_string()).ok_or_else(|| anyhow!("{USAGE}"))?)
            }
            "--remote" | "-r" => {
                o.remote = it.next().map(|s| s.to_string()).ok_or_else(|| anyhow!("{USAGE}"))?
            }
            "--title" | "-t" => {
                o.title = Some(it.next().map(|s| s.to_string()).ok_or_else(|| anyhow!("{USAGE}"))?)
            }
            other if other.starts_with("--base=") => o.base = Some(other["--base=".len()..].to_string()),
            other if other.starts_with("--remote=") => {
                o.remote = other["--remote=".len()..].to_string()
            }
            other if other.starts_with("--title=") => {
                o.title = Some(other["--title=".len()..].to_string())
            }
            other if other.starts_with("--body=") => o.body = Some(other["--body=".len()..].to_string()),
            other if other.starts_with('-') => return Err(anyhow!("option inconnue `{other}` — {USAGE}")),
            other => title.push(other),
        }
    }
    if o.title.is_none() && !title.is_empty() {
        o.title = Some(title.join(" "));
    }
    if o.remote.trim().is_empty() {
        return Err(anyhow!("remote vide — {USAGE}"));
    }
    Ok(o)
}

/// Branches sur lesquelles on refuse le push direct sans `--allow-main`.
pub fn is_protected(branch: &str) -> bool {
    matches!(branch.trim(), "main" | "master")
}

/// Valide l'état du dépôt avant toute action (fonction pure sur les faits).
pub fn validate(branch: &str, dirty: usize, opts: &PushOptions) -> Result<()> {
    if branch.trim().is_empty() {
        return Err(anyhow!("branche courante introuvable (HEAD détaché ?)"));
    }
    if is_protected(branch) && !opts.allow_main {
        return Err(anyhow!(
            "refus : push sur `{branch}` — passe sur une branche de feature, \
             ou force avec --allow-main"
        ));
    }
    if dirty > 0 && !opts.commit && !opts.dry_run {
        return Err(anyhow!(
            "arbre sale ({dirty} fichier(s)) — committe d'abord, ou relance avec --commit"
        ));
    }
    Ok(())
}

/// Résultat d'une exécution (lignes à afficher, succès global).
#[derive(Debug, Clone)]
pub struct PushReport {
    pub lines: Vec<String>,
    pub ok: bool,
    /// URL de la PR si créée.
    pub pr_url: Option<String>,
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| anyhow!("git introuvable : {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(anyhow!(
            "git {} : {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

fn run_cmd(root: &Path, prog: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(prog)
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| anyhow!("{prog} introuvable : {e}"))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if out.status.success() {
        Ok(text.trim().to_string())
    } else {
        Err(anyhow!("{prog} {} : {}", args.join(" "), text.trim()))
    }
}

/// Exécute le pipeline complet. `root` = racine du dépôt (défaut : projet courant).
pub fn run(opts: &PushOptions) -> Result<PushReport> {
    run_in(&crate::mods::project_root(), opts)
}

/// Variante testable avec une racine explicite.
pub fn run_in(root: &Path, opts: &PushOptions) -> Result<PushReport> {
    let mut lines: Vec<String> = Vec::new();
    let mut pr_url: Option<String> = None;

    if !root.join("Cargo.toml").is_file() || !root.join(".git").exists() {
        return Err(anyhow!(
            "pas un checkout de développement : {} (Cargo.toml + .git requis)",
            root.display()
        ));
    }
    let branch = git(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    let dirty = git(root, &["status", "--porcelain"])
        .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count())
        .unwrap_or(0);
    validate(&branch, dirty, opts)?;

    let title = opts
        .title
        .clone()
        .unwrap_or_else(|| branch.clone());
    let base = opts
        .base
        .clone()
        .or_else(|| {
            git(root, &["symbolic-ref", "refs/remotes/origin/HEAD"])
                .ok()
                .map(|s| {
                    s.trim()
                        .trim_start_matches("refs/remotes/")
                        .trim_start_matches("origin/")
                        .to_string()
                })
        })
        .unwrap_or_else(|| "main".to_string());

    lines.push(format!(
        "🌿 branche : {branch} → base : {base} (remote {})",
        opts.remote
    ));
    lines.push(format!("📌 titre PR : {title}"));

    if opts.dry_run {
        lines.push("🧪 dry-run : aucune commande exécutée".to_string());
        if !opts.no_tests {
            lines.push("   1. cargo test".to_string());
        }
        if dirty > 0 && opts.commit {
            lines.push("   2. git add -u && git commit".to_string());
        }
        lines.push(format!("   3. git push -u {} {branch}", opts.remote));
        lines.push(format!(
            "   4. gh pr create --base {base} --title \"{title}\"{}",
            if opts.draft { " --draft" } else { "" }
        ));
        return Ok(PushReport { lines, ok: true, pr_url });
    }

    // 1. Tests
    if !opts.no_tests {
        lines.push("🧪 cargo test…".to_string());
        match run_cmd(root, "cargo", &["test"]) {
            Ok(out) => {
                let tail: Vec<&str> = out.lines().rev().take(3).collect();
                for l in tail.iter().rev() {
                    lines.push(format!("   {l}"));
                }
                lines.push("✅ tests OK".to_string());
            }
            Err(e) => {
                lines.push(format!("❌ tests en échec : {e}"));
                lines.push("push annulé — corrige, ou relance avec --no-tests".to_string());
                return Ok(PushReport { lines, ok: false, pr_url });
            }
        }
    } else {
        lines.push("⏭ tests sautés (--no-tests)".to_string());
    }

    // 2. Commit éventuel
    if dirty > 0 && opts.commit {
        lines.push("📝 commit de l'arbre sale…".to_string());
        // `git add -u` seul laisse les **nouveaux** fichiers hors du commit :
        // c'est ce qui a produit une CI rouge (module absent du dépôt).
        // On ajoute donc aussi les fichiers non suivis, en respectant .gitignore.
        git(root, &["add", "-u"])?;
        let untracked = git(root, &["ls-files", "--others", "--exclude-standard"])?;
        let new_files: Vec<&str> = untracked
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        if !new_files.is_empty() {
            let mut add: Vec<&str> = vec!["add", "--"];
            add.extend(new_files.iter().copied());
            git(root, &add)?;
            lines.push(format!("   + {} nouveau(x) fichier(s)", new_files.len()));
        }
        git(root, &["commit", "-m", &title])?;
        lines.push("✅ commité".to_string());
    }

    // 3. Push (jamais --force)
    lines.push(format!("⬆ git push -u {} {branch}…", opts.remote));
    match git(root, &["push", "-u", &opts.remote, &branch]) {
        Ok(out) => {
            if !out.is_empty() {
                lines.push(out);
            }
            lines.push("✅ poussé".to_string());
        }
        Err(e) => {
            lines.push(format!("❌ push en échec : {e}"));
            return Ok(PushReport { lines, ok: false, pr_url });
        }
    }

    // 4. PR via gh
    lines.push("🔀 création de la PR…".to_string());
    let mut gh_args: Vec<String> = vec![
        "pr".into(),
        "create".into(),
        "--base".into(),
        base.clone(),
        "--title".into(),
        title.clone(),
    ];
    if let Some(body) = &opts.body {
        gh_args.push("--body".into());
        gh_args.push(body.clone());
    } else {
        gh_args.push("--body".into());
        gh_args.push("PR créée par /zer0push (tests inclus).".into());
    }
    if opts.draft {
        gh_args.push("--draft".into());
    }
    let gh_refs: Vec<&str> = gh_args.iter().map(String::as_str).collect();
    match run_cmd(root, "gh", &gh_refs) {
        Ok(out) => {
            pr_url = out.lines().find(|l| l.contains("http")).map(str::to_string);
            lines.push(format!("✅ PR : {}", pr_url.clone().unwrap_or(out)));
        }
        Err(e) => {
            lines.push(format!("⚠ PR non créée : {e}"));
            lines.push(format!(
                "   relance : gh pr create --base {base} --title \"{title}\""
            ));
        }
    }

    Ok(PushReport { lines, ok: true, pr_url })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn parse_minimal() {
        let o = parse_args(&[]).unwrap();
        assert_eq!(o, PushOptions::default());
    }

    #[test]
    fn parse_title_and_flags() {
        let o = parse_args(&["ajout", "zer0push", "--draft", "--base", "main", "--no-tests"]).unwrap();
        assert_eq!(o.title.as_deref(), Some("ajout zer0push"));
        assert!(o.draft);
        assert!(o.no_tests);
        assert_eq!(o.base.as_deref(), Some("main"));
    }

    #[test]
    fn parse_rejects_unknown_option() {
        assert!(parse_args(&["--force"]).is_err());
        assert!(parse_args(&["--base"]).is_err());
    }

    #[test]
    fn protected_branches() {
        assert!(is_protected("main"));
        assert!(is_protected("master"));
        assert!(!is_protected("fix/quelque-chose"));
        assert!(!is_protected(""));
    }

    #[test]
    fn validate_refuses_main_and_dirty() {
        let o = PushOptions::default();
        assert!(validate("main", 0, &o).is_err());
        assert!(validate("fix/x", 3, &o).is_err());
        assert!(validate("fix/x", 0, &o).is_ok());
        let o = PushOptions { allow_main: true, ..Default::default() };
        assert!(validate("main", 0, &o).is_ok());
        let o = PushOptions { commit: true, ..Default::default() };
        assert!(validate("fix/x", 3, &o).is_ok());
    }

    #[test]
    fn dry_run_on_temp_repo() {
        // Dépôt git temporaire minimal : Cargo.toml + .git, branche feature.
        let dir: PathBuf = std::env::temp_dir().join(format!("zer0push-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname=\"x\"\nversion=\"0.1.0\"\n").unwrap();
        let g = |a: &[&str]| {
            assert!(Command::new("git").args(a).current_dir(&dir).output().unwrap().status.success());
        };
        g(&["init", "-b", "fix/test"]);
        g(&["config", "user.email", "t@t"]);
        g(&["config", "user.name", "t"]);
        std::fs::write(dir.join("a.txt"), "x").unwrap();
        g(&["add", "."]);
        g(&["commit", "-m", "init"]);
        let o = PushOptions { dry_run: true, ..Default::default() };
        let r = run_in(&dir, &o).unwrap();
        assert!(r.ok);
        assert!(r.lines.iter().any(|l| l.contains("dry-run")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
