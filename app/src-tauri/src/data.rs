//! The open project the app serves to the UI, in pages of rows (ADR-0004).
//!
//! The Rust side owns the project: its model, trace graph, coverage, and search index. The
//! UI never holds a whole table; each query returns one page of at most `MAX_PAGE` rows as
//! JSON text. Element IDs are indexes into the loaded model and stay valid until the
//! project is opened again. Nothing here depends on Tauri, so tests call it directly.

use std::collections::{BTreeSet, HashMap};
use std::fmt::Display;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard};
use std::thread::JoinHandle;

use clew_core::check::{SetupError, TRACE_CONFIG};
use clew_core::index::{Index, IndexError};
use clew_core::json::quote;
use clew_core::live::{Reloaded, Workspace};
use clew_core::model::{Element, ElementId, LIBRARY_PATH, Model};
use clew_core::project::find_root;
use clew_core::report::{Project, RequirementCoverage, fully_waived, project_coverage};
use clew_core::trace::{End, Link, Toward};
use clew_core::watch::{ProjectWatcher, Trouble, watch_project};

// @implements CT-EFF-005, CT-EFF-004
/// The most rows one page holds. Lists longer than this are virtualized in the UI, which
/// asks for the page it shows.
pub const MAX_PAGE: usize = 200;

// @implements CT-TRC-004
/// The coverage filters, in the order the UI lists them. `untraced` lists elements traced
/// to no requirement; the others list requirements with that gap.
pub const GAP_FILTERS: [&str; 4] = [
    "no-parent",
    "no-implementation",
    "no-verification",
    "untraced",
];

// @implements CT-TRC-004, CT-EFF-005
/// The largest file the source view opens. A bigger one is refused rather than streamed,
/// because the view is a read-only look at a tagged line, not an editor.
pub const MAX_SOURCE_BYTES: usize = 2_000_000;

// @implements CT-USE-007
/// An error the UI shows: what went wrong, where, and how to fix it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppError {
    pub what: String,
    /// A file and line, a folder, or empty when the error has no place.
    pub place: String,
    pub help: String,
}

// @implements CT-USE-007
impl AppError {
    pub(crate) fn new(
        what: impl Into<String>,
        place: impl Into<String>,
        help: impl Into<String>,
    ) -> Self {
        AppError {
            what: what.into(),
            place: place.into(),
            help: help.into(),
        }
    }

    /// The error that stopped `trace.toml` or the model files from being read.
    fn setup(root: &Path, e: SetupError) -> Self {
        if e.path == TRACE_CONFIG && e.line == 0 && e.message.starts_with("not found") {
            return AppError::new(
                "trace.toml not found",
                root.display().to_string(),
                "Open the project folder, the one that holds trace.toml.",
            );
        }
        let place = match e.line {
            _ if e.path.is_empty() => root.display().to_string(),
            0 => e.path.clone(),
            line => format!("{}:{line}", e.path),
        };
        let (what, help) = match e.message.split_once("\n  help: ") {
            Some((what, help)) => (what.to_string(), help.to_string()),
            None => (
                e.message.clone(),
                "Fix the file named here, then open the project again.".to_string(),
            ),
        };
        AppError::new(what, place, help)
    }

    fn index(e: &IndexError) -> Self {
        AppError::new(e.message.clone(), e.path.clone(), e.help)
    }

    fn no_element(id: usize) -> Self {
        AppError::new(
            format!("no element has ID {id}"),
            "",
            "That element was renamed, moved, or deleted by an edit. Pick it again from the tree.",
        )
    }

    fn json(&self) -> String {
        Obj::new()
            .str("what", &self.what)
            .str("where", &self.place)
            .str("help", &self.help)
            .end()
    }
}

// @implements CT-USE-007
/// One row of the problems list, split out of the `place: severity: what\n  help: help`
/// text the core library renders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub severity: &'static str,
    pub what: String,
    pub place: String,
    pub help: String,
}

// @implements CT-USE-007
impl Problem {
    pub fn parse(text: &str) -> Problem {
        let (first, help) = text.split_once("\n  help: ").unwrap_or((text, ""));
        let mut problem = Problem {
            severity: "error",
            what: first.to_string(),
            place: String::new(),
            help: help.to_string(),
        };
        for severity in ["error", "warning"] {
            if let Some((place, what)) = first.split_once(&format!(": {severity}: ")) {
                problem.severity = severity;
                problem.place = place.to_string();
                problem.what = what.to_string();
                break;
            }
        }
        if problem.help.is_empty() {
            problem.help =
                "Fix the file named here; the project reloads when it is opened again.".to_string();
        }
        problem
    }
}

// @implements CT-EFF-004, CT-MOD-001, CT-MOD-003
/// A project read from its `.sysml` text and code, with its coverage and search index. The
/// workspace keeps each model file's syntax tree, so an edit re-parses one file (ADR-0005).
struct Live {
    workspace: Workspace,
    /// Coverage of each requirement usage that has an ID.
    coverage: HashMap<ElementId, RequirementCoverage>,
    /// Indexes into `Project::requirements` by requirement ID.
    requirements: HashMap<String, usize>,
    /// The elements behind each of `GAP_FILTERS`, in declaration order.
    gaps: [Vec<ElementId>; 4],
    /// The search index, or why it could not be opened. SQLite connections are not
    /// shared between threads, so queries take turns. It reads the saved files, so it
    /// does not see unsaved edits.
    index: Mutex<Result<Index, IndexError>>,
    /// Counts the edits taken, so the UI can drop a reply that a later edit overtook.
    revision: u64,
}

// @implements CT-EFF-007, CT-USE-009, CT-USE-008
/// What a burst of file events did to the open project.
#[derive(Debug)]
pub struct DiskNews {
    /// Files whose new content on disk the project now uses.
    pub reloaded: Vec<String>,
    /// Model files with unsaved edits that changed on disk.
    pub conflicts: Vec<String>,
    /// Why the new content could not be used; the project is the last one built.
    pub error: Option<AppError>,
    /// The project counts after the change, as `summary` gives them.
    pub summary: String,
}

// @implements CT-EFF-004, CT-EFF-005, CT-MOD-001, CT-MOD-003, CT-USE-007
impl Live {
    /// Reads the project that holds `path`, then brings its search index up to date.
    fn open(path: &Path) -> Result<Live, AppError> {
        let place = path.display().to_string();
        if !path.exists() {
            return Err(AppError::new(
                "folder not found",
                place,
                "Check the path, then open the project again.",
            ));
        }
        if !path.is_dir() {
            return Err(AppError::new(
                "not a folder",
                place,
                "Open the project folder, not a file in it.",
            ));
        }
        let workspace = Workspace::open(&find_root(path)).map_err(|e| AppError::setup(path, e))?;
        let index = Index::open(&workspace.project().root).and_then(|mut index| {
            index.update()?;
            Ok(index)
        });
        let mut live = Live {
            workspace,
            coverage: HashMap::new(),
            requirements: HashMap::new(),
            gaps: Default::default(),
            index: Mutex::new(index),
            revision: 0,
        };
        live.derive();
        Ok(live)
    }

    fn project(&self) -> &Project {
        self.workspace.project()
    }
}

// @implements CT-MOD-003, CT-EFF-004, CT-EFF-005, CT-MOD-005, CT-USE-007
impl Live {
    /// Works out the coverage lists from the current project.
    fn derive(&mut self) {
        let project = self.workspace.project();
        let found = project_coverage(project);
        self.gaps = gaps_of(&found.requirements, found.untraced);
        self.coverage = (found.requirements.into_iter())
            .map(|c| (c.requirement, c))
            .collect();
        self.requirements = (project.requirements.iter().enumerate())
            .map(|(i, r)| (r.id.clone(), i))
            .collect();
    }

    /// The stable ID the UI knows an element by.
    fn sid(&self, id: ElementId) -> u64 {
        self.workspace.stable_id(id)
    }

    /// Takes `text` as the content of the model file `path` and rebuilds the project from
    /// it and the cached syntax trees of the other files. Nothing is written.
    fn edit(&mut self, path: &str, text: &str) -> Result<String, AppError> {
        let root = self.project().root.clone();
        let result = self.workspace.edit(path, text).map(|_| ());
        // On a setup error the workspace has rebuilt the last project, which has the same
        // model, so what `derive` works out from it is the same too.
        self.derive();
        match result {
            Ok(()) => {
                self.revision += 1;
                Ok(self.edit_reply(path))
            }
            Err(e) => Err(AppError::setup(&root, e)),
        }
    }

    /// The reply to an edit: the problems found in the edited file, which the editor
    /// shows as markers, and the project counts.
    fn edit_reply(&self, path: &str) -> String {
        let p = self.project();
        let rows = (p.problems.iter().chain(&p.warnings))
            .filter_map(|text| {
                let problem = Problem::parse(text);
                let (file, line, column) = place_parts(&problem.place)?;
                (file == path).then(|| {
                    Obj::new()
                        .str("severity", problem.severity)
                        .str("what", &problem.what)
                        .str("where", &problem.place)
                        .str("help", &problem.help)
                        .num("line", line)
                        .num("column", column)
                        .end()
                })
            })
            .take(MAX_PAGE)
            .collect();
        Obj::new()
            .num("revision", self.revision)
            .raw("diagnostics", &array(rows))
            .raw("summary", &self.summary())
            .end()
    }

    /// Takes the content on disk of the `changed` paths, or of the whole project when
    /// `everything` is set, after file events. Model files with unsaved edits keep them
    /// and are listed as conflicts. Nothing is written; the search index is brought up to
    /// date by the caller.
    // @implements CT-EFF-007, CT-USE-009, CT-USE-008
    fn disk_changed(&mut self, changed: &[String], everything: bool) -> Reloaded {
        let done = if everything {
            self.workspace.reload_all()
        } else {
            self.workspace.reload(changed)
        };
        if !done.reloaded.is_empty() {
            self.derive();
            self.revision += 1;
        }
        done
    }

    /// Counts the status bar and the first view need.
    fn summary(&self) -> String {
        let p = &self.project();
        let elements = (p.model.elements.iter())
            .filter(|e| !self.is_library(e))
            .count();
        let search = lock(&self.index).is_ok();
        Obj::new()
            .str("root", &p.root.display().to_string())
            .num("elements", elements.saturating_sub(1))
            .num("requirements", p.requirements.len())
            .num("links", p.graph.links.len())
            .num("problems", p.problems.len())
            .num("warnings", p.warnings.len())
            .bool("search", search)
            .raw("gaps", &self.gap_counts())
            .end()
    }

    /// How many elements each coverage filter lists, for the badges on the filters.
    fn gap_counts(&self) -> String {
        let counts = GAP_FILTERS.iter().zip(&self.gaps);
        (counts.fold(Obj::new(), |obj, (name, list)| obj.num(name, list.len()))).end()
    }

