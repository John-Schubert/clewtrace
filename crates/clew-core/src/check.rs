//! `clew check --self`: scans the source roots in `trace.toml` and reports untraced lines,
//! unknown requirement IDs, oversized scopes, dependencies missing from `deps.toml`, ADRs
//! that cite no requirement, Markdown documents that implement a requirement not verified by
//! Inspection or Demonstration, and requirements due in a started phase that lack an
//! implementation or a verification and have no waiver. Rules: docs/TRACEABILITY.md.

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::config::{ConfigError, TraceConfig, parse_tables};
use crate::report;
use crate::reqs::{self, Requirement};
use crate::scan::{self, ProblemKind, ScopeKind, Syntax, TagKind};

/// Where the checker finds its inputs, relative to the project root.
// @implements CT-DEV-004
pub const TRACE_CONFIG: &str = "trace.toml";
// @implements CT-DEV-004
pub const DEPS_FILE: &str = "deps.toml";
// @implements CT-DEV-004
pub const ADR_DIR: &str = "docs/adr";

// @implements CT-DEV-002, CT-DEV-004, CT-USE-007
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
}

/// One reported problem. `line` is 0 when the finding is about a whole file.
// @implements CT-DEV-002, CT-DEV-004, CT-USE-007
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    pub path: String,
    pub line: usize,
    pub severity: Severity,
    pub message: String,
    pub help: String,
}

// @implements CT-DEV-002, CT-DEV-004, CT-USE-007
impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let severity = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        match self.line {
            0 => write!(f, "{}: {severity}: {}", self.path, self.message)?,
            line => write!(f, "{}:{line}: {severity}: {}", self.path, self.message)?,
        }
        write!(f, "\n  help: {}", self.help)
    }
}

// @implements CT-DEV-002, CT-DEV-004
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    pub files: usize,
    pub findings: Vec<Finding>,
}

// @implements CT-DEV-002, CT-DEV-004
impl Report {
    pub fn count(&self, severity: Severity) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == severity)
            .count()
    }
}

/// A problem that stops the check from running, such as a broken `trace.toml`.
// @implements CT-DEV-004, CT-USE-007
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupError {
    pub path: String,
    pub line: usize,
    pub message: String,
}

// @implements CT-DEV-004, CT-USE-007
impl fmt::Display for SetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            _ if self.path.is_empty() => write!(f, "{}", self.message),
            0 => write!(f, "{}: {}", self.path, self.message),
            line => write!(f, "{}:{line}: {}", self.path, self.message),
        }
    }
}

// @implements CT-DEV-004, CT-USE-007
fn setup_error(path: &str, e: ConfigError) -> SetupError {
    SetupError {
        path: path.to_string(),
        line: e.line,
        message: e.message,
    }
}

/// The subset of regular expressions `id_pattern` may use: literal characters,
/// `(a|b)` groups of literal choices, `[0-9A-Z]` classes, and `{n}` repeat counts.
// @implements CT-DEV-004
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdPattern {
    atoms: Vec<(Atom, usize)>,
}

// @implements CT-DEV-004
#[derive(Debug, Clone, PartialEq, Eq)]
enum Atom {
    Char(char),
    Choice(Vec<String>),
    Class(Vec<(char, char)>),
}

// @implements CT-DEV-004, CT-USE-007
impl IdPattern {
    pub fn parse(pattern: &str) -> Result<IdPattern, String> {
        let body = pattern.strip_prefix('^').unwrap_or(pattern);
        let body = body.strip_suffix('$').unwrap_or(body);
        let unsupported = |c: char| {
            format!("`{c}` is not supported in id_pattern. Use literals, (a|b), [0-9], and {{n}}")
        };
        let mut atoms: Vec<(Atom, usize)> = Vec::new();
        let mut chars = body.chars();
        while let Some(c) = chars.next() {
            let atom = match c {
                '(' => {
                    let group: String = chars.by_ref().take_while(|&c| c != ')').collect();
                    if let Some(bad) = group
                        .chars()
                        .find(|&c| !(c.is_ascii_alphanumeric() || c == '|' || c == '-'))
                    {
                        return Err(unsupported(bad));
                    }
                    Atom::Choice(group.split('|').map(str::to_string).collect())
                }
                '[' => {
                    let class: Vec<char> = chars.by_ref().take_while(|&c| c != ']').collect();
                    let mut ranges = Vec::new();
                    let mut i = 0;
                    while i < class.len() {
                        if i + 2 < class.len() && class[i + 1] == '-' {
                            ranges.push((class[i], class[i + 2]));
                            i += 3;
                        } else {
                            ranges.push((class[i], class[i]));
                            i += 1;
                        }
                    }
                    Atom::Class(ranges)
                }
                '{' => {
                    let count: String = chars.by_ref().take_while(|&c| c != '}').collect();
                    let n = count.parse().map_err(|_| unsupported('{'))?;
                    let last = atoms.last_mut().ok_or_else(|| unsupported('{'))?;
                    last.1 = n;
                    continue;
                }
                c if c.is_ascii_alphanumeric() || c == '-' || c == '_' => Atom::Char(c),
                c => return Err(unsupported(c)),
            };
            atoms.push((atom, 1));
        }
        Ok(IdPattern { atoms })
    }

    pub fn matches(&self, text: &str) -> bool {
        match_atoms(&self.atoms, text)
    }
}

// @implements CT-DEV-004
fn match_atoms(atoms: &[(Atom, usize)], text: &str) -> bool {
    let Some(((atom, count), rest)) = atoms.split_first() else {
        return text.is_empty();
    };
    if *count == 0 {
        return match_atoms(rest, text);
    }
    let mut remaining = atoms.to_vec();
    remaining[0].1 -= 1;
    let next = |len: usize| match_atoms(&remaining, &text[len..]);
    match atom {
        Atom::Char(c) => text.starts_with(*c) && next(c.len_utf8()),
        Atom::Choice(options) => options.iter().any(|o| text.starts_with(o) && next(o.len())),
        Atom::Class(ranges) => text.chars().next().is_some_and(|c| {
            ranges.iter().any(|&(lo, hi)| (lo..=hi).contains(&c)) && next(c.len_utf8())
        }),
    }
}

/// Matches a `/`-separated path against a glob: `*` and `?` stay inside one path segment,
/// `**` crosses segments. A glob without `/` is matched against the file name only.
// @implements CT-DEV-002
pub fn glob_matches(glob: &str, path: &str) -> bool {
    let target = if glob.contains('/') {
        path
    } else {
        path.rsplit('/').next().unwrap_or(path)
    };
    glob_bytes(glob.as_bytes(), target.as_bytes())
}

// @implements CT-DEV-002
fn glob_bytes(glob: &[u8], text: &[u8]) -> bool {
    match glob {
        [] => text.is_empty(),
        [b'*', b'*', rest @ ..] => {
            let rest = rest.strip_prefix(b"/").unwrap_or(rest);
            (0..=text.len()).any(|i| {
                (i == 0 || text[i - 1] == b'/' || rest.is_empty()) && glob_bytes(rest, &text[i..])
            })
        }
        [b'*', rest @ ..] => (0..=text.len())
            .take_while(|&i| i == 0 || text[i - 1] != b'/')
            .any(|i| glob_bytes(rest, &text[i..])),
        [b'?', rest @ ..] => {
            text.first().is_some_and(|&c| c != b'/') && glob_bytes(rest, &text[1..])
        }
        [c, rest @ ..] => text.first() == Some(c) && glob_bytes(rest, &text[1..]),
    }
}

