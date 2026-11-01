//! Kills a process in the middle of saving a file and checks the file holds the old or the
//! new content in full, never a mix (CT-USE-008).
//!
//! The test binary runs itself as the writer: `writer_child` saves in a loop when
//! `CLEW_KILL_SAVE_TARGET` is set and does nothing otherwise. It prints a line after each
//! save, and the parent reads those lines to choose when to kill it, so nothing is timed.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use clew_core::io::write_atomic;

// @verifies CT-USE-008
/// Large enough that a save takes long enough to be killed partway through it.
const SIZE: usize = 4 << 20;

// @verifies CT-USE-008
fn content(fill: u8) -> Vec<u8> {
    vec![fill; SIZE]
}

// @verifies CT-USE-008
#[test]
fn writer_child() {
    let Some(target) = std::env::var_os("CLEW_KILL_SAVE_TARGET") else {
        return;
    };
    let (old, new) = (content(b'o'), content(b'n'));
    let mut out = std::io::stdout();
    for round in 0_u64.. {
        let bytes = if round % 2 == 0 { &new } else { &old };
        write_atomic(Path::new(&target), bytes).unwrap();
        writeln!(out, "saved {round}").unwrap();
        out.flush().unwrap();
    }
}

// @verifies CT-USE-008
fn spawn_writer(target: &Path) -> Child {
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "writer_child", "--nocapture", "--test-threads=1"])
        .env("CLEW_KILL_SAVE_TARGET", target)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

// @verifies CT-USE-008
fn temp_files(dir: &PathBuf) -> usize {
    (fs::read_dir(dir).unwrap())
        .filter(|e| {
            let name = e.as_ref().unwrap().file_name();
            name.to_string_lossy().ends_with(".clewtmp")
        })
        .count()
}

// @verifies CT-USE-008
#[test]
fn a_writer_killed_mid_save_leaves_the_old_or_the_new_content() {
    let dir = std::env::temp_dir().join("clewtrace-kill-save");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let target = dir.join("model.sysml");
    let (old, new) = (content(b'o'), content(b'n'));
    fs::write(&target, &old).unwrap();
    let mut seen_new = false;
    for round in 0..12_usize {
        let mut child = spawn_writer(&target);
        // Let the child finish a few saves, a different number each round, then kill it.
        // It is saving again by then, so the kill lands in the write, the flush, or the
        // rename. The test harness prints lines of its own, so only save lines count.
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let mut saves = 0;
        while saves < round % 4 + 1 {
            let line = lines.next().expect("the writer is running").unwrap();
            saves += usize::from(line.starts_with("saved "));
        }
        child.kill().unwrap();
        child.wait().unwrap();
        let now = fs::read(&target).unwrap();
        assert!(
            now == old || now == new,
            "round {round}: the file is torn ({} bytes)",
            now.len()
        );
        seen_new |= now == new;
    }
    // A killed save leaves its temp file behind, which shows a kill landed inside a save.
    assert!(temp_files(&dir) > 0, "no kill landed inside a save");
    assert!(
        seen_new,
        "no save finished before a kill, so the test proved nothing"
    );
}