    fn is_library(&self, e: &Element) -> bool {
        self.project().model.files[e.file] == LIBRARY_PATH
    }

    fn get(&self, id: usize) -> Result<(ElementId, &Element), AppError> {
        let found = self.workspace.element(id as u64);
        let element = found.map(|found| (found, self.project().model.element(found)));
        match element {
            Some((found, e)) if !self.is_library(e) || found == Model::ROOT => Ok((found, e)),
            _ => Err(AppError::no_element(id)),
        }
    }

    /// The members of an element that come from project files, so the tree hides the
    /// built-in library.
    fn members(&self, e: &Element) -> Vec<ElementId> {
        let model = &self.project().model;
        (e.members.iter().copied())
            .filter(|&m| !self.is_library(model.element(m)))
            .collect()
    }

    /// One page of the tree children of element `parent`; 0 is the root.
    pub fn children(&self, parent: usize, offset: usize, limit: usize) -> Result<String, AppError> {
        let (_, e) = self.get(parent)?;
        let members = self.members(e);
        let rows = (page_of(&members, offset, limit).iter())
            .map(|&m| self.tree_row(m))
            .collect();
        Ok(page(offset, Some(members.len()), rows))
    }

    fn tree_row(&self, id: ElementId) -> String {
        let model = &self.project().model;
        let e = model.element(id);
        Obj::new()
            .num("id", self.sid(id))
            .str("kind", e.kind.label())
            .opt_str("name", e.name.as_deref())
            .opt_str("shortName", e.short_name.as_deref())
            .num("children", self.members(e).len())
            .str("file", &model.files[e.file])
            .num("line", e.pos.line)
            .end()
    }

    /// Everything the detail view shows about one element, with link counts so the UI can
    /// size its link lists before asking for pages.
    pub fn element(&self, id: usize) -> Result<String, AppError> {
        let (element_id, e) = self.get(id)?;
        let model = &self.project().model;
        let graph = &self.project().graph;
        let owner = e.owner.map(|o| self.sid(o).to_string());
        Ok(Obj::new()
            .num("id", id)
            .str("kind", e.kind.label())
            .opt_str("name", e.name.as_deref())
            .opt_str("shortName", e.short_name.as_deref())
            .str("qualifiedName", &model.qualified_name(element_id))
            .str("file", &model.files[e.file])
            .num("line", e.pos.line)
            .num("column", e.pos.column)
            .raw("owner", owner.as_deref().unwrap_or("null"))
            .num("children", self.members(e).len())
            .num("linksFrom", graph.links_from(element_id).count())
            .num("linksTo", graph.links_to(element_id).count())
            .raw("requirement", &self.requirement(element_id, e))
            .end())
    }

    /// The text and coverage of a requirement usage with an ID, or `null`.
    fn requirement(&self, id: ElementId, e: &Element) -> String {
        let found = (e.short_name.as_ref())
            .and_then(|s| self.requirements.get(s))
            .map(|&i| &self.project().requirements[i]);
        let (Some(r), Some(c)) = (found, self.coverage.get(&id)) else {
            return "null".to_string();
        };
        let phase = r.phase.map(|p| p.to_string());
        Obj::new()
            .str("doc", &r.doc)
            .opt_str("verificationMethod", r.verification_method.as_deref())
            .raw("phase", phase.as_deref().unwrap_or("null"))
            .bool("hasParent", c.has_parent)
            .bool("implemented", c.implemented)
            .bool("verified", c.verified)
            .bool("waived", c.waiver.is_some())
            .end()
    }
}

// @implements CT-EFF-005, CT-USE-007
impl Live {
    /// One page of the links that start at an element (`from`: what it traces to) or end
    /// at it (`to`: what traces to it).
    pub fn links(
        &self,
        id: usize,
        direction: &str,
        offset: usize,
        limit: usize,
    ) -> Result<String, AppError> {
        let (element_id, _) = self.get(id)?;
        let graph = &self.project().graph;
        let mut links: Vec<(&Link, &End)> = match direction {
            "from" => graph.links_from(element_id).map(|l| (l, &l.to)).collect(),
            "to" => graph.links_to(element_id).map(|l| (l, &l.from)).collect(),
            other => {
                return Err(AppError::new(
                    format!("unknown link direction `{other}`"),
                    "",
                    "Ask for links `from` or `to` the element.",
                ));
            }
        };
        // Grouped by kind; the sort is stable, so each group keeps declaration order.
        links.sort_by_key(|(link, _)| link.kind.verb());
        let rows = (page_of(&links, offset, limit).iter())
            .map(|(link, other)| {
                Obj::new()
                    .str("kind", link.kind.verb())
                    .raw("other", &self.end(other))
                    .str("file", &link.path)
                    .num("line", link.line)
                    .end()
            })
            .collect();
        Ok(page(offset, Some(links.len()), rows))
    }

    // @implements CT-TRC-005, CT-EFF-005, CT-USE-007
    /// One page of every end reachable from an element by following links `upstream` (what it
    /// traces to) or `downstream` (what traces to it), nearest first. Each row carries its
    /// depth and the link that reached it.
    pub fn impact(
        &self,
        id: usize,
        direction: &str,
        offset: usize,
        limit: usize,
    ) -> Result<String, AppError> {
        let (element_id, _) = self.get(id)?;
        let toward = match direction {
            "upstream" => Toward::Upstream,
            "downstream" => Toward::Downstream,
            other => {
                return Err(AppError::new(
                    format!("unknown impact direction `{other}`"),
                    "",
                    "Ask for the impact `upstream` or `downstream` of the element.",
                ));
            }
        };
        let reached = self.project().graph.reachable(&[element_id], toward);
        let rows = (page_of(&reached, offset, limit).iter())
            .map(|r| {
                Obj::new()
                    .num("depth", r.depth)
                    .str("kind", r.link.kind.verb())
                    .raw("other", &self.end(r.end))
                    .str("file", &r.link.path)
                    .num("line", r.link.line)
                    .end()
            })
            .collect();
        Ok(page(offset, Some(reached.len()), rows))
    }

    fn end(&self, end: &End) -> String {
        match end {
            End::Element(id) => Obj::new()
                .num("id", self.sid(*id))
                .str("kind", self.project().model.element(*id).kind.label())
                .str("name", &self.project().model.qualified_name(*id))
                .end(),
            End::Code {
                path,
                start_line,
                end_line,
            } => Obj::new()
                .str("path", path)
                .num("startLine", start_line)
                .num("endLine", end_line)
                .end(),
        }
    }

    /// One page of the project's problems, errors first, then warnings.
    pub fn problems(&self, offset: usize, limit: usize) -> String {
        let p = &self.project();
        let all: Vec<&String> = p.problems.iter().chain(&p.warnings).collect();
        let rows = (page_of(&all, offset, limit).iter())
            .map(|text| {
                let problem = Problem::parse(text);
                Obj::new()
                    .str("severity", problem.severity)
                    .str("what", &problem.what)
                    .str("where", &problem.place)
                    .str("help", &problem.help)
                    .end()
            })
            .collect();
        page(offset, Some(all.len()), rows)
    }

    /// One page of full-text search hits, best first. The total is not counted, which
    /// would cost a second query; `more` says whether another page follows.
    pub fn search(&self, text: &str, offset: usize, limit: usize) -> Result<String, AppError> {
        let limit = limit.min(MAX_PAGE);
        let index = lock(&self.index);
        let index = index.as_ref().map_err(AppError::index)?;
        let mut hits = index
            .search(text, to_u32(limit + 1), to_u32(offset))
            .map_err(|e| AppError::index(&e))?;
        let more = hits.len() > limit;
        hits.truncate(limit);
        let rows = (hits.iter())
            .map(|hit| {
                let id = self.project().model.find(&hit.qualified_name);
                let id = id.map(|i| self.sid(i).to_string());
                Obj::new()
                    .raw("id", id.as_deref().unwrap_or("null"))
                    .str("kind", &hit.kind)
                    .str("qualifiedName", &hit.qualified_name)
                    .opt_str("shortName", hit.short_name.as_deref())
                    .str("file", &hit.file)
                    .num("line", hit.line)
                    .num("column", hit.column)
                    .end()
            })
            .collect();
        Ok(Obj::new()
            .num("offset", offset)
            .raw("total", "null")
            .bool("more", more)
            .raw("rows", &array(rows))
            .end())
    }
}

// @implements CT-TRC-004, CT-EFF-005, CT-EFF-006, CT-USE-007
impl Live {
    /// One page of the elements a coverage filter lists. The lists are worked out when the
    /// project opens, so a page is a slice. A requirement waived in full is left out of the
    /// implementation and verification gaps, as in `clew report coverage`.
    pub fn coverage_gaps(
        &self,
        filter: &str,
        offset: usize,
        limit: usize,
    ) -> Result<String, AppError> {
        let Some(at) = GAP_FILTERS.iter().position(|f| *f == filter) else {
            return Err(AppError::new(
                format!("unknown coverage filter `{filter}`"),
                "",
                format!("Use one of: {}.", GAP_FILTERS.join(", ")),
            ));
        };
        let list = &self.gaps[at];
        let rows = (page_of(list, offset, limit).iter())
            .map(|&id| self.gap_row(id))
            .collect();
        Ok(page(offset, Some(list.len()), rows))
    }

    fn gap_row(&self, id: ElementId) -> String {
        let model = &self.project().model;
        let e = model.element(id);
        Obj::new()
            .num("id", self.sid(id))
            .str("kind", e.kind.label())
            .opt_str("name", e.name.as_deref())
            .opt_str("shortName", e.short_name.as_deref())
            .str("qualifiedName", &model.qualified_name(id))
            .str("file", &model.files[e.file])
            .num("line", e.pos.line)
            .end()
    }

    /// The full path of a file in the project, checked. `path` is relative to the project
    /// folder. A path that leaves the folder, by `..`, an absolute path, or a link, is
    /// refused (CT-SEC-003), as is a folder or a file over `MAX_SOURCE_BYTES`.
    // @implements CT-TRC-004, CT-MOD-004, CT-SEC-003
    fn project_file(&self, path: &str) -> Result<PathBuf, AppError> {
        let root = &self.project().root;
        let inside = !path.is_empty()
            && Path::new(path)
                .components()
                .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
        if !inside {
            return Err(AppError::new(
                "that path is not inside the project",
                path,
                "Open a file by a path relative to the project folder.",
            ));
        }
        let missing = |_| {
            AppError::new(
                "file not found",
                path,
                "Check that the file still exists, then open it again.",
            )
        };
        let full = root.join(path).canonicalize().map_err(missing)?;
        if !full.starts_with(root.canonicalize().map_err(missing)?) {
            return Err(AppError::new(
                "that path is not inside the project",
                path,
                "Open a file by a path relative to the project folder.",
            ));
        }
        let meta = std::fs::metadata(&full).map_err(missing)?;
        if !meta.is_file() {
            return Err(AppError::new(
                "that path is a folder, not a file",
                path,
                "Choose a file to look at.",
            ));
        }
        if usize::try_from(meta.len()).map_or(true, |n| n > MAX_SOURCE_BYTES) {
            return Err(AppError::new(
                format!("the file is too large to show (over {MAX_SOURCE_BYTES} bytes)"),
                path,
                "Open it in an editor instead.",
            ));
        }
        Ok(full)
    }

