//! Performance harness: a generator for large test projects, and the CT-EFF budgets
//! measured on one of 10,000 requirements and 50,000 trace links.
//!
//! `cargo test` runs the generator at a small size. The full-size run is ignored by default;
//! the `perf` CI job runs it in release mode, limited to 4 GB of RAM and 2 CPUs:
//! `cargo test --release -p clew-core --test perf -- --ignored --nocapture`. The job also
//! runs a canary that writes 5 GiB under the same limits and must be killed.
//! A budget fails the run only when its requirement's phase has started (`current_phase`
//! in `trace.toml`); later budgets are measured and printed as not due yet.

mod generate;

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clew_core::config::TraceConfig;
use clew_core::index::Index;
use clew_core::report::{load_project, project_coverage};
use clew_core::reqs::read_requirements;
use generate::generate;

// @verifies CT-EFF-009, CT-EFF-004
/// A fresh folder for a generated project.
fn project_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("perf")
        .join(name)
}

// @verifies CT-EFF-009, CT-EFF-004, CT-EFF-007
/// What loading and indexing a project measured.
struct Measured {
    requirements: usize,
    links: usize,
    files: usize,
    index: Duration,
    load: Duration,
    search: Duration,
    reparsed: usize,
    incremental: Duration,
    /// Peak resident memory in bytes, where the OS reports it.
    peak: Option<u64>,
}

// @verifies CT-EFF-009, CT-EFF-004, CT-EFF-007
/// Indexes the project under `root`, loads its model, trace graph, and coverage, searches
/// it, then changes one file and indexes again. Checks that the project is complete: no
/// problems, and every requirement has a parent, an implementation, and a verification.
fn measure(root: &Path) -> Measured {
    reset_peak();
    let started = Instant::now();
    let mut index = Index::open(root).unwrap();
    let stats = index.update().unwrap();
    let index_time = started.elapsed();
    assert!(stats.problems.is_empty(), "{:?}", stats.problems);

    let started = Instant::now();
    let project = load_project(root).unwrap();
    let found = project_coverage(&project);
    let load = started.elapsed();
    assert!(
        project.problems.is_empty(),
        "{:?}",
        project.problems.first()
    );
    assert!(found.untraced.is_empty());
    let gaps = (found.requirements.iter())
        .filter(|r| !(r.has_parent && r.implemented && r.verified))
        .count();
    assert_eq!(gaps, 0, "requirements with a coverage gap");

    let started = Instant::now();
    let hits = index.search("budget", 50, 0).unwrap();
    let search = started.elapsed();
    assert_eq!(hits.len(), 50.min(found.requirements.len()));
    let peak = peak_memory();

    let changed = root.join("model/reqs/r000.sysml");
    let text = fs::read_to_string(&changed).unwrap();
    fs::write(&changed, text.replace("case 1 ", "case one ")).unwrap();
    let started = Instant::now();
    let again = index.update().unwrap();
    let incremental = started.elapsed();
    Measured {
        requirements: found.requirements.len(),
        links: project.graph.links.len(),
        files: stats.files,
        index: index_time,
        load,
        search,
        reparsed: again.parsed,
        incremental,
        peak,
    }
}

// @verifies CT-EFF-009, CT-EFF-004
/// Resets the peak resident memory the OS reports, so it covers only what follows.
fn reset_peak() {
    if cfg!(target_os = "linux") {
        // Linux 4.0 and later reset VmHWM when 5 is written here. Older kernels keep it.
        let _ = fs::write("/proc/self/clear_refs", "5");
    }
}

// @verifies CT-EFF-009, CT-EFF-004
/// The peak resident memory of this process in bytes, from `VmHWM` on Linux.
fn peak_memory() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}

// @verifies CT-EFF-009
/// One budget check: the requirement, what was measured, and whether it passed.
struct Budget {
    id: &'static str,
    what: &'static str,
    measured: String,
    limit: &'static str,
    passed: bool,
}

// @verifies CT-EFF-009, CT-EFF-004, CT-EFF-006, CT-EFF-007
/// The budgets the measurements are checked against. CT-EFF-004 is measured on the core
/// library: index, model, trace graph, and coverage, without the desktop app.
fn budgets(m: &Measured) -> Vec<Budget> {
    let ms = |d: Duration| format!("{} ms", d.as_millis());
    let mut list = vec![
        Budget {
            id: "CT-EFF-004",
            what: "load model, trace graph, and coverage",
            measured: ms(m.load),
            limit: "3000 ms",
            passed: m.load <= Duration::from_secs(3),
        },
        Budget {
            id: "CT-EFF-006",
            what: "full-text search, first page",
            measured: ms(m.search),
            limit: "200 ms",
            passed: m.search <= Duration::from_millis(200),
        },
        Budget {
            id: "CT-EFF-007",
            what: "files re-parsed after one file changed",
            measured: m.reparsed.to_string(),
            limit: "1",
            passed: m.reparsed == 1,
        },
    ];
    if let Some(peak) = m.peak {
        list.push(Budget {
            id: "CT-EFF-004",
            what: "peak resident memory",
            measured: format!("{} MB", peak / 1_000_000),
            limit: "400 MB",
            passed: peak <= 400_000_000,
        });
    }
    list
}

