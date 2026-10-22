//! End-to-end tests for `clew report coverage`, `clew report verify`, and `clew impact`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

// @verifies CT-TRC-008
/// Runs clew in `dir`. Git does not look above `dir`, so the fixture is in no repository
/// even though it sits under this repository's target folder.
fn clew_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_clew"))
        .args(args)
        .current_dir(dir)
        .env("GIT_CEILING_DIRECTORIES", dir.parent().unwrap())
        .output()
        .expect("clew runs")
}

// @verifies CT-TRC-009
/// The report after its header, checking that the header names `command`, no commit, this
/// tool, and the four fixture files.
fn body<'a>(stdout: &'a str, command: &str) -> &'a str {
    let (header, body) = stdout.split_once("\n\n").expect("a header");
    let lines: Vec<&str> = header.lines().collect();
    assert_eq!(
        lines[..3],
        [
            format!("Report: {command}"),
            "Commit: none".to_string(),
            format!("Tool: clew {}", env!("CARGO_PKG_VERSION")),
        ]
    );
    assert!(lines[3].starts_with("Inputs: sha256 "), "{stdout}");
    assert!(lines[3].ends_with(" over 4 files"), "{stdout}");
    body
}

// @verifies CT-TRC-004, CT-TRC-005
fn fixture(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("model")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("trace.toml"),
        "[project]\nrequirements = [\"model/reqs.sysml\"]\nid_pattern = \"CT-[A-Z]{3}-[0-9]{3}\"\n\
         current_phase = 1\n[coverage]\nsource_roots = [\"src\"]\nlanguages = [\"rust\"]\n\
         file_level_allowed = []\nmax_scope_lines = 200\n",
    )
    .unwrap();
    fs::write(
        root.join("model/reqs.sysml"),
        "package Reqs {\n    requirement <'CT-TRC-001'> top;\n    requirement <'CT-TRC-002'> child;\n    \
         #derivation dependency child to top;\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("model/design.sysml"),
        "part def Engine {\n    satisfy Reqs::child;\n}\npart def Loose;\n",
    )
    .unwrap();
    fs::write(
        root.join("src/lib.rs"),
        "// @verifies CT-TRC-002\n#[test]\nfn t() {}\n",
    )
    .unwrap();
    root
}

