//! `clew report coverage` and `clew impact`: what the trace graph says about a project.
//!
//! A project is every `.sysml` file under its root, built into one model, and the code under
//! the source roots in `trace.toml`, scanned for tags. The coverage report lists the
//! requirements that lack a parent, an implementation, or a verification, and the model
//! elements traced to no requirement. Impact lists everything reachable from one element
//! through trace links, upstream and downstream.

use std::collections::HashSet;
use std::path::Path;

use crate::check::{self, SetupError};
use crate::config::{TOP_LEVEL_DEFINITIONS, TraceConfig};
use crate::index;
use crate::model::{ElementId, ElementKind, LIBRARY_PATH, Model, Role};
use crate::parse::ast::{Kind, Member, UsageKind};
use crate::parse::parser;
use crate::reproduce;
use crate::scan::{self, FileScan};
use crate::trace::{End, LinkKind, Toward, TraceGraph};

// @implements CT-TRC-004, CT-TRC-005
/// A project's model and trace graph, and the problems found while reading them.
pub struct Project {
    pub model: Model,
    pub graph: TraceGraph,
    /// Unreadable model files, syntax errors, unresolved names, and links that could not be
    /// made, each rendered with where and how to fix it.
    pub problems: Vec<String>,
    /// Every file read as text, `trace.toml` included, with the SHA-256 of its text from
    /// `reproduce::text_sha256`.
    pub inputs: Vec<(String, Vec<u8>)>,
    /// The requirement definitions in `trace.toml` whose usages need no parent.
    pub top_level: Vec<ElementId>,
}

// @implements CT-TRC-004, CT-TRC-005, CT-USE-007, CT-TRC-009
/// Reads the project under `root`: its `trace.toml`, every `.sysml` file, and the tags in
/// the code under the source roots. Code files that `clew check --self` would report as
/// excluded, untraceable, or unreadable add no links.
pub fn load_project(root: &Path) -> Result<Project, SetupError> {
    let config = check::read_config(root)?;
    let paths = index::sysml_files(root).map_err(|e| SetupError {
        path: String::new(),
        line: 0,
        message: e.to_string(),
    })?;
    let mut problems = Vec::new();
    let mut hashed = Vec::new();
    if let Ok(loaded) = index::read_file(&root.join(check::TRACE_CONFIG)) {
        let sha256 = reproduce::text_sha256(&loaded.text);
        hashed.push((check::TRACE_CONFIG.to_string(), sha256));
    }
    let mut parsed: Vec<(&str, Vec<Member>)> = Vec::new();
    for path in &paths {
        match index::read_file(&root.join(path)) {
            Ok(loaded) => {
                let result = parser::parse(&loaded.text);
                problems.extend(result.errors.iter().map(|e| e.render(path)));
                parsed.push((path, result.members));
                hashed.push((path.clone(), reproduce::text_sha256(&loaded.text)));
            }
            Err(message) => problems.push(index::unreadable(path, &message)),
        }
    }
    let inputs: Vec<(&str, &[Member])> = (parsed.iter())
        .map(|(path, members)| (*path, members.as_slice()))
        .collect();
    let model = Model::build(&inputs);
    problems.extend(model.errors.iter().map(ToString::to_string));
    let top_level = top_level_definitions(&model, &config)?;
    let mut scans = Vec::new();
    for path in check::source_files(root, &config)? {
        let excluded = (config.excludes.iter()).any(|e| check::glob_matches(&e.path, &path));
        let syntax = check::syntax_for(&config, &path).filter(|_| !excluded);
        let loaded = syntax.and_then(|_| index::read_file(&root.join(&path)).ok());
        if let (Some(syntax), Some(loaded)) = (syntax, loaded) {
            scans.push((path.clone(), scan::scan_source(syntax, &loaded.text)));
            hashed.push((path, reproduce::text_sha256(&loaded.text)));
        }
    }
    let code: Vec<(&str, &FileScan)> = (scans.iter()).map(|(p, s)| (p.as_str(), s)).collect();
    let graph = TraceGraph::build(&model, &code);
    problems.extend(graph.problems.iter().map(ToString::to_string));
    Ok(Project {
        model,
        graph,
        problems,
        inputs: hashed,
        top_level,
    })
}

