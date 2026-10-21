//! End-to-end tests for `clew check-commit` and the `commit-msg` hook installers.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

// @verifies CT-DEV-005
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// @verifies CT-DEV-005
fn check_commit(message: &str, name: &str) -> Output {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("check-commit");
    fs::create_dir_all(&dir).unwrap();
    let file = dir.join(name);
    fs::write(&file, message).unwrap();
    Command::new(env!("CARGO_BIN_EXE_clew"))
        .args(["check-commit", file.to_str().unwrap()])
        .current_dir(repo_root())
        .output()
        .expect("clew runs")
}

// @verifies CT-DEV-005, CT-USE-007
#[test]
fn accepts_a_message_with_a_real_trailer() {
    let out = check_commit("Add a thing\n\nRefs: CT-DEV-005\n", "good.txt");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert!(
        stdout.contains("clew check-commit: 1 file checked, 0 errors"),
        "{stdout}"
    );
}

// @verifies CT-DEV-005, CT-USE-007
#[test]
fn rejects_a_missing_or_unknown_trailer() {
    let out = check_commit("Add a thing\n\nNo trailer.\n", "missing.txt");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains("missing.txt: error: commit message has no `Refs:` trailer"));
    assert!(stdout.contains("help: "), "{stdout}");

    let out = check_commit("Add a thing\n\nRefs: CT-DEV-999\n", "unknown.txt");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains("unknown.txt:3: error: unknown requirement ID CT-DEV-999"));
}

// @verifies CT-DEV-005, CT-USE-007
#[test]
fn unreadable_message_exits_two() {
    let out = Command::new(env!("CARGO_BIN_EXE_clew"))
        .args(["check-commit", "no/such/COMMIT_EDITMSG"])
        .current_dir(repo_root())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no/such/COMMIT_EDITMSG"));
}

/// Installs the hook into a scratch repository and checks that it blocks a commit
/// without a trailer and allows one with a trailer.
// @verifies CT-DEV-005
#[cfg(unix)]
#[test]
fn shell_installer_adds_a_working_commit_msg_hook() {
    let repo = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hook-repo");
    let _ = fs::remove_dir_all(&repo);
    fs::create_dir_all(repo.join("model")).unwrap();
    let root = repo_root();
    fs::copy(root.join("trace.toml"), repo.join("trace.toml")).unwrap();
    fs::copy(
        root.join("model/requirements.sysml"),
        repo.join("model/requirements.sysml"),
    )
    .unwrap();
    let git = |args: &[&str]| {
        Command::new("git")
            .args(["-c", "user.name=T", "-c", "user.email=t@example.com"])
            .args(args)
            .current_dir(&repo)
            .env("CLEW", env!("CARGO_BIN_EXE_clew"))
            .output()
            .unwrap()
    };
    assert!(git(&["init", "-q"]).status.success());
    let out = Command::new("sh")
        .arg(root.join("scripts/install-hooks.sh"))
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(repo.join(".git/hooks/commit-msg").is_file());

    git(&["add", "."]);
    let bad = git(&["commit", "-q", "-m", "No trailer"]);
    assert!(!bad.status.success());
    // Git sends hook output to stderr.
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&bad.stdout),
        String::from_utf8_lossy(&bad.stderr)
    );
    assert!(said.contains("no `Refs:` trailer"), "{said}");
    let good = git(&["commit", "-q", "-m", "Add files\n\nRefs: CT-DEV-005"]);
    assert!(
        good.status.success(),
        "{}{}",
        String::from_utf8_lossy(&good.stdout),
        String::from_utf8_lossy(&good.stderr)
    );
}
