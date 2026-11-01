//! Watching a real folder with the OS's file events (CT-EFF-008, task 2.10b). These tests
//! sleep and wait with timeouts so that a missed event fails the test instead of hanging
//! it, which `tests/idle_cpu.rs` allows for this file by name. Test only, never shipped.

// @begin CT-EFF-008
// Windows is not watched until task 2.10c.
#![cfg(not(windows))]
// @end

use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use clew_core::watch::{Batch, QUIET, watch_project};

// @verifies CT-EFF-008
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("clew-watch-it").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    std::path::absolute(dir).unwrap()
}

// @verifies CT-EFF-008, CT-EFF-007, CT-USE-009
#[cfg(not(windows))]
#[test]
fn changes_on_disk_arrive_as_project_relative_paths() {
    let root = folder("live");
    fs::create_dir_all(root.join("model")).unwrap();
    fs::write(root.join("model/a.sysml"), "package A;\n").unwrap();
    let (out, got) = mpsc::channel();
    let watcher = watch_project(&root, move |c| out.send(c).unwrap())
        .unwrap()
        .expect("watching works on this OS");
    std::thread::sleep(Duration::from_millis(100));
    fs::write(root.join("model/a.sysml"), "package B;\n").unwrap();
    fs::write(root.join("model/.a.sysml.1.clewtmp"), "temp").unwrap();
    let mut seen = Vec::new();
    while let Ok(change) = got.recv_timeout(Duration::from_secs(5)) {
        seen.extend(change.paths);
        if seen.iter().any(|p| p == "model/a.sysml") {
            break;
        }
    }
    assert!(seen.iter().any(|p| p == "model/a.sysml"), "{seen:?}");
    assert!(seen.iter().all(|p| !p.ends_with(".clewtmp")), "{seen:?}");
    // Reading a file is not a change.
    let _ = fs::read_to_string(root.join("model/a.sysml")).unwrap();
    assert!(got.recv_timeout(QUIET * 3).is_err());
    drop(watcher);
}

// @verifies CT-EFF-008, CT-EFF-007
#[cfg(not(windows))]
#[test]
fn a_new_folder_is_watched_too() {
    let root = folder("new-folder");
    let (out, got) = mpsc::channel();
    let _watcher = watch_project(&root, move |c| out.send(c).unwrap())
        .unwrap()
        .unwrap();
    std::thread::sleep(Duration::from_millis(100));
    fs::create_dir_all(root.join("more")).unwrap();
    let first = got.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(first.everything, "{first:?}");
    std::thread::sleep(Duration::from_millis(100));
    fs::write(root.join("more/b.sysml"), "package B;\n").unwrap();
    let mut seen = Vec::new();
    while let Ok(batch) = got.recv_timeout(Duration::from_secs(5)) {
        seen.extend(batch.paths);
        if seen.iter().any(|p| p == "more/b.sysml") {
            break;
        }
    }
    assert!(seen.iter().any(|p| p == "more/b.sysml"), "{seen:?}");
}

// @verifies CT-EFF-008
#[cfg(not(windows))]
#[test]
fn dropping_the_watcher_ends_its_thread() {
    let root = folder("drop");
    let (out, got) = mpsc::channel::<Batch>();
    let watcher = watch_project(&root, move |c| drop(out.send(c))).unwrap();
    drop(watcher);
    // The callback, and with it the sender, is dropped when the thread ends.
    assert!(matches!(
        got.recv_timeout(Duration::from_secs(5)),
        Err(mpsc::RecvTimeoutError::Disconnected)
    ));
}
