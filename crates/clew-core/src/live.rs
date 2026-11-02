//! Live validation: rebuilding a project as the user edits one file, without reading or
//! parsing the others again (ADR-0005).

use std::collections::{BTreeSet, HashMap, HashSet};
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
            let file = sources.read_model(root, path);
            sources.files.push(file);
        }
        for path in check::source_files(root, &sources.config)? {
            if let Some(scanned) = scan_code(root, &sources.config, path) {
                sources.scans.push(scanned);
            }
        }
        let (problems, inputs) = (&mut sources.other_problems, &mut sources.other_inputs);
        sources.checklists = report::read_checklists(root, problems, inputs);
        sources.documents = report::read_documents(root, problems, inputs);
        Ok(sources)
    }

    /// Reads and parses the model file at `path` from disk.
    fn read_model(&mut self, root: &Path, path: String) -> ModelFile {
        match index::read_file(&root.join(&path)) {
            Ok(loaded) => self.parse(path, &loaded.text),
            Err(message) => ModelFile {
                problems: vec![index::unreadable(&path, &message)],
                path,
                members: None,
                sha256: Vec::new(),
            },
        }
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

    /// The cached model file at `path`.
    fn file(&self, path: &str) -> Option<&ModelFile> {
        let at = self.files.binary_search_by(|f| f.path.as_str().cmp(path));
        at.ok().map(|at| &self.files[at])
    }

    /// Puts `scan` in place of the code scan with its path, or adds it. Returns the scan
    /// it replaced. `order` puts the scans back in the order `read` gives them.
    fn put_scan(&mut self, scan: Scan, order: &[String]) -> Option<Scan> {
        let old = self.remove_scan(&scan.0);
        self.scans.push(scan);
        let rank: HashMap<&str, usize> = (order.iter().enumerate())
            .map(|(i, p)| (p.as_str(), i))
            .collect();
        (self.scans).sort_by_key(|(p, _, _)| rank.get(p.as_str()).copied().unwrap_or(usize::MAX));
        old
    }

    /// Takes the code scan of `path` out.
    fn remove_scan(&mut self, path: &str) -> Option<Scan> {
        let at = self.scans.iter().position(|(p, _, _)| p == path)?;
        Some(self.scans.remove(at))
    }

    /// Reads the checklists and documents again. Returns the old ones when what they found
    /// changed, or puts the old ones back and returns `None` when nothing did.
    fn reread_others(&mut self, root: &Path) -> Option<Others> {
        let (mut problems, mut inputs) = (Vec::new(), Vec::new());
        let checklists = report::read_checklists(root, &mut problems, &mut inputs);
        let documents = report::read_documents(root, &mut problems, &mut inputs);
        if problems == self.other_problems && inputs == self.other_inputs {
            return None;
        }
        Some(Others {
            checklists: std::mem::replace(&mut self.checklists, checklists),
            documents: std::mem::replace(&mut self.documents, documents),
            problems: std::mem::replace(&mut self.other_problems, problems),
            inputs: std::mem::replace(&mut self.other_inputs, inputs),
        })
    }

    /// Puts back the checklists and documents `reread_others` replaced.
    fn restore_others(&mut self, old: Others) {
        self.checklists = old.checklists;
        self.documents = old.documents;
        self.other_problems = old.problems;
        self.other_inputs = old.inputs;
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

// @implements CT-EFF-007, CT-TRC-004
/// A code file's path, its scan, and the SHA-256 of its text.
type Scan = (String, FileScan, Vec<u8>);

// @implements CT-EFF-007, CT-TRC-004
/// Scans the code file at `path`, unless `clew check --self` would report it as excluded,
/// untraceable, or unreadable.
fn scan_code(root: &Path, config: &TraceConfig, path: String) -> Option<Scan> {
    let excluded = (config.excludes.iter()).any(|e| check::glob_matches(&e.path, &path));
    let syntax = check::syntax_for(config, &path).filter(|_| !excluded)?;
    let loaded = index::read_file(&root.join(&path)).ok()?;
    let sha256 = reproduce::text_sha256(&loaded.text);
    Some((path, scan::scan_source(syntax, &loaded.text), sha256))
}

// @implements CT-EFF-007, CT-USE-009
/// The checklists and documents as `Sources` held them before they were read again.
struct Others {
    checklists: Vec<Checklist>,
    documents: Vec<Document>,
    problems: Vec<String>,
    inputs: Vec<(String, Vec<u8>)>,
}

// @implements CT-EFF-007, CT-USE-009
/// One input `reload` replaced, kept so a reload that breaks the setup can be undone.
enum Undo {
    /// A model file and the SHA-256 the disk had for it before.
    Model(String, Option<ModelFile>, Option<Vec<u8>>),
    /// A code file, the scan it had, and where that scan was in the list.
    Scan(String, Option<Scan>, usize),
    Others(Others),
}

// @implements CT-EFF-007, CT-USE-009, CT-USE-008
/// What `Workspace::reload` found.
#[derive(Debug, Default)]
pub struct Reloaded {
    /// Project-relative paths whose new content on disk the project now uses, sorted.
    pub reloaded: Vec<String>,
    /// Model files with unsaved edits that changed on disk. The project keeps the edits;
    /// the user chooses which text to keep.
    pub conflicts: Vec<String>,
    /// Why the new content could not be used. The project is the last one built.
    pub error: Option<SetupError>,
}

// @implements CT-USE-008
/// How old a temp file must be before `Workspace::open` treats it as left by a killed save.
/// A save takes far less, so a younger one may belong to a save still running elsewhere.
pub const STALE_TEMP_AGE: std::time::Duration = std::time::Duration::from_secs(60);

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
    /// The SHA-256 of each model file as last read from or written to disk. A cached file
    /// whose hash differs holds unsaved edits.
    disk: HashMap<String, Vec<u8>>,
}

// @implements CT-MOD-003, CT-EFF-007, CT-USE-007
impl Workspace {
    /// Reads the project at `root`, which must be the project root, keeping each model
    /// file's syntax tree. First it removes the temp files that killed saves left behind
    /// (`io::remove_stale_temps`, older than `STALE_TEMP_AGE`).
    pub fn open(root: &Path) -> Result<Workspace, SetupError> {
        crate::io::remove_stale_temps(root, STALE_TEMP_AGE);
        let sources = Sources::read(root)?;
        let project = sources.project(root)?;
        let mut ids = StableIds::default();
        ids.assign(&project.model);
        let disk = (sources.files.iter())
            .map(|f| (f.path.clone(), f.sha256.clone()))
            .collect();
        Ok(Workspace {
            root: root.to_path_buf(),
            sources,
            project: Some(project),
            ids,
            disk,
        })
    }
}

// @implements CT-EFF-007, CT-USE-009, CT-USE-008
impl Workspace {
    /// Notes that the app wrote `text` to the model file at `path`, so the file event the
    /// write causes is known as its own and is not reloaded.
    pub fn saved(&mut self, path: &str, text: &str) {
        self.disk
            .insert(path.to_string(), reproduce::text_sha256(text));
    }

    /// Whether the cached model file at `path` differs from what the disk had, given the
    /// disk's hash `before` the change being handled.
    fn unsaved(&self, path: &str, before: Option<&Vec<u8>>) -> bool {
        self.sources
            .file(path)
            .is_some_and(|f| before != Some(&f.sha256))
    }

    /// Takes the content on disk of the project-relative `changed` paths, which file events
    /// reported, and rebuilds the project when any input changed. Only changed model files
    /// are parsed again and only changed code files scanned again; a change under
    /// `docs/` or to a `.md` file reads the checklists and documents again, and a change to
    /// `trace.toml` reads the whole project again. A file whose content is what the app
    /// last read or saved is skipped, so the app's own saves cost one hash each.
    ///
    /// A model file with unsaved edits is never replaced: it is listed in `conflicts`, and
    /// the project keeps the edits until the editor sends the text the user chose. When the
    /// new content breaks the setup, the last project is rebuilt and the error returned.
    pub fn reload(&mut self, changed: &[String]) -> Reloaded {
        let mut changed: Vec<&String> = changed.iter().collect();
        changed.sort();
        changed.dedup();
        if changed.iter().any(|p| p.as_str() == check::TRACE_CONFIG) {
            return self.reread(&changed);
        }
        let mut done = Reloaded::default();
        let root = self.root.clone();
        let (models, code) = match self.listed() {
            Ok(lists) => lists,
            Err(error) => {
                done.error = Some(error);
                return done;
            }
        };
        let mut undo = Vec::new();
        let mut markdown = Vec::new();
        for path in changed {
            let model_file = path.ends_with(".sysml");
            if model_file && (models.contains(path) || self.sources.file(path).is_some()) {
                let on_disk = models.contains(path);
                let sha = on_disk.then(|| {
                    index::read_file(&root.join(path))
                        .map(|l| reproduce::text_sha256(&l.text))
                        .unwrap_or_default()
                });
                let before = self.disk.get(path.as_str()).cloned();
                if sha.is_some() && sha == before {
                    continue;
                }
                if self.unsaved(path, before.as_ref()) {
                    done.conflicts.push(path.clone());
                    continue;
                }
                let old = if on_disk {
                    let file = self.sources.read_model(&root, path.clone());
                    self.disk.insert(path.clone(), file.sha256.clone());
                    self.sources.put(file)
                } else {
                    self.disk.remove(path.as_str());
                    self.sources.remove(path)
                };
                undo.push(Undo::Model(path.clone(), old, before));
                done.reloaded.push(path.clone());
            } else if code.contains(path) {
                let scanned = scan_code(&root, &self.sources.config, path.clone());
                let at = self.scan_at(path);
                let before = (self.sources.scans.iter()).find(|(p, _, _)| p == path);
                if before.map(|(_, _, sha)| sha) == scanned.as_ref().map(|(_, _, sha)| sha) {
                    continue;
                }
                let old = match scanned {
                    Some(scan) => self.sources.put_scan(scan, &code),
                    None => self.sources.remove_scan(path),
                };
                undo.push(Undo::Scan(path.clone(), old, at));
                done.reloaded.push(path.clone());
            } else if let at = self.scan_at(path)
                && let Some(old) = self.sources.remove_scan(path)
            {
                undo.push(Undo::Scan(path.clone(), Some(old), at));
                done.reloaded.push(path.clone());
            }
            if path.ends_with(".md") {
                markdown.push(path.clone());
            }
        }
        if !markdown.is_empty()
            && let Some(old) = self.sources.reread_others(&root)
        {
            undo.push(Undo::Others(old));
            done.reloaded.extend(markdown);
        }
        if undo.is_empty() {
            return done;
        }
        if let Err(error) = self.rebuild() {
            for step in undo.into_iter().rev() {
                self.undo(step);
            }
            done.error = Some(error);
            done.reloaded.clear();
            self.project = Some(
                (self.sources.project(&self.root))
                    .expect("the restored inputs built the last project"),
            );
        }
        done.reloaded.sort();
        done.reloaded.dedup();
        done
    }
}

// @implements CT-EFF-007, CT-USE-009, CT-USE-008
/// The steps of `reload`.
impl Workspace {
    /// Where the scan of `path` is in the list, or the end when there is none.
    fn scan_at(&self, path: &str) -> usize {
        let scans = &self.sources.scans;
        scans
            .iter()
            .position(|(p, _, _)| p == path)
            .unwrap_or(scans.len())
    }

    /// The model files and code files the project reads, as listed on disk now.
    fn listed(&self) -> Result<(HashSet<String>, Vec<String>), SetupError> {
        let models = index::sysml_files(&self.root).map_err(|e| SetupError {
            path: String::new(),
            line: 0,
            message: e.to_string(),
        })?;
        let code = check::source_files(&self.root, &self.sources.config)?;
        Ok((models.into_iter().collect(), code))
    }

    /// Rebuilds the project from the cached inputs, dropping the old one first so only one
    /// is held at a time (CT-EFF-004). On an error there is no project until the caller
    /// puts the old inputs back and builds again.
    fn rebuild(&mut self) -> Result<(), SetupError> {
        self.project = None;
        let project = self.sources.project(&self.root)?;
        self.ids.assign(&project.model);
        self.project = Some(project);
        Ok(())
    }

    /// Puts back one input `reload` replaced.
    fn undo(&mut self, step: Undo) {
        match step {
            Undo::Model(path, old, disk) => {
                match old {
                    Some(file) => drop(self.sources.put(file)),
                    None => drop(self.sources.remove(&path)),
                }
                match disk {
                    Some(sha) => self.disk.insert(path, sha),
                    None => self.disk.remove(&path),
                };
            }
            Undo::Scan(path, old, at) => {
                self.sources.remove_scan(&path);
                if let Some(scan) = old {
                    let at = at.min(self.sources.scans.len());
                    self.sources.scans.insert(at, scan);
                }
            }
            Undo::Others(old) => self.sources.restore_others(old),
        }
    }

    /// Reads every input of the project again, for when what changed is not known file by
    /// file, such as a folder renamed. Model files with unsaved edits keep them, as in
    /// `reload`; `reloaded` lists the model files whose content on disk changed.
    pub fn reload_all(&mut self) -> Reloaded {
        self.reread(&[])
    }

    /// `reload` after `trace.toml` changed, and `reload_all`: reads every input again,
    /// keeping the cached text of model files with unsaved edits.
    fn reread(&mut self, changed: &[&String]) -> Reloaded {
        let mut done = Reloaded::default();
        let mut fresh = match Sources::read(&self.root) {
            Ok(fresh) => fresh,
            Err(error) => {
                done.error = Some(error);
                return done;
            }
        };
        fresh.parses += self.sources.parses;
        let mut disk: HashMap<String, Vec<u8>> = (fresh.files.iter())
            .map(|f| (f.path.clone(), f.sha256.clone()))
            .collect();
        let unsaved: Vec<String> = (self.sources.files.iter())
            .filter(|f| self.unsaved(&f.path, self.disk.get(&f.path)))
            .map(|f| f.path.clone())
            .collect();
        for path in &unsaved {
            let path = path.clone();
            let now = disk.get(&path).cloned();
            if now != self.disk.get(&path).cloned() {
                done.conflicts.push(path.clone());
            }
            match self.disk.get(&path) {
                Some(sha) => disk.insert(path.clone(), sha.clone()),
                None => disk.remove(&path),
            };
            if let Some(file) = self.sources.remove(&path) {
                fresh.put(file);
            }
        }
        let mut news: BTreeSet<String> = changed.iter().map(|p| p.to_string()).collect();
        let paths = (disk.keys()).chain(self.disk.keys());
        news.extend(paths.filter(|p| disk.get(*p) != self.disk.get(*p)).cloned());
        let old_sources = std::mem::replace(&mut self.sources, fresh);
        let old_disk = std::mem::replace(&mut self.disk, disk);
        match self.rebuild() {
            Ok(()) => done.reloaded = news.into_iter().collect(),
            Err(error) => {
                // The kept files went into the new inputs; move them back.
                let mut old_sources = old_sources;
                for path in &unsaved {
                    if let Some(file) = self.sources.remove(path) {
                        old_sources.put(file);
                    }
                }
                done.conflicts.clear();
                self.sources = old_sources;
                self.disk = old_disk;
                self.project = Some(
                    (self.sources.project(&self.root))
                        .expect("the old inputs built the last project"),
                );
                done.error = Some(error);
            }
        }
        done
    }
}

// @implements CT-MOD-003, CT-EFF-007, CT-USE-007
impl Workspace {
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

    // @verifies CT-EFF-007, CT-USE-009
    /// The project rebuilt by `reload` matches one read from disk afresh.
    fn same_as_fresh(ws: &Workspace) {
        let fresh = load_project(&ws.root).unwrap();
        let p = ws.project();
        assert_eq!(p.model.files, fresh.model.files);
        assert_eq!(p.model.elements.len(), fresh.model.elements.len());
        assert_eq!(p.graph.links, fresh.graph.links);
        assert_eq!(p.inputs, fresh.inputs);
        assert_eq!(p.problems, fresh.problems);
        assert_eq!(p.requirements, fresh.requirements);
        assert_eq!(p.documents, fresh.documents);
        assert_eq!(p.checklists, fresh.checklists);
    }

    // @verifies CT-EFF-007
    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    // @verifies CT-EFF-007, CT-USE-009
    #[test]
    fn a_model_file_changed_on_disk_is_reparsed_alone() {
        let root = project("disk-change");
        let mut ws = Workspace::open(&root).unwrap();
        let text = DESIGN.replace("part wheel;", "part wheel;\n    part axle;");
        fs::write(root.join("model/design.sysml"), &text).unwrap();
        let done = ws.reload(&paths(&["model/design.sysml"]));
        assert!(done.error.is_none(), "{:?}", done.error);
        assert_eq!(done.reloaded, ["model/design.sysml"]);
        assert!(done.conflicts.is_empty());
        assert_eq!(ws.parses(), 3);
        assert!(ws.project().model.find("Design::axle").is_some());
        same_as_fresh(&ws);
    }

    // @verifies CT-EFF-007, CT-EFF-008
    #[test]
    fn an_event_for_a_file_that_did_not_change_parses_nothing() {
        let root = project("no-change");
        let mut ws = Workspace::open(&root).unwrap();
        let done = ws.reload(&paths(&["model/design.sysml", "src/lib.rs", "notes.txt"]));
        assert!(done.reloaded.is_empty() && done.conflicts.is_empty());
        assert!(done.error.is_none());
        assert_eq!(ws.parses(), 2);
    }

    // @verifies CT-USE-009, CT-USE-008
    #[test]
    fn the_apps_own_save_is_not_news() {
        let root = project("own-save");
        let mut ws = Workspace::open(&root).unwrap();
        let saved = DESIGN.replace("wheel", "tyre");
        ws.edit("model/design.sysml", &saved).unwrap();
        fs::write(root.join("model/design.sysml"), &saved).unwrap();
        ws.saved("model/design.sysml", &saved);
        // The user typed on before the file event for the save came in.
        ws.edit("model/design.sysml", &format!("{saved}// more\n"))
            .unwrap();
        let parses = ws.parses();
        let done = ws.reload(&paths(&["model/design.sysml"]));
        assert!(done.reloaded.is_empty() && done.conflicts.is_empty());
        assert_eq!(ws.parses(), parses);
    }

    // @verifies CT-USE-009, CT-USE-008
    #[test]
    fn a_file_with_unsaved_edits_that_changed_on_disk_is_a_conflict_and_keeps_the_edits() {
        let root = project("conflict");
        let mut ws = Workspace::open(&root).unwrap();
        let mine = DESIGN.replace("part wheel;", "part wheel;\n    part mine;");
        ws.edit("model/design.sysml", &mine).unwrap();
        let theirs = DESIGN.replace("part wheel;", "part wheel;\n    part theirs;");
        fs::write(root.join("model/design.sysml"), &theirs).unwrap();
        let done = ws.reload(&paths(&["model/design.sysml"]));
        assert_eq!(done.conflicts, ["model/design.sysml"]);
        assert!(done.reloaded.is_empty());
        assert!(ws.project().model.find("Design::mine").is_some());
        assert!(ws.project().model.find("Design::theirs").is_none());
        // Taking the disk text, as the editor does when the user loads it, ends the conflict.
        ws.edit("model/design.sysml", &theirs).unwrap();
        same_as_fresh(&ws);
    }

    // @verifies CT-EFF-007, CT-USE-009
    #[test]
    fn a_deleted_model_file_leaves_and_a_new_one_joins() {
        let root = project("delete-add");
        let mut ws = Workspace::open(&root).unwrap();
        fs::write(
            root.join("model/extra.sysml"),
            "package Extra { part spare; }\n",
        )
        .unwrap();
        let done = ws.reload(&paths(&["model/extra.sysml"]));
        assert_eq!(done.reloaded, ["model/extra.sysml"]);
        assert!(ws.project().model.find("Extra::spare").is_some());
        same_as_fresh(&ws);
        fs::remove_file(root.join("model/extra.sysml")).unwrap();
        let done = ws.reload(&paths(&["model/extra.sysml"]));
        assert_eq!(done.reloaded, ["model/extra.sysml"]);
        assert!(ws.project().model.find("Extra::spare").is_none());
        same_as_fresh(&ws);
        assert_eq!(ws.parses(), 3);
    }

    // @verifies CT-EFF-007, CT-USE-009
    #[test]
    fn a_changed_or_new_code_file_is_scanned_again() {
        let root = project("code");
        let mut ws = Workspace::open(&root).unwrap();
        fs::write(
            root.join("src/lib.rs"),
            "// @verifies X-002\n#[test]\nfn b() {}\n",
        )
        .unwrap();
        fs::write(
            root.join("src/more.rs"),
            "// @implements X-001\nfn c() {}\n",
        )
        .unwrap();
        let done = ws.reload(&paths(&["src/lib.rs", "src/more.rs"]));
        assert!(done.error.is_none(), "{:?}", done.error);
        assert_eq!(done.reloaded, ["src/lib.rs", "src/more.rs"]);
        assert_eq!(ws.parses(), 2);
        same_as_fresh(&ws);
        fs::remove_file(root.join("src/more.rs")).unwrap();
        assert_eq!(
            ws.reload(&paths(&["src/more.rs"])).reloaded,
            ["src/more.rs"]
        );
        same_as_fresh(&ws);
    }

    // @verifies CT-EFF-007, CT-USE-009
    #[test]
    fn a_changed_document_or_checklist_is_read_again() {
        let root = project("docs");
        let mut ws = Workspace::open(&root).unwrap();
        fs::create_dir_all(root.join("docs/verification")).unwrap();
        fs::write(
            root.join("docs/verification/check.md"),
            "<!-- @verifies X-001 -->\n# Check\n",
        )
        .unwrap();
        let done = ws.reload(&paths(&["docs/verification/check.md"]));
        assert_eq!(done.reloaded, ["docs/verification/check.md"]);
        same_as_fresh(&ws);
    }

    // @verifies CT-EFF-007, CT-USE-009
    #[test]
    fn a_trace_toml_change_reads_the_project_again_but_keeps_unsaved_edits() {
        let root = project("config");
        let mut ws = Workspace::open(&root).unwrap();
        let mine = DESIGN.replace("part wheel;", "part wheel;\n    part mine;");
        ws.edit("model/design.sysml", &mine).unwrap();
        let config = fs::read_to_string(root.join("trace.toml")).unwrap();
        fs::write(
            root.join("trace.toml"),
            config.replace("source_roots = [\"src\"]", "source_roots = []"),
        )
        .unwrap();
        let done = ws.reload(&paths(&["trace.toml"]));
        assert!(done.error.is_none(), "{:?}", done.error);
        assert_eq!(done.reloaded, ["trace.toml"]);
        assert!(done.conflicts.is_empty());
        assert!(ws.project().model.find("Design::mine").is_some());
        assert!(ws.project().inputs.iter().all(|(p, _)| p != "src/lib.rs"));
    }

    // @verifies CT-USE-009, CT-USE-007
    #[test]
    fn a_disk_change_that_breaks_the_setup_keeps_the_last_project() {
        let root = project("breaks");
        let mut ws = Workspace::open(&root).unwrap();
        fs::write(root.join("model/reqs.sysml"), "package Reqs;\n").unwrap();
        let done = ws.reload(&paths(&["model/reqs.sysml"]));
        let error = done.error.expect("Reqs::Top no longer resolves");
        assert!(error.message.contains("Top"), "{error:?}");
        assert!(ws.project().model.find("Reqs::a").is_some());
        // Fixing the file on disk brings the project back in step.
        fs::write(
            root.join("model/reqs.sysml"),
            REQS.replace("b : Top", "c : Top"),
        )
        .unwrap();
        let done = ws.reload(&paths(&["model/reqs.sysml"]));
        assert!(done.error.is_none(), "{:?}", done.error);
        same_as_fresh(&ws);
    }

    // @verifies CT-USE-008
    #[test]
    fn opening_removes_stale_temp_files() {
        let root = project("temps");
        let stale = root.join(format!("model/.reqs.sysml.9{}", crate::io::TEMP_SUFFIX));
        fs::write(&stale, "torn").unwrap();
        let old = std::time::SystemTime::now() - STALE_TEMP_AGE * 2;
        fs::File::options()
            .write(true)
            .open(&stale)
            .unwrap()
            .set_modified(old)
            .unwrap();
        Workspace::open(&root).unwrap();
        assert!(!stale.exists());
    }

    // @verifies CT-EFF-007, CT-USE-009
    #[test]
    fn reloading_everything_after_a_folder_rename_lists_the_moved_files() {
        let root = project("rename");
        let mut ws = Workspace::open(&root).unwrap();
        fs::create_dir_all(root.join("more")).unwrap();
        fs::rename(
            root.join("model/design.sysml"),
            root.join("more/design.sysml"),
        )
        .unwrap();
        let done = ws.reload_all();
        assert!(done.error.is_none(), "{:?}", done.error);
        assert_eq!(done.reloaded, ["model/design.sysml", "more/design.sysml"]);
        same_as_fresh(&ws);
    }
}
