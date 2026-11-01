//! Live validation: rebuilding a project as the user edits one file, without reading or
//! parsing the others again (ADR-0005).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::check::{self, SetupError};
use crate::config::TraceConfig;
use crate::index;
use crate::model::{ElementId, Model};
use crate::parse::ast::Member;
use crate::parse::parser;
use crate::report::{self, Checklist, Document, Project};
use crate::reproduce;
use crate::reqs;
use crate::scan::{self, FileScan};
use crate::trace::TraceGraph;

// @implements CT-EFF-007, CT-MOD-003
/// One model file as last read from disk or taken from the editor.
struct ModelFile {
    /// Project-relative, with `/` separators.
    path: String,
    /// The syntax tree, or `None` when the file could not be read.
    members: Option<Vec<Member>>,
    /// Syntax errors rendered with path, line, and column, or why the file is unreadable.
    problems: Vec<String>,
    /// The SHA-256 of the text, from `reproduce::text_sha256`. Empty when unreadable.
    sha256: Vec<u8>,
}

// @implements CT-EFF-007, CT-MOD-003, CT-TRC-004, CT-TRC-009
/// Everything a project is built from, each file read and parsed or scanned once:
/// `trace.toml`, the model files' syntax trees, the code scans, the checklists, and the
/// documents. `project` builds the model and trace graph from them, so a change to one
/// model file re-parses that file alone.
pub(crate) struct Sources {
    config: TraceConfig,
    config_sha256: Option<Vec<u8>>,
    /// Model files in path order, as `index::sysml_files` lists them.
    files: Vec<ModelFile>,
    /// Code files with their scan and the SHA-256 of their text.
    scans: Vec<(String, FileScan, Vec<u8>)>,
    checklists: Vec<Checklist>,
    documents: Vec<Document>,
    /// What reading the checklists and documents found unreadable, and what it hashed.
    other_problems: Vec<String>,
    other_inputs: Vec<(String, Vec<u8>)>,
    /// Model files parsed so far.
    parses: usize,
}

// @implements CT-EFF-007, CT-MOD-003, CT-TRC-004, CT-TRC-009, CT-USE-007
impl Sources {
    /// Reads every input of the project under `root`. Code files that `clew check --self`
    /// would report as excluded, untraceable, or unreadable are not scanned.
    pub(crate) fn read(root: &Path) -> Result<Sources, SetupError> {
        let config = check::read_config(root)?;
        let config_sha256 = index::read_file(&root.join(check::TRACE_CONFIG))
            .ok()
            .map(|loaded| reproduce::text_sha256(&loaded.text));
        let paths = index::sysml_files(root).map_err(|e| SetupError {
            path: String::new(),
            line: 0,
            message: e.to_string(),
        })?;
        let mut sources = Sources {
            config,
            config_sha256,
            files: Vec::with_capacity(paths.len()),
            scans: Vec::new(),
            checklists: Vec::new(),
            documents: Vec::new(),
            other_problems: Vec::new(),
            other_inputs: Vec::new(),
            parses: 0,
        };
        for path in paths {
            let file = match index::read_file(&root.join(&path)) {
                Ok(loaded) => sources.parse(path, &loaded.text),
                Err(message) => ModelFile {
                    problems: vec![index::unreadable(&path, &message)],
                    path,
                    members: None,
                    sha256: Vec::new(),
                },
            };
            sources.files.push(file);
        }
        let config = &sources.config;
        for path in check::source_files(root, config)? {
            let excluded = (config.excludes.iter()).any(|e| check::glob_matches(&e.path, &path));
            let syntax = check::syntax_for(config, &path).filter(|_| !excluded);
            let loaded = syntax.and_then(|_| index::read_file(&root.join(&path)).ok());
            if let (Some(syntax), Some(loaded)) = (syntax, loaded) {
                let sha256 = reproduce::text_sha256(&loaded.text);
                sources
                    .scans
                    .push((path, scan::scan_source(syntax, &loaded.text), sha256));
            }
        }
        let (problems, inputs) = (&mut sources.other_problems, &mut sources.other_inputs);
        sources.checklists = report::read_checklists(root, problems, inputs);
        sources.documents = report::read_documents(root, problems, inputs);
        Ok(sources)
    }

