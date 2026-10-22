//! End-to-end tests for `clew check --self`.

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

/// Writes a small project with one untraced line, one unknown ID, one oversized scope, and
/// one due requirement with no verification.
// @verifies CT-DEV-002, CT-DEV-004
fn fixture(name: &str, max_scope_lines: usize) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("model")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("model/requirements.sysml"),
        "package P {\n    requirement <'CT-DEV-002'> everyLineTraced : R {\n    }\n    requirement def R;\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("trace.toml"),
        format!(
            "[project]\nrequirements = [\"model/requirements.sysml\"]\n\
             id_pattern = \"CT-(DEV|TRC)-[0-9]{{3}}\"\ncurrent_phase = 0\n\n\
             [coverage]\nsource_roots = [\"src\"]\nlanguages = [\"rust\"]\n\
             file_level_allowed = []\nmax_scope_lines = {max_scope_lines}\n"
        ),
    )
    .unwrap();
    fs::write(
        root.join("src/lib.rs"),
        "// @implements CT-DEV-002\nfn a() {\n    1;\n}\n\nfn b() {}\n\n// @implements CT-TRC-999\nfn c() {}\n",
    )
    .unwrap();
    root
}

// @verifies CT-DEV-002, CT-DEV-003, CT-DEV-004, CT-TRC-008, CT-USE-007
#[test]
fn reports_findings_with_file_and_line_and_exits_nonzero() {
    let root = fixture("check-findings", 2);
    let out = clew_in(&root, &["check", "--self"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(
        stdout.contains("src/lib.rs:6: error: line 6 is not inside a tagged scope"),
        "{stdout}"
    );
    assert!(
        stdout.contains("src/lib.rs:8: error: unknown requirement ID CT-TRC-999"),
        "{stdout}"
    );
    assert!(
        stdout.contains("src/lib.rs:2: warning: scope is 3 lines long"),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "model/requirements.sysml:2: error: CT-DEV-002 (no phase) is due at current_phase 0 \
             but has no verification"
        ),
        "{stdout}"
    );
    assert!(stdout.contains("help: "), "{stdout}");
    assert!(stdout.contains("3 errors, 1 warning"), "{stdout}");
}

// @verifies CT-DEV-004, CT-TRC-008, CT-USE-007
#[test]
fn missing_config_exits_two() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("check-no-config");
    fs::create_dir_all(&root).unwrap();
    let out = clew_in(&root, &["check", "--self"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("trace.toml"));
}

// @verifies CT-DEV-002, CT-DEV-004, CT-TRC-008
#[test]
fn own_repository_passes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = clew_in(&root, &["check", "--self"]);
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains(" 0 errors"));
}
