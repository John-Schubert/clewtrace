//! Performance harness: a generator for large test projects, and the CT-EFF budgets
//! measured on one of 10,000 requirements and 50,000 trace links.
//!
//! `cargo test` runs the generator at a small size. The full-size run is ignored by default;
//! the `perf` CI job runs it in release mode, limited to 4 GB of RAM and 2 CPUs:
//! `cargo test --release -p clew-core --test perf -- --ignored --nocapture`.
//! A budget fails the run only when its requirement's phase has started (`current_phase`
//! in `trace.toml`); later budgets are measured and printed as not due yet.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clew_core::config::TraceConfig;
use clew_core::index::Index;
use clew_core::report::{coverage, load_project};
use clew_core::reqs::read_requirements;

// @verifies CT-EFF-009, CT-EFF-004
/// Requirements per model file, and the number of requirements per top-level one.
const PER_FILE: usize = 100;
// @verifies CT-EFF-009, CT-EFF-004
const PER_TOP: usize = 10;

// @verifies CT-EFF-009, CT-EFF-004
/// Writes a project of `requirements` requirements and `links` trace links under `root`.
///
/// One requirement in ten is top level; each other one derives from the top-level one
/// before it. Every requirement is satisfied by a part, verified by a part definition, and
/// implemented and verified by tagged Rust code. The remaining links are second
/// derivations of requirements that are not top level. Files hold 100 requirements each,
/// in `model/reqs`, `model/design`, and `src`.
fn generate(root: &Path, requirements: usize, links: usize) {
    let tops = requirements.div_ceil(PER_TOP);
    let base = 4 * requirements + (requirements - tops);
    assert!(
        (base..=base + requirements - tops).contains(&links),
        "{requirements} requirements take {base} to {} links",
        base + requirements - tops
    );
    let mut extra = links - base;
    let _ = fs::remove_dir_all(root);
    for dir in ["model/reqs", "model/design", "src"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    fs::write(
        root.join("trace.toml"),
        "[project]\nrequirements = [\"model/perf.sysml\"]\nid_pattern = \"PF-[0-9]{5}\"\n\
         current_phase = 1\n[coverage]\nsource_roots = [\"src\"]\nlanguages = [\"rust\"]\n\
         file_level_allowed = []\nmax_scope_lines = 200\n\
         top_level_definitions = [\"Perf::Product\"]\n",
    )
    .unwrap();
    fs::write(
        root.join("model/perf.sysml"),
        "package Perf {\n    requirement def Product {\n        attribute phase;\n    }\n    \
         requirement def Derived :> Product;\n}\n",
    )
    .unwrap();
    let name = |i: usize| format!("R{:03}::r{i:05}", i / PER_FILE);
    for file in 0..requirements.div_ceil(PER_FILE) {
        let range = file * PER_FILE..requirements.min((file + 1) * PER_FILE);
        let (mut reqs, mut design, mut code) = (String::new(), String::new(), String::new());
        writeln!(reqs, "package R{file:03} {{").unwrap();
        writeln!(design, "package D{file:03} {{").unwrap();
        for i in range {
            let top = i % PER_TOP == 0;
            let def = if top {
                "Perf::Product"
            } else {
                "Perf::Derived"
            };
            write!(
                reqs,
                "    requirement <'PF-{i:05}'> r{i:05} : {def} {{\n        doc /* The \
                 system shall handle case {i} within its budget. */\n        \
                 attribute :>> phase = {};\n    }}\n",
                i % 6
            )
            .unwrap();
            let r = name(i);
            write!(
                design,
                "    part p{i:05} {{\n        satisfy {r};\n    }}\n    \
                 part def T{i:05} {{\n        verify {r};\n    }}\n"
            )
            .unwrap();
            if !top {
                let parent = name(i - i % PER_TOP);
                writeln!(design, "    #derivation dependency {r} to {parent};").unwrap();
                if extra > 0 {
                    let other = name((i - i % PER_TOP + PER_TOP) % requirements);
                    writeln!(design, "    #derivation dependency {r} to {other};").unwrap();
                    extra -= 1;
                }
            }
            write!(
                code,
                "// @implements PF-{i:05}\nfn f{i:05}() {{}}\n\n\
                 // @verifies PF-{i:05}\n#[test]\nfn t{i:05}() {{\n    f{i:05}();\n}}\n\n"
            )
            .unwrap();
        }
        reqs.push_str("}\n");
        design.push_str("}\n");
        fs::write(root.join(format!("model/reqs/r{file:03}.sysml")), reqs).unwrap();
        fs::write(root.join(format!("model/design/d{file:03}.sysml")), design).unwrap();
        fs::write(root.join(format!("src/c{file:03}.rs")), code).unwrap();
    }
}

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
    let found = coverage(&project.model, &project.graph, &project.top_level);
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