    /// Parses the text of the model file at `path`.
    fn parse(&mut self, path: String, text: &str) -> ModelFile {
        self.parses += 1;
        let parsed = parser::parse(text);
        ModelFile {
            problems: parsed.errors.iter().map(|e| e.render(&path)).collect(),
            path,
            members: Some(parsed.members),
            sha256: reproduce::text_sha256(text),
        }
    }

    /// Puts `file` in place of the model file with its path, or adds it in path order.
    /// Returns the file it replaced.
    fn put(&mut self, file: ModelFile) -> Option<ModelFile> {
        match self.files.binary_search_by(|f| f.path.cmp(&file.path)) {
            Ok(at) => Some(std::mem::replace(&mut self.files[at], file)),
            Err(at) => {
                self.files.insert(at, file);
                None
            }
        }
    }

    /// Takes the model file at `path` out.
    fn remove(&mut self, path: &str) -> Option<ModelFile> {
        let at = self.files.binary_search_by(|f| f.path.as_str().cmp(path));
        at.ok().map(|at| self.files.remove(at))
    }

    /// Builds the model, trace graph, and problem list from the cached inputs.
    pub(crate) fn project(&self, root: &Path) -> Result<Project, SetupError> {
        let mut problems = Vec::new();
        let mut inputs = Vec::new();
        if let Some(sha256) = &self.config_sha256 {
            inputs.push((check::TRACE_CONFIG.to_string(), sha256.clone()));
        }
        let mut parsed: Vec<(&str, &[Member])> = Vec::with_capacity(self.files.len());
        for file in &self.files {
            problems.extend(file.problems.iter().cloned());
            if let Some(members) = &file.members {
                parsed.push((&file.path, members));
                inputs.push((file.path.clone(), file.sha256.clone()));
            }
        }
        let defaults = reqs::PhaseDefaults::new(parsed.iter().map(|(_, m)| *m));
        let requirements = (parsed.iter())
            .flat_map(|(_, members)| reqs::requirements_with_defaults(members, &defaults))
            .collect();
        let model = Model::build(&parsed);
        let (warnings, errors): (Vec<_>, Vec<_>) = model.errors.iter().partition(|e| e.warning);
        problems.extend(errors.iter().map(ToString::to_string));
        let warnings = warnings.iter().map(ToString::to_string).collect();
        let top_level = report::top_level_definitions(&model, &self.config)?;
        let code: Vec<(&str, &FileScan)> = (self.scans.iter())
            .map(|(p, s, _)| (p.as_str(), s))
            .collect();
        let graph = TraceGraph::build(&model, &code);
        problems.extend(graph.problems.iter().map(ToString::to_string));
        problems.extend(self.other_problems.iter().cloned());
        inputs.extend((self.scans.iter()).map(|(p, _, sha256)| (p.clone(), sha256.clone())));
        inputs.extend(self.other_inputs.iter().cloned());
        Ok(Project {
            root: root.to_path_buf(),
            model,
            graph,
            problems,
            warnings,
            inputs,
            top_level,
            requirements,
            checklists: self.checklists.clone(),
            documents: self.documents.clone(),
            waivers: self.config.waivers.clone(),
        })
    }
}

// @implements CT-MOD-003
/// What makes an element the same one across a rebuild, apart from its model index.
#[derive(Debug, PartialEq, Eq, Hash)]
enum Key {
    /// A model file, whose top-level members are keyed under it.
    File(String),
    /// An element: its owner's stable ID, its kind, its name (or short name), and how many
    /// earlier members of the same owner share that kind and name.
    Member {
        owner: u64,
        kind: &'static str,
        name: Option<String>,
        ordinal: usize,
    },
}

// @implements CT-MOD-003
/// Stable element IDs. An element keeps its ID across a rebuild while it keeps its file,
/// kind, name, and place among same-named siblings, and so do all its owners. The root
/// is always 0.
#[derive(Default)]
struct StableIds {
    keys: HashMap<Key, u64>,
    /// The stable ID of each element, by model index.
    of: Vec<u64>,
    /// The model index of each stable ID.
    element: HashMap<u64, ElementId>,
    /// The last stable ID handed out.
    last: u64,
}

