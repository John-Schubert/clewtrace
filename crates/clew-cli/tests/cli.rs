//! End-to-end tests for the `clew` binary, run from the repository root.

use std::path::Path;
use std::process::{Command, Output};

// @verifies CT-TRC-008
fn clew(args: &[&str]) -> Output {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    Command::new(env!("CARGO_BIN_EXE_clew"))
        .args(args)
        .current_dir(root)
        .output()
        .expect("clew runs")
}

// @verifies CT-TRC-008, CT-DEV-001, CT-TRC-001
#[test]
fn lists_every_requirement() {
    let out = clew(&["reqs", "list"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.lines().count(), 79);
    let line = stdout
        .lines()
        .find(|l| l.starts_with("CT-DEV-001"))
        .unwrap();
    assert!(line.contains("reqsInRepo") && line.contains("Inspection"));
}

// @verifies CT-TRC-008, CT-DEV-001, CT-TRC-001
#[test]
fn shows_one_requirement() {
    let out = clew(&["reqs", "show", "CT-TRC-008"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("CT-TRC-008 headlessCli"));
    assert!(stdout.contains("Verification: Test"));
    assert!(stdout.contains("Phase: 1"));
    assert!(stdout.contains("A command-line interface shall run every trace check"));
}

// @verifies CT-TRC-008
#[test]
fn unknown_id_exits_nonzero() {
    let out = clew(&["reqs", "show", "CT-XXX-999"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("CT-XXX-999"));
}