/// The checker's view of the project: config, known IDs, and the ID pattern.
// @implements CT-DEV-002, CT-DEV-004
struct Checker {
    root: PathBuf,
    config: TraceConfig,
    pattern: IdPattern,
    known: BTreeSet<String>,
    /// The requirements in the `requirements` files, each with its file.
    requirements: Vec<(String, Requirement)>,
    report: Report,
}

/// Reads `trace.toml` under `root`.
// @implements CT-DEV-004, CT-TRC-004, CT-USE-007
pub fn read_config(root: &Path) -> Result<TraceConfig, SetupError> {
    let config_path = root.join(TRACE_CONFIG);
    if !config_path.is_file() {
        return Err(SetupError {
            path: TRACE_CONFIG.to_string(),
            line: 0,
            message: "not found. Run clew from the project root, where trace.toml lives".into(),
        });
    }
    TraceConfig::read(&config_path).map_err(|e| setup_error(TRACE_CONFIG, e))
}

/// Every file under the config's source roots, as sorted `/`-separated paths relative to
/// `root`.
// @implements CT-DEV-002, CT-TRC-004
pub fn source_files(root: &Path, config: &TraceConfig) -> Result<Vec<String>, SetupError> {
    let mut files = Vec::new();
    let mut dirs: Vec<PathBuf> = (config.source_roots.iter())
        .map(|r| root.join(r))
        .filter(|p| p.is_dir())
        .collect();
    while let Some(dir) = dirs.pop() {
        let io = |e: std::io::Error| SetupError {
            path: dir.display().to_string(),
            line: 0,
            message: e.to_string(),
        };
        for entry in std::fs::read_dir(&dir).map_err(io)? {
            let entry = entry.map_err(io)?;
            let kind = entry.file_type().map_err(io)?;
            if kind.is_dir() {
                dirs.push(entry.path());
            } else if kind.is_file() {
                let relative = entry.path();
                let relative = relative.strip_prefix(root).unwrap_or(&relative);
                let parts: Vec<_> = relative
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect();
                files.push(parts.join("/"));
            }
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

/// The syntax to scan a file with, or `None` when the config does not trace its type.
// @implements CT-DEV-002, CT-TRC-004
pub fn syntax_for(config: &TraceConfig, path: &str) -> Option<Syntax> {
    let syntax = Syntax::from_path(Path::new(path))?;
    let allowed = match syntax {
        Syntax::Rust => config.languages.iter().any(|l| l == "rust"),
        Syntax::TypeScript => config.languages.iter().any(|l| l == "typescript"),
        Syntax::Svelte => config.languages.iter().any(|l| l == "svelte"),
        Syntax::HashComments | Syntax::Workflow => {
            (config.file_level_allowed.iter()).any(|g| glob_matches(g, path))
        }
    };
    allowed.then_some(syntax)
}

/// Reads `trace.toml`, its `id_pattern`, and the known requirement IDs under `root`.
// @implements CT-DEV-004, CT-DEV-005, CT-USE-007
pub fn load_ids(root: &Path) -> Result<(TraceConfig, IdPattern, BTreeSet<String>), SetupError> {
    let (config, pattern, requirements) = load_requirements(root)?;
    let known = requirements.into_iter().map(|(_, r)| r.id).collect();
    Ok((config, pattern, known))
}

/// Reads `trace.toml`, its `id_pattern`, and the requirements in its `requirements` files
/// under `root`, each with its file.
// @implements CT-DEV-003, CT-DEV-004, CT-DEV-005, CT-USE-007
#[allow(clippy::type_complexity)]
fn load_requirements(
    root: &Path,
) -> Result<(TraceConfig, IdPattern, Vec<(String, Requirement)>), SetupError> {
    let config = read_config(root)?;
    let pattern = IdPattern::parse(&config.id_pattern).map_err(|message| SetupError {
        path: TRACE_CONFIG.to_string(),
        line: 0,
        message: format!("id_pattern: {message}"),
    })?;
    let mut requirements = Vec::new();
    for file in &config.requirements {
        let text = std::fs::read_to_string(root.join(file)).map_err(|e| SetupError {
            path: file.clone(),
            line: 0,
            message: e.to_string(),
        })?;
        // The reader's errors already start with the path, line, and column.
        let all = reqs::parse_requirements(&text, file).map_err(|e| SetupError {
            path: String::new(),
            line: 0,
            message: e.to_string(),
        })?;
        requirements.extend(all.into_iter().map(|r| (file.clone(), r)));
    }
    Ok((config, pattern, requirements))
}

/// Runs every check on the project rooted at `root`.
// @implements CT-DEV-002, CT-DEV-003, CT-DEV-004, CT-TRC-008
pub fn check_project(root: &Path) -> Result<Report, SetupError> {
    let (config, pattern, requirements) = load_requirements(root)?;
    let mut checker = Checker {
        root: root.to_path_buf(),
        config,
        pattern,
        known: requirements.iter().map(|(_, r)| r.id.clone()).collect(),
        requirements,
        report: Report::default(),
    };
    checker.check_config()?;
    checker.check_sources()?;
    checker.check_deps()?;
    checker.check_adrs()?;
    checker.check_requirements()?;
    let mut report = checker.report;
    report.findings.sort();
    Ok(report)
}

/// Recording findings, checking IDs, and walking the source roots.
// @implements CT-DEV-002, CT-DEV-004, CT-USE-007
impl Checker {
    fn push(&mut self, path: &str, line: usize, severity: Severity, message: String, help: &str) {
        self.report.findings.push(Finding {
            path: path.to_string(),
            line,
            severity,
            message,
            help: help.to_string(),
        });
    }

    /// Reports an ID that does not match `id_pattern` or is not in the model.
    fn check_id(&mut self, path: &str, line: usize, id: &str) {
        if !self.pattern.matches(id) {
            let message = format!(
                "`{id}` does not match id_pattern {}",
                self.config.id_pattern
            );
            self.push(
                path,
                line,
                Severity::Error,
                message,
                "write the ID as in model/requirements.sysml, for example CT-TRC-004",
            );
        } else if !self.known.contains(id) {
            self.push(
                path,
                line,
                Severity::Error,
                format!("unknown requirement ID {id}"),
                "cite an ID from the model (`clew reqs list`), or propose a new requirement",
            );
        }
    }

    fn check_config(&mut self) -> Result<(), SetupError> {
        for language in &self.config.languages {
            if !matches!(language.as_str(), "rust" | "typescript" | "svelte") {
                return Err(SetupError {
                    path: TRACE_CONFIG.to_string(),
                    line: 0,
                    message: format!(
                        "unknown language `{language}` in `languages`. Use rust, typescript, or svelte"
                    ),
                });
            }
        }
        for exclude in self.config.excludes.clone() {
            if exclude.reason.trim().is_empty() {
                self.push(
                    TRACE_CONFIG,
                    exclude.line,
                    Severity::Error,
                    format!("[[exclude]] for {} has an empty reason", exclude.path),
                    "say why this code is generated or vendored",
                );
            }
            self.check_id(TRACE_CONFIG, exclude.line, &exclude.requirement);
        }
        Ok(())
    }

    fn check_sources(&mut self) -> Result<(), SetupError> {
        let files = source_files(&self.root, &self.config)?;
        let mut used = vec![false; self.config.excludes.len()];
        for path in files {
            let excluded = self
                .config
                .excludes
                .iter()
                .position(|e| glob_matches(&e.path, &path));
            if let Some(index) = excluded {
                used[index] = true;
                continue;
            }
            self.report.files += 1;
            let Some(syntax) = syntax_for(&self.config, &path) else {
                self.push(
                    &path,
                    0,
                    Severity::Error,
                    "file type cannot be traced".into(),
                    "use a traced language or a file_level_allowed type, or add an [[exclude]] entry with a reason and requirement to trace.toml",
                );
                continue;
            };
            let text = match std::fs::read_to_string(self.root.join(&path)) {
                Ok(text) => text,
                Err(e) => {
                    self.push(
                        &path,
                        0,
                        Severity::Error,
                        format!("cannot read file: {e}"),
                        "make the file readable UTF-8 text",
                    );
                    continue;
                }
            };
            self.check_file(&path, syntax, &text);
        }
        for (exclude, used) in self.config.excludes.clone().iter().zip(used) {
            if !used {
                self.push(
                    TRACE_CONFIG,
                    exclude.line,
                    Severity::Warning,
                    format!("[[exclude]] path {} matches no file", exclude.path),
                    "remove the entry or fix its path",
                );
            }
        }
        Ok(())
    }
}

/// Checks on one source file, and the ID checks for deps.toml and the ADRs.
// @implements CT-DEV-002, CT-DEV-004, CT-USE-007
impl Checker {
    fn check_file(&mut self, path: &str, syntax: Syntax, text: &str) {
        let result = scan::scan_source(syntax, text);
        for problem in &result.problems {
            let help = match problem.kind {
                ProblemKind::OrphanTag(_) => {
                    "put the tag in the comment directly above a function, type, or impl block"
                }
                ProblemKind::EmptyTag(_) => "list at least one requirement ID after the tag",
                ProblemKind::TagNotAllowed(TagKind::File) => {
                    "use `@file` only in the first comment of a file_level_allowed file"
                }
                ProblemKind::TagNotAllowed(_) => {
                    "use the tags docs/TRACEABILITY.md allows for this file type"
                }
                ProblemKind::NestedBegin | ProblemKind::UnclosedBegin => {
                    "close each `@begin` with an `@end` before the next `@begin`"
                }
                ProblemKind::EndWithoutBegin => "remove the `@end` or add the missing `@begin`",
                ProblemKind::MissingFileTag => "add `# @file CT-XXX-NNN` as the first comment",
                ProblemKind::SyntaxError => "fix the syntax error so item spans can be found",
                ProblemKind::NotAboveStep => {
                    "put `# @verifies IDs` in the comment directly above the step's `- name:` line"
                }
                ProblemKind::ContinueOnError(_) => {
                    "remove `continue-on-error` from the step and its job (or set it to false), or remove the `@verifies` tag"
                }
            };
            let message = problem.to_string();
            let message = message.split_once(": ").map_or(&*message, |(_, m)| m);
            self.push(
                path,
                problem.line,
                Severity::Error,
                message.to_string(),
                help,
            );
        }
        for tag in &result.tags {
            for id in &tag.ids {
                self.check_id(path, tag.line, id);
            }
        }
        let untraced = result.untraced_lines();
        let mut start = 0;
        while start < untraced.len() {
            let mut end = start;
            while end + 1 < untraced.len() && untraced[end + 1] == untraced[end] + 1 {
                end += 1;
            }
            let (first, last) = (untraced[start], untraced[end]);
            let message = if first == last {
                format!("line {first} is not inside a tagged scope")
            } else {
                format!("lines {first}-{last} are not inside a tagged scope")
            };
            self.push(
                path,
                first,
                Severity::Error,
                message,
                "tag the enclosing item with `@implements` (or `@verifies` for a test), or wrap module-level code in `@begin`/`@end`",
            );
            start = end + 1;
        }
        let max = self.config.max_scope_lines;
        for scope in &result.scopes {
            let lines = scope.end_line - scope.start_line + 1;
            // Test scopes are exempt: each test inside carries its own `@verifies` tag.
            let sized = match scope.kind {
                ScopeKind::Item => !scope.implements.is_empty(),
                ScopeKind::Region => true,
                ScopeKind::File | ScopeKind::Import => false,
            };
            if sized && lines > max {
                self.push(
                    path,
                    scope.start_line,
                    Severity::Warning,
                    format!("scope is {lines} lines long, over max_scope_lines ({max})"),
                    "split it, or tag the items inside it so each tag stays meaningful",
                );
            }
        }
    }

    /// Checks the `serves` IDs of every `[[dep]]` in `deps.toml`, and that every direct
    /// dependency in a `Cargo.toml` or `package.json` has a `[[dep]]` entry.
    fn check_deps(&mut self) -> Result<(), SetupError> {
        let text = std::fs::read_to_string(self.root.join(DEPS_FILE)).unwrap_or_default();
        let tables = parse_tables(&text).map_err(|e| setup_error(DEPS_FILE, e))?;
        // (ecosystem, name) of every listed dependency.
        let mut listed = BTreeSet::new();
        for dep in tables.iter().filter(|t| t.array && t.name == "dep") {
            let serves = dep.list("serves").map_err(|e| setup_error(DEPS_FILE, e))?;
            let line = dep.get("serves").map_or(dep.line, |e| e.line);
            for id in serves {
                self.check_id(DEPS_FILE, line, id);
            }
            if let (Ok(name), Ok(ecosystem)) = (dep.string("name"), dep.string("ecosystem")) {
                listed.insert((ecosystem.to_string(), name.to_string()));
            }
        }
        for path in self.manifests()? {
            let Ok(text) = std::fs::read_to_string(self.root.join(&path)) else {
                continue;
            };
            let (ecosystem, deps) = if path.ends_with("package.json") {
                ("npm", package_deps(&text))
            } else {
                ("cargo", manifest_deps(&text))
            };
            for dep in deps {
                if !dep.local && !listed.contains(&(ecosystem.to_string(), dep.name.clone())) {
                    self.push(
                        &path,
                        dep.line,
                        Severity::Error,
                        format!(
                            "{ecosystem} dependency `{}` is not listed in deps.toml",
                            dep.name
                        ),
                        "add a [[dep]] entry with its license, why, and the requirements it serves, after John approves the dependency",
                    );
                }
            }
        }
        Ok(())
    }

    /// Every `Cargo.toml` and `package.json` in the project, skipping build output,
    /// installed packages, and hidden directories.
    fn manifests(&self) -> Result<Vec<String>, SetupError> {
        let mut found = Vec::new();
        let mut dirs = vec![PathBuf::new()];
        while let Some(dir) = dirs.pop() {
            let io = |e: std::io::Error| SetupError {
                path: dir.display().to_string(),
                line: 0,
                message: e.to_string(),
            };
            for entry in std::fs::read_dir(self.root.join(&dir)).map_err(io)? {
                let entry = entry.map_err(io)?;
                let name = entry.file_name().to_string_lossy().into_owned();
                let kind = entry.file_type().map_err(io)?;
                if kind.is_dir()
                    && !(name.starts_with('.') || name == "target" || name == "node_modules")
                {
                    dirs.push(dir.join(&name));
                } else if kind.is_file() && (name == "Cargo.toml" || name == "package.json") {
                    let parts: Vec<_> = dir
                        .components()
                        .map(|c| c.as_os_str().to_string_lossy())
                        .collect();
                    found.push(parts.iter().map(|p| format!("{p}/")).collect::<String>() + &name);
                }
            }
        }
        found.sort();
        Ok(found)
    }

    /// The literal text every ID starts with, used to spot IDs in prose.
    fn id_prefix(&self) -> String {
        self.pattern
            .atoms
            .iter()
            .map_while(|(atom, count)| match atom {
                Atom::Char(c) if *count == 1 => Some(*c),
                _ => None,
            })
            .collect()
    }
}

/// The ADR check.
// @implements CT-DEV-004, CT-DEV-010, CT-USE-007
impl Checker {
    /// Checks every word in an ADR that looks like a requirement ID, and that each ADR
    /// cites at least one requirement it serves.
    fn check_adrs(&mut self) -> Result<(), SetupError> {
        let Ok(entries) = std::fs::read_dir(self.root.join(ADR_DIR)) else {
            return Ok(());
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "md"))
            .collect();
        paths.sort();
        let prefix = self.id_prefix();
        for path in paths {
            let name = format!(
                "{ADR_DIR}/{}",
                path.file_name().unwrap_or_default().to_string_lossy()
            );
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let mut cites = false;
            for (index, line) in text.lines().enumerate() {
                let words = line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'));
                for word in words.filter(|w| !prefix.is_empty() && w.starts_with(&prefix)) {
                    cites |= self.known.contains(word);
                    self.check_id(&name, index + 1, word);
                }
            }
            if !cites {
                self.push(
                    &name,
                    0,
                    Severity::Error,
                    "ADR cites no requirement".into(),
                    "add a `Serves:` line listing the IDs of the requirements this decision serves",
                );
            }
        }
        Ok(())
    }
}

/// The requirement coverage gate and the waivers and checklists it reads.
// @implements CT-DEV-003, CT-DEV-004, CT-USE-007
impl Checker {
    /// Reports each requirement due in a started phase, or with no phase, that has no
    /// implementation or no verification and no waiver, using the same coverage as
    /// `clew report coverage`. Also checks the waivers and the checklists' IDs.
    fn check_requirements(&mut self) -> Result<(), SetupError> {
        let project = report::load_project(&self.root)?;
        let coverage = report::project_coverage(&project);
        for checklist in &project.checklists {
            for id in &checklist.ids {
                self.check_id(&checklist.path, checklist.line, id);
            }
        }
        self.check_documents(&project.documents);
        self.check_waivers();
        let current = self.config.current_phase;
        for (path, req) in std::mem::take(&mut self.requirements) {
            let row = (project.graph.requirement(&req.id))
                .and_then(|e| coverage.requirements.iter().find(|r| r.requirement == e));
            let implemented = row.is_some_and(|r| r.implemented);
            let verified = row.is_some_and(|r| r.verified);
            // A waiver for part of a requirement excuses nothing and is never needless.
            let waiver = (self.config.waivers.iter())
                .find(|w| w.id == req.id && w.part.is_none())
                .map(|w| w.line);
            if let Some(line) = waiver.filter(|_| implemented && verified) {
                let message = format!(
                    "waiver for {} is not needed: it is implemented and verified",
                    req.id
                );
                self.push(
                    TRACE_CONFIG,
                    line,
                    Severity::Warning,
                    message,
                    "remove the waiver",
                );
            }
            let due = req.phase.is_none_or(|p| i64::from(p) <= current);
            if !due || waiver.is_some() || (implemented && verified) {
                continue;
            }
            let missing = match (implemented, verified) {
                (false, false) => "no implementation and no verification",
                (false, true) => "no implementation",
                _ => "no verification",
            };
            let phase = match req.phase {
                Some(p) => format!("phase {p}"),
                None => "no phase".to_string(),
            };
            self.push(
                &path,
                req.line,
                Severity::Error,
                format!("{} ({phase}) is due at current_phase {current} but has {missing}", req.id),
                "tag code with `@implements` and a test with `@verifies` (an Inspection or Demonstration requirement may instead be cited by a Markdown document's `@implements` tag and a docs/verification/ checklist), or propose a [[waiver]] in trace.toml for John to approve",
            );
        }
        Ok(())
    }

    /// A Markdown document may implement only known requirements whose verification method
    /// is Inspection or Demonstration.
    fn check_documents(&mut self, documents: &[report::Document]) {
        for document in documents {
            for id in &document.ids {
                self.check_id(&document.path, document.line, id);
                let Some((_, req)) = self.requirements.iter().find(|(_, r)| &r.id == id) else {
                    continue;
                };
                let method = req.verification_method.as_deref();
                if method.is_some_and(|m| report::CHECKLIST_METHODS.contains(&m)) {
                    continue;
                }
                let method = method.map_or("not set".to_string(), |m| format!("`{m}`"));
                self.push(
                    &document.path,
                    document.line,
                    Severity::Error,
                    format!(
                        "document implements {id}, whose verification method is {method}, not Inspection or Demonstration"
                    ),
                    "implement it in code with `@implements` and a `@verifies` test, or remove it from the document's `@implements` tag",
                );
            }
        }
    }

    /// Each waiver needs a known ID, a reason, a `YYYY-MM-DD` date, and a status: `approved`
    /// with the approver's name, or `proposed` with an empty `approved_by`. A proposed
    /// waiver is a warning until it is approved.
    fn check_waivers(&mut self) {
        let mut seen = BTreeSet::new();
        for w in self.config.waivers.clone() {
            let line = w.line;
            self.check_id(TRACE_CONFIG, line, &w.id);
            let error = |checker: &mut Checker, message: String, help: &str| {
                checker.push(TRACE_CONFIG, line, Severity::Error, message, help);
            };
            if !seen.insert(w.id.clone()) {
                error(
                    self,
                    format!("{} has more than one waiver", w.id),
                    "keep one [[waiver]] per requirement",
                );
            }
            if w.part.as_deref().is_some_and(|p| p.trim().is_empty()) {
                error(
                    self,
                    format!("waiver for {} has an empty part", w.id),
                    "name the part of the requirement it covers, or remove `part`",
                );
            }
            if w.reason.trim().is_empty() {
                error(
                    self,
                    format!("waiver for {} has an empty reason", w.id),
                    "say why the requirement cannot be met yet and when it will be",
                );
            }
            if !is_date(&w.date) {
                error(
                    self,
                    format!("waiver for {} has date `{}`, not YYYY-MM-DD", w.id, w.date),
                    "write the date the waiver was approved or proposed, like 2026-10-22",
                );
            }
            match (w.status.as_str(), w.approved_by.trim().is_empty()) {
                ("approved", false) | ("proposed", true) => {}
                ("approved", true) => error(
                    self,
                    format!("approved waiver for {} names no approver", w.id),
                    "set approved_by to the person who approved it",
                ),
                ("proposed", false) => error(
                    self,
                    format!("proposed waiver for {} has an approver", w.id),
                    "set status = \"approved\" once it is approved, or clear approved_by",
                ),
                (status, _) => error(
                    self,
                    format!("waiver for {} has unknown status `{status}`", w.id),
                    "use status = \"approved\" or status = \"proposed\"",
                ),
            }
            if w.status == "proposed" {
                self.push(
                    TRACE_CONFIG,
                    line,
                    Severity::Warning,
                    format!("waiver for {} is proposed and not approved", w.id),
                    "John approves it by setting status = \"approved\" and approved_by, or drops it",
                );
            }
        }
    }
}

/// Whether `text` is a date written `YYYY-MM-DD` with a month from 01 to 12 and a day
/// from 01 to 31.
// @implements CT-DEV-003, CT-DEV-004
fn is_date(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    let number = |s: &str, len: usize| {
        (s.len() == len && s.bytes().all(|b| b.is_ascii_digit())).then(|| s.parse::<u32>().ok())?
    };
    match parts[..] {
        [y, m, d] => {
            number(y, 4).is_some()
                && number(m, 2).is_some_and(|m| (1..=12).contains(&m))
                && number(d, 2).is_some_and(|d| (1..=31).contains(&d))
        }
        _ => false,
    }
}

/// One dependency declared in a `Cargo.toml`. `local` marks path and workspace-inherited
/// dependencies, which are first-party or listed where the workspace declares them.
// @implements CT-DEV-008
#[derive(Debug, Clone, PartialEq, Eq)]
struct ManifestDep {
    name: String,
    line: usize,
    local: bool,
}

/// Reads the dependency names from a `package.json`: the keys of its top-level
/// `dependencies`, `devDependencies`, `optionalDependencies`, and `peerDependencies`
/// objects. A small JSON tokenizer is enough, since only object keys and nesting matter.
// @implements CT-DEV-008
fn package_deps(text: &str) -> Vec<ManifestDep> {
    const SECTIONS: [&str; 4] = [
        "dependencies",
        "devDependencies",
        "optionalDependencies",
        "peerDependencies",
    ];
    let mut deps = Vec::new();
    let mut depth = 0usize;
    let mut line = 1;
    // The last string read and its line, until the next token shows whether it is a key.
    let mut last: Option<(String, usize)> = None;
    // The top-level key whose value is being read, and whether that key is a section.
    let mut key: Option<String> = None;
    let mut in_section = false;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\n' => line += 1,
            '"' => {
                let mut value = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => value.extend(chars.next()),
                        '\n' => line += 1,
                        c => value.push(c),
                    }
                }
                last = Some((value, line));
            }
            ':' => {
                if let Some((name, at)) = last.take() {
                    if depth == 1 {
                        key = Some(name);
                    } else if depth == 2 && in_section {
                        deps.push(ManifestDep {
                            name,
                            line: at,
                            local: false,
                        });
                    }
                }
            }
            '{' | '[' => {
                if depth == 1 {
                    in_section = c == '{' && key.as_deref().is_some_and(|k| SECTIONS.contains(&k));
                }
                depth += 1;
                last = None;
            }
            '}' | ']' => {
                depth = depth.saturating_sub(1);
                if depth == 1 {
                    in_section = false;
                }
                last = None;
            }
            _ => {}
        }
    }
    deps
}