// @implements CT-MOD-003
impl StableIds {
    /// Gives every element of `model` its stable ID, reusing the ID of the same element
    /// in the model given last time. IDs of elements that are gone are forgotten.
    fn assign(&mut self, model: &Model) {
        let old = std::mem::take(&mut self.keys);
        let count = model.elements.len();
        let mut keys = HashMap::with_capacity(count + model.files.len());
        let mut ordinals: HashMap<(u64, &'static str, Option<&str>), usize> = HashMap::new();
        let mut of = vec![0; count];
        let last = &mut self.last;
        let mut take = |key: Key| {
            let id = old.get(&key).copied().unwrap_or_else(|| {
                *last += 1;
                *last
            });
            keys.insert(key, id);
            id
        };
        // Owners come before their members in `Model::elements`.
        for (index, element) in model.elements.iter().enumerate().skip(1) {
            let owner = element.owner.unwrap_or(Model::ROOT);
            let owner = if owner == Model::ROOT {
                take(Key::File(model.files[element.file].clone()))
            } else {
                of[owner.0]
            };
            let kind = element.kind.label();
            let name = element.name.as_deref().or(element.short_name.as_deref());
            let ordinal = ordinals.entry((owner, kind, name)).or_default();
            let key = Key::Member {
                owner,
                kind,
                name: name.map(str::to_string),
                ordinal: *ordinal,
            };
            *ordinal += 1;
            of[index] = take(key);
        }
        self.element = (of.iter().enumerate())
            .map(|(i, &s)| (s, ElementId(i)))
            .collect();
        self.of = of;
        self.keys = keys;
    }
}

// @implements CT-MOD-003, CT-EFF-007
/// An open project that takes the unsaved text of one model file at a time and rebuilds,
/// re-parsing that file only. The other files' syntax trees, the code scans, the
/// checklists, and the documents are kept from `open`.
pub struct Workspace {
    root: PathBuf,
    sources: Sources,
    /// The last project built. `None` only while `edit` rebuilds, so the old and new
    /// project are never held together (CT-EFF-004).
    project: Option<Project>,
    ids: StableIds,
}

// @implements CT-MOD-003, CT-EFF-007, CT-USE-007
impl Workspace {
    /// Reads the project at `root`, which must be the project root, keeping each model
    /// file's syntax tree.
    pub fn open(root: &Path) -> Result<Workspace, SetupError> {
        let sources = Sources::read(root)?;
        let project = sources.project(root)?;
        let mut ids = StableIds::default();
        ids.assign(&project.model);
        Ok(Workspace {
            root: root.to_path_buf(),
            sources,
            project: Some(project),
            ids,
        })
    }

    /// The project as last built.
    pub fn project(&self) -> &Project {
        (self.project.as_ref()).expect("`edit` always leaves a project behind")
    }

    /// The number of model files parsed since `open`, counting those `open` parsed.
    pub fn parses(&self) -> usize {
        self.sources.parses
    }

    /// Takes `text` as the content of the model file at `path`, a project-relative path
    /// with `/` separators, and rebuilds the project. The file need not be saved, or exist
    /// yet. Only this file is parsed; the model, trace graph, and problems are rebuilt from
    /// the cached syntax trees of the others.
    ///
    /// The old project is dropped before the new one is built, so only one is held at a
    /// time. When the rebuilt model breaks the setup, such as a `top_level_definitions`
    /// entry in `trace.toml` that no longer resolves, the error is returned and the last
    /// project is rebuilt from the file's previous syntax tree, with no parse. The cache
    /// keeps that previous tree, so it always holds the inputs of the current project, and
    /// the editor sends the file's text again on its next edit.
    pub fn edit(&mut self, path: &str, text: &str) -> Result<&Project, SetupError> {
        if !path.ends_with(".sysml") {
            return Err(SetupError {
                path: path.to_string(),
                line: 0,
                message: "is not a model file\n  help: live validation takes `.sysml` files \
                          only; save other files to see their effect"
                    .to_string(),
            });
        }
        let file = self.sources.parse(path.to_string(), text);
        let previous = self.sources.put(file);
        self.project = None;
        let error = match self.sources.project(&self.root) {
            Ok(project) => {
                self.ids.assign(&project.model);
                return Ok(self.project.insert(project));
            }
            Err(error) => error,
        };
        match previous {
            Some(previous) => self.sources.put(previous),
            None => self.sources.remove(path),
        };
        // The cache now holds the inputs that built the last project, so this build
        // succeeds and gives the same model, and the stable IDs still match it.
        self.project = Some(self.sources.project(&self.root)?);
        Err(error)
    }

