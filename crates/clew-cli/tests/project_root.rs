//! End-to-end tests that `clew` commands find the project root from a subfolder.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

// @verifies CT-TRC-008
fn clew_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_clew"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("clew runs")
}

// @verifies CT-TRC-008
fn repo_subfolder() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

// @verifies CT-TRC-008, CT-DEV-001, CT-TRC-001
#[test]
fn reqs_list_works_from_a_subfolder() {
    let out = clew_in(&repo_subfolder(), &["reqs", "list"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).lines().count(), 79);
}

// @verifies CT-TRC-008, CT-DEV-001, CT-TRC-001
#[test]
fn reqs_show_reports_the_location_from_a_subfolder() {
    let out = clew_in(&repo_subfolder(), &["reqs", "show", "CT-TRC-008"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Location: model/requirements.sysml:"),
        "{stdout}"
    );
}

// @verifies CT-DEV-002, CT-DEV-004, CT-TRC-008
#[test]
fn check_self_works_from_a_subfolder() {
    let out = clew_in(&repo_subfolder(), &["check", "--self"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    assert!(stdout.contains(" 0 errors"), "{stdout}");
}

// @verifies CT-TRC-004, CT-TRC-008
#[test]
fn report_coverage_works_from_a_subfolder() {
    let out = clew_in(&repo_subfolder(), &["report", "coverage"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.starts_with("Report: clew report coverage\n"),
        "{stdout}"
    );
}

// @verifies CT-TRC-005, CT-TRC-008
#[test]
fn impact_works_from_a_subfolder() {
    let out = clew_in(&repo_subfolder(), &["impact", "CT-TRC-008"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("Report: clew impact"), "{stdout}");
}

// @verifies CT-TRC-009, CT-TRC-008
#[test]
fn report_verify_works_from_a_subfolder() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let saved = dir.join("root-report.txt");
    let made = clew_in(&repo_subfolder(), &["impact", "CT-TRC-008"]);
    fs::write(&saved, &made.stdout).unwrap();
    let out = clew_in(
        &repo_subfolder(),
        &["report", "verify", saved.to_str().unwrap()],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    assert!(stdout.contains("matches a fresh run"), "{stdout}");
}

// @verifies CT-DEV-005, CT-TRC-008
#[test]
fn check_commit_works_from_a_subfolder() {
    let message = Path::new(env!("CARGO_TARGET_TMPDIR")).join("root-msg.txt");
    fs::write(&message, "Add a thing\n\nRefs: CT-DEV-005\n").unwrap();
    let out = clew_in(
        &repo_subfolder(),
        &["check-commit", message.to_str().unwrap()],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

// @verifies CT-MOD-001, CT-TRC-008
#[test]
fn index_uses_the_project_root_from_a_subfolder() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("root-index");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join(".clewtrace")).unwrap();
    fs::create_dir_all(root.join("model/deep")).unwrap();
    fs::write(root.join("model/a.sysml"), "part def Engine;\n").unwrap();
    let out = clew_in(&root.join("model/deep"), &["index"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert!(stdout.contains("1 files, 1 re-indexed"), "{stdout}");
    assert!(root.join(".clewtrace/index.db").is_file());
    assert!(!root.join("model/deep/.clewtrace").exists());
}