// @implements CT-TRC-004, CT-USE-007
/// Resolves `[coverage] top_level_definitions` in `trace.toml`. Each entry must be the
/// qualified name of a requirement definition.
fn top_level_definitions(
    model: &Model,
    config: &TraceConfig,
) -> Result<Vec<ElementId>, SetupError> {
    let error = |name: &str, problem: &str| SetupError {
        path: check::TRACE_CONFIG.to_string(),
        line: config.top_level_line,
        message: format!(
            "`{TOP_LEVEL_DEFINITIONS}` entry `{name}` {problem}\n  help: list the qualified \
             names of requirement definitions, such as `Requirements::TopLevelRequirement`"
        ),
    };
    let mut found = Vec::new();
    for name in &config.top_level_definitions {
        let id = model
            .find(name)
            .ok_or_else(|| error(name, "names no element in the model"))?;
        if model.element(id).kind != ElementKind::Definition(Kind::Requirement) {
            return Err(error(name, "is not a requirement definition"));
        }
        found.push(id);
    }
    Ok(found)
}

// @implements CT-TRC-004
/// Whether `id` is typed by one of the `top_level` definitions or by a definition that
/// specializes one, directly or through other definitions.
fn is_top_level(model: &Model, id: ElementId, top_level: &[ElementId]) -> bool {
    let mut seen = HashSet::new();
    let mut todo: Vec<ElementId> = model.cited(id, Role::Type).collect();
    while let Some(definition) = todo.pop() {
        if top_level.contains(&definition) {
            return true;
        }
        if seen.insert(definition) {
            todo.extend(model.cited(definition, Role::Specializes));
        }
    }
    false
}

// @implements CT-TRC-004, CT-DEV-003
/// What one requirement is traced to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequirementCoverage {
    pub requirement: ElementId,
    /// It derives from or refines another requirement, or is nested in one.
    pub has_parent: bool,
    /// Code implements it or a model element satisfies it.
    pub implemented: bool,
    /// A test or a model element verifies it.
    pub verified: bool,
}

// @implements CT-TRC-004, CT-DEV-003
/// The coverage of every requirement usage outside the built-in library, and the elements
/// traced to no requirement, both in declaration order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coverage {
    pub requirements: Vec<RequirementCoverage>,
    pub untraced: Vec<ElementId>,
}

// @implements CT-TRC-004, CT-DEV-003
/// Works out the coverage of a model and its trace graph. A requirement typed by one of
/// the `top_level` definitions, or by a definition that specializes one, needs no parent.
///
/// An element counts as traced when it, an element that owns it, or an element it owns
/// is linked to a requirement, directly or through other links, in either direction. Definitions and usages count; packages, links,
/// metadata definitions, and the features of a requirement do not.
pub fn coverage(model: &Model, graph: &TraceGraph, top_level: &[ElementId]) -> Coverage {
    let is_requirement = |id: ElementId| {
        matches!(
            model.element(id).kind,
            ElementKind::Definition(Kind::Requirement)
                | ElementKind::Usage(UsageKind::Of(Kind::Requirement))
        )
    };
    let in_library = |id: ElementId| model.files[model.element(id).file] == LIBRARY_PATH;
    let owners =
        |id: ElementId| std::iter::successors(model.element(id).owner, |&o| model.element(o).owner);
    let ids = || (0..model.elements.len()).map(ElementId);
    let requirements = ids()
        .filter(|&id| {
            model.element(id).kind == ElementKind::Usage(UsageKind::Of(Kind::Requirement))
                && !in_library(id)
        })
        .map(|id| {
            let parent_link = graph.links_from(id).any(|l| {
                matches!(l.kind, LinkKind::Derives | LinkKind::Refines)
                    && matches!(l.to, End::Element(to) if is_requirement(to))
            });
            let into = || graph.links_to(id).map(|l| l.kind);
            RequirementCoverage {
                requirement: id,
                has_parent: parent_link
                    || model.element(id).owner.is_some_and(is_requirement)
                    || is_top_level(model, id, top_level),
                implemented: into()
                    .any(|k| matches!(k, LinkKind::Implements | LinkKind::Satisfies)),
                verified: into().any(|k| k == LinkKind::Verifies),
            }
        })
        .collect();
    let all: Vec<ElementId> = ids().filter(|&id| is_requirement(id)).collect();
    let mut reached = graph.reachable(&all, Toward::Downstream);
    reached.extend(graph.reachable(&all, Toward::Upstream));
    let reaches: HashSet<ElementId> = (reached.iter())
        .filter_map(|r| match r.end {
            End::Element(id) => Some(*id),
            End::Code { .. } => None,
        })
        .collect();
    // An element whose member is linked to a requirement is traced, and so are its owners.
    let mut traced = HashSet::new();
    for &id in &reaches {
        traced.insert(id);
        traced.extend(owners(id));
    }
    let untraced = ids()
        .filter(|&id| {
            let counted = match model.element(id).kind {
                ElementKind::Definition(kind) => {
                    !matches!(kind, Kind::Requirement | Kind::Metadata)
                }
                ElementKind::Usage(kind) => kind != UsageKind::Of(Kind::Requirement),
                _ => false,
            };
            counted
                && !in_library(id)
                && !traced.contains(&id)
                && !owners(id).any(|o| is_requirement(o) || reaches.contains(&o))
        })
        .collect();
    Coverage {
        requirements,
        untraced,
    }
}