    /// The stable ID of an element of the current model. It stays the same across edits
    /// while the element keeps its file, kind, name, and place among same-named siblings,
    /// and so do its owners.
    pub fn stable_id(&self, id: ElementId) -> u64 {
        self.ids.of[id.0]
    }

    /// The element of the current model with the stable ID `stable`, or `None` when that
    /// element is gone.
    pub fn element(&self, stable: u64) -> Option<ElementId> {
        self.ids.element.get(&stable).copied()
    }
}

// @verifies CT-MOD-003, CT-EFF-007
#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::load_project;
    use std::fs;
    use std::path::PathBuf;

    // @verifies CT-MOD-003
    const REQS: &str = "package Reqs {\n    requirement def Top;\n    \
                        requirement <'X-001'> a : Top;\n    requirement <'X-002'> b : Top;\n}\n";
    // @verifies CT-MOD-003
    const DESIGN: &str = "package Design {\n    part engine {\n        satisfy Reqs::a;\n    \
                          }\n    part wheel;\n    satisfy Reqs::b by wheel;\n}\n";

    // @verifies CT-MOD-003, CT-EFF-007
    /// A small project with two model files and one code file, under a fresh folder.
    fn project(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join("clew-live-tests").join(name);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("model")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        let config = "[project]\nrequirements = [\"model/reqs.sysml\"]\n\
                      id_pattern = \"X-[0-9]{3}\"\ncurrent_phase = 1\n\
                      [coverage]\nsource_roots = [\"src\"]\nlanguages = [\"rust\"]\n\
                      file_level_allowed = []\nmax_scope_lines = 200\n\
                      top_level_definitions = [\"Reqs::Top\"]\n";
        fs::write(root.join("trace.toml"), config).unwrap();
        fs::write(root.join("model/reqs.sysml"), REQS).unwrap();
        fs::write(root.join("model/design.sysml"), DESIGN).unwrap();
        fs::write(
            root.join("src/lib.rs"),
            "// @implements X-001\nfn a() {}\n// @verifies X-002\n#[test]\nfn b() {}\n",
        )
        .unwrap();
        root
    }

    // @verifies CT-MOD-003
    /// The element with qualified name `name` in the workspace's current model.
    fn find(ws: &Workspace, name: &str) -> ElementId {
        ws.project().model.find(name).unwrap()
    }

    // @verifies CT-MOD-003, CT-EFF-007
    #[test]
    fn opening_builds_the_same_project_as_loading_it() {
        let root = project("same");
        let ws = Workspace::open(&root).unwrap();
        let loaded = load_project(&root).unwrap();
        let p = ws.project();
        assert!(p.problems.is_empty(), "{:?}", p.problems);
        assert_eq!(p.model.files, loaded.model.files);
        assert_eq!(p.model.elements.len(), loaded.model.elements.len());
        assert_eq!(p.graph.links, loaded.graph.links);
        assert_eq!(p.inputs, loaded.inputs);
        assert_eq!(p.top_level, loaded.top_level);
        assert_eq!(p.requirements, loaded.requirements);
        assert_eq!(ws.parses(), 2);
    }

    // @verifies CT-EFF-007
    #[test]
    fn an_edit_reparses_only_the_edited_file() {
        let root = project("only-one");
        let mut ws = Workspace::open(&root).unwrap();
        ws.edit("model/design.sysml", &DESIGN.replace("wheel;", "tyre;"))
            .unwrap();
        assert_eq!(ws.parses(), 3);
        ws.edit("model/reqs.sysml", REQS).unwrap();
        assert_eq!(ws.parses(), 4);
    }

