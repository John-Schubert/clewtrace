//! Fixed-seed stress test for files from other people (CT-SEC-009, task 2.12): deep nesting,
//! random bytes, and truncated files go through the parser and the project loader, which
//! must report them and carry on. No dependency: the generator is a small xorshift. Full
//! fuzzing comes with the importers in task 3.8. Test only, never shipped.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use clew_core::index::MAX_FILE_BYTES;
use clew_core::parse::parser;
use clew_core::report::load_project;

// @verifies CT-SEC-009
/// A xorshift64 generator. The same seed gives the same bytes on every machine.
struct Rng(u64);

// @verifies CT-SEC-009
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }
}

// @verifies CT-SEC-009
const VALID: &str = "package Reqs {\n    requirement def <'R'> Req;\n    \
    requirement <'CT-EFF-007'> incrementalIndex : Req {\n        \
    doc /* Re-index only the files that changed. */\n    }\n    part def Vehicle {\n        \
    part engine : Engine;\n        attribute mass = 12.5;\n    }\n    part def Engine;\n    \
    satisfy incrementalIndex by Vehicle;\n    #derivation dependency incrementalIndex to Req;\n}\n";

// @verifies CT-SEC-009
/// Runs `work` on a thread with a small stack and fails if it panics or runs over `limit`.
fn guarded(limit: Duration, work: impl FnOnce() + Send + 'static) {
    let started = Instant::now();
    std::thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(work)
        .unwrap()
        .join()
        .expect("the work panicked or overflowed the stack");
    assert!(started.elapsed() < limit, "took {:?}", started.elapsed());
}

// @verifies CT-SEC-009
/// A project folder with a valid model and a `trace.toml`, plus the given files.
fn project(name: &str, files: &[(&str, Vec<u8>)]) -> PathBuf {
    let root = std::env::temp_dir().join("clew-stress-tests").join(name);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("model")).unwrap();
    let config = "[project]\nrequirements = [\"model/reqs.sysml\"]\n\
                  id_pattern = \"CT-[A-Z]{3}-[0-9]{3}\"\ncurrent_phase = 1\n\
                  [coverage]\nsource_roots = [\"src\"]\nlanguages = [\"rust\"]\n\
                  file_level_allowed = []\nmax_scope_lines = 200\n";
    fs::write(root.join("trace.toml"), config).unwrap();
    fs::write(root.join("model/reqs.sysml"), VALID).unwrap();
    for (path, bytes) in files {
        fs::write(root.join(path), bytes).unwrap();
    }
    root
}

// @verifies CT-SEC-009
#[test]
fn deep_nesting_of_every_kind_is_an_error_not_a_crash() {
    guarded(Duration::from_secs(20), || {
        for open in ["package p {", "part def d {", "requirement r {", "part x {"] {
            let src = format!("{}\n", open.repeat(200_000));
            let parsed = parser::parse(&src);
            assert!(
                parsed
                    .errors
                    .iter()
                    .any(|e| e.message.contains("levels deep")),
                "{open}"
            );
        }
    });
}

// @verifies CT-SEC-009
#[test]
fn random_bytes_never_crash_or_hang_the_parser() {
    guarded(Duration::from_secs(60), || {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        for round in 0..300 {
            let len = 1 + rng.below(4_000);
            let text = String::from_utf8_lossy(&rng.bytes(len)).into_owned();
            let _ = parser::parse(&text);
            // Valid text with a few bytes swapped for random ones.
            let mut bytes = VALID.as_bytes().to_vec();
            for _ in 0..1 + round % 8 {
                let at = rng.below(bytes.len());
                bytes[at] = rng.next() as u8;
            }
            let _ = parser::parse(&String::from_utf8_lossy(&bytes));
        }
    });
}

// @verifies CT-SEC-009
#[test]
fn a_file_cut_short_anywhere_is_parsed_without_a_crash() {
    guarded(Duration::from_secs(60), || {
        for end in (0..=VALID.len()).filter(|&n| VALID.is_char_boundary(n)) {
            let _ = parser::parse(&VALID[..end]);
        }
    });
}

// @verifies CT-SEC-009, CT-USE-007
#[test]
fn a_project_with_hostile_files_loads_and_reports_each_one() {
    let mut rng = Rng(42);
    let mut nested = "package p {\n".repeat(100_000).into_bytes();
    nested.extend(b"}\n");
    let long_line = format!("package Q; // {}\n", "x".repeat(2_000_000));
    let root = project(
        "hostile",
        &[
            ("model/random.sysml", rng.bytes(5_000)),
            ("model/deep.sysml", nested),
            ("model/long.sysml", long_line.into_bytes()),
            (
                "model/cut.sysml",
                VALID.as_bytes()[..VALID.len() / 2].to_vec(),
            ),
        ],
    );
    let big = fs::File::create(root.join("model/big.sysml")).unwrap();
    big.set_len(MAX_FILE_BYTES + 1).unwrap();
    drop(big);
    let loaded = std::thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(move || load_project(&root).map(|p| p.problems))
        .unwrap()
        .join()
        .expect("the loader panicked")
        .expect("hostile files must not stop the project from loading");
    let named = |path: &str| loaded.iter().any(|p| p.starts_with(path));
    for path in [
        "model/big.sysml",
        "model/deep.sysml",
        "model/long.sysml",
        "model/cut.sysml",
    ] {
        assert!(named(path), "no problem for {path}: {loaded:?}");
    }
    assert!(
        loaded.iter().all(|p| p.contains("\n  help: ")),
        "{loaded:?}"
    );
}
