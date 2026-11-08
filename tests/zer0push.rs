//! Tests d'intégration de `/zer0push`.
//!
//! Aucun réseau, aucun `gh` : on vérifie le parseur, les garde-fous et le
//! dry-run sur un dépôt git temporaire. Le vrai push est laissé à l'utilisateur.

use std::path::{Path, PathBuf};
use std::process::Command;

use zer0::zer0push::{is_protected, parse_args, run_in, validate, PushOptions};

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git doit être installé");
    assert!(
        out.status.success(),
        "git {} : {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Dépôt jetable : Cargo.toml + .git + un commit, sur une branche de feature.
fn temp_repo(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zer0push-it-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    git(&dir, &["init", "-b", "fix/test"]);
    git(&dir, &["config", "user.email", "test@example.com"]);
    git(&dir, &["config", "user.name", "test"]);
    std::fs::write(dir.join("a.txt"), "x").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-m", "init"]);
    dir
}

#[test]
fn parse_accepts_the_documented_syntax() {
    let o = parse_args(&["corrige", "le", "menu", "--base", "main", "--draft"]).unwrap();
    assert_eq!(o.title.as_deref(), Some("corrige le menu"));
    assert_eq!(o.base.as_deref(), Some("main"));
    assert!(o.draft);
    assert_eq!(o.remote, "origin");
}

#[test]
fn parse_refuses_force_and_missing_values() {
    assert!(parse_args(&["--force"]).is_err());
    assert!(parse_args(&["--base"]).is_err());
    assert!(parse_args(&["--remote", ""]).is_err());
}

#[test]
fn main_and_master_are_protected() {
    assert!(is_protected("main"));
    assert!(is_protected("master"));
    assert!(!is_protected("fix/linear-selection-text"));
}

#[test]
fn validate_blocks_main_and_dirty_tree() {
    let plain = PushOptions::default();
    assert!(validate("main", 0, &plain).is_err());
    assert!(validate("fix/x", 2, &plain).is_err());
    assert!(validate("fix/x", 0, &plain).is_ok());

    let allow = PushOptions {
        allow_main: true,
        ..Default::default()
    };
    assert!(validate("main", 0, &allow).is_ok());

    let commit = PushOptions {
        commit: true,
        ..Default::default()
    };
    assert!(validate("fix/x", 2, &commit).is_ok());
}

#[test]
fn dry_run_plans_without_touching_the_remote() {
    let dir = temp_repo("dry");
    let opts = PushOptions {
        dry_run: true,
        ..Default::default()
    };
    let report = run_in(&dir, &opts).expect("le dry-run doit réussir");
    assert!(report.ok);
    assert!(report.pr_url.is_none());
    let text = report.lines.join("\n");
    assert!(text.contains("dry-run"), "sortie : {text}");
    assert!(text.contains("cargo test"), "sortie : {text}");
    assert!(text.contains("gh pr create"), "sortie : {text}");
    // Rien n'a été poussé : aucun remote n'existe dans ce dépôt.
    let remotes = Command::new("git")
        .args(["remote"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&remotes.stdout).trim().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn commit_mode_includes_new_untracked_files() {
    // Régression : `git add -u` seul laissait les nouveaux fichiers hors du
    // commit → la CI compilait un module absent (E0583).
    let dir = temp_repo("commit");
    std::fs::write(dir.join("nouveau.rs"), "// neuf\n").unwrap();
    let opts = PushOptions {
        commit: true,
        no_tests: true,
        ..Default::default()
    };
    // Le push échouera (aucun remote) : on ne teste que l'étape commit.
    let _ = run_in(&dir, &opts);
    let tracked = Command::new("git")
        .args(["ls-files"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let tracked = String::from_utf8_lossy(&tracked.stdout);
    assert!(
        tracked.lines().any(|l| l.trim() == "nouveau.rs"),
        "fichier non suivi absent du commit : {tracked}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn refuses_a_directory_that_is_not_a_checkout() {
    let dir = std::env::temp_dir().join(format!("zer0push-nogit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let err = run_in(&dir, &PushOptions::default()).unwrap_err().to_string();
    assert!(err.contains("checkout"), "erreur : {err}");
    let _ = std::fs::remove_dir_all(&dir);
}
