//! Trace graph: the derives, refines, satisfies, verifies, implements, and evidences links
//! between model elements and code.
//!
//! Links come from two places. The model gives `satisfy` (the satisfying element, or the
//! owner when there is no `by`), `verify` (the owner verifies the requirement), and the
//! `#derivation`, `#refinement`, and `#evidence` dependencies (each source to each target).
//! Code tags give implements links from `@implements` scopes and verifies links from
//! `@verifies` scopes, each pointing at the requirement whose short name the tag cites.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

use crate::model::{ElementId, ElementKind, Model, Role};
use crate::parse::ast::{DependencyKind, Kind, UsageKind};
use crate::scan::{FileScan, ScopeKind};

// @implements CT-TRC-002
/// The six kinds of trace link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LinkKind {
    Derives,
    Refines,
    Satisfies,
    Verifies,
    Implements,
    Evidences,
}

// @implements CT-TRC-002
impl LinkKind {
    /// The verb used in reports, such as `satisfies`.
    pub fn verb(self) -> &'static str {
        match self {
            LinkKind::Derives => "derives",
            LinkKind::Refines => "refines",
            LinkKind::Satisfies => "satisfies",
            LinkKind::Verifies => "verifies",
            LinkKind::Implements => "implements",
            LinkKind::Evidences => "evidences",
        }
    }
}

// @implements CT-TRC-002, CT-TRC-006
/// One end of a link: a model element, or the lines of a tagged code scope.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum End {
    Element(ElementId),
    Code {
        path: String,
        start_line: usize,
        end_line: usize,
    },
}

// @implements CT-TRC-002, CT-TRC-006
/// A trace link and the place in the text that declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub kind: LinkKind,
    pub from: End,
    pub to: End,
    pub path: String,
    pub line: usize,
}

// @implements CT-TRC-002, CT-TRC-006, CT-USE-007
/// A link that could not be made: where, why, and how to fix it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceProblem {
    pub path: String,
    pub line: usize,
    pub message: String,
    pub help: &'static str,
}

// @implements CT-USE-007
impl fmt::Display for TraceProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: error: {}\n  help: {}",
            self.path, self.line, self.message, self.help
        )
    }
}

// @implements CT-TRC-005
/// Which way to follow links. Every link points upstream, from the element that derives
/// from, refines, satisfies, verifies, implements, or evidences, to the element it traces to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toward {
    /// To what the start traces to, such as the requirement a part satisfies.
    Upstream,
    /// To what traces to the start, such as the code that implements a requirement.
    Downstream,
}

// @implements CT-TRC-005
/// An end reached by following links, the link that reached it, and how many links away
/// from the start it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reached<'a> {
    pub end: &'a End,
    pub link: &'a Link,
    pub depth: usize,
}

// @implements CT-TRC-002, CT-TRC-006
/// Every trace link of a model and its code, with lookups by element.
#[derive(Debug, Clone, Default)]
pub struct TraceGraph {
    /// Model links in declaration order, then code links by file and line.
    pub links: Vec<Link>,
    pub problems: Vec<TraceProblem>,
    /// Requirements by short name, the IDs that code tags cite.
    requirements: HashMap<String, ElementId>,
    /// Indexes into `links` by the element at each end.
    outgoing: HashMap<ElementId, Vec<usize>>,
    incoming: HashMap<ElementId, Vec<usize>>,
}

// @implements CT-TRC-002, CT-TRC-006
impl TraceGraph {
    /// Builds the graph of a model and the tag scans of its code, given as `(path, scan)`.
    /// Names the model could not resolve are already in `Model::errors` and make no link.
    pub fn build(model: &Model, code: &[(&str, &FileScan)]) -> TraceGraph {
        let mut graph = TraceGraph::default();
        for (index, element) in model.elements.iter().enumerate() {
            let is_requirement = matches!(
                element.kind,
                ElementKind::Definition(Kind::Requirement)
                    | ElementKind::Usage(UsageKind::Of(Kind::Requirement))
            );
            if let (true, Some(id)) = (is_requirement, &element.short_name) {
                graph
                    .requirements
                    .entry(id.clone())
                    .or_insert(ElementId(index));
            }
        }
        graph.add_model_links(model);
        for (path, scan) in code {
            graph.add_code_links(path, scan);
        }
        for (index, link) in graph.links.iter().enumerate() {
            if let End::Element(from) = link.from {
                graph.outgoing.entry(from).or_default().push(index);
            }
            if let End::Element(to) = link.to {
                graph.incoming.entry(to).or_default().push(index);
            }
        }
        graph
    }