// @verifies CT-TRC-004, CT-DEV-003, CT-TRC-008
#[test]
fn reports_coverage_gaps_and_exits_0() {
    let root = fixture("report-coverage");
    let out = clew_in(&root, &["report", "coverage"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert_eq!(
        body(&stdout, "clew report coverage"),
        "Requirements without a parent (1):
  CT-TRC-001 Reqs::top  model/reqs.sysml:2
Requirements without an implementation (1):
  CT-TRC-001 Reqs::top  model/reqs.sysml:2
Requirements without a verification (1):
  CT-TRC-001 Reqs::top  model/reqs.sysml:2
Waived requirements (0):
Elements traced to no requirement (1):
  Loose  model/design.sysml:4
clew report coverage: 2 requirements, 1 with a parent, 1 implemented, 1 verified, 0 waived, \
1 elements traced to no requirement
"
    );
}

// @verifies CT-TRC-005, CT-TRC-008
#[test]
fn lists_the_impact_of_an_element_by_id_or_qualified_name() {
    let root = fixture("report-impact");
    let out = clew_in(&root, &["impact", "CT-TRC-001"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert_eq!(
        body(&stdout, "clew impact CT-TRC-001"),
        "CT-TRC-001 Reqs::top  model/reqs.sysml:2
Upstream, what it traces to (0):
Downstream, what traces to it (3):
  1. CT-TRC-002 Reqs::child derives CT-TRC-001 Reqs::top  at model/reqs.sysml:4
  2. Engine satisfies CT-TRC-002 Reqs::child  at model/design.sysml:2
  2. src/lib.rs:2-3 verifies CT-TRC-002 Reqs::child  at src/lib.rs:2
"
    );
    let out = clew_in(&root, &["impact", "Engine"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        body(&stdout, "clew impact Engine")
            .starts_with("Engine  model/design.sysml:1\nUpstream, what it traces to (2):\n"),
        "{stdout}"
    );
}

// @verifies CT-TRC-005, CT-USE-007, CT-TRC-008
#[test]
fn impact_of_an_unknown_element_exits_1_with_help() {
    let root = fixture("report-impact-unknown");
    let out = clew_in(&root, &["impact", "CT-NOT-000"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.starts_with("clew: no element named CT-NOT-000\n  help: "),
        "{stderr}"
    );
}

// @verifies CT-TRC-004, CT-TRC-005, CT-USE-007, CT-TRC-008, CT-TRC-009
#[test]
fn prints_model_problems_first_and_exits_1() {
    let root = fixture("report-problems");
    fs::write(root.join("model/broken.sysml"), "part def 42;\n").unwrap();
    for args in [&["report", "coverage"][..], &["impact", "CT-TRC-001"]] {
        let out = clew_in(&root, args);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(1), "{stdout}");
        let (_, body) = stdout.split_once("\n\n").unwrap();
        assert!(
            body.starts_with("model/broken.sysml:1:10: error: "),
            "{stdout}"
        );
        assert!(stdout.contains("\n  help: "), "{stdout}");
    }
}

// @verifies CT-USE-007, CT-TRC-008
#[test]
fn exits_2_without_trace_toml() {
    let root = fixture("report-no-config");
    fs::remove_file(root.join("trace.toml")).unwrap();
    for args in [&["report", "coverage"][..], &["impact", "CT-TRC-001"]] {
        let out = clew_in(&root, args);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(2), "{stderr}");
        assert!(
            stderr.starts_with("clew: trace.toml: not found."),
            "{stderr}"
        );
    }
}

// @verifies CT-TRC-009, CT-TRC-008
#[test]
fn verifies_a_saved_report_and_exits_0() {
    let root = fixture("report-verify");
    for args in [&["report", "coverage"][..], &["impact", "Engine"]] {
        fs::write(root.join("saved.txt"), clew_in(&root, args).stdout).unwrap();
        let out = clew_in(&root, &["report", "verify", "saved.txt"]);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(0), "{stdout}");
        assert_eq!(
            stdout,
            "clew report verify: saved.txt matches a fresh run\n"
        );
    }
}

// @verifies CT-TRC-009, CT-USE-007, CT-TRC-008
#[test]
fn a_report_that_differs_exits_1_with_help() {
    let root = fixture("report-verify-differs");
    let saved = String::from_utf8(clew_in(&root, &["report", "coverage"]).stdout).unwrap();
    let saved = saved.replacen("Commit: none\n", "Commit: 0123abc\n", 1);
    fs::write(root.join("saved.txt"), saved).unwrap();
    fs::write(root.join("model/design.sysml"), "part def Engine;\n").unwrap();
    let out = clew_in(&root, &["report", "verify", "saved.txt"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines[0],
        "saved.txt: note: recorded at commit 0123abc, now at none"
    );
    assert!(
        lines[1].starts_with("saved.txt: error: the inputs differ: recorded sha256 "),
        "{stdout}"
    );
    assert!(lines[2].starts_with("  help: "), "{stdout}");
    assert_eq!(
        lines.last(),
        Some(&"clew report verify: saved.txt differs from a fresh run")
    );
}

// @verifies CT-TRC-009, CT-USE-007, CT-TRC-008
#[test]
fn verify_exits_2_when_it_cannot_compare() {
    let root = fixture("report-verify-cannot");
    fs::write(root.join("notes.txt"), "hello\n").unwrap();
    let out = clew_in(&root, &["report", "verify", "notes.txt"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.starts_with("clew: notes.txt: not a clew report: "),
        "{stderr}"
    );
    let out = clew_in(&root, &["report", "verify", "missing.txt"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(stderr.starts_with("clew: missing.txt: "), "{stderr}");
}
