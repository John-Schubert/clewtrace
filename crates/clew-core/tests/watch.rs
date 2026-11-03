//! Watching a real folder with the OS's file events (CT-EFF-008, tasks 2.10b and 2.10c):
//! inotify on Linux, FSEvents on macOS, and ReadDirectoryChangesW on Windows. These tests
//! sleep and wait with timeouts, and each runs inside `within`, so that a missed event or a
//! stuck watcher fails the test instead of hanging it. `tests/idle_cpu.rs` allows this file
//! by name. Test only, never shipped.

use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use clew_core::watch::{Batch, QUIET, watch_project};

// @verifies CT-EFF-008
/// Runs `test` on a thread of its own and fails if it has not ended within a minute, so
/// a watcher bug fails the run instead of hanging it. A panic in `test` fails too.
fn within(test: impl FnOnce() + Send + 'static) {
    let (done, ended) = mpsc::channel();
    let thread = std::thread::spawn(move || {
        test();
        let _ = done.send(());
    });
    match ended.recv_timeout(Duration::from_secs(60)) {
        Ok(()) => thread.join().unwrap(),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            std::panic::resume_unwind(thread.join().unwrap_err())
        }
        Err(mpsc::RecvTimeoutError::Timeout) => panic!("the test did not end within 60 s"),
    }
}

// @verifies CT-EFF-008
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("clew-watch-it").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    std::path::absolute(dir).unwrap()
}

// @verifies CT-EFF-008, CT-EFF-007, CT-USE-009
#[test]
fn changes_on_disk_arrive_as_project_relative_paths() {
    within(|| {
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
    });
}

// @verifies CT-EFF-008, CT-EFF-007
#[test]
fn a_new_folder_is_watched_too() {
    within(|| {
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
    });
}

// @verifies CT-EFF-008
#[test]
fn dropping_the_watcher_ends_its_thread() {
    within(|| {
        let root = folder("drop");
        let (out, got) = mpsc::channel::<Batch>();
        let watcher = watch_project(&root, move |c| drop(out.send(c))).unwrap();
        drop(watcher);
        // The callback, and with it the sender, is dropped when the thread ends.
        assert!(matches!(
            got.recv_timeout(Duration::from_secs(5)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
    });
}

// @verifies CT-EFF-008, CT-EFF-007
#[test]
fn a_folder_renamed_away_asks_for_everything() {
    within(|| {
        let root = folder("renamed");
        fs::create_dir_all(root.join("old")).unwrap();
        fs::write(root.join("old/a.sysml"), "package A;\n").unwrap();
        let (out, got) = mpsc::channel();
        let _watcher = watch_project(&root, move |c| out.send(c).unwrap())
            .unwrap()
            .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        fs::rename(root.join("old"), root.join("new")).unwrap();
        let mut everything = false;
        while let Ok(batch) = got.recv_timeout(Duration::from_secs(5)) {
            everything |= batch.everything;
            if everything {
                break;
            }
        }
        assert!(everything);
    });
}

// @verifies CT-EFF-008, CT-EFF-007, CT-USE-009
#[test]
fn a_watch_started_by_a_thread_that_ends_keeps_watching() {
    within(|| {
        // The app starts the watch on its load thread, which ends once the project is
        // loaded. Windows cancels overlapped reads when the thread that started them
        // ends, so the watcher must start its reads on its own thread.
        let root = folder("thread-ends");
        let (out, got) = mpsc::channel();
        let starter = root.clone();
        let watcher = std::thread::spawn(move || {
            watch_project(&starter, move |c| out.send(c).unwrap())
                .unwrap()
                .unwrap()
        })
        .join()
        .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        fs::write(root.join("a.sysml"), "package A;\n").unwrap();
        let mut seen = Vec::new();
        while let Ok(batch) = got.recv_timeout(Duration::from_secs(5)) {
            assert!(batch.trouble.is_empty(), "{:?}", batch.trouble);
            seen.extend(batch.paths);
            if seen.iter().any(|p| p == "a.sysml") {
                break;
            }
        }
        assert!(seen.iter().any(|p| p == "a.sysml"), "{seen:?}");
        drop(watcher);
    });
}

// @verifies CT-EFF-008
#[test]
fn dropping_the_watcher_on_another_thread_ends_its_threads() {
    within(|| {
        let root = folder("drop-elsewhere");
        let (out, got) = mpsc::channel::<Batch>();
        let watcher = watch_project(&root, move |c| drop(out.send(c))).unwrap();
        std::thread::spawn(move || drop(watcher)).join().unwrap();
        assert!(matches!(
            got.recv_timeout(Duration::from_secs(5)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
    });
}