    /// The text of a project file. Refuses what `project_file` refuses and a file that is
    /// not UTF-8 text.
    // @implements CT-TRC-004, CT-MOD-004
    fn read_text(&self, path: &str) -> Result<String, AppError> {
        let full = self.project_file(path)?;
        let bytes = std::fs::read(&full).map_err(|_| {
            AppError::new(
                "file not found",
                path,
                "Check that the file still exists, then open it again.",
            )
        })?;
        String::from_utf8(bytes).map_err(|_| {
            AppError::new(
                "the file is not text",
                path,
                "The source view shows text files only.",
            )
        })
    }

    /// The whole text of a `.sysml` file, for the editor. Files are capped at
    /// `MAX_SOURCE_BYTES`, so the web view never holds a large file.
    // @implements CT-MOD-004, CT-EFF-005
    pub fn source_text(&self, path: &str) -> Result<String, AppError> {
        let text = self.read_text(path)?;
        Ok(Obj::new().str("path", path).str("text", &text).end())
    }

    /// Saves the editor's text over an existing `.sysml` file in the project, through an
    /// atomic write (CT-USE-008). Nothing else can be written: not a new file, not another
    /// kind of file, not a path outside the project folder (CT-SEC-003).
    // @implements CT-USE-008, CT-SEC-003
    pub fn save_source(&self, path: &str, text: &str) -> Result<String, AppError> {
        let full = self.project_file(path)?;
        if full.extension().is_none_or(|e| e != "sysml") {
            return Err(AppError::new(
                "only .sysml files can be saved",
                path,
                "Edit this file in another program.",
            ));
        }
        if text.len() > MAX_SOURCE_BYTES {
            return Err(AppError::new(
                format!("the text is too large to save (over {MAX_SOURCE_BYTES} bytes)"),
                path,
                "Split the model into smaller files.",
            ));
        }
        clew_core::io::write_atomic(&full, text.as_bytes()).map_err(|e| {
            AppError::new(
                format!("the file could not be saved: {e}"),
                path,
                "Your edits are still in the editor. Check the folder is writable, then save again.",
            )
        })?;
        Ok(Obj::new().str("path", path).num("bytes", text.len()).end())
    }

    /// One page of the lines of a text file in the project, for the read-only source view.
    /// `path` is relative to the project folder.
    // @implements CT-TRC-004, CT-EFF-005
    pub fn source(&self, path: &str, offset: usize, limit: usize) -> Result<String, AppError> {
        let text = self.read_text(path)?;
        let lines: Vec<&str> = text.lines().collect();
        let first = offset.min(lines.len());
        let rows = (page_of(&lines, offset, limit).iter().enumerate())
            .map(|(i, line)| {
                Obj::new()
                    .num("line", first + i + 1)
                    .str("text", line)
                    .end()
            })
            .collect();
        Ok(page(offset, Some(lines.len()), rows))
    }
}

// @implements CT-EFF-004, CT-MOD-003
/// The open project, shared between commands. Queries read it together; an edit takes it
/// alone while it rebuilds, which is about as long as the core takes to re-parse one file.
pub struct Loaded {
    live: RwLock<Live>,
    /// The watch of the project folder, while the project is open (CT-EFF-008).
    watcher: Mutex<Option<ProjectWatcher>>,
}

// @implements CT-EFF-004, CT-EFF-005, CT-MOD-001, CT-MOD-003, CT-MOD-005, CT-USE-007
impl Loaded {
    /// Reads the project that holds `path`, then brings its search index up to date.
    pub fn open(path: &Path) -> Result<Loaded, AppError> {
        Ok(Loaded {
            live: RwLock::new(Live::open(path)?),
            watcher: Mutex::new(None),
        })
    }

    /// The project folder.
    #[cfg(test)]
    fn root(&self) -> PathBuf {
        self.read().project().root.clone()
    }

    fn read(&self) -> RwLockReadGuard<'_, Live> {
        self.live.read().unwrap_or_else(PoisonError::into_inner)
    }

    /// Takes `text` as the unsaved content of the model file `path`, a path relative to
    /// the project folder, and rebuilds the project from it. Nothing is written. Replies
    /// with the problems found in that file, for the editor's markers, and the project
    /// counts. The browser, problems, trace, and coverage queries see the new text at once.
    pub fn edit(&self, path: &str, text: &str) -> Result<String, AppError> {
        self.read().project_file(path)?;
        if text.len() > MAX_SOURCE_BYTES {
            return Err(AppError::new(
                format!("the text is too large to check (over {MAX_SOURCE_BYTES} bytes)"),
                path,
                "Split the model into smaller files.",
            ));
        }
        let mut live = self.live.write().unwrap_or_else(PoisonError::into_inner);
        live.edit(path, text)
    }

    /// Takes the saved text of the model file `path` in place of any unsaved edit, so the
    /// project matches the disk again after the user discards their changes.
    pub fn revert(&self, path: &str) -> Result<String, AppError> {
        let text = self.read().read_text(path)?;
        self.edit(path, &text)
    }

    pub fn children(&self, parent: usize, offset: usize, limit: usize) -> Result<String, AppError> {
        self.read().children(parent, offset, limit)
    }

    pub fn element(&self, id: usize) -> Result<String, AppError> {
        self.read().element(id)
    }

    pub fn links(
        &self,
        id: usize,
        direction: &str,
        offset: usize,
        limit: usize,
    ) -> Result<String, AppError> {
        self.read().links(id, direction, offset, limit)
    }

    pub fn impact(
        &self,
        id: usize,
        direction: &str,
        offset: usize,
        limit: usize,
    ) -> Result<String, AppError> {
        self.read().impact(id, direction, offset, limit)
    }

    pub fn problems(&self, offset: usize, limit: usize) -> String {
        self.read().problems(offset, limit)
    }

    pub fn search(&self, text: &str, offset: usize, limit: usize) -> Result<String, AppError> {
        self.read().search(text, offset, limit)
    }

    pub fn coverage_gaps(
        &self,
        filter: &str,
        offset: usize,
        limit: usize,
    ) -> Result<String, AppError> {
        self.read().coverage_gaps(filter, offset, limit)
    }

    pub fn source_text(&self, path: &str) -> Result<String, AppError> {
        self.read().source_text(path)
    }

    /// Saves the editor's text and notes it, so the file event the save causes is known as
    /// the app's own. The save and the note happen under one lock, so a file event handled
    /// in between cannot take the save for a change made elsewhere.
    // @implements CT-USE-008, CT-USE-009
    pub fn save_source(&self, path: &str, text: &str) -> Result<String, AppError> {
        let mut live = self.live.write().unwrap_or_else(PoisonError::into_inner);
        let reply = live.save_source(path, text)?;
        live.workspace.saved(path, text);
        Ok(reply)
    }

    /// Takes the content on disk of the `changed` project-relative paths, or of the whole
    /// project when `everything` is set, then brings the search index up to date. Only
    /// changed files are parsed and indexed again (CT-EFF-007).
    // @implements CT-EFF-007, CT-USE-009, CT-USE-008
    pub fn disk_changed(&self, changed: &[String], everything: bool) -> DiskNews {
        let mut live = self.live.write().unwrap_or_else(PoisonError::into_inner);
        let done = live.disk_changed(changed, everything);
        drop(live);
        let live = self.read();
        let models = everything || (done.reloaded.iter()).any(|p| p.ends_with(".sysml"));
        if models && let Ok(index) = lock(&live.index).as_mut() {
            // The index is a rebuildable cache; a failed update leaves search a step behind.
            let _ = index.update();
        }
        let root = live.project().root.clone();
        DiskNews {
            reloaded: done.reloaded,
            conflicts: done.conflicts,
            error: done.error.map(|e| AppError::setup(&root, e)),
            summary: live.summary(),
        }
    }

    pub fn source(&self, path: &str, offset: usize, limit: usize) -> Result<String, AppError> {
        self.read().source(path, offset, limit)
    }

    /// Counts the status bar and the first view need.
    fn summary(&self) -> String {
        self.read().summary()
    }
}

// @implements CT-MOD-003
/// The file, line, and column in a problem place such as `model/a.sysml:12:5`. A place
/// with a line and no column gets column 1; a place without a line is not in a file's text.
fn place_parts(place: &str) -> Option<(&str, usize, usize)> {
    let (rest, last) = place.rsplit_once(':')?;
    let last = last.parse().ok()?;
    match rest.rsplit_once(':') {
        Some((file, line)) => match line.parse() {
            Ok(line) => Some((file, line, last)),
            Err(_) => Some((rest, last, 1)),
        },
        None => Some((rest, last, 1)),
    }
}

// @implements CT-TRC-004
/// The four coverage lists, in the order of `GAP_FILTERS`.
fn gaps_of(requirements: &[RequirementCoverage], untraced: Vec<ElementId>) -> [Vec<ElementId>; 4] {
    let list = |test: fn(&RequirementCoverage) -> bool| -> Vec<ElementId> {
        (requirements.iter())
            .filter(|r| test(r))
            .map(|r| r.requirement)
            .collect()
    };
    [
        list(|r| !r.has_parent),
        list(|r| !r.implemented && !fully_waived(r)),
        list(|r| !r.verified && !fully_waived(r)),
        untraced,
    ]
}

// @implements CT-EFF-005
/// The rows of `all` from `offset`, at most `limit` and never more than `MAX_PAGE`.
fn page_of<T>(all: &[T], offset: usize, limit: usize) -> &[T] {
    let start = offset.min(all.len());
    let end = start + limit.min(MAX_PAGE).min(all.len() - start);
    &all[start..end]
}

// @implements CT-EFF-005
/// A page of rows: where it starts, how many rows there are in all, and whether more follow.
fn page(offset: usize, total: Option<usize>, rows: Vec<String>) -> String {
    let more = total.is_some_and(|t| offset.saturating_add(rows.len()) < t);
    let total = total.map(|t| t.to_string());
    Obj::new()
        .num("offset", offset)
        .raw("total", total.as_deref().unwrap_or("null"))
        .bool("more", more)
        .raw("rows", &array(rows))
        .end()
}