    // @verifies CT-MOD-003, CT-EFF-007
    #[test]
    fn an_edit_uses_the_new_text_without_saving_it() {
        let root = project("unsaved");
        let mut ws = Workspace::open(&root).unwrap();
        let text = DESIGN.replace("part wheel;", "part wheel;\n    part axle;");
        let p = ws.edit("model/design.sysml", &text).unwrap();
        assert!(p.model.find("Design::axle").is_some());
        assert!(p.problems.is_empty(), "{:?}", p.problems);
        let on_disk = fs::read_to_string(root.join("model/design.sysml")).unwrap();
        assert_eq!(on_disk, DESIGN);
        let sha = crate::reproduce::text_sha256(&text);
        assert!(p.inputs.contains(&("model/design.sysml".to_string(), sha)));
    }

    // @verifies CT-MOD-003
    #[test]
    fn a_syntax_error_is_reported_with_file_line_and_column() {
        let root = project("syntax");
        let mut ws = Workspace::open(&root).unwrap();
        let p = ws
            .edit(
                "model/design.sysml",
                &DESIGN.replace("part wheel;", "part wheel"),
            )
            .unwrap();
        assert!(
            p.problems
                .iter()
                .any(|e| e.starts_with("model/design.sysml:6:")),
            "{:?}",
            p.problems
        );
        assert!(
            p.problems.iter().all(|e| e.contains("help:")),
            "{:?}",
            p.problems
        );
        let p = ws.edit("model/design.sysml", DESIGN).unwrap();
        assert!(p.problems.is_empty(), "{:?}", p.problems);
    }

    // @verifies CT-MOD-003
    #[test]
    fn a_broken_reference_in_another_file_is_reported() {
        let root = project("reference");
        let mut ws = Workspace::open(&root).unwrap();
        let p = ws
            .edit("model/reqs.sysml", &REQS.replace("> a :", "> alpha :"))
            .unwrap();
        assert!(
            p.problems
                .iter()
                .any(|e| e.starts_with("model/design.sysml:3:17:") && e.contains("Reqs::a")),
            "{:?}",
            p.problems
        );
        assert!(p.graph.links.len() < load_project(&root).unwrap().graph.links.len());
    }

    // @verifies CT-MOD-003, CT-EFF-007
    #[test]
    fn a_new_model_file_joins_the_project() {
        let root = project("new-file");
        let mut ws = Workspace::open(&root).unwrap();
        let p = ws
            .edit("model/extra.sysml", "package Extra { part x; }\n")
            .unwrap();
        assert!(p.model.find("Extra::x").is_some());
        assert!(p.model.files.iter().any(|f| f == "model/extra.sysml"));
        let err = ws.edit("src/lib.rs", "fn a() {}\n").err().unwrap();
        assert!(err.to_string().contains(".sysml"), "{err}");
    }

    // @verifies CT-MOD-003
    #[test]
    fn a_broken_top_level_definition_keeps_the_last_project() {
        let root = project("top-level");
        let mut ws = Workspace::open(&root).unwrap();
        let a = ws.stable_id(find(&ws, "Reqs::a"));
        let links = ws.project().graph.links.clone();
        let err = ws
            .edit("model/reqs.sysml", &REQS.replace("def Top", "def Topp"))
            .err()
            .unwrap();
        assert!(err.to_string().contains("Reqs::Top"), "{err}");
        // The last good project is rebuilt from the cached tree, not parsed again.
        assert_eq!(ws.parses(), 3);
        assert!(ws.project().model.find("Reqs::Top").is_some());
        assert!(
            ws.project().problems.is_empty(),
            "{:?}",
            ws.project().problems
        );
        assert_eq!(ws.project().graph.links, links);
        assert_eq!(ws.element(a), Some(find(&ws, "Reqs::a")));
        // The broken text is dropped, so an edit to another file builds on the last good
        // version of this one.
        let design = DESIGN.replace("part wheel;", "part wheel;\n    part axle;");
        let p = ws.edit("model/design.sysml", &design).unwrap();
        assert!(p.model.find("Reqs::Top").is_some());
        assert!(p.model.find("Design::axle").is_some());
        let p = ws
            .edit("model/reqs.sysml", &REQS.replace("> b :", "> beta :"))
            .unwrap();
        assert!(p.model.find("Reqs::beta").is_some());
    }

