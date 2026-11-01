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

// @verifies CT-USE-009, CT-TRC-008
#[test]
fn clew_index_runs_while_the_app_holds_and_rebuilds_the_index() {
    use std::sync::mpsc;
    let root = fixture("index-concurrent");
    for n in 0..60 {
        fs::write(
            root.join(format!("model/p{n}.sysml")),
            format!("package P{n} {{\n    part def A{n};\n    part def B{n};\n}}\n"),
        )
        .unwrap();
    }
    // The app keeps its index open for as long as the project is open. It writes the index
    // in bursts, on opening a project or after files change; here each `clew` run starts a
    // burst of three rebuilds, so the app and the CLI write at the same time.
    let mut app = clew_core::index::Index::open(&root).unwrap();
    app.update().unwrap();
    let (go, bursts) = mpsc::channel::<()>();
    let (done, finished) = mpsc::channel::<()>();
    let writer = std::thread::spawn(move || {
        for () in bursts {
            for _ in 0..3 {
                app.rebuild().unwrap();
            }
            done.send(()).unwrap();
        }
        app
    });
    for args in [&["index", "--rebuild"][..], &["index"]].repeat(5) {
        let child = Command::new(env!("CARGO_BIN_EXE_clew"))
            .args(args)
            .current_dir(&root)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        go.send(()).unwrap();
        let out = child.wait_with_output().unwrap();
        finished.recv().unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    drop(go);
    let app = writer.join().unwrap();
    assert_eq!(app.element_count().unwrap(), 3 + 60 * 3);
}