// @implements CT-EFF-005
fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

// @implements CT-EFF-004
/// Takes a lock even if a thread panicked while holding it; the data is only read.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

// @implements CT-EFF-004, CT-USE-007
/// Where opening a project has got to.
enum Status {
    Loading(PathBuf),
    Ready(Arc<Loaded>),
    Failed(AppError),
}

// @implements CT-EFF-004
#[derive(Default)]
struct Opened {
    /// `None` until a project is opened.
    status: Option<Status>,
    /// Counts calls to `open`, so a slow earlier load cannot replace a later one.
    generation: u64,
    /// What file events did since the UI last asked.
    news: Pending,
}

// @implements CT-EFF-008, CT-EFF-007, CT-USE-009
/// File event news the UI has not taken yet, merged across bursts.
#[derive(Default)]
struct Pending {
    /// Whether the open project's folder is being watched.
    watching: bool,
    reloaded: BTreeSet<String>,
    conflicts: BTreeSet<String>,
    /// Trouble and errors, as `{what, where, help}` JSON.
    problems: Vec<String>,
    /// The project counts after the latest change, or `None` when nothing was rebuilt.
    summary: Option<String>,
}

// @implements CT-EFF-008, CT-USE-009
impl Pending {
    fn is_empty(&self) -> bool {
        self.reloaded.is_empty()
            && self.conflicts.is_empty()
            && self.problems.is_empty()
            && self.summary.is_none()
    }

    /// The news as JSON for the UI, emptied so the next wait starts afresh.
    fn take(&mut self) -> String {
        let strings = |set: &mut BTreeSet<String>| {
            array(std::mem::take(set).iter().map(|p| quote(p)).collect())
        };
        Obj::new()
            .bool("watching", self.watching)
            .raw("reloaded", &strings(&mut self.reloaded))
            .raw("conflicts", &strings(&mut self.conflicts))
            .raw("problems", &array(std::mem::take(&mut self.problems)))
            .raw("summary", self.summary.take().as_deref().unwrap_or("null"))
            .end()
    }
}

// @implements CT-EFF-008, CT-USE-007
fn trouble_json(trouble: &Trouble) -> String {
    AppError::new(trouble.what.clone(), "", trouble.help.clone()).json()
}

// @implements CT-EFF-004, CT-EFF-005, CT-EFF-008
/// The app's open project. Loading runs on its own thread, so the window stays responsive,
/// and anyone waiting is woken when it ends: no polling.
#[derive(Default)]
pub struct AppState {
    opened: Mutex<Opened>,
    changed: Condvar,
}

// @implements CT-EFF-004, CT-EFF-005, CT-USE-007, CT-EFF-008
impl AppState {
    /// Starts loading the project that holds `path` on a background thread and returns at
    /// once. The status is `loading` until the thread ends. Returns the thread, or `None`
    /// when it could not start and the status is `failed`.
    pub fn open(self: &Arc<Self>, path: PathBuf) -> Option<JoinHandle<()>> {
        let generation = {
            let mut opened = lock(&self.opened);
            opened.generation += 1;
            opened.status = Some(Status::Loading(path.clone()));
            opened.news = Pending::default();
            opened.generation
        };
        // A waiting `wait_for_disk_change` hears that the project changed.
        self.changed.notify_all();
        let state = Arc::clone(self);
        let load = move || {
            let status = match Loaded::open(&path) {
                Ok(loaded) => {
                    let loaded = Arc::new(loaded);
                    state.watch(&loaded, generation);
                    Status::Ready(loaded)
                }
                Err(e) => Status::Failed(e),
            };
            let mut opened = lock(&state.opened);
            if opened.generation == generation {
                opened.status = Some(status);
            }
            drop(opened);
            state.changed.notify_all();
        };
        let spawned = std::thread::Builder::new()
            .name("clew-load".into())
            .spawn(load);
        match spawned {
            Ok(handle) => Some(handle),
            Err(e) => {
                let failed = AppError::new(
                    format!("could not start loading: {e}"),
                    "",
                    "Close other programs to free memory, then open the project again.",
                );
                lock(&self.opened).status = Some(Status::Failed(failed));
                self.changed.notify_all();
                None
            }
        }
    }

    /// The status as JSON: `none`, `loading` with the path, `ready` with the project
    /// summary, or `failed` with the error.
    pub fn status(&self) -> String {
        status_json(&lock(&self.opened))
    }

    /// Like `status`, but first waits for a load in progress to end.
    pub fn wait(&self) -> String {
        let mut opened = lock(&self.opened);
        while matches!(opened.status, Some(Status::Loading(_))) {
            opened = self
                .changed
                .wait(opened)
                .unwrap_or_else(PoisonError::into_inner);
        }
        status_json(&opened)
    }

    /// Starts watching the folder of `loaded`, the project of open number `generation`.
    /// Each burst of file events updates the project on the watcher's thread, then wakes
    /// the UI's `wait_for_disk_change`. The watcher holds the state and the project weakly,
    /// so it never keeps a closed project alive; the project owns the watcher, so it stops
    /// when the project is dropped.
    // @implements CT-EFF-008, CT-EFF-007, CT-USE-009, CT-USE-007
    fn watch(self: &Arc<Self>, loaded: &Arc<Loaded>, generation: u64) {
        let state = Arc::downgrade(self);
        let project = Arc::downgrade(loaded);
        let root = loaded.read().project().root.clone();
        let started = watch_project(&root, move |batch| {
            let (Some(state), Some(project)) = (state.upgrade(), project.upgrade()) else {
                return;
            };
            let news = (!batch.paths.is_empty() || batch.everything)
                .then(|| project.disk_changed(&batch.paths, batch.everything));
            let mut opened = lock(&state.opened);
            if opened.generation != generation {
                return;
            }
            let pending = &mut opened.news;
            pending
                .problems
                .extend(batch.trouble.iter().map(trouble_json));
            if let Some(news) = news {
                if !news.reloaded.is_empty() {
                    pending.summary = Some(news.summary);
                }
                pending.reloaded.extend(news.reloaded);
                pending.conflicts.extend(news.conflicts);
                pending.problems.extend(news.error.map(|e| e.json()));
            }
            let wake = !pending.is_empty();
            drop(opened);
            if wake {
                state.changed.notify_all();
            }
        });
        let mut opened = lock(&self.opened);
        if opened.generation != generation {
            return;
        }
        match started {
            Ok(Some(watcher)) => {
                *lock(&loaded.watcher) = Some(watcher);
                opened.news.watching = true;
            }
            Ok(None) => {}
            Err(trouble) => opened.news.problems.push(trouble_json(&trouble)),
        }
    }

    /// Waits until file events have changed the open project, then replies with what
    /// changed since the last call: `watching`, `reloaded` and `conflicts` paths,
    /// `problems`, and the new project `summary` or null. Replies at once when the project
    /// is not watched (none is open, it is loading, or the OS is Windows), and when another
    /// project opens. The wait sleeps on a condition variable: no polling (CT-EFF-008).
    // @implements CT-EFF-008, CT-USE-009
    pub fn wait_for_disk_change(&self) -> String {
        let mut opened = lock(&self.opened);
        let generation = opened.generation;
        while opened.news.watching && opened.news.is_empty() && opened.generation == generation {
            opened = (self.changed)
                .wait(opened)
                .unwrap_or_else(PoisonError::into_inner);
        }
        opened.news.take()
    }

    /// The loaded project, or why there is none.
    pub fn project(&self) -> Result<Arc<Loaded>, AppError> {
        match &lock(&self.opened).status {
            Some(Status::Ready(loaded)) => Ok(Arc::clone(loaded)),
            Some(Status::Failed(e)) => Err(e.clone()),
            Some(Status::Loading(path)) => Err(AppError::new(
                "the project is still loading",
                path.display().to_string(),
                "Wait for it to finish, then try again.",
            )),
            None => Err(AppError::new(
                "no project is open",
                "",
                "Start Clewtrace with a project folder: clewtrace <folder>.",
            )),
        }
    }
}

// @implements CT-EFF-004, CT-USE-007
fn status_json(opened: &Opened) -> String {
    let status = Obj::new();
    match &opened.status {
        None => status.str("state", "none"),
        Some(Status::Loading(path)) => status
            .str("state", "loading")
            .str("path", &path.display().to_string()),
        Some(Status::Ready(loaded)) => status
            .str("state", "ready")
            .raw("project", &loaded.summary()),
        Some(Status::Failed(e)) => status.str("state", "failed").raw("error", &e.json()),
    }
    .end()
}

// @implements CT-USE-007, CT-EFF-005
/// The reply a command sends: `{"ok": value}` or `{"error": {what, where, help}}`.
pub fn reply(result: Result<String, AppError>) -> String {
    match result {
        Ok(value) => format!("{{\"ok\":{value}}}"),
        Err(e) => format!("{{\"error\":{}}}", e.json()),
    }
}

// @implements CT-MOD-004
/// The SysML v2 reserved words as a JSON array of strings. They come from the lexer's own
/// list, so the editor's highlighting cannot drift from what the parser treats as a keyword.
pub fn editor_keywords() -> String {
    array(
        clew_core::parse::lexer::KEYWORDS
            .iter()
            .map(|k| quote(k))
            .collect(),
    )
}

// @implements CT-EFF-005
/// A JSON object written field by field, so the app needs no serialization crate.
pub struct Obj(String);

// @implements CT-EFF-005
impl Obj {
    pub fn new() -> Obj {
        Obj("{".to_string())
    }

    /// Adds `key` and a value that is already JSON text.
    pub fn raw(mut self, key: &str, json: &str) -> Obj {
        if self.0.len() > 1 {
            self.0.push(',');
        }
        self.0.push_str(&quote(key));
        self.0.push(':');
        self.0.push_str(json);
        self
    }

    pub fn str(self, key: &str, value: &str) -> Obj {
        self.raw(key, &quote(value))
    }

    pub fn opt_str(self, key: &str, value: Option<&str>) -> Obj {
        match value {
            Some(v) => self.str(key, v),
            None => self.raw(key, "null"),
        }
    }

    pub fn num(self, key: &str, value: impl Display) -> Obj {
        self.raw(key, &value.to_string())
    }

    pub fn bool(self, key: &str, value: bool) -> Obj {
        self.raw(key, if value { "true" } else { "false" })
    }

    pub fn end(mut self) -> String {
        self.0.push('}');
        self.0
    }
}

// @implements CT-EFF-005
/// A JSON array of values that are already JSON text.
pub fn array(items: Vec<String>) -> String {
    format!("[{}]", items.join(","))
}