    // @verifies CT-MOD-003
    #[test]
    fn a_new_file_that_breaks_the_setup_is_left_out_of_the_last_project() {
        let root = project("new-file-breaks");
        let mut ws = Workspace::open(&root).unwrap();
        let files = ws.project().model.files.clone();
        let text = "package Reqs { part def Top; }\n";
        let err = ws.edit("model/a.sysml", text).err().unwrap();
        assert!(err.to_string().contains("Reqs::Top"), "{err}");
        assert_eq!(ws.project().model.files, files);
        assert!(
            ws.project().problems.is_empty(),
            "{:?}",
            ws.project().problems
        );
        let p = ws.edit("model/a.sysml", "package A;\n").unwrap();
        assert!(p.model.files.iter().any(|f| f == "model/a.sysml"));
    }

    // @verifies CT-MOD-003
    #[test]
    fn element_ids_stay_stable_across_a_rebuild() {
        let root = project("stable");
        let mut ws = Workspace::open(&root).unwrap();
        let names = ["Reqs::a", "Reqs::b", "Design::engine", "Design::wheel"];
        let before: Vec<u64> = names.iter().map(|n| ws.stable_id(find(&ws, n))).collect();
        let satisfy = ws
            .project()
            .model
            .element(find(&ws, "Design::engine"))
            .members[0];
        let satisfy_before = ws.stable_id(satisfy);
        // Elements added ahead of the others shift every model index after them.
        let reqs = REQS.replace("{\n", "{\n    part def Early;\n    part early;\n");
        ws.edit("model/reqs.sysml", &reqs).unwrap();
        let design = DESIGN.replace("{\n    part engine", "{\n    part first;\n    part engine");
        ws.edit("model/design.sysml", &design).unwrap();
        let after: Vec<u64> = names.iter().map(|n| ws.stable_id(find(&ws, n))).collect();
        assert_eq!(before, after);
        let satisfy = ws
            .project()
            .model
            .element(find(&ws, "Design::engine"))
            .members[0];
        assert_eq!(ws.stable_id(satisfy), satisfy_before);
        for (name, stable) in names.iter().zip(&after) {
            assert_eq!(ws.element(*stable), Some(find(&ws, name)));
        }
        let early = ws.stable_id(find(&ws, "Reqs::early"));
        assert!(!before.contains(&early));
    }

    // @verifies CT-MOD-003
    #[test]
    fn a_removed_or_renamed_element_loses_its_id() {
        let root = project("removed");
        let mut ws = Workspace::open(&root).unwrap();
        let wheel = ws.stable_id(find(&ws, "Design::wheel"));
        let engine = ws.stable_id(find(&ws, "Design::engine"));
        let design = DESIGN
            .replace("part wheel;", "part tyre;")
            .replace(" by wheel", " by tyre");
        ws.edit("model/design.sysml", &design).unwrap();
        assert_eq!(ws.element(wheel), None);
        assert_eq!(ws.element(engine), Some(find(&ws, "Design::engine")));
        assert_ne!(ws.stable_id(find(&ws, "Design::tyre")), wheel);
    }

    // @verifies CT-MOD-003
    #[test]
    fn same_named_packages_in_two_files_keep_their_own_ids() {
        let root = project("two-files");
        fs::write(
            root.join("model/more.sysml"),
            "package Design { part spare; }\n",
        )
        .unwrap();
        let mut ws = Workspace::open(&root).unwrap();
        let ids = |ws: &Workspace| -> Vec<u64> {
            let m = &ws.project().model;
            let packages = m.element(crate::model::Model::ROOT).members.iter();
            let design = packages.filter(|&&e| m.element(e).name.as_deref() == Some("Design"));
            design.map(|&e| ws.stable_id(e)).collect()
        };
        let before = ids(&ws);
        assert_eq!(before.len(), 2);
        assert_ne!(before[0], before[1]);
        ws.edit("model/a.sysml", "package A;\n").unwrap();
        assert_eq!(ids(&ws), before);
    }
}