/// Reads the dependency names from a `Cargo.toml`, line by line. Cargo files use inline
/// tables and booleans, which the `config` reader does not take, and only the names and a
/// few keys matter here.
// @implements CT-DEV-008
fn manifest_deps(text: &str) -> Vec<ManifestDep> {
    let is_deps = |h: &str| {
        h == "dependencies" || h.ends_with("-dependencies") || h.ends_with(".dependencies")
    };
    let unquote = |t: &str| t.trim().trim_matches(|c| c == '"' || c == '\'').to_string();
    let mut deps: Vec<ManifestDep> = Vec::new();
    // Some(true) inside a [dependencies] table, Some(false) inside a [dependencies.name] table.
    let mut section = None;
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        if let Some(header) = line.strip_prefix('[') {
            let header = header
                .trim_start_matches('[')
                .split(']')
                .next()
                .unwrap_or("")
                .trim();
            section = None;
            if is_deps(header) {
                section = Some(true);
            } else if let Some((parent, name)) = header.rsplit_once('.')
                && is_deps(parent)
            {
                section = Some(false);
                deps.push(ManifestDep {
                    name: unquote(name),
                    line: index + 1,
                    local: false,
                });
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (unquote(key), value.trim());
        match section {
            Some(true) => {
                let mut dep = ManifestDep {
                    name: key,
                    line: index + 1,
                    local: false,
                };
                // Inline table: { version = "1", package = "real-name", path = "../x" }
                let inner = value.trim_start_matches('{').trim_end_matches('}');
                for (k, v) in inner.split(',').filter_map(|p| p.split_once('=')) {
                    match k.trim() {
                        "path" | "workspace" => dep.local = true,
                        "package" => dep.name = unquote(v),
                        _ => {}
                    }
                }
                deps.push(dep);
            }
            Some(false) => {
                if let Some(dep) = deps.last_mut() {
                    match key.as_str() {
                        "path" | "workspace" => dep.local = true,
                        "package" => dep.name = unquote(value),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    deps
}

// @verifies CT-DEV-002, CT-DEV-004, CT-DEV-008
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // @verifies CT-DEV-004
    #[test]
    fn id_pattern_matches_the_documented_shape() {
        let pattern =
            IdPattern::parse("CT-(EFF|USE|TRC|MOD|XCH|CMP|SEC|PLT|DEV)-[0-9]{3}").unwrap();
        for good in ["CT-TRC-004", "CT-EFF-001", "CT-DEV-013"] {
            assert!(pattern.matches(good), "{good}");
        }
        for bad in [
            "CT-TRC-04",
            "CT-TRC-0044",
            "CT-ABC-004",
            "ct-TRC-004",
            "CT-TRC-00x",
            "",
        ] {
            assert!(!pattern.matches(bad), "{bad}");
        }
        assert!(IdPattern::parse("^A(B|BC)C$").unwrap().matches("ABCC"));
        assert!(IdPattern::parse("CT-.*").is_err());
        assert!(IdPattern::parse("{3}").is_err());
    }

    // @verifies CT-DEV-002
    #[test]
    fn globs_match_paths() {
        let cases = [
            ("*.toml", "crates/clew-core/Cargo.toml", true),
            ("*.toml", "crates/x.rs", false),
            ("app/src-tauri/gen/**", "app/src-tauri/gen/a/b.rs", true),
            ("app/src-tauri/gen/**", "app/src-tauri/other.rs", false),
            ("app/*/x.json", "app/a/x.json", true),
            ("app/*/x.json", "app/a/b/x.json", false),
            ("app/**/x.json", "app/x.json", true),
            ("app/**/x.json", "app/a/b/x.json", true),
            ("a?c.rs", "abc.rs", true),
        ];
        for (glob, path, want) in cases {
            assert_eq!(glob_matches(glob, path), want, "{glob} vs {path}");
        }
    }

    /// Builds a project under the system temp directory.
    // @verifies CT-DEV-002, CT-DEV-004
    fn project(name: &str, extra_config: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("clew-check-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("model")).unwrap();
        fs::write(
            root.join("model/req.sysml"),
            "requirement <'CT-DEV-002'> a : R { attribute :>> phase = 1; }\n\
             requirement <'CT-DEV-004'> b : R { attribute :>> phase = 1; }\n\
             requirement def R { attribute phase; }\n",
        )
        .unwrap();
        let config = format!(
            "[project]\nrequirements = [\"model/req.sysml\"]\nid_pattern = \"CT-[A-Z]{{3}}-[0-9]{{3}}\"\n\
             current_phase = 0\n[coverage]\nsource_roots = [\"src\", \"missing\"]\n\
             languages = [\"rust\", \"svelte\"]\nfile_level_allowed = [\"*.yml\"]\nmax_scope_lines = 3\n{extra_config}"
        );
        fs::write(root.join(TRACE_CONFIG), config).unwrap();
        for (path, text) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        root
    }

    // @verifies CT-DEV-002, CT-DEV-004
    fn summary(report: &Report) -> Vec<(String, usize, Severity)> {
        let rows = report.findings.iter();
        rows.map(|f| (f.path.clone(), f.line, f.severity)).collect()
    }

    // @verifies CT-DEV-002, CT-DEV-004
    #[test]
    fn clean_project_has_no_findings() {
        let root = project(
            "clean",
            "",
            &[
                (
                    "src/a.rs",
                    "use std::fmt;\n// @implements CT-DEV-002\nfn a() {}\n",
                ),
                ("src/ci.yml", "# @file CT-DEV-004\nname: ci\n"),
                (
                    "src/b.svelte",
                    "<script lang=\"ts\">\n  // @begin CT-DEV-002\n  let a = 1;\n  // @end\n</script>\n",
                ),
            ],
        );
        let report = check_project(&root).unwrap();
        assert_eq!(report.files, 3);
        assert!(report.findings.is_empty(), "{:?}", report.findings);
    }

    // @verifies CT-DEV-002, CT-DEV-004, CT-USE-007
    #[test]
    fn reports_every_kind_of_finding() {
        let root = project(
            "findings",
            "[[exclude]]\npath = \"src/gen/**\"\nreason = \"Generated\"\nrequirement = \"CT-DEV-002\"\n\
             [[exclude]]\npath = \"nothing/**\"\nreason = \" \"\nrequirement = \"CT-XYZ-001\"\n",
            &[
                (
                    "src/a.rs",
                    "fn a() {}\nfn b() {}\n// @implements CT-DEV-999, ct-dev-002\nfn c() {\n    1;\n    2;\n}\n",
                ),
                ("src/gen/out.rs", "fn untraced() {}\n"),
                ("src/config.json", "{}\n"),
                ("src/ci.yml", "name: ci\n"),
                (
                    "deps.toml",
                    "[[dep]]\nname = \"x\"\nserves = [\"CT-DEV-002\", \"CT-ABC-123\"]\n",
                ),
                (
                    "docs/adr/0001-x.md",
                    "# ADR\n\nServes: CT-DEV-004, CT-DEV-777.\n",
                ),
            ],
        );
        let report = check_project(&root).unwrap();
        let want = vec![
            ("deps.toml".to_string(), 3, Severity::Error),
            ("docs/adr/0001-x.md".to_string(), 3, Severity::Error),
            ("src/a.rs".to_string(), 1, Severity::Error),
            ("src/a.rs".to_string(), 3, Severity::Error),
            ("src/a.rs".to_string(), 3, Severity::Error),
            ("src/a.rs".to_string(), 4, Severity::Warning),
            ("src/ci.yml".to_string(), 1, Severity::Error),
            ("src/ci.yml".to_string(), 1, Severity::Error),
            ("src/config.json".to_string(), 0, Severity::Error),
            (TRACE_CONFIG.to_string(), 14, Severity::Error),
            (TRACE_CONFIG.to_string(), 14, Severity::Error),
            (TRACE_CONFIG.to_string(), 14, Severity::Warning),
        ];
        assert_eq!(summary(&report), want, "{:#?}", report.findings);
        assert_eq!(report.files, 3);
        let text: Vec<String> = report.findings.iter().map(|f| f.to_string()).collect();
        assert!(text.contains(&"src/a.rs:1: error: lines 1-2 are not inside a tagged scope\n  help: tag the enclosing item with `@implements` (or `@verifies` for a test), or wrap module-level code in `@begin`/`@end`".to_string()), "{text:#?}");
        assert!(
            text.iter()
                .any(|t| t.starts_with("src/a.rs:3: error: unknown requirement ID CT-DEV-999"))
        );
        assert!(text.iter().any(|t| t.starts_with("src/a.rs:3: error: `ct-dev-002` does not match id_pattern")));
        assert!(
            text.iter()
                .any(|t| t.starts_with("src/config.json: error: file type cannot be traced"))
        );
        assert!(
            text.iter()
                .any(|t| t.starts_with("src/ci.yml:1: error: first comment has no `@file` tag"))
        );
        assert_eq!(report.count(Severity::Error), 10);
        assert_eq!(report.count(Severity::Warning), 2);
    }

    // @verifies CT-DEV-002
    #[test]
    fn long_test_modules_do_not_warn() {
        let items = "    fn a() {}\n    fn b() {}\n    fn c() {}\n";
        let statements = "    1;\n    2;\n    3;\n";
        let root = project(
            "scope-kinds",
            "",
            &[(
                "src/a.rs",
                &format!(
                    "// @verifies CT-DEV-002\nmod tests {{\n{items}}}\n\
                     // @implements CT-DEV-002\nfn a() {{\n{statements}}}\n\
                     // @begin CT-DEV-002\nconst B: u8 = 1;\nconst C: u8 = 1;\nconst D: u8 = 1;\nconst E: u8 = 1;\n// @end\n"
                ),
            )],
        );
        let report = check_project(&root).unwrap();
        let want = vec![
            ("src/a.rs".to_string(), 8, Severity::Warning),
            ("src/a.rs".to_string(), 13, Severity::Warning),
        ];
        assert_eq!(summary(&report), want, "{:#?}", report.findings);
    }

    /// A model where every requirement is typed by `R`, which has a phase and a
    /// verification method. Each entry is (ID, name, phase, method).
    // @verifies CT-DEV-003
    fn model(reqs: &[(&str, &str, Option<u8>, &str)]) -> String {
        let mut text =
            String::from("requirement def R { attribute phase; attribute verificationMethod; }\n");
        for (id, name, phase, method) in reqs {
            let phase = phase.map_or(String::new(), |p| format!(" attribute :>> phase = {p};"));
            text.push_str(&format!(
                "requirement <'{id}'> {name} : R {{{phase} attribute :>> verificationMethod = \"{method}\"; }}\n"
            ));
        }
        text
    }

    /// A `[[waiver]]` table.
    // @verifies CT-DEV-003
    fn waiver(id: &str, reason: &str, status: &str, approved_by: &str, date: &str) -> String {
        format!(
            "[[waiver]]\nid = \"{id}\"\nreason = \"{reason}\"\nstatus = \"{status}\"\n\
             approved_by = \"{approved_by}\"\ndate = \"{date}\"\n"
        )
    }

    // @verifies CT-DEV-003, CT-DEV-004, CT-USE-007
    #[test]
    fn reports_due_requirements_without_coverage_or_waiver() {
        let reqs = model(&[
            ("CT-DEV-002", "covered", Some(0), "Test"),
            ("CT-DEV-003", "untested", Some(0), "Test"),
            ("CT-DEV-004", "bare", Some(0), "Test"),
            ("CT-DEV-005", "later", Some(1), "Test"),
            ("CT-DEV-008", "phaseless", None, "Test"),
            ("CT-DEV-010", "inspected", Some(0), "Inspection"),
            ("CT-DEV-012", "unimplemented", Some(0), "Test"),
            ("CT-DEV-013", "waived", Some(0), "Inspection"),
            ("CT-DEV-014", "proposed", Some(0), "Test"),
            ("CT-DEV-009", "needless", Some(0), "Test"),
        ]);
        let waivers = waiver(
            "CT-DEV-013",
            "Policy",
            "approved",
            "John Schubert",
            "2026-10-22",
        ) + &waiver("CT-DEV-014", "Release only", "proposed", "", "2026-10-22")
            + &waiver(
                "CT-DEV-009",
                "Old",
                "approved",
                "John Schubert",
                "2026-10-21",
            );
        let root = project(
            "gate",
            &waivers,
            &[
                ("model/req.sysml", reqs.as_str()),
                (
                    "src/a.rs",
                    "// @implements CT-DEV-002, CT-DEV-003, CT-DEV-010, CT-DEV-009\nfn a() {}\n\
                     // @verifies CT-DEV-002, CT-DEV-012, CT-DEV-009\n#[test]\nfn t() {}\n",
                ),
                (
                    "docs/verification/c.md",
                    "<!-- @verifies CT-DEV-010, CT-DEV-999 -->\n# Check\n",
                ),
            ],
        );
        let report = check_project(&root).unwrap();
        let text: Vec<String> = report.findings.iter().map(|f| f.to_string()).collect();
        let first_lines: Vec<&str> = text.iter().map(|t| t.lines().next().unwrap()).collect();
        assert_eq!(
            first_lines,
            [
                "docs/verification/c.md:1: error: unknown requirement ID CT-DEV-999",
                "model/req.sysml:3: error: CT-DEV-003 (phase 0) is due at current_phase 0 but has no verification",
                "model/req.sysml:4: error: CT-DEV-004 (phase 0) is due at current_phase 0 but has no implementation and no verification",
                "model/req.sysml:6: error: CT-DEV-008 (no phase) is due at current_phase 0 but has no implementation and no verification",
                "model/req.sysml:8: error: CT-DEV-012 (phase 0) is due at current_phase 0 but has no implementation",
                "trace.toml:16: warning: waiver for CT-DEV-014 is proposed and not approved",
                "trace.toml:22: warning: waiver for CT-DEV-009 is not needed: it is implemented and verified",
            ]
        );
        assert!(text[1].ends_with(
            "help: tag code with `@implements` and a test with `@verifies` (an Inspection or \
             Demonstration requirement may instead be cited by a Markdown document's \
             `@implements` tag and a docs/verification/ checklist), or propose a [[waiver]] in \
             trace.toml for John to approve"
        ));
    }

    // @verifies CT-DEV-003, CT-DEV-004, CT-USE-007
    #[test]
    fn documents_implement_only_known_inspections_and_demonstrations() {
        let reqs = model(&[
            ("CT-DEV-002", "inspected", Some(0), "Inspection"),
            ("CT-DEV-003", "shown", Some(1), "Demonstration"),
            ("CT-DEV-004", "tested", Some(1), "Test"),
        ]) + "requirement <'CT-DEV-005'> unset : R { attribute :>> phase = 1; }\n";
        let root = project(
            "documents",
            "",
            &[
                ("model/req.sysml", reqs.as_str()),
                (
                    "LICENSING.md",
                    "<!-- @implements CT-DEV-002 -->\n# Licensing\n",
                ),
                (
                    "docs/guide.md",
                    "<!--\n  @implements CT-DEV-003, CT-DEV-004, CT-DEV-005, CT-DEV-999\n-->\n",
                ),
                ("docs/verification/c.md", "<!-- @verifies CT-DEV-002 -->\n"),
            ],
        );
        let report = check_project(&root).unwrap();
        let text: Vec<String> = report.findings.iter().map(|f| f.to_string()).collect();
        let first_lines: Vec<&str> = text.iter().map(|t| t.lines().next().unwrap()).collect();
        assert_eq!(
            first_lines,
            [
                "docs/guide.md:2: error: document implements CT-DEV-004, whose verification method is `Test`, not Inspection or Demonstration",
                "docs/guide.md:2: error: document implements CT-DEV-005, whose verification method is not set, not Inspection or Demonstration",
                "docs/guide.md:2: error: unknown requirement ID CT-DEV-999",
            ]
        );
        assert!(text[0].ends_with(
            "help: implement it in code with `@implements` and a `@verifies` test, or remove it \
             from the document's `@implements` tag"
        ));
    }

    // @verifies CT-DEV-003, CT-DEV-004, CT-TRC-006, CT-USE-007
    #[test]
    fn workflow_steps_verify_only_when_their_failure_fails_the_build() {
        let reqs = model(&[
            ("CT-DEV-002", "released", Some(0), "Test"),
            ("CT-DEV-004", "lenient", Some(1), "Test"),
        ]);
        let workflow = "# @file CT-DEV-002\njobs:\n  build:\n    steps:\n\
                        \x20     # @verifies CT-DEV-002\n      - name: Size\n        run: exit 1\n\
                        \x20     # @verifies CT-DEV-004\n      - name: Warn\n\
                        \x20       continue-on-error: true\n\
                        \x20     # @verifies CT-DEV-004\n";
        let root = project(
            "workflow",
            "",
            &[
                ("model/req.sysml", reqs.as_str()),
                ("src/a.rs", "// @implements CT-DEV-002\nfn a() {}\n"),
                ("src/.github/workflows/release.yml", workflow),
            ],
        );
        let report = check_project(&root).unwrap();
        let text: Vec<String> = report.findings.iter().map(|f| f.to_string()).collect();
        assert_eq!(
            text,
            [
                "src/.github/workflows/release.yml:8: error: `continue-on-error` on line 10 lets \
                 this step fail without failing the build\n  help: remove `continue-on-error` \
                 from the step and its job (or set it to false), or remove the `@verifies` tag",
                "src/.github/workflows/release.yml:11: error: `@verifies` is not directly above a \
                 workflow step's `- name:` line\n  help: put `# @verifies IDs` in the comment \
                 directly above the step's `- name:` line",
            ]
        );
    }

    // @verifies CT-DEV-003, CT-DEV-004, CT-USE-007
    #[test]
    fn a_waiver_for_part_of_a_requirement_excuses_nothing() {
        let reqs = model(&[
            ("CT-DEV-002", "covered", Some(0), "Test"),
            ("CT-DEV-004", "bare", Some(0), "Test"),
            ("CT-DEV-008", "later", Some(1), "Test"),
        ]);
        let approved = |id: &str, part: &str| {
            waiver(id, "ARM64 later", "approved", "John Schubert", "2026-10-22")
                + &format!("part = \"{part}\"\n")
        };
        let waivers = approved("CT-DEV-002", "ARM64 installers")
            + &approved("CT-DEV-004", "ARM64 installers")
            + &approved("CT-DEV-008", " ");
        let root = project(
            "part",
            &waivers,
            &[
                ("model/req.sysml", reqs.as_str()),
                (
                    "src/a.rs",
                    "// @implements CT-DEV-002\nfn a() {}\n// @verifies CT-DEV-002\n#[test]\nfn t() {}\n",
                ),
            ],
        );
        let report = check_project(&root).unwrap();
        let text: Vec<String> = report.findings.iter().map(|f| f.to_string()).collect();
        let first_lines: Vec<&str> = text.iter().map(|t| t.lines().next().unwrap()).collect();
        assert_eq!(
            first_lines,
            [
                "model/req.sysml:3: error: CT-DEV-004 (phase 0) is due at current_phase 0 but has no implementation and no verification",
                "trace.toml:24: error: waiver for CT-DEV-008 has an empty part",
            ]
        );
        assert!(
            text[1].ends_with("help: name the part of the requirement it covers, or remove `part`")
        );
    }

    // @verifies CT-DEV-003, CT-DEV-004, CT-USE-007
    #[test]
    fn waivers_need_a_known_id_reason_date_and_approval() {
        let waivers = waiver(
            "CT-DEV-999",
            "Unknown",
            "approved",
            "John Schubert",
            "2026-10-22",
        ) + &waiver("CT-DEV-002", " ", "approved", "", "2026-13-01")
            + &waiver(
                "CT-DEV-002",
                "Twice",
                "proposed",
                "John Schubert",
                "22-10-2026",
            )
            + &waiver("CT-DEV-004", "Odd", "maybe", "", "2026-1-01");
        let root = project("waivers", &waivers, &[]);
        let report = check_project(&root).unwrap();
        let text: Vec<String> = report.findings.iter().map(|f| f.to_string()).collect();
        let first_lines: Vec<&str> = text.iter().map(|t| t.lines().next().unwrap()).collect();
        assert_eq!(
            first_lines,
            [
                "trace.toml:10: error: unknown requirement ID CT-DEV-999",
                "trace.toml:16: error: approved waiver for CT-DEV-002 names no approver",
                "trace.toml:16: error: waiver for CT-DEV-002 has an empty reason",
                "trace.toml:16: error: waiver for CT-DEV-002 has date `2026-13-01`, not YYYY-MM-DD",
                "trace.toml:22: error: CT-DEV-002 has more than one waiver",
                "trace.toml:22: error: proposed waiver for CT-DEV-002 has an approver",
                "trace.toml:22: error: waiver for CT-DEV-002 has date `22-10-2026`, not YYYY-MM-DD",
                "trace.toml:22: warning: waiver for CT-DEV-002 is proposed and not approved",
                "trace.toml:28: error: waiver for CT-DEV-004 has date `2026-1-01`, not YYYY-MM-DD",
                "trace.toml:28: error: waiver for CT-DEV-004 has unknown status `maybe`",
            ]
        );
    }

    // @verifies CT-DEV-003, CT-DEV-004
    #[test]
    fn dates_are_year_month_day() {
        for good in ["2026-10-22", "2000-01-01", "2026-12-31"] {
            assert!(is_date(good), "{good}");
        }
        for bad in [
            "2026-00-10",
            "2026-10-32",
            "2026-10-00",
            "2026-10",
            "2026-10-2a",
            "+026-10-22",
            "2026-10-22-1",
            "",
        ] {
            assert!(!is_date(bad), "{bad}");
        }
    }

    // @verifies CT-DEV-010, CT-DEV-004
    #[test]
    fn adrs_must_cite_a_requirement() {
        let root = project(
            "adrs",
            "",
            &[
                (
                    "docs/adr/0001-none.md",
                    "# ADR 1\n\nNo requirements here.\n",
                ),
                (
                    "docs/adr/0002-unknown.md",
                    "# ADR 2\n\nServes: CT-DEV-777\n",
                ),
                ("docs/adr/0003-good.md", "# ADR 3\n\nServes: CT-DEV-004\n"),
                ("docs/adr/README.txt", "Not an ADR\n"),
            ],
        );
        let report = check_project(&root).unwrap();
        let text: Vec<String> = report.findings.iter().map(|f| f.to_string()).collect();
        assert_eq!(
            text,
            [
                "docs/adr/0001-none.md: error: ADR cites no requirement\n  help: add a \
                 `Serves:` line listing the IDs of the requirements this decision serves",
                "docs/adr/0002-unknown.md: error: ADR cites no requirement\n  help: add a \
                 `Serves:` line listing the IDs of the requirements this decision serves",
                "docs/adr/0002-unknown.md:3: error: unknown requirement ID CT-DEV-777\n  help: \
                 cite an ID from the model (`clew reqs list`), or propose a new requirement",
            ]
        );
    }

    // @verifies CT-DEV-008
    #[test]
    fn reports_dependencies_missing_from_deps_toml() {
        let root = project(
            "deps",
            "",
            &[
                (
                    "Cargo.toml",
                    "[workspace]\nmembers = [\"a\"]\n\n[workspace.dependencies]\nlisted = \"1\"\nshared = \"2\"\n",
                ),
                (
                    "a/Cargo.toml",
                    "[package]\nname = \"a\"\npublish = false\n\n[dependencies]\nlisted = { workspace = true }\n\
                     local = { path = \"../local\" }\nrenamed = { version = \"1\", package = \"real-name\" }\n\
                     \"quoted\" = \"1\"\n\n[target.'cfg(windows)'.dev-dependencies]\nwin = \"1\"\n\n\
                     [build-dependencies.table-form]\nversion = \"1\"\n\n[dependencies.local-table]\npath = \"x\"\n\n\
                     [features]\nnot-a-dep = []\n",
                ),
                (
                    "target/debug/build/Cargo.toml",
                    "[dependencies]\nignored = \"1\"\n",
                ),
                (
                    "deps.toml",
                    "[[dep]]\nname = \"listed\"\necosystem = \"cargo\"\nserves = [\"CT-DEV-002\"]\n\
                     [[dep]]\nname = \"win\"\necosystem = \"npm\"\nserves = [\"CT-DEV-002\"]\n",
                ),
            ],
        );
        let report = check_project(&root).unwrap();
        let want = vec![
            ("Cargo.toml".to_string(), 6, Severity::Error),
            ("a/Cargo.toml".to_string(), 8, Severity::Error),
            ("a/Cargo.toml".to_string(), 9, Severity::Error),
            ("a/Cargo.toml".to_string(), 12, Severity::Error),
            ("a/Cargo.toml".to_string(), 14, Severity::Error),
        ];
        assert_eq!(summary(&report), want, "{:#?}", report.findings);
        let text = report.findings[1].to_string();
        assert!(
            text.starts_with(
                "a/Cargo.toml:8: error: cargo dependency `real-name` is not listed in deps.toml"
            ),
            "{text}"
        );
    }

    // @verifies CT-DEV-008
    #[test]
    fn reports_npm_dependencies_missing_from_deps_toml() {
        let root = project(
            "npm",
            "",
            &[
                (
                    "app/package.json",
                    "{\n  \"name\": \"app\",\n  \"scripts\": { \"dev\": \"vite\" },\n  \"dependencies\": {\n    \"listed\": \"1.0.0\"\n  },\n  \"devDependencies\": {\n    \"@scope/missing\": \"2.0.0\",\n    \"cargo-only\": \"1\", \"esc\\\"aped\": \"1\"\n  },\n  \"config\": { \"dependencies\": { \"nested\": \"1\" } }\n}\n",
                ),
                (
                    "app/node_modules/x/package.json",
                    "{ \"dependencies\": { \"ignored\": \"1\" } }",
                ),
                (
                    "deps.toml",
                    "[[dep]]\nname = \"listed\"\necosystem = \"npm\"\nserves = [\"CT-DEV-002\"]\n\
                     [[dep]]\nname = \"cargo-only\"\necosystem = \"cargo\"\nserves = [\"CT-DEV-002\"]\n",
                ),
            ],
        );
        let report = check_project(&root).unwrap();
        let want = vec![
            ("app/package.json".to_string(), 8, Severity::Error),
            ("app/package.json".to_string(), 9, Severity::Error),
            ("app/package.json".to_string(), 9, Severity::Error),
        ];
        assert_eq!(summary(&report), want, "{:#?}", report.findings);
        let text = report.findings[0].to_string();
        assert!(
            text.starts_with(
                "app/package.json:8: error: npm dependency `@scope/missing` is not listed in deps.toml"
            ),
            "{text}"
        );
        assert!(report.findings[2].to_string().contains("`esc\"aped`"));
    }

    // @verifies CT-DEV-004, CT-USE-007
    #[test]
    fn setup_errors_name_the_file() {
        let root = project("setup", "", &[]);
        fs::write(root.join(TRACE_CONFIG), "[project]\nrequirements = [\n").unwrap();
        let err = check_project(&root).unwrap_err();
        assert_eq!((err.path.as_str(), err.line), (TRACE_CONFIG, 2));
        fs::remove_file(root.join(TRACE_CONFIG)).unwrap();
        let err = check_project(&root).unwrap_err();
        assert!(
            err.to_string().contains("Run clew from the project root"),
            "{err}"
        );
        // The coverage gate reads the whole project, so its setup errors stop the check too.
        let root = project("setup-gate", "top_level_definitions = [\"Nope::R\"]\n", &[]);
        let err = check_project(&root).unwrap_err();
        assert!(
            err.to_string().contains("`Nope::R` names no element"),
            "{err}"
        );
    }

    // @verifies CT-DEV-004, CT-USE-007
    #[test]
    fn requirement_errors_name_the_file_line_and_column() {
        let root = project("bad-reqs", "", &[]);
        fs::write(
            root.join("model/req.sysml"),
            "requirement <'CT-DEV-002'> a {
",
        )
        .unwrap();
        let err = check_project(&root).unwrap_err();
        assert!(
            err.to_string()
                .starts_with("model/req.sysml:1:30: error: `{` is never closed"),
            "{err}"
        );
        fs::remove_file(root.join("model/req.sysml")).unwrap();
        let err = check_project(&root).unwrap_err();
        assert_eq!((err.path.as_str(), err.line), ("model/req.sysml", 0));
    }
}