// @verifies CT-EFF-004, CT-EFF-005, CT-USE-007
#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::generate;
    use clew_core::json::{self, Json};
    use std::fs;
    use std::sync::OnceLock;
    use std::time::{Duration, Instant};

    // @verifies CT-EFF-004, CT-EFF-005
    /// A fresh folder under the system temp folder, which has no project above it.
    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("clewtrace-app-tests").join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    // @verifies CT-EFF-004, CT-EFF-005
    /// The 205-requirement generated project, generated and loaded once for every test.
    fn generated() -> &'static (PathBuf, Arc<AppState>) {
        static LOADED: OnceLock<(PathBuf, Arc<AppState>)> = OnceLock::new();
        LOADED.get_or_init(|| {
            let root = folder("generated");
            generate(&root, 205, 1_010);
            let state = Arc::new(AppState::default());
            state.open(root.clone());
            state.wait();
            (root, state)
        })
    }

    // @verifies CT-EFF-004, CT-EFF-005
    /// The open generated project.
    fn loaded() -> Arc<Loaded> {
        generated()
            .1
            .project()
            .expect("the generated project opens")
    }

    // @verifies CT-EFF-004, CT-EFF-005, CT-USE-007
    /// Parses a reply envelope, panicking with the error when it is not `ok`.
    fn ok(reply: &str) -> Json {
        let value = json::parse(reply).unwrap_or_else(|e| panic!("{e}: {reply}"));
        match value.get("ok") {
            Some(v) => v.clone(),
            None => panic!("not ok: {reply}"),
        }
    }

    // @verifies CT-USE-007
    /// The `error` of a reply envelope.
    fn error(reply: &str) -> Json {
        let value = json::parse(reply).unwrap_or_else(|e| panic!("{e}: {reply}"));
        value
            .get("error")
            .cloned()
            .unwrap_or_else(|| panic!("ok: {reply}"))
    }

    // @verifies CT-EFF-004, CT-EFF-005
    fn num(value: &Json, key: &str) -> usize {
        match value.get(key) {
            Some(Json::Num(n)) => n.parse().unwrap(),
            other => panic!("`{key}` is {other:?}"),
        }
    }

    // @verifies CT-EFF-004, CT-EFF-005
    fn text<'a>(value: &'a Json, key: &str) -> &'a str {
        value
            .get(key)
            .and_then(Json::as_str)
            .unwrap_or_else(|| panic!("no `{key}`"))
    }

    // @verifies CT-EFF-004, CT-EFF-005
    /// The element ID of the first search hit for `query`.
    fn find(query: &str) -> usize {
        let page = ok(&reply(loaded().search(query, 0, 1)));
        num(&page.get("rows").unwrap().items()[0], "id")
    }

    // @verifies CT-EFF-004, CT-EFF-005
    /// The row named `name` in a page of tree children.
    fn package<'a>(page: &'a Json, name: &str) -> &'a Json {
        (page.get("rows").unwrap().items().iter())
            .find(|r| r.get("name").and_then(Json::as_str) == Some(name))
            .unwrap_or_else(|| panic!("no `{name}` in {page:?}"))
    }

    // @verifies CT-EFF-004, CT-MOD-001
    #[test]
    fn opens_the_generated_project_on_a_background_thread() {
        let (root, state) = generated();
        let status = json::parse(&state.status()).unwrap();
        assert_eq!(text(&status, "state"), "ready");
        let project = status.get("project").unwrap();
        assert_eq!(num(project, "requirements"), 205);
        assert_eq!(num(project, "links"), 1_010);
        assert_eq!(num(project, "problems"), 0);
        assert_eq!(num(project, "warnings"), 0);
        assert_eq!(project.get("search").and_then(Json::as_bool), Some(true));
        assert_eq!(text(project, "root"), root.display().to_string());
        assert!(root.join(".clewtrace/index.db").is_file());
    }

    // @verifies CT-EFF-004
    #[test]
    fn opening_returns_before_the_project_is_loaded() {
        let root = folder("background");
        generate(&root, 205, 1_010);
        let state = Arc::new(AppState::default());
        let started = Instant::now();
        let loader = state.open(root.join("model/reqs")).unwrap();
        let opened = started.elapsed();
        let status = json::parse(&state.status()).unwrap();
        let waited = json::parse(&state.wait()).unwrap();
        loader.join().unwrap();
        assert!(opened < Duration::from_millis(100), "open took {opened:?}");
        assert!(
            ["loading", "ready"].contains(&text(&status, "state")),
            "{status:?}"
        );
        assert_eq!(text(&waited, "state"), "ready");
        let project = waited.get("project").unwrap();
        assert_eq!(text(project, "root"), root.display().to_string());
    }

    // @verifies CT-EFF-004
    #[test]
    fn a_later_open_wins_over_an_earlier_one() {
        let (root, _) = generated();
        let missing = folder("later").join("missing");
        let state = Arc::new(AppState::default());
        let first = state.open(missing).unwrap();
        let second = state.open(root.clone()).unwrap();
        first.join().unwrap();
        second.join().unwrap();
        let status = json::parse(&state.wait()).unwrap();
        assert_eq!(text(&status, "state"), "ready", "{status:?}");
    }

    // @verifies CT-EFF-004, CT-EFF-005
    #[test]
    fn tree_children_come_in_pages() {
        let project = loaded();
        let all = ok(&reply(project.children(0, 0, 50)));
        let mut names: Vec<&str> = all
            .get("rows")
            .unwrap()
            .items()
            .iter()
            .map(|r| text(r, "name"))
            .collect();
        let third = names[2].to_string();
        names.sort();
        assert_eq!(
            names,
            ["D000", "D001", "D002", "Perf", "R000", "R001", "R002"]
        );
        assert_eq!(num(&all, "total"), 7);
        assert_eq!(all.get("more").and_then(Json::as_bool), Some(false));

        let page = ok(&reply(project.children(0, 2, 3)));
        let rows = page.get("rows").unwrap().items();
        assert_eq!((num(&page, "offset"), rows.len()), (2, 3));
        assert_eq!(text(&rows[0], "name"), third);
        assert_eq!(page.get("more").and_then(Json::as_bool), Some(true));

        let r000 = package(&all, "R000");
        assert_eq!(
            (text(r000, "kind"), num(r000, "children")),
            ("package", 100)
        );
        let reqs = ok(&reply(project.children(num(r000, "id"), 0, 10)));
        let first = &reqs.get("rows").unwrap().items()[0];
        assert_eq!(text(first, "kind"), "requirement");
        assert_eq!(text(first, "name"), "r00000");
        assert_eq!(text(first, "shortName"), "PF-00000");
        assert_eq!(text(first, "file"), "model/reqs/r000.sysml");
        assert_eq!(num(first, "line"), 2);
    }

    // @verifies CT-EFF-005
    #[test]
    fn a_page_holds_at_most_max_page_rows() {
        let project = loaded();
        let d000 = num(
            package(&ok(&reply(project.children(0, 0, 50))), "D000"),
            "id",
        );
        let page = ok(&reply(project.children(d000, 0, 100_000)));
        assert!(num(&page, "total") > MAX_PAGE);
        assert_eq!(page.get("rows").unwrap().items().len(), MAX_PAGE);
        assert_eq!(page.get("more").and_then(Json::as_bool), Some(true));
        let past = ok(&reply(project.children(d000, 100_000, 10)));
        assert!(past.get("rows").unwrap().items().is_empty());
    }

    // @verifies CT-EFF-005, CT-MOD-001
    #[test]
    fn element_detail_shows_requirement_text_and_coverage() {
        let project = loaded();
        let id = find("PF-00007");
        let detail = ok(&reply(project.element(id)));
        assert_eq!(text(&detail, "qualifiedName"), "R000::r00007");
        assert_eq!(text(&detail, "kind"), "requirement");
        assert_eq!(text(&detail, "file"), "model/reqs/r000.sysml");
        assert_eq!((num(&detail, "line"), num(&detail, "column")), (30, 5));
        assert_eq!(num(&detail, "linksFrom"), 1);
        assert!(num(&detail, "linksTo") >= 4);
        let req = detail.get("requirement").unwrap();
        assert_eq!(
            text(req, "doc"),
            "The system shall handle case 7 within its budget."
        );
        assert_eq!(num(req, "phase"), 1);
        for key in ["hasParent", "implemented", "verified"] {
            assert_eq!(req.get(key).and_then(Json::as_bool), Some(true), "{key}");
        }
        let package = ok(&reply(project.element(num(&detail, "owner"))));
        assert_eq!(text(&package, "kind"), "package");
        assert_eq!(package.get("requirement"), Some(&Json::Null));
    }

    // @verifies CT-EFF-005
    #[test]
    fn links_to_and_from_an_element_come_in_pages() {
        let project = loaded();
        // r00001 to r00006 carry a second derivation; r00007 has one.
        let child = find("PF-00007");
        let from = ok(&reply(project.links(child, "from", 0, 10)));
        assert_eq!(num(&from, "total"), 1);
        let row = &from.get("rows").unwrap().items()[0];
        assert_eq!(text(row, "kind"), "derives");
        let other = row.get("other").unwrap();
        assert_eq!(text(other, "name"), "R000::r00000");
        assert_eq!(num(other, "id"), find("PF-00000"));
        assert_eq!(text(row, "file"), "model/design/d000.sysml");

        let to = ok(&reply(project.links(child, "to", 0, 10)));
        let kinds: Vec<&str> = to
            .get("rows")
            .unwrap()
            .items()
            .iter()
            .map(|r| text(r, "kind"))
            .collect();
        for kind in ["satisfies", "verifies", "implements"] {
            assert!(kinds.contains(&kind), "{kinds:?}");
        }
        let code = (to.get("rows").unwrap().items().iter())
            .map(|r| r.get("other").unwrap())
            .find(|o| o.get("path").is_some())
            .expect("a code end");
        assert_eq!(text(code, "path"), "src/c000.rs");
        assert!(num(code, "startLine") <= num(code, "endLine"));

        let top = find("PF-00000");
        let page = ok(&reply(project.links(top, "to", 2, 3)));
        assert_eq!(page.get("rows").unwrap().items().len(), 3);
        assert!(num(&page, "total") > 5);
    }

    // @verifies CT-TRC-005
    #[test]
    fn impact_lists_everything_upstream_and_downstream() {
        let project = loaded();
        let top = find("PF-00000");
        let down = ok(&reply(project.impact(top, "downstream", 0, MAX_PAGE)));
        let rows = down.get("rows").unwrap().items();
        assert!(!rows.is_empty());
        assert_eq!(num(&down, "total"), rows.len());
        // Nearest first, and code ends are included.
        assert_eq!(num(&rows[0], "depth"), 1);
        assert!(
            rows.windows(2)
                .all(|w| num(&w[0], "depth") <= num(&w[1], "depth"))
        );
        assert!(
            rows.iter()
                .any(|r| r.get("other").unwrap().get("path").is_some())
        );
        // The start is never listed.
        assert!(rows.iter().all(|r| {
            r.get("other")
                .unwrap()
                .get("id")
                .map(|_| num(r.get("other").unwrap(), "id"))
                != Some(top)
        }));

        let child = find("PF-00007");
        let up = ok(&reply(project.impact(child, "upstream", 0, MAX_PAGE)));
        let ups = up.get("rows").unwrap().items();
        assert!(
            ups.iter()
                .any(|r| num(r.get("other").unwrap(), "id") == find("PF-00000"))
        );

        let page = ok(&reply(project.impact(top, "downstream", 1, 2)));
        assert_eq!(page.get("rows").unwrap().items().len(), 2);
        assert_eq!(num(&page, "total"), rows.len());
    }

    // @verifies CT-USE-007
    #[test]
    fn impact_in_an_unknown_direction_is_an_error() {
        let e = error(&reply(loaded().impact(find("PF-00000"), "sideways", 0, 10)));
        assert!(text(&e, "what").contains("`sideways`"), "{e:?}");
        assert!(!text(&e, "help").is_empty());
    }

    // @verifies CT-TRC-005
    #[test]
    fn links_are_grouped_by_kind() {
        let to = ok(&reply(loaded().links(find("PF-00007"), "to", 0, MAX_PAGE)));
        let kinds: Vec<&str> = (to.get("rows").unwrap().items().iter())
            .map(|r| text(r, "kind"))
            .collect();
        let mut sorted = kinds.clone();
        sorted.sort();
        assert_eq!(kinds, sorted);
    }

    // @verifies CT-USE-007
    #[test]
    fn a_direction_other_than_to_or_from_is_an_error() {
        let e = error(&reply(loaded().links(find("PF-00000"), "up", 0, 10)));
        assert!(text(&e, "what").contains("`up`"), "{e:?}");
        assert!(!text(&e, "help").is_empty());
    }

    // @verifies CT-EFF-005, CT-EFF-006
    #[test]
    fn search_hits_point_at_elements() {
        let project = loaded();
        let page = ok(&reply(project.search("budget", 0, 20)));
        let rows = page.get("rows").unwrap().items();
        assert_eq!(rows.len(), 20);
        assert_eq!(page.get("more").and_then(Json::as_bool), Some(true));
        assert_eq!(page.get("total"), Some(&Json::Null));
        let hits = ok(&reply(project.search("PF-00042", 0, 5)));
        let hit = &hits.get("rows").unwrap().items()[0];
        assert_eq!(text(hit, "qualifiedName"), "R000::r00042");
        assert_eq!(text(hit, "kind"), "requirement");
        let detail = ok(&reply(project.element(num(hit, "id"))));
        assert_eq!(text(&detail, "shortName"), "PF-00042");
        let none = ok(&reply(project.search("   ", 0, 5)));
        assert!(none.get("rows").unwrap().items().is_empty());
    }

    // @verifies CT-EFF-005
    #[test]
    fn every_command_answers_within_100_ms() {
        let project = loaded();
        let state = &generated().1;
        let id = find("PF-00100");
        let budget = Duration::from_millis(100);
        let calls: [(&str, &dyn Fn() -> String); 16] = [
            ("status", &|| state.status()),
            ("children", &|| reply(project.children(0, 0, MAX_PAGE))),
            ("element", &|| reply(project.element(id))),
            ("links from", &|| {
                reply(project.links(id, "from", 0, MAX_PAGE))
            }),
            ("links to", &|| reply(project.links(id, "to", 0, MAX_PAGE))),
            ("impact up", &|| {
                reply(project.impact(id, "upstream", 0, MAX_PAGE))
            }),
            ("impact down", &|| {
                reply(project.impact(id, "downstream", 0, MAX_PAGE))
            }),
            ("problems", &|| reply(Ok(project.problems(0, MAX_PAGE)))),
            ("search", &|| {
                reply(project.search("system budget", 0, MAX_PAGE))
            }),
            ("filter no parent", &|| {
                reply(project.coverage_gaps("no-parent", 0, MAX_PAGE))
            }),
            ("filter no implementation", &|| {
                reply(project.coverage_gaps("no-implementation", 0, MAX_PAGE))
            }),
            ("filter no verification", &|| {
                reply(project.coverage_gaps("no-verification", 0, MAX_PAGE))
            }),
            ("filter untraced", &|| {
                reply(project.coverage_gaps("untraced", 0, MAX_PAGE))
            }),
            ("source", &|| {
                reply(project.source("model/perf.sysml", 0, MAX_PAGE))
            }),
            ("editor text", &|| {
                reply(project.source_text("model/perf.sysml"))
            }),
            ("editor keywords", &|| reply(Ok(editor_keywords()))),
        ];
        for (name, call) in calls {
            let started = Instant::now();
            let out = call();
            let took = started.elapsed();
            assert!(took < budget, "{name} took {took:?}");
            json::parse(&out).unwrap_or_else(|e| panic!("{name}: {e}: {out}"));
        }
    }

    // @verifies CT-TRC-004, CT-EFF-005, CT-EFF-006
    /// A small project with known gaps: two requirements with no parent, two with no
    /// implementation or verification, and two elements traced to no requirement.
    fn gappy(name: &str) -> Loaded {
        let root = folder(name);
        let files = [
            (
                "trace.toml",
                "[project]\nrequirements = [\"reqs.sysml\"]\nid_pattern = \"X-[0-9]{3}\"\n\
                 current_phase = 1\n[coverage]\nsource_roots = [\"src\"]\nlanguages = [\"rust\"]\n\
                 file_level_allowed = []\nmax_scope_lines = 200\n\
                 [[waiver]]\nid = \"X-004\"\nreason = \"later\"\nstatus = \"approved\"\n\
                 approved_by = \"John\"\ndate = \"2026-01-01\"\n",
            ),
            (
                "reqs.sysml",
                "package Reqs {\n  requirement def Req;\n  requirement <'X-001'> top : Req;\n\
                 \x20 requirement <'X-002'> child : Req {\n    requirement <'X-003'> nested : Req;\n  }\n\
                 \x20 requirement <'X-004'> lone : Req;\n}\n",
            ),
            (
                "design.sysml",
                "package Design {\n  part def Engine {\n    satisfy Reqs::top;\n  }\n\
                 \x20 part def Tester {\n    verify Reqs::child;\n  }\n\
                 \x20 part def Loose {\n    part inner;\n  }\n\
                 \x20 #derivation dependency Reqs::child to Reqs::top;\n}\n",
            ),
            (
                "library/Clewtrace.sysml",
                include_str!("../../../library/Clewtrace.sysml"),
            ),
            ("src/lib.rs", "// @implements X-002\nfn a() {}\n"),
        ];
        for (path, text) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        Loaded::open(&root).unwrap()
    }

    // @verifies CT-TRC-004, CT-EFF-005
    /// The short names or qualified names of the rows of one coverage filter.
    fn gap_names(project: &Loaded, filter: &str) -> Vec<String> {
        let page = ok(&reply(project.coverage_gaps(filter, 0, MAX_PAGE)));
        assert_eq!(num(&page, "total"), page.get("rows").unwrap().items().len());
        (page.get("rows").unwrap().items().iter())
            .map(|r| match r.get("shortName") {
                Some(Json::Str(s)) => s.clone(),
                _ => text(r, "qualifiedName").to_string(),
            })
            .collect()
    }

    // @verifies CT-TRC-004
    #[test]
    fn coverage_filters_list_each_kind_of_gap() {
        let project = gappy("coverage_filters_list_each_kind_of_gap");
        assert_eq!(gap_names(&project, "no-parent"), ["X-001", "X-004"]);
        // X-004 is waived in full, so it is left out of these two, as in `clew report coverage`.
        assert_eq!(gap_names(&project, "no-implementation"), ["X-003"]);
        assert_eq!(gap_names(&project, "no-verification"), ["X-001", "X-003"]);
        assert_eq!(
            gap_names(&project, "untraced"),
            ["Design::Loose", "Design::Loose::inner"]
        );
    }

    // @verifies CT-TRC-004, CT-EFF-005
    #[test]
    fn coverage_rows_can_be_selected_and_come_in_pages() {
        let project = gappy("coverage_rows_can_be_selected_and_come_i");
        let page = ok(&reply(project.coverage_gaps("no-parent", 1, 1)));
        assert_eq!(num(&page, "offset"), 1);
        assert_eq!(num(&page, "total"), 2);
        let rows = page.get("rows").unwrap().items();
        assert_eq!(rows.len(), 1);
        assert_eq!(text(&rows[0], "shortName"), "X-004");
        // The row's ID opens the element.
        let detail = ok(&reply(project.element(num(&rows[0], "id"))));
        assert_eq!(text(&detail, "shortName"), "X-004");
        let none = ok(&reply(project.coverage_gaps("no-parent", 50, 10)));
        assert!(none.get("rows").unwrap().items().is_empty());
    }

    // @verifies CT-TRC-004, CT-USE-007
    #[test]
    fn an_unknown_coverage_filter_is_an_error_that_names_the_choices() {
        let e = error(&reply(
            gappy("an_unknown_coverage_filter_is_an_error_t").coverage_gaps("everything", 0, 10),
        ));
        assert!(text(&e, "what").contains("everything"), "{e:?}");
        assert!(text(&e, "help").contains("no-parent"), "{e:?}");
    }

    // @verifies CT-TRC-004
    #[test]
    fn the_summary_counts_each_gap() {
        let summary = gappy("the_summary_counts_each_gap").summary();
        let status = json::parse(&summary).unwrap();
        let gaps = status.get("gaps").expect("the summary has gap counts");
        assert_eq!(num(gaps, "no-parent"), 2);
        assert_eq!(num(gaps, "no-implementation"), 1);
        assert_eq!(num(gaps, "no-verification"), 2);
        assert_eq!(num(gaps, "untraced"), 2);
    }

    // @verifies CT-TRC-004, CT-EFF-005
    #[test]
    fn the_source_view_returns_the_lines_of_a_project_file_in_pages() {
        let project = gappy("the_source_view_returns_the_lines_of_a_p");
        let page = ok(&reply(project.source("src/lib.rs", 0, MAX_PAGE)));
        assert_eq!(num(&page, "total"), 2);
        let rows = page.get("rows").unwrap().items();
        assert_eq!(text(&rows[0], "text"), "// @implements X-002");
        assert_eq!(num(&rows[0], "line"), 1);
        assert_eq!(text(&rows[1], "text"), "fn a() {}");
        let second = ok(&reply(project.source("src/lib.rs", 1, 1)));
        assert_eq!(num(&second.get("rows").unwrap().items()[0], "line"), 2);
        // A model file opens the same way.
        assert!(project.source("reqs.sysml", 0, 1).is_ok());
    }

    // @verifies CT-TRC-004, CT-SEC-003, CT-USE-007
    #[test]
    fn the_source_view_reads_only_files_inside_the_project() {
        let project = gappy("the_source_view_reads_only_files_inside_");
        let outside = folder("outside").join("secret.txt");
        fs::write(&outside, "secret").unwrap();
        let absolute = outside.display().to_string();
        for path in [
            "../outside/secret.txt",
            "src/../../outside/secret.txt",
            absolute.as_str(),
            "src",
            "src/missing.rs",
            "",
        ] {
            let e = project.source(path, 0, 10).expect_err(path);
            assert!(!e.what.is_empty() && !e.help.is_empty(), "{path}: {e:?}");
        }
    }

    // @verifies CT-TRC-004, CT-SEC-003
    #[cfg(unix)]
    #[test]
    fn the_source_view_does_not_follow_a_link_out_of_the_project() {
        let project = gappy("the_source_view_does_not_follow_a_link_o");
        let outside = folder("outside-link").join("secret.txt");
        fs::write(&outside, "secret").unwrap();
        let link = project.root().join("src").join("link.rs");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        assert!(project.source("src/link.rs", 0, 10).is_err());
    }

    // @verifies CT-TRC-004, CT-USE-007
    #[test]
    fn the_source_view_refuses_a_file_that_is_not_text_or_is_too_large() {
        let project = gappy("the_source_view_refuses_a_file_that_is_n");
        let root = project.root().clone();
        fs::write(root.join("src/binary.bin"), [0u8, 159, 146, 150]).unwrap();
        let e = project.source("src/binary.bin", 0, 10).unwrap_err();
        assert!(e.what.contains("text"), "{e:?}");
        let big = vec![b'a'; MAX_SOURCE_BYTES + 1];
        fs::write(root.join("src/big.rs"), big).unwrap();
        let e = project.source("src/big.rs", 0, 10).unwrap_err();
        assert!(e.what.contains("large"), "{e:?}");
    }

    // @verifies CT-MOD-004, CT-EFF-005
    #[test]
    fn the_editor_opens_a_model_file_whole_and_refuses_what_the_source_view_refuses() {
        let project = gappy("the_editor_opens_a_model_file_whole");
        let root = project.root().clone();
        let opened = ok(&reply(project.source_text("reqs.sysml")));
        assert_eq!(text(&opened, "path"), "reqs.sysml");
        assert_eq!(
            text(&opened, "text"),
            fs::read_to_string(root.join("reqs.sysml")).unwrap()
        );
        for bad in ["../outside.sysml", "/etc/passwd", "", "nope.sysml"] {
            assert!(project.source_text(bad).is_err(), "{bad}");
        }
        fs::write(root.join("big.sysml"), vec![b'a'; MAX_SOURCE_BYTES + 1]).unwrap();
        assert!(project.source_text("big.sysml").is_err());
        fs::write(root.join("bin.sysml"), [0u8, 159, 146, 150]).unwrap();
        assert!(project.source_text("bin.sysml").is_err());
    }

    // @verifies CT-USE-008, CT-SEC-003
    #[test]
    fn saving_replaces_a_model_file_inside_the_project_and_nothing_else() {
        let project = gappy("saving_replaces_a_model_file_inside");
        let root = project.root().clone();
        let saved = ok(&reply(project.save_source("reqs.sysml", "package P {}\n")));
        assert_eq!(text(&saved, "path"), "reqs.sysml");
        assert_eq!(num(&saved, "bytes"), 13);
        assert_eq!(
            fs::read_to_string(root.join("reqs.sysml")).unwrap(),
            "package P {}\n"
        );
        // Only an existing `.sysml` file inside the folder can be saved.
        for bad in [
            "../escape.sysml",
            "/tmp/escape.sysml",
            "new.sysml",
            "src/lib.rs",
            "trace.toml",
            "",
        ] {
            assert!(project.save_source(bad, "x").is_err(), "{bad}");
        }
        assert!(!root.join("new.sysml").exists());
        assert_eq!(
            fs::read_to_string(root.join("src/lib.rs")).unwrap(),
            "// @implements X-002\nfn a() {}\n"
        );
        let big = "a".repeat(MAX_SOURCE_BYTES + 1);
        assert!(project.save_source("reqs.sysml", &big).is_err());
        let leftovers = (fs::read_dir(&root).unwrap())
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".clewtmp")
            })
            .count();
        assert_eq!(leftovers, 0);
    }

    // @verifies CT-USE-008, CT-SEC-003
    #[cfg(unix)]
    #[test]
    fn saving_does_not_follow_a_link_out_of_the_project() {
        let project = gappy("saving_does_not_follow_a_link_out");
        let root = project.root().clone();
        let outside = root.parent().unwrap().join("saving-outside.sysml");
        fs::write(&outside, "secret").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link.sysml")).unwrap();
        assert!(project.save_source("link.sysml", "x").is_err());
        assert_eq!(fs::read_to_string(&outside).unwrap(), "secret");
    }

    // @verifies CT-MOD-004
    #[test]
    fn the_editor_keywords_are_the_lexers_own() {
        let served = ok(&reply(Ok(editor_keywords())));
        let served: Vec<&str> = (served.items().iter())
            .map(|k| match k {
                Json::Str(s) => s.as_str(),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(served, clew_core::parse::lexer::KEYWORDS);
    }

    // @verifies CT-SEC-001, CT-EFF-005
    #[test]
    fn every_command_is_listed_for_the_build_and_granted_to_the_window() {
        let commands = include_str!("commands.rs");
        let build = include_str!("../build.rs");
        let capability = include_str!("../capabilities/main.json");
        let names: Vec<&str> = (commands.lines())
            .filter_map(|l| {
                l.strip_prefix("pub fn ")
                    .or(l.strip_prefix("pub async fn "))
            })
            .filter_map(|l| l.split('(').next())
            .collect();
        assert!(names.len() >= 12, "{names:?}");
        for name in names {
            assert!(
                build.contains(&format!("\"{name}\"")),
                "build.rs lacks {name}"
            );
            let permission = format!("\"allow-{}\"", name.replace('_', "-"));
            assert!(
                capability.contains(&permission),
                "main.json lacks {permission}"
            );
        }
    }

    // @verifies CT-USE-007
    #[test]
    fn problems_say_what_where_and_how_to_fix() {
        let root = folder("problems");
        fs::write(
            root.join("trace.toml"),
            "[project]\nrequirements = [\"m.sysml\"]\nid_pattern = \"X-[0-9]{3}\"\n\
             current_phase = 1\n[coverage]\nsource_roots = []\nlanguages = []\n\
             file_level_allowed = []\nmax_scope_lines = 200\n",
        )
        .unwrap();
        fs::write(
            root.join("m.sysml"),
            "package P {\n  part p : Missing;\n  part ;\n}\n",
        )
        .unwrap();
        let project = Loaded::open(&root).unwrap();
        let page = ok(&reply(Ok(project.problems(0, 10))));
        let rows = page.get("rows").unwrap().items();
        assert!(!rows.is_empty());
        for row in rows {
            assert_eq!(text(row, "severity"), "error");
            assert!(text(row, "where").starts_with("m.sysml:"), "{row:?}");
            assert!(!text(row, "what").is_empty(), "{row:?}");
            assert!(!text(row, "help").is_empty(), "{row:?}");
        }
        assert_eq!(num(&page, "total"), rows.len());
    }

    // @verifies CT-USE-007
    #[test]
    fn rendered_problems_split_into_what_where_and_help() {
        let p = Problem::parse("a.sysml:3:5: warning: name `x` is ambiguous\n  help: qualify it");
        assert_eq!(
            (
                p.severity,
                p.place.as_str(),
                p.what.as_str(),
                p.help.as_str()
            ),
            (
                "warning",
                "a.sysml:3:5",
                "name `x` is ambiguous",
                "qualify it"
            )
        );
        let p = Problem::parse("something odd");
        assert_eq!(
            (p.severity, p.place.as_str(), p.what.as_str()),
            ("error", "", "something odd")
        );
        assert!(!p.help.is_empty());
    }

    // @verifies CT-USE-007
    #[test]
    fn opening_errors_say_what_where_and_how_to_fix() {
        let dir = folder("errors");
        let state = Arc::new(AppState::default());
        let empty = error(&reply(state.project().map(|_| String::new())));
        assert_eq!(text(&empty, "what"), "no project is open");

        for (path, what) in [
            (dir.join("missing"), "folder not found"),
            (dir.join("trace.toml"), "not a folder"),
            (dir.clone(), "trace.toml not found"),
        ] {
            if path.ends_with("trace.toml") {
                fs::write(&path, "[project\n").unwrap();
            }
            let state = Arc::new(AppState::default());
            state.open(path.clone()).unwrap().join().unwrap();
            let status = json::parse(&state.wait()).unwrap();
            assert_eq!(text(&status, "state"), "failed");
            let e = status.get("error").unwrap();
            assert_eq!(text(e, "what"), what, "{e:?}");
            assert!(!text(e, "where").is_empty(), "{e:?}");
            assert!(!text(e, "help").is_empty(), "{e:?}");
            fs::remove_file(dir.join("trace.toml")).ok();
        }

        fs::write(dir.join("trace.toml"), "[project\n").unwrap();
        let e = Loaded::open(&dir).err().unwrap();
        assert_eq!(e.place, "trace.toml:1");
        assert!(!e.help.is_empty());

        let e = error(&reply(loaded().element(usize::MAX)));
        assert!(text(&e, "what").contains("no element"), "{e:?}");
        assert!(!text(&e, "help").is_empty());
    }

    // @verifies CT-MOD-003, CT-MOD-005
    /// The `diagnostics` rows of an edit reply.
    fn diagnostics(reply_text: &str) -> Vec<Json> {
        let ok = ok(reply_text);
        ok.get("diagnostics").unwrap().items().to_vec()
    }

    // @verifies CT-MOD-003, CT-MOD-005
    /// The first row of the model tree under `parent` whose name is `name`, as its ID.
    fn child_id(project: &Loaded, parent: usize, name: &str) -> Option<usize> {
        let page = ok(&reply(project.children(parent, 0, MAX_PAGE)));
        (page.get("rows").unwrap().items().iter())
            .find(|r| r.get("name").and_then(Json::as_str) == Some(name))
            .map(|r| num(r, "id"))
    }

    // @verifies CT-MOD-003, CT-USE-007
    #[test]
    fn an_edit_returns_the_problems_of_that_file_with_place_and_help() {
        let project = gappy("an_edit_returns_the_problems_of_that_file");
        let broken = "package Design {\n  part def Engine {\n    satisfy Reqs::nope;\n  }\n}\n";
        let rows = diagnostics(&reply(project.edit("design.sysml", broken)));
        assert!(!rows.is_empty());
        for row in &rows {
            assert!(text(row, "where").starts_with("design.sysml:3:"), "{row:?}");
            assert_eq!(num(row, "line"), 3, "{row:?}");
            assert!(num(row, "column") >= 1, "{row:?}");
            assert!(!text(row, "what").is_empty(), "{row:?}");
            assert!(!text(row, "help").is_empty(), "{row:?}");
            assert!(["error", "warning"].contains(&text(row, "severity")));
        }
        // The problems list sees the same text, so the panel can refresh from it.
        let page = ok(&reply(Ok(project.problems(0, MAX_PAGE))));
        assert!(num(&page, "total") >= rows.len());
    }

    // @verifies CT-MOD-003
    #[test]
    fn a_fixing_edit_clears_the_problems_and_the_reply_counts_them() {
        let project = gappy("a_fixing_edit_clears_the_problems");
        let broken = "package Design {\n  part def Engine {\n    satisfy Reqs::nope;\n  }\n}\n";
        let first = ok(&reply(project.edit("design.sysml", broken)));
        assert!(num(first.get("summary").unwrap(), "problems") > 0);
        let fixed = "package Design {\n  part def Engine {\n    satisfy Reqs::top;\n  }\n}\n";
        let second = ok(&reply(project.edit("design.sysml", fixed)));
        assert!(second.get("diagnostics").unwrap().items().is_empty());
        assert_eq!(num(second.get("summary").unwrap(), "problems"), 0);
        assert!(num(&second, "revision") > num(&first, "revision"));
    }

    // @verifies CT-MOD-003, CT-MOD-005
    #[test]
    fn element_ids_stay_the_same_across_an_edit_and_a_gone_element_is_an_error() {
        let project = gappy("element_ids_stay_the_same_across_an_edit");
        let design = child_id(&project, 0, "Design").unwrap();
        let engine = child_id(&project, design, "Engine").unwrap();
        let tester = child_id(&project, design, "Tester").unwrap();
        // A new part before `Engine` shifts every model index after it.
        let edited = "package Design {\n  part def Added;\n  part def Engine {\n    satisfy Reqs::top;\n  }\n\
                      \x20 part def Tester {\n    verify Reqs::child;\n  }\n}\n";
        ok(&reply(project.edit("design.sysml", edited)));
        assert_eq!(child_id(&project, design, "Engine"), Some(engine));
        assert_eq!(child_id(&project, design, "Tester"), Some(tester));
        let detail = ok(&reply(project.element(engine)));
        assert_eq!(text(&detail, "name"), "Engine");
        assert_eq!(num(&detail, "id"), engine);
        // `Tester` is deleted, so the ID it had is gone.
        let removed = "package Design {\n  part def Engine {\n    satisfy Reqs::top;\n  }\n}\n";
        ok(&reply(project.edit("design.sysml", removed)));
        let gone = error(&reply(project.element(tester)));
        assert!(text(&gone, "what").contains("no element"), "{gone:?}");
        assert_eq!(num(&ok(&reply(project.element(engine))), "id"), engine);
    }

    // @verifies CT-MOD-005
    #[test]
    fn the_tree_and_the_trace_links_follow_the_edited_text() {
        let project = gappy("the_tree_follows_the_edited_text");
        let design = child_id(&project, 0, "Design").unwrap();
        assert_eq!(child_id(&project, design, "Added"), None);
        let before = num(&ok(&reply(project.element(design))), "children");
        let edited = "package Design {\n  part def Added;\n  part def Engine {\n    satisfy Reqs::top;\n  }\n}\n";
        ok(&reply(project.edit("design.sysml", edited)));
        assert!(child_id(&project, design, "Added").is_some());
        let after = num(&ok(&reply(project.element(design))), "children");
        assert_ne!(before, after);
        // `Tester` held the only `verify` of `child`, so the gap lists see it go.
        assert!(gap_names(&project, "no-verification").contains(&"X-002".to_string()));
        let top = child_id(&project, child_id(&project, 0, "Reqs").unwrap(), "top");
        let links = ok(&reply(project.links(top.unwrap(), "to", 0, MAX_PAGE)));
        assert_eq!(num(&links, "total"), 1);
    }

    // @verifies CT-MOD-003, CT-SEC-003
    #[test]
    fn an_edit_is_never_written_and_only_model_files_are_taken() {
        let project = gappy("an_edit_is_never_written");
        let root = std::env::temp_dir()
            .join("clewtrace-app-tests")
            .join("an_edit_is_never_written");
        let before = fs::read_to_string(root.join("design.sysml")).unwrap();
        ok(&reply(project.edit("design.sysml", "package Design {}\n")));
        assert_eq!(
            fs::read_to_string(root.join("design.sysml")).unwrap(),
            before
        );
        for bad in ["src/lib.rs", "trace.toml", "../x.sysml", "/etc/x.sysml"] {
            let e = error(&reply(project.edit(bad, "x")));
            assert!(!text(&e, "what").is_empty(), "{bad}");
            assert!(!text(&e, "help").is_empty(), "{bad}");
        }
    }

    // @verifies CT-MOD-003
    #[test]
    fn reverting_a_file_takes_the_saved_text_again() {
        let project = gappy("reverting_a_file");
        ok(&reply(project.edit(
            "design.sysml",
            "package Design { part def Gone; }\n",
        )));
        let design = child_id(&project, 0, "Design").unwrap();
        assert!(child_id(&project, design, "Gone").is_some());
        ok(&reply(project.revert("design.sysml")));
        assert!(child_id(&project, design, "Gone").is_none());
        assert!(child_id(&project, design, "Engine").is_some());
    }

    // @verifies CT-MOD-003, CT-EFF-005
    #[test]
    fn an_edit_in_a_project_of_two_hundred_requirements_answers_within_one_second() {
        let project = loaded();
        let text = fs::read_to_string(generated().0.join("model/perf.sysml")).unwrap();
        let started = Instant::now();
        let rows = diagnostics(&reply(project.edit("model/perf.sysml", &text)));
        let took = started.elapsed();
        assert!(took < Duration::from_secs(1), "an edit took {took:?}");
        assert!(rows.is_empty(), "{rows:?}");
    }

    // @verifies CT-EFF-005
    #[test]
    fn json_text_is_escaped() {
        let out = Obj::new()
            .str("a\"b", "line\nbreak")
            .num("n", 3)
            .opt_str("none", None)
            .end();
        assert_eq!(out, r#"{"a\"b":"line\nbreak","n":3,"none":null}"#);
        assert_eq!(array(Vec::new()), "[]");
    }

    // @verifies CT-EFF-007, CT-USE-009
    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    // @verifies CT-USE-009, CT-USE-008
    #[test]
    fn the_apps_own_save_is_not_reported_back_as_a_disk_change() {
        let project = gappy("own_save_is_not_news");
        project.save_source("reqs.sysml", "package Reqs;\n").ok();
        let text = fs::read_to_string(project.root().join("design.sysml")).unwrap();
        project
            .edit("design.sysml", &format!("{text}// more\n"))
            .unwrap();
        project
            .save_source("design.sysml", &format!("{text}// more\n"))
            .unwrap();
        let news = project.disk_changed(&paths(&["design.sysml"]), false);
        assert!(news.reloaded.is_empty(), "{:?}", news.reloaded);
        assert!(news.conflicts.is_empty(), "{:?}", news.conflicts);
    }

    // @verifies CT-EFF-007, CT-USE-009
    #[test]
    fn a_file_changed_on_disk_reaches_the_queries_and_the_search_index() {
        let project = gappy("disk_change_reaches_queries");
        let root = project.root();
        let text = fs::read_to_string(root.join("design.sysml")).unwrap();
        let text = text.replace("part def Loose {", "part def Gearbox;\n  part def Loose {");
        fs::write(root.join("design.sysml"), text).unwrap();
        let news = project.disk_changed(&paths(&["design.sysml"]), false);
        assert_eq!(news.reloaded, ["design.sysml"]);
        assert!(news.error.is_none(), "{:?}", news.error);
        let hits = ok(&reply(project.search("Gearbox", 0, 10)));
        assert_eq!(hits.get("rows").unwrap().items().len(), 1, "{hits:?}");
        assert!(
            project
                .read()
                .project()
                .model
                .find("Design::Gearbox")
                .is_some()
        );
    }

    // @verifies CT-USE-009, CT-USE-008
    #[test]
    fn a_file_with_unsaved_edits_that_changed_on_disk_is_a_conflict() {
        let project = gappy("disk_change_conflict");
        let root = project.root();
        let text = fs::read_to_string(root.join("design.sysml")).unwrap();
        project
            .edit("design.sysml", &format!("{text}package Mine;\n"))
            .unwrap();
        fs::write(
            root.join("design.sysml"),
            format!("{text}package Theirs;\n"),
        )
        .unwrap();
        let news = project.disk_changed(&paths(&["design.sysml"]), false);
        assert_eq!(news.conflicts, ["design.sysml"]);
        assert!(news.reloaded.is_empty());
        assert!(project.read().project().model.find("Mine").is_some());
    }

    // @verifies CT-EFF-008, CT-USE-009
    #[test]
    fn without_a_watched_project_the_disk_wait_returns_at_once() {
        let state = AppState::default();
        let news = json::parse(&state.wait_for_disk_change()).unwrap();
        assert_eq!(news.get("watching").and_then(Json::as_bool), Some(false));
    }

    // @verifies CT-EFF-008, CT-EFF-007, CT-USE-009
    #[cfg(not(windows))]
    #[test]
    fn an_open_project_is_watched_and_the_ui_hears_of_changes() {
        let root = gappy("watched").root();
        let state = Arc::new(AppState::default());
        state.open(root.clone()).unwrap().join().unwrap();
        let text = fs::read_to_string(root.join("design.sysml")).unwrap();
        fs::write(root.join("design.sysml"), format!("{text}package Later;\n")).unwrap();
        let mut reloaded = Vec::new();
        let started = Instant::now();
        while !reloaded.iter().any(|p| p == "design.sysml") {
            assert!(started.elapsed() < Duration::from_secs(10), "{reloaded:?}");
            let news = json::parse(&state.wait_for_disk_change()).unwrap();
            assert_eq!(news.get("watching").and_then(Json::as_bool), Some(true));
            let rows = news.get("reloaded").unwrap().items();
            reloaded.extend(rows.iter().filter_map(|r| r.as_str().map(String::from)));
        }
        let loaded = state.project().unwrap();
        assert!(loaded.read().project().model.find("Later").is_some());
    }
}