    /// The requirement with this short name, such as `CT-TRC-002`.
    pub fn requirement(&self, id: &str) -> Option<ElementId> {
        self.requirements.get(id).copied()
    }

    /// Links that start at the element.
    pub fn links_from(&self, element: ElementId) -> impl Iterator<Item = &Link> {
        self.lookup(&self.outgoing, element)
    }

    /// Links that end at the element, such as everything that satisfies a requirement.
    pub fn links_to(&self, element: ElementId) -> impl Iterator<Item = &Link> {
        self.lookup(&self.incoming, element)
    }

    fn lookup<'a>(
        &'a self,
        by: &'a HashMap<ElementId, Vec<usize>>,
        element: ElementId,
    ) -> impl Iterator<Item = &'a Link> {
        let found = by.get(&element).map_or(&[][..], Vec::as_slice);
        found.iter().map(|&i| &self.links[i])
    }

    fn add_model_links(&mut self, model: &Model) {
        // What each satisfy names after `by`, and each dependency's targets.
        let mut by: HashMap<ElementId, Option<ElementId>> = HashMap::new();
        let mut suppliers: HashMap<ElementId, Vec<ElementId>> = HashMap::new();
        for r in &model.references {
            match r.role {
                Role::SatisfiedBy => {
                    by.insert(r.source, r.target);
                }
                Role::Supplier => suppliers.entry(r.source).or_default().extend(r.target),
                _ => {}
            }
        }
        for r in &model.references {
            let Some(target) = r.target else { continue };
            let source = model.element(r.source);
            let path = &model.files[source.file];
            let line = source.pos.line as usize;
            let (kind, from) = match (source.kind, r.role) {
                (ElementKind::Satisfy, Role::Requirement) => {
                    let from = match by.get(&r.source) {
                        Some(named) => *named,
                        None => self.owner(
                            model,
                            r.source,
                            "satisfy",
                            "add `by` and the satisfying element",
                        ),
                    };
                    (LinkKind::Satisfies, from)
                }
                (ElementKind::Verify, Role::Requirement) => {
                    let from = self.owner(
                        model,
                        r.source,
                        "verify",
                        "move the verify into the element that verifies the requirement",
                    );
                    (LinkKind::Verifies, from)
                }
                (ElementKind::Dependency(kind), Role::Client) => {
                    let kind = match kind {
                        DependencyKind::Derivation => LinkKind::Derives,
                        DependencyKind::Refinement => LinkKind::Refines,
                        DependencyKind::Evidence => LinkKind::Evidences,
                    };
                    let to = suppliers.get(&r.source).map_or(&[][..], Vec::as_slice);
                    for &to in to {
                        self.links.push(Link {
                            kind,
                            from: End::Element(target),
                            to: End::Element(to),
                            path: path.clone(),
                            line,
                        });
                    }
                    continue;
                }
                _ => continue,
            };
            if let Some(from) = from {
                self.links.push(Link {
                    kind,
                    from: End::Element(from),
                    to: End::Element(target),
                    path: path.clone(),
                    line,
                });
            }
        }
    }

    /// The element that owns a `satisfy` or `verify`, or a problem when it is at the top
    /// level of a file, where nothing owns it.
    fn owner(
        &mut self,
        model: &Model,
        id: ElementId,
        what: &str,
        help: &'static str,
    ) -> Option<ElementId> {
        let element = model.element(id);
        match element.owner {
            Some(owner) if owner != Model::ROOT => Some(owner),
            _ => {
                self.problems.push(TraceProblem {
                    path: model.files[element.file].clone(),
                    line: element.pos.line as usize,
                    message: format!("this {what} is not inside an element, so it links nothing"),
                    help,
                });
                None
            }
        }
    }

    fn add_code_links(&mut self, path: &str, scan: &FileScan) {
        let mut scopes: Vec<_> = scan.scopes.iter().collect();
        scopes.sort_by_key(|s| (s.start_line, s.end_line));
        for scope in scopes {
            // Import lines carry the union of the file's tags; the items already link.
            if scope.kind == ScopeKind::Import {
                continue;
            }
            let tagged = (scope.implements.iter().map(|id| (LinkKind::Implements, id)))
                .chain(scope.verifies.iter().map(|id| (LinkKind::Verifies, id)));
            for (kind, id) in tagged {
                let Some(&requirement) = self.requirements.get(id) else {
                    self.problems.push(TraceProblem {
                        path: path.to_string(),
                        line: scope.start_line,
                        message: format!("`{id}` names no requirement in the model"),
                        help: "cite the short name of a requirement in the model, such as \
                               `CT-TRC-002`, or add the requirement first",
                    });
                    continue;
                };
                self.links.push(Link {
                    kind,
                    from: End::Code {
                        path: path.to_string(),
                        start_line: scope.start_line,
                        end_line: scope.end_line,
                    },
                    to: End::Element(requirement),
                    path: path.to_string(),
                    line: scope.start_line,
                });
            }
        }
    }
}

