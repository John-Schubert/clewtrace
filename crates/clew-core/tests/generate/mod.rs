//! A generator for large test projects, shared by the perf harness in `tests/perf.rs` and
//! the desktop app's command tests, which include this file with `#[path]`.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

// @verifies CT-EFF-009, CT-EFF-004
/// Requirements per model file, and the number of requirements per top-level one.
pub const PER_FILE: usize = 100;
// @verifies CT-EFF-009, CT-EFF-004
pub const PER_TOP: usize = 10;

// @verifies CT-EFF-009, CT-EFF-004
/// Writes a project of `requirements` requirements and `links` trace links under `root`.
///
/// One requirement in ten is top level; each other one derives from the top-level one
/// before it. Every requirement is satisfied by a part, verified by a part definition, and
/// implemented and verified by tagged Rust code. The remaining links are second
/// derivations of requirements that are not top level. Files hold 100 requirements each,
/// in `model/reqs`, `model/design`, and `src`.
pub fn generate(root: &Path, requirements: usize, links: usize) {
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