// @implements CT-TRC-004, CT-DEV-003, CT-TRC-008
/// The coverage report as text: one section per gap, then a summary line.
pub fn coverage_text(model: &Model, coverage: &Coverage) -> String {
    type Test = fn(&RequirementCoverage) -> bool;
    let sections: [(&str, Test); 3] = [
        ("Requirements without a parent", |r| !r.has_parent),
        ("Requirements without an implementation", |r| !r.implemented),
        ("Requirements without a verification", |r| !r.verified),
    ];
    let mut out = String::new();
    let mut lacking = Vec::new();
    for (title, test) in sections {
        let rows: Vec<ElementId> = (coverage.requirements.iter())
            .filter(|r| test(r))
            .map(|r| r.requirement)
            .collect();
        section(&mut out, model, title, &rows);
        lacking.push(rows.len());
    }
    let untraced = &coverage.untraced;
    section(
        &mut out,
        model,
        "Elements traced to no requirement",
        untraced,
    );
    let total = coverage.requirements.len();
    out.push_str(&format!(
        "clew report coverage: {total} requirements, {} with a parent, {} implemented, \
         {} verified, {} elements traced to no requirement\n",
        total - lacking[0],
        total - lacking[1],
        total - lacking[2],
        untraced.len()
    ));
    out
}

// @implements CT-TRC-004, CT-TRC-008
fn section(out: &mut String, model: &Model, title: &str, rows: &[ElementId]) {
    out.push_str(&format!("{title} ({}):\n", rows.len()));
    for &id in rows {
        out.push_str(&format!("  {}\n", located(model, id)));
    }
}

// @implements CT-TRC-005
/// The element a name selects: a requirement ID such as `CT-TRC-005`, or a qualified name
/// such as `Design::Engine`.
pub fn find_element(project: &Project, name: &str) -> Option<ElementId> {
    (project.graph.requirement(name)).or_else(|| project.model.find(name))
}

// @implements CT-TRC-005, CT-TRC-008
/// The impact of an element as text: the element, then every element and code scope
/// reachable upstream and downstream, each with the link that reached it and its distance.
pub fn impact_text(model: &Model, graph: &TraceGraph, element: ElementId) -> String {
    let mut out = format!("{}\n", located(model, element));
    for (title, toward) in [
        ("Upstream, what it traces to", Toward::Upstream),
        ("Downstream, what traces to it", Toward::Downstream),
    ] {
        let reached = graph.reachable(&[element], toward);
        out.push_str(&format!("{title} ({}):\n", reached.len()));
        for r in &reached {
            out.push_str(&format!(
                "  {}. {} {} {}  at {}:{}\n",
                r.depth,
                end_label(model, &r.link.from),
                r.link.kind.verb(),
                end_label(model, &r.link.to),
                r.link.path,
                r.link.line
            ));
        }
    }
    out
}