// @implements CT-TRC-005
/// Impact analysis: following links from a set of elements.
impl TraceGraph {
    /// Every end reachable from the starts by following links one way, each once, breadth
    /// first, so each end comes with a shortest path's last link. The starts are not listed.
    pub fn reachable(&self, starts: &[ElementId], toward: Toward) -> Vec<Reached<'_>> {
        let by = match toward {
            Toward::Upstream => &self.outgoing,
            Toward::Downstream => &self.incoming,
        };
        let mut seen: HashSet<End> = starts.iter().map(|&s| End::Element(s)).collect();
        let mut queue: VecDeque<(ElementId, usize)> = starts.iter().map(|&s| (s, 0)).collect();
        let mut found = Vec::new();
        while let Some((at, depth)) = queue.pop_front() {
            for link in self.lookup(by, at) {
                let end = match toward {
                    Toward::Upstream => &link.to,
                    Toward::Downstream => &link.from,
                };
                if !seen.insert(end.clone()) {
                    continue;
                }
                if let End::Element(next) = end {
                    queue.push_back((*next, depth + 1));
                }
                found.push(Reached {
                    end,
                    link,
                    depth: depth + 1,
                });
            }
        }
        found
    }
}

// @verifies CT-TRC-002, CT-TRC-006, CT-TRC-005, CT-USE-007
#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::Member;
    use crate::parse::parser::parse;
    use crate::scan::{Syntax, scan_source};

    const REQS: &str = "package Reqs {\n\
        requirement def <'R0'> Base;\n\
        requirement <'R1'> top : Base;\n\
        requirement <'R2'> child : Base;\n\
        requirement <'R3'> detail : Base;\n\
    }\n";

    const DESIGN: &str = "package Design {\n\
        part def Engine {\n\
            satisfy Reqs::top;\n\
        }\n\
        part car {\n\
            part engine : Engine;\n\
            satisfy Reqs::child by engine;\n\
        }\n\
        part def EngineTest {\n\
            verify requirement Reqs::top;\n\
        }\n\
        part report;\n\
        #derivation dependency Reqs::child to Reqs::top;\n\
        #refinement dependency Reqs::detail to Reqs::child, Reqs::top;\n\
        #evidence dependency report to Reqs::top;\n\
    }\n";

    /// Builds the graph of `.sysml` texts, which must parse and resolve, and of Rust files.
    fn graph(models: &[(&str, &str)], rust: &[(&str, &str)]) -> (Model, TraceGraph) {
        let parsed: Vec<_> = models.iter().map(|(_, text)| parse(text)).collect();
        for p in &parsed {
            assert!(p.errors.is_empty(), "{:?}", p.errors);
        }
        let inputs: Vec<(&str, &[Member])> = (models.iter().zip(&parsed))
            .map(|((path, _), p)| (*path, p.members.as_slice()))
            .collect();
        let model = Model::build(&inputs);
        assert!(model.errors.is_empty(), "{:?}", model.errors);
        let scans: Vec<FileScan> = rust
            .iter()
            .map(|(_, text)| scan_source(Syntax::Rust, text))
            .collect();
        let code: Vec<(&str, &FileScan)> = rust.iter().map(|(p, _)| *p).zip(&scans).collect();
        let graph = TraceGraph::build(&model, &code);
        (model, graph)
    }

    /// Each link as `kind from -> to`, with elements by qualified name and code as path:lines.
    fn described(model: &Model, links: &[&Link]) -> Vec<String> {
        let end = |e: &End| match e {
            End::Element(id) => model.qualified_name(*id),
            End::Code {
                path,
                start_line,
                end_line,
            } => format!("{path}:{start_line}-{end_line}"),
        };
        links
            .iter()
            .map(|l| format!("{} {} -> {}", end(&l.from), l.kind.verb(), end(&l.to)))
            .collect()
    }

    // @verifies CT-TRC-002
    #[test]
    fn links_every_model_kind_from_source_to_target() {
        let (model, graph) = graph(&[("reqs.sysml", REQS), ("design.sysml", DESIGN)], &[]);
        assert!(graph.problems.is_empty(), "{:?}", graph.problems);
        let all: Vec<&Link> = graph.links.iter().collect();
        assert_eq!(
            described(&model, &all),
            [
                "Design::Engine satisfies -> Reqs::top",
                "Design::car::engine satisfies -> Reqs::child",
                "Design::EngineTest verifies -> Reqs::top",
                "Reqs::child derives -> Reqs::top",
                "Reqs::detail refines -> Reqs::child",
                "Reqs::detail refines -> Reqs::top",
                "Design::report evidences -> Reqs::top",
            ]
        );
        let satisfy = &graph.links[1];
        assert_eq!((satisfy.path.as_str(), satisfy.line), ("design.sysml", 7));
    }

    // @verifies CT-TRC-006
    #[test]
    fn links_code_tags_to_the_requirements_they_cite() {
        let code = "use std::fmt;\n\
                    // @implements R1, R2\n\
                    pub fn a() {\n    1;\n}\n\
                    // @verifies R1\n\
                    #[test]\n\
                    fn t() {}\n";
        let (model, graph) = graph(&[("reqs.sysml", REQS)], &[("src/a.rs", code)]);
        assert!(graph.problems.is_empty(), "{:?}", graph.problems);
        let all: Vec<&Link> = graph.links.iter().collect();
        assert_eq!(
            described(&model, &all),
            [
                "src/a.rs:3-5 implements -> Reqs::top",
                "src/a.rs:3-5 implements -> Reqs::child",
                "src/a.rs:7-8 verifies -> Reqs::top",
            ]
        );
        assert_eq!(graph.requirement("R1"), model.find("Reqs::top"));
        assert_eq!(graph.requirement("R0"), model.find("Reqs::Base"));
        assert_eq!(graph.requirement("CT-NOT-000"), None);
    }

    // @verifies CT-TRC-002, CT-TRC-006
    #[test]
    fn looks_up_links_by_element_at_either_end() {
        let code = "// @implements R1\nfn a() {}\n";
        let (model, graph) = graph(
            &[("reqs.sysml", REQS), ("design.sysml", DESIGN)],
            &[("a.rs", code)],
        );
        let top = model.find("Reqs::top").unwrap();
        let into_top: Vec<&Link> = graph.links_to(top).collect();
        assert_eq!(
            described(&model, &into_top),
            [
                "Design::Engine satisfies -> Reqs::top",
                "Design::EngineTest verifies -> Reqs::top",
                "Reqs::child derives -> Reqs::top",
                "Reqs::detail refines -> Reqs::top",
                "Design::report evidences -> Reqs::top",
                "a.rs:2-2 implements -> Reqs::top",
            ]
        );
        let detail = model.find("Reqs::detail").unwrap();
        let from_detail: Vec<&Link> = graph.links_from(detail).collect();
        assert_eq!(from_detail.len(), 2);
        assert!(from_detail.iter().all(|l| l.kind == LinkKind::Refines));
        assert_eq!(graph.links_to(detail).count(), 0);
        assert_eq!(graph.links_from(top).count(), 0);
    }

    // @verifies CT-TRC-006, CT-USE-007
    #[test]
    fn reports_tags_that_cite_no_requirement_in_the_model() {
        let code = "// @implements R1, CT-XXX-001\nfn a() {}\n";
        let (_, graph) = graph(&[("reqs.sysml", REQS)], &[("a.rs", code)]);
        assert_eq!(graph.links.len(), 1);
        assert_eq!(
            graph
                .problems
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            [
                "a.rs:2: error: `CT-XXX-001` names no requirement in the model\n  help: cite the \
              short name of a requirement in the model, such as `CT-TRC-002`, or add the \
              requirement first"
            ]
        );
    }

    // @verifies CT-TRC-002, CT-USE-007
    #[test]
    fn reports_satisfy_and_verify_that_no_element_owns() {
        let (_, graph) = graph(
            &[(
                "top.sysml",
                "requirement <'R1'> r;\nsatisfy r;\nverify r;\nsatisfy r by r;\n",
            )],
            &[],
        );
        let found: Vec<(usize, &str, &str)> = (graph.problems.iter())
            .map(|p| (p.line, p.message.as_str(), p.help))
            .collect();
        assert_eq!(
            found,
            [
                (
                    2,
                    "this satisfy is not inside an element, so it links nothing",
                    "add `by` and the satisfying element"
                ),
                (
                    3,
                    "this verify is not inside an element, so it links nothing",
                    "move the verify into the element that verifies the requirement"
                ),
            ]
        );
        assert_eq!(graph.links.len(), 1);
        assert_eq!(graph.links[0].kind, LinkKind::Satisfies);
    }

    // @verifies CT-TRC-002
    #[test]
    fn unresolved_names_make_no_link() {
        let src = "package P {\n  requirement <'R1'> r;\n  part p {\n    satisfy Missing;\n    \
                   satisfy r by Gone;\n  }\n  #evidence dependency p to r, Nope;\n  \
                   #evidence dependency Nope to r;\n}\n";
        let parsed = parse(src);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let model = Model::build(&[("p.sysml", parsed.members.as_slice())]);
        assert_eq!(model.errors.len(), 4);
        let graph = TraceGraph::build(&model, &[]);
        let all: Vec<&Link> = graph.links.iter().collect();
        assert_eq!(described(&model, &all), ["P::p evidences -> P::r"]);
        assert!(graph.problems.is_empty());
    }

    /// Each reached end as `depth: link`, in the order found.
    fn walked(model: &Model, reached: &[Reached<'_>]) -> Vec<String> {
        (reached.iter())
            .map(|r| format!("{}: {}", r.depth, described(model, &[r.link])[0]))
            .collect()
    }

    // @verifies CT-TRC-005
    #[test]
    fn walks_links_upstream_and_downstream_breadth_first() {
        let code = "// @implements R3\nfn a() {}\n";
        let (model, graph) = graph(
            &[("reqs.sysml", REQS), ("design.sysml", DESIGN)],
            &[("a.rs", code)],
        );
        let child = model.find("Reqs::child").unwrap();
        let up = graph.reachable(&[child], Toward::Upstream);
        assert_eq!(walked(&model, &up), ["1: Reqs::child derives -> Reqs::top"]);
        assert_eq!(up[0].end, &End::Element(model.find("Reqs::top").unwrap()));
        let down = graph.reachable(&[child], Toward::Downstream);
        assert_eq!(
            walked(&model, &down),
            [
                "1: Design::car::engine satisfies -> Reqs::child",
                "1: Reqs::detail refines -> Reqs::child",
                "2: a.rs:2-2 implements -> Reqs::detail",
            ]
        );
        let detail = model.find("Reqs::detail").unwrap();
        let up = graph.reachable(&[detail], Toward::Upstream);
        // Reqs::top is reached once, through the shorter path.
        assert_eq!(
            walked(&model, &up),
            [
                "1: Reqs::detail refines -> Reqs::child",
                "1: Reqs::detail refines -> Reqs::top",
            ]
        );
    }

    // @verifies CT-TRC-005
    #[test]
    fn walks_cycles_once_and_never_returns_to_the_start() {
        let src = "package P {\n  part a;\n  part b;\n  #derivation dependency a to b;\n  \
                   #derivation dependency b to a;\n}\n";
        let (model, graph) = graph(&[("p.sysml", src)], &[]);
        let a = model.find("P::a").unwrap();
        let up = graph.reachable(&[a], Toward::Upstream);
        assert_eq!(walked(&model, &up), ["1: P::a derives -> P::b"]);
        let down = graph.reachable(&[a], Toward::Downstream);
        assert_eq!(walked(&model, &down), ["1: P::b derives -> P::a"]);
        let b = model.find("P::b").unwrap();
        assert!(graph.reachable(&[a, b], Toward::Upstream).is_empty());
    }

    // @verifies CT-TRC-002
    #[test]
    fn names_every_link_kind() {
        let verbs: Vec<&str> = [
            LinkKind::Derives,
            LinkKind::Refines,
            LinkKind::Satisfies,
            LinkKind::Verifies,
            LinkKind::Implements,
            LinkKind::Evidences,
        ]
        .iter()
        .map(|k| k.verb())
        .collect();
        assert_eq!(
            verbs,
            [
                "derives",
                "refines",
                "satisfies",
                "verifies",
                "implements",
                "evidences"
            ]
        );
    }
}
