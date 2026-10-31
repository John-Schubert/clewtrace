//! End-to-end tests for `clew index`.

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

// @verifies CT-MOD-001
fn fixture(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join("clew-cli-tests").join(name);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("model")).unwrap();
    fs::write(
        root.join("model/a.sysml"),
        "package A {\n    part def Engine;\n}\n",
    )
    .unwrap();
    fs::write(root.join("model/b.sysml"), "part def Wheel;\n").unwrap();
    root
}

// @verifies CT-MOD-001, CT-EFF-007, CT-SEC-003, CT-TRC-008
#[test]
fn indexes_then_re_indexes_only_changed_files() {
    let root = fixture("index-incremental");
    let out = clew_in(&root, &["index"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert_eq!(
        stdout,
        "clew index: 2 files, 2 re-indexed, 0 removed, 3 elements, 0 problems\n"
    );
    assert!(root.join(".clewtrace/index.db").is_file());

    fs::write(
        root.join("model/b.sysml"),
        "part def Wheel;\npart def Tire;\n",
    )
    .unwrap();
    let out = clew_in(&root, &["index"]);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "clew index: 2 files, 1 re-indexed, 0 removed, 4 elements, 0 problems\n"
    );

    let out = clew_in(&root, &["index", "--rebuild"]);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "clew index: 2 files, 2 re-indexed, 0 removed, 4 elements, 0 problems\n"
    );
}

// @verifies CT-USE-007, CT-TRC-008
#[test]
fn reports_syntax_errors_and_exits_1() {
    let root = fixture("index-syntax-error");
    fs::write(root.join("model/b.sysml"), "part def 42;\n").unwrap();
    let out = clew_in(&root, &["index"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(stdout.starts_with("model/b.sysml:1:"), "{stdout}");
    assert!(stdout.contains("help:"), "{stdout}");
    assert!(stdout.ends_with(", 1 problems\n"), "{stdout}");
}

// @verifies CT-USE-007, CT-TRC-008
#[test]
fn exits_2_when_the_index_cannot_be_opened() {
    let root = fixture("index-cannot-open");
    fs::write(root.join(".clewtrace"), "a file, not a folder").unwrap();
    let out = clew_in(&root, &["index"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains("cannot create the index folder"),
        "{stderr}"
    );
    assert!(stderr.contains("help:"), "{stderr}");
}