// @implements CT-TRC-004, CT-TRC-005
/// An element's short name, if any, and qualified name.
fn label(model: &Model, id: ElementId) -> String {
    let name = model.qualified_name(id);
    match &model.element(id).short_name {
        Some(short) => format!("{short} {name}"),
        None => name,
    }
}

// @implements CT-TRC-004, CT-TRC-005
/// An element's label and where it is declared.
fn located(model: &Model, id: ElementId) -> String {
    let element = model.element(id);
    let path = &model.files[element.file];
    format!("{}  {path}:{}", label(model, id), element.pos.line)
}

// @implements CT-TRC-005
fn end_label(model: &Model, end: &End) -> String {
    match end {
        End::Element(id) => label(model, *id),
        End::Code {
            path,
            start_line,
            end_line,
        } => format!("{path}:{start_line}-{end_line}"),
    }
}

// @verifies CT-TRC-004, CT-TRC-005, CT-DEV-003, CT-USE-007
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    const REQS: &str = "package Reqs {
    requirement def Req;
    requirement <'CT-TRC-001'> top : Req;
    requirement <'CT-TRC-002'> child : Req {
        requirement <'CT-TRC-003'> nested : Req;
    }
    requirement <'CT-TRC-004'> lone : Req;
}
";

    const DESIGN: &str = "package Design {
    part def Engine {
        attribute mass;
        satisfy Reqs::top;
    }
    part car {
        part engine : Engine;
        satisfy Reqs::child by engine;
    }
    part def Loose {
        part inner;
    }
    part def Tester {
        verify Reqs::child;
    }
    #derivation dependency Reqs::child to Reqs::top;
}
";

    const CODE: &str =
        "// @implements CT-TRC-001\nfn a() {}\n// @verifies CT-TRC-001\n#[test]\nfn t() {}\n";

    /// A project folder with `trace.toml`, the model files, the Clewtrace library, and code.
    fn project(name: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let root = std::env::temp_dir().join("clew-report-tests").join(name);
        let _ = fs::remove_dir_all(&root);
        let config = "[project]\nrequirements = [\"model/reqs.sysml\"]\n\
                      id_pattern = \"CT-[A-Z]{3}-[0-9]{3}\"\ncurrent_phase = 1\n\
                      [coverage]\nsource_roots = [\"src\"]\nlanguages = [\"rust\"]\n\
                      file_level_allowed = []\nmax_scope_lines = 200\n\
                      [[exclude]]\npath = \"src/gen/**\"\nreason = \"generated\"\n\
                      requirement = \"CT-TRC-001\"\n";
        let library = include_str!("../../../library/Clewtrace.sysml");
        let base: [(&str, &[u8]); 5] = [
            ("trace.toml", config.as_bytes()),
            ("model/reqs.sysml", REQS.as_bytes()),
            ("model/design.sysml", DESIGN.as_bytes()),
            ("library/Clewtrace.sysml", library.as_bytes()),
            ("src/lib.rs", CODE.as_bytes()),
        ];
        for (path, text) in base.iter().chain(files) {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        root
    }

    // @verifies CT-TRC-004, CT-DEV-003
    #[test]
    fn reports_requirements_lacking_a_parent_implementation_or_verification() {
        let root = project("coverage", &[]);
        let project = load_project(&root).unwrap();
        assert!(project.problems.is_empty(), "{:?}", project.problems);
        let model = &project.model;
        let found = coverage(model, &project.graph, &project.top_level);
        let summary: Vec<(String, bool, bool, bool)> = (found.requirements.iter())
            .map(|r| {
                let name = model.qualified_name(r.requirement);
                (name, r.has_parent, r.implemented, r.verified)
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("Reqs::top".to_string(), false, true, true),
                ("Reqs::child".to_string(), true, true, true),
                ("Reqs::child::nested".to_string(), true, false, false),
                ("Reqs::lone".to_string(), false, false, false),
            ]
        );
        assert_eq!(
            coverage_text(model, &found),
            "Requirements without a parent (2):
  CT-TRC-001 Reqs::top  model/reqs.sysml:3
  CT-TRC-004 Reqs::lone  model/reqs.sysml:7
Requirements without an implementation (2):
  CT-TRC-003 Reqs::child::nested  model/reqs.sysml:5
  CT-TRC-004 Reqs::lone  model/reqs.sysml:7
Requirements without a verification (2):
  CT-TRC-003 Reqs::child::nested  model/reqs.sysml:5
  CT-TRC-004 Reqs::lone  model/reqs.sysml:7
Elements traced to no requirement (2):
  Design::Loose  model/design.sysml:10
  Design::Loose::inner  model/design.sysml:11
clew report coverage: 4 requirements, 2 with a parent, 2 implemented, 2 verified, 2 elements \
traced to no requirement
"
        );
    }

    // @verifies CT-TRC-004
    #[test]
    fn a_parent_must_be_a_requirement() {
        let links =
            "package Links {\n    part p;\n    #refinement dependency Reqs::lone to p;\n}\n";
        let root = project("parent", &[("model/links.sysml", links.as_bytes())]);
        let project = load_project(&root).unwrap();
        let found = coverage(&project.model, &project.graph, &project.top_level);
        assert!(!found.requirements[3].has_parent);
        // The part is traced all the same: a requirement refines it.
        let p = project.model.find("Links::p").unwrap();
        assert!(!found.untraced.contains(&p));
        assert_eq!(found.untraced.len(), 2);
    }

    /// Sets `top_level_definitions` in the project's `trace.toml`, on line 10.
    fn set_top_level(root: &Path, list: &str) {
        let path = root.join("trace.toml");
        let text = fs::read_to_string(&path).unwrap().replace(
            "max_scope_lines = 200\n",
            &format!("max_scope_lines = 200\ntop_level_definitions = {list}\n"),
        );
        fs::write(path, text).unwrap();
    }

    // @verifies CT-TRC-004, CT-DEV-003
    #[test]
    fn requirements_of_a_top_level_definition_need_no_parent() {
        let top = "package Top {
    requirement def Product;
    requirement def Special :> Product;
    requirement def Loop1 :> Loop2;
    requirement def Loop2 :> Loop1;
    requirement <'CT-TRC-005'> a : Product;
    requirement <'CT-TRC-006'> b : Special;
    requirement <'CT-TRC-007'> c : Reqs::Req;
    requirement <'CT-TRC-008'> d : Loop1;
}
";
        let root = project("top-level", &[("model/top.sysml", top.as_bytes())]);
        let parents = |root: &Path| {
            let project = load_project(root).unwrap();
            assert!(project.problems.is_empty(), "{:?}", project.problems);
            let found = coverage(&project.model, &project.graph, &project.top_level);
            (found.requirements.iter())
                .filter(|r| r.has_parent)
                .map(|r| project.model.qualified_name(r.requirement))
                .collect::<Vec<String>>()
        };
        assert_eq!(parents(&root), ["Reqs::child", "Reqs::child::nested"]);
        set_top_level(&root, "[\"Top::Product\"]");
        assert_eq!(
            parents(&root),
            ["Reqs::child", "Reqs::child::nested", "Top::a", "Top::b"]
        );
    }

    // @verifies CT-TRC-004, CT-USE-007
    #[test]
    fn top_level_entries_must_be_requirement_definitions() {
        let cases = [
            (
                "[\"Reqs::Missing\"]",
                "`Reqs::Missing` names no element in the model",
            ),
            (
                "[\"Design::Engine\"]",
                "`Design::Engine` is not a requirement definition",
            ),
            (
                "[\"Reqs::top\"]",
                "`Reqs::top` is not a requirement definition",
            ),
        ];
        for (list, problem) in cases {
            let root = project("top-level-errors", &[]);
            set_top_level(&root, list);
            let err = load_project(&root).err().unwrap();
            assert_eq!(
                err.to_string(),
                format!(
                    "trace.toml:10: `top_level_definitions` entry {problem}\n  help: list the \
                     qualified names of requirement definitions, such as \
                     `Requirements::TopLevelRequirement`"
                )
            );
        }
    }

    // @verifies CT-TRC-005
    #[test]
    fn lists_everything_reachable_upstream_and_downstream() {
        let root = project("impact", &[]);
        let project = load_project(&root).unwrap();
        let child = find_element(&project, "CT-TRC-002").unwrap();
        assert_eq!(Some(child), project.model.find("Reqs::child"));
        assert_eq!(
            impact_text(&project.model, &project.graph, child),
            "CT-TRC-002 Reqs::child  model/reqs.sysml:4
Upstream, what it traces to (1):
  1. CT-TRC-002 Reqs::child derives CT-TRC-001 Reqs::top  at model/design.sysml:16
Downstream, what traces to it (2):
  1. Design::car::engine satisfies CT-TRC-002 Reqs::child  at model/design.sysml:8
  1. Design::Tester verifies CT-TRC-002 Reqs::child  at model/design.sysml:14
"
        );
        let engine = find_element(&project, "Design::car::engine").unwrap();
        assert_eq!(
            impact_text(&project.model, &project.graph, engine),
            "Design::car::engine  model/design.sysml:7
Upstream, what it traces to (2):
  1. Design::car::engine satisfies CT-TRC-002 Reqs::child  at model/design.sysml:8
  2. CT-TRC-002 Reqs::child derives CT-TRC-001 Reqs::top  at model/design.sysml:16
Downstream, what traces to it (0):
"
        );
        let top = find_element(&project, "CT-TRC-001").unwrap();
        let text = impact_text(&project.model, &project.graph, top);
        assert!(
            text.contains("  1. src/lib.rs:2-2 implements CT-TRC-001 Reqs::top  at src/lib.rs:2\n"),
            "{text}"
        );
        assert!(
            text.contains("  2. Design::car::engine satisfies"),
            "{text}"
        );
        assert_eq!(find_element(&project, "Nowhere::x"), None);
    }

    // @verifies CT-TRC-004, CT-USE-007
    #[test]
    fn collects_problems_and_skips_code_it_cannot_trace() {
        let files: &[(&str, &[u8])] = &[
            ("model/broken.sysml", b"part def 42;\n"),
            ("model/latin1.sysml", b"part def Caf\xe9;\n"),
            ("model/unresolved.sysml", b"part x : Missing;\n"),
            ("src/unknown.rs", b"// @implements CT-XXX-001\nfn u() {}\n"),
            (
                "src/latin1.rs",
                b"// @implements CT-TRC-004\nfn caf\xe9() {}\n",
            ),
            ("src/gen/out.rs", b"// @implements CT-TRC-004\nfn g() {}\n"),
            ("src/notes.txt", b"@implements CT-TRC-004\n"),
        ];
        let root = project("problems", files);
        let project = load_project(&root).unwrap();
        let starts: Vec<&str> = (project.problems.iter())
            .map(|p| p.split(": error").next().unwrap())
            .collect();
        assert_eq!(
            starts,
            [
                "model/broken.sysml:1:10",
                "model/latin1.sysml",
                "model/unresolved.sysml:1:10",
                "src/unknown.rs:2",
            ]
        );
        assert!(project.problems.iter().all(|p| p.contains("\n  help: ")));
        // Excluded, unreadable, and untraceable code adds no link to Reqs::lone.
        let lone = project.model.find("Reqs::lone").unwrap();
        assert_eq!(project.graph.links_to(lone).count(), 0);
    }

    // @verifies CT-USE-007
    #[test]
    fn cannot_run_without_trace_toml() {
        let root = std::env::temp_dir().join("clew-report-tests/empty");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let err = load_project(&root).err().unwrap();
        assert_eq!(err.path, "trace.toml");
    }
}
