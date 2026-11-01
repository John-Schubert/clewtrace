//! The open project the app serves to the UI, in pages of rows (ADR-0004).
//!
//! The Rust side owns the project: its model, trace graph, coverage, and search index. The
//! UI never holds a whole table; each query returns one page of at most `MAX_PAGE` rows as
//! JSON text. Element IDs are indexes into the loaded model and stay valid until the
//! project is opened again. Nothing here depends on Tauri, so tests call it directly.

use std::collections::HashMap;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;

use clew_core::check::{SetupError, TRACE_CONFIG};
use clew_core::index::{Index, IndexError};
use clew_core::json::quote;
use clew_core::model::{Element, ElementId, LIBRARY_PATH, Model};
use clew_core::report::{Project, RequirementCoverage, project_coverage};
use clew_core::trace::{End, Link, Toward};

// @implements CT-EFF-005, CT-EFF-004
/// The most rows one page holds. Lists longer than this are virtualized in the UI, which
/// asks for the page it shows.
pub const MAX_PAGE: usize = 200;

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
            "Element IDs change when a project is opened again. Reload the view.",
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

// @implements CT-EFF-004, CT-MOD-001
/// A project read from its `.sysml` text and code, with its coverage and search index.
pub struct Loaded {
    project: Project,
    /// Coverage of each requirement usage that has an ID.
    coverage: HashMap<ElementId, RequirementCoverage>,
    /// Indexes into `Project::requirements` by requirement ID.
    requirements: HashMap<String, usize>,
    /// The search index, or why it could not be opened. SQLite connections are not
    /// shared between threads, so queries take turns.
    index: Mutex<Result<Index, IndexError>>,
}

// @implements CT-EFF-004, CT-EFF-005, CT-MOD-001, CT-USE-007
impl Loaded {
    /// Reads the project that holds `path`, then brings its search index up to date.
    pub fn open(path: &Path) -> Result<Loaded, AppError> {
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
        let project = Project::open(path).map_err(|e| AppError::setup(path, e))?;
        let coverage = (project_coverage(&project).requirements.into_iter())
            .map(|c| (c.requirement, c))
            .collect();
        let requirements = (project.requirements.iter().enumerate())
            .map(|(i, r)| (r.id.clone(), i))
            .collect();
        let index = Index::open(&project.root).and_then(|mut index| {
            index.update()?;
            Ok(index)
        });
        Ok(Loaded {
            project,
            coverage,
            requirements,
            index: Mutex::new(index),
        })
    }

    /// Counts the status bar and the first view need.
    fn summary(&self) -> String {
        let p = &self.project;
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
            .end()
    }

    fn is_library(&self, e: &Element) -> bool {
        self.project.model.files[e.file] == LIBRARY_PATH
    }

    fn get(&self, id: usize) -> Result<(ElementId, &Element), AppError> {
        let elements = &self.project.model.elements;
        match elements.get(id) {
            Some(e) if !self.is_library(e) || id == Model::ROOT.0 => Ok((ElementId(id), e)),
            _ => Err(AppError::no_element(id)),
        }
    }

    /// The members of an element that come from project files, so the tree hides the
    /// built-in library.
    fn members(&self, e: &Element) -> Vec<ElementId> {
        let model = &self.project.model;
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
        let model = &self.project.model;
        let e = model.element(id);
        Obj::new()
            .num("id", id.0)
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
        let model = &self.project.model;
        let graph = &self.project.graph;
        let owner = e.owner.map(|o| o.0.to_string());
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
            .map(|&i| &self.project.requirements[i]);
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
impl Loaded {
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
        let graph = &self.project.graph;
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
        let reached = self.project.graph.reachable(&[element_id], toward);
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
                .num("id", id.0)
                .str("kind", self.project.model.element(*id).kind.label())
                .str("name", &self.project.model.qualified_name(*id))
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
        let p = &self.project;
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
                let id = self.project.model.find(&hit.qualified_name);
                let id = id.map(|i| i.0.to_string());
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
            opened.generation
        };
        let state = Arc::clone(self);
        let load = move || {
            let status = match Loaded::open(&path) {
                Ok(loaded) => Status::Ready(Arc::new(loaded)),
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
        let calls: [(&str, &dyn Fn() -> String); 9] = [
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
        ];
        for (name, call) in calls {
            let started = Instant::now();
            let out = call();
            let took = started.elapsed();
            assert!(took < budget, "{name} took {took:?}");
            json::parse(&out).unwrap_or_else(|e| panic!("{name}: {e}: {out}"));
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
}
