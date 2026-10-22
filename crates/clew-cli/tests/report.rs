//! End-to-end tests for `clew report coverage` and `clew impact`.

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
        stdout,
        "Requirements without a parent (1):
  CT-TRC-001 Reqs::top  model/reqs.sysml:2
Requirements without an implementation (1):
  CT-TRC-001 Reqs::top  model/reqs.sysml:2
Requirements without a verification (1):
  CT-TRC-001 Reqs::top  model/reqs.sysml:2
Elements traced to no requirement (1):
  Loose  model/design.sysml:4
clew report coverage: 2 requirements, 1 with a parent, 1 implemented, 1 verified, 1 elements \
traced to no requirement
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
        stdout,
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
        stdout.starts_with("Engine  model/design.sysml:1\nUpstream, what it traces to (2):\n"),
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

// @verifies CT-TRC-004, CT-TRC-005, CT-USE-007, CT-TRC-008
#[test]
fn prints_model_problems_first_and_exits_1() {
    let root = fixture("report-problems");
    fs::write(root.join("model/broken.sysml"), "part def 42;\n").unwrap();
    for args in [&["report", "coverage"][..], &["impact", "CT-TRC-001"]] {
        let out = clew_in(&root, args);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(1), "{stdout}");
        assert!(
            stdout.starts_with("model/broken.sysml:1:10: error: "),
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
