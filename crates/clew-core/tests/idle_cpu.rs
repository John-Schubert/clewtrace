//! CT-EFF-008: the app uses no CPU while idle beyond responding to OS events. The analysis
//! is in docs/verification/idle-cpu.md; this test is the verification. It fails when
//! first-party code polls: a repeating timer, an animation-frame loop, a sleeping thread,
//! or a wait with a timeout. A use that is not polling goes in ALLOWED with its reason.
//! Later UI tasks add their probes here.

use std::fs;
use std::path::{Path, PathBuf};

/// What polling looks like in first-party code.
// @verifies CT-EFF-008
const POLLING: [&str; 4] = [
    "setInterval",
    "requestAnimationFrame",
    "thread::sleep",
    "recv_timeout",
];

/// Files that may name a polling call, each with its reason. A new entry needs John's
/// approval, and the reason must show why the code is not a polling loop.
// @verifies CT-EFF-008
const ALLOWED: [(&str, &str); 4] = [
    (
        "crates/clew-core/tests/idle_cpu.rs",
        "this guard names the calls it looks for, so it would fail its own scan",
    ),
    (
        "app/src-tauri/src/probe.rs",
        "built only with the perf-probe feature, which release builds never use. It sleeps \
         once to let the idle window settle before reading memory, once for a watchdog, and \
         its injected script waits for two painted frames once per scripted action. Nothing \
         repeats, and the UI bundle holds none of it",
    ),
    (
        "crates/clew-core/src/watch/burst.rs",
        "the file watcher's burst timer (task 2.10b). Its thread blocks in `recv`, with no \
         timeout, until the OS reports a file event; `recv_timeout` then runs only until the \
         burst has been still for 200 ms or has lasted 2 s, and the thread goes back to \
         `recv`. Nothing runs while no files change. Its tests sleep to space out events",
    ),
    (
        "crates/clew-core/tests/watch.rs",
        "test only, never shipped: watches a real folder and waits with timeouts, so a \
         missed file event fails the test instead of hanging it",
    ),
];

/// Folders holding first-party code, relative to the repository root.
// @verifies CT-EFF-008
const ROOTS: [&str; 6] = [
    "crates/clew-core/src",
    "crates/clew-core/tests",
    "crates/clew-cli/src",
    "app/src",
    "app/src-tauri/src",
    "app/src-tauri/tests",
];

// @verifies CT-EFF-008
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// @verifies CT-EFF-008
fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, found);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("rs" | "ts" | "svelte")
        ) {
            found.push(path);
        }
    }
}

/// Every first-party source file, as a path relative to the repository root.
// @verifies CT-EFF-008
fn first_party() -> Vec<(String, String)> {
    let root = root();
    let mut paths = Vec::new();
    for dir in ROOTS {
        sources(&root.join(dir), &mut paths);
    }
    paths.sort();
    let mut files: Vec<(String, String)> = paths
        .iter()
        .map(|p| {
            let rel = p.strip_prefix(&root).unwrap_or(p);
            let text = fs::read_to_string(p).expect("a source file is readable");
            (rel.to_string_lossy().replace('\\', "/"), text)
        })
        .collect();
    for build in ["app/src-tauri/build.rs", "crates/clew-cli/build.rs"] {
        if let Ok(text) = fs::read_to_string(root.join(build)) {
            files.push((build.into(), text));
        }
    }
    files
}

/// Each polling call in `files` outside the allowlist, as `file: call`.
// @verifies CT-EFF-008
fn offenders(files: &[(String, String)]) -> Vec<String> {
    files
        .iter()
        .filter(|(rel, _)| !ALLOWED.iter().any(|(allowed, _)| allowed == rel))
        .flat_map(|(rel, text)| {
            POLLING
                .iter()
                .filter(|call| text.contains(*call))
                .map(move |call| format!("{rel}: `{call}`"))
        })
        .collect()
}

// @verifies CT-EFF-008
#[test]
fn first_party_code_does_not_poll() {
    let files = first_party();
    assert!(files.len() > 20, "found only {} source files", files.len());
    let found = offenders(&files);
    assert!(
        found.is_empty(),
        "polling calls outside the allowlist. Use an OS event or a condition variable, or add \
         the file to ALLOWED with its reason:\n{}",
        found.join("\n")
    );
}

// @verifies CT-EFF-008
#[test]
fn the_check_finds_each_kind_of_polling() {
    let file = |text: &str| vec![("src/x.rs".to_string(), text.to_string())];
    for call in POLLING {
        assert_eq!(
            offenders(&file(&format!("fn f() {{ {call}(); }}"))).len(),
            1,
            "{call}"
        );
    }
    assert!(offenders(&file("fn f() { condvar.wait(guard); }")).is_empty());
}

// @verifies CT-EFF-008
#[test]
fn every_allowlist_entry_is_current_and_explained() {
    let files = first_party();
    for (path, reason) in ALLOWED {
        let (_, text) = files
            .iter()
            .find(|(rel, _)| rel == path)
            .unwrap_or_else(|| panic!("{path} is allowlisted but does not exist"));
        assert!(
            POLLING.iter().any(|call| text.contains(call)),
            "{path} no longer names a polling call. Remove it from ALLOWED"
        );
        assert!(reason.len() > 40, "{path} needs a real reason");
    }
}

// @verifies CT-EFF-008
#[test]
fn the_ui_bundle_holds_no_probe_code() {
    let files = first_party();
    let bundle: Vec<_> = files
        .iter()
        .filter(|(rel, _)| rel.starts_with("app/src/"))
        .collect();
    assert!(!bundle.is_empty());
    for (rel, text) in bundle {
        for word in ["clew-probe", "CLEW_PROBE", "perf-probe"] {
            assert!(!text.contains(word), "{rel} holds probe code (`{word}`)");
        }
    }
}