// @verifies CT-EFF-009
/// The budgets as a table, with each one's phase and whether it is due. Returns the table
/// and whether every due budget passed.
fn judge(budgets: &[Budget], phase_of: impl Fn(&str) -> u8, current: i64) -> (String, bool) {
    let mut table = String::new();
    let mut ok = true;
    for b in budgets {
        let phase = phase_of(b.id);
        let due = i64::from(phase) <= current;
        let verdict = match (due, b.passed) {
            (true, true) => "pass".to_string(),
            (true, false) => "FAIL".to_string(),
            (false, true) => format!("pass, not due until phase {phase}"),
            (false, false) => format!("over, not due until phase {phase}"),
        };
        ok &= b.passed || !due;
        writeln!(
            table,
            "  {} {}: {} (budget {}) {verdict}",
            b.id, b.what, b.measured, b.limit
        )
        .unwrap();
    }
    (table, ok)
}

// @verifies CT-EFF-009, CT-EFF-004
#[test]
fn generates_the_requested_requirements_and_links() {
    let root = project_dir("small");
    generate(&root, 205, 1_010);
    let m = measure(&root);
    assert_eq!((m.requirements, m.links), (205, 1_010));
    // Three files of each kind, plus perf.sysml.
    assert_eq!(m.files, 7);
    assert_eq!(m.reparsed, 1);
}

// @verifies CT-EFF-009, CT-EFF-004
#[test]
#[should_panic(expected = "10 requirements take 49 to 58 links")]
fn generator_rejects_link_counts_it_cannot_make() {
    generate(&project_dir("impossible"), 10, 100);
}

// @verifies CT-EFF-009
#[test]
fn only_due_budgets_fail_the_run() {
    let budget = |id, passed| Budget {
        id,
        what: "x",
        measured: "2".into(),
        limit: "1",
        passed,
    };
    let list = [
        budget("CT-A", true),
        budget("CT-B", false),
        budget("CT-C", true),
    ];
    let phase_of = |id: &str| if id == "CT-A" { 1 } else { 2 };
    let (table, ok) = judge(&list, phase_of, 1);
    assert!(ok);
    assert_eq!(
        table,
        "  CT-A x: 2 (budget 1) pass\n  CT-B x: 2 (budget 1) over, not due until phase 2\n  \
         CT-C x: 2 (budget 1) pass, not due until phase 2\n"
    );
    let (table, ok) = judge(&list, phase_of, 2);
    assert!(!ok);
    assert!(table.contains("CT-B x: 2 (budget 1) FAIL\n"), "{table}");
}

// @verifies CT-EFF-009, CT-EFF-004
#[test]
fn budgets_cover_memory_only_where_it_is_measured() {
    let m = Measured {
        requirements: 0,
        links: 0,
        files: 0,
        index: Duration::ZERO,
        load: Duration::from_secs(4),
        search: Duration::ZERO,
        reparsed: 2,
        incremental: Duration::ZERO,
        peak: None,
    };
    let passed: Vec<bool> = budgets(&m).iter().map(|b| b.passed).collect();
    assert_eq!(passed, [false, true, false]);
    let m = Measured {
        peak: Some(500_000_000),
        ..m
    };
    let last = budgets(&m).pop().unwrap();
    assert_eq!((last.measured.as_str(), last.passed), ("500 MB", false));
}

// @verifies CT-EFF-009, CT-EFF-004, CT-EFF-006, CT-EFF-007
#[test]
#[ignore = "full size; run by the perf CI job in release mode"]
fn budgets_hold_at_10000_requirements_and_50000_links() {
    let started = Instant::now();
    let root = project_dir("full");
    generate(&root, 10_000, 50_000);
    println!("generated in {} ms", started.elapsed().as_millis());
    let m = measure(&root);
    assert_eq!((m.requirements, m.links), (10_000, 50_000));
    println!(
        "{} files, {} requirements, {} links, {} CPUs",
        m.files,
        m.requirements,
        m.links,
        std::thread::available_parallelism().map_or(0, |n| n.get())
    );
    println!("  full index: {} ms", m.index.as_millis());
    println!("  incremental index: {} ms", m.incremental.as_millis());

    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config = TraceConfig::read(&repo.join("trace.toml")).unwrap();
    let reqs = read_requirements(&repo.join("model/requirements.sysml")).unwrap();
    let phase_of = |id: &str| {
        let req = reqs.iter().find(|r| r.id == id);
        req.and_then(|r| r.phase)
            .expect("budget requirement has a phase")
    };
    let (table, ok) = judge(&budgets(&m), phase_of, config.current_phase);
    println!("{table}");
    assert!(ok, "a due CT-EFF budget is exceeded:\n{table}");
}

// @verifies CT-EFF-009
/// The canary for the perf job's limits. It writes 5 GiB, more than the job's 4 GB with no
/// swap, so the kernel must kill it; the job fails if it finishes. It does nothing unless
/// `CLEW_PERF_CANARY` is set, so `--ignored` on a machine without the limits is safe.
#[test]
#[ignore = "run by the perf CI job under its limits, where it must be killed"]
fn canary_writes_more_than_the_memory_limit() {
    if std::env::var_os("CLEW_PERF_CANARY").is_none() {
        return;
    }
    let mut blocks = Vec::new();
    for _ in 0..5 * 1024 {
        // Nonzero bytes, so every page is written, not just reserved.
        blocks.push(std::hint::black_box(vec![1u8; 1 << 20]));
    }
    println!("wrote {} MiB and was not killed", blocks.len());
}
