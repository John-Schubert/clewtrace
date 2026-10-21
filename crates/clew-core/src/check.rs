//! `clew check --self`: scans the source roots in `trace.toml` and reports untraced lines,
//! unknown requirement IDs, and oversized scopes. Rules: docs/TRACEABILITY.md.

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::config::{ConfigError, TraceConfig, parse_tables};
use crate::reqs;
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
    report: Report,
}

/// Reads `trace.toml`, its `id_pattern`, and the known requirement IDs under `root`.
// @implements CT-DEV-004, CT-DEV-005, CT-USE-007
pub fn load_ids(root: &Path) -> Result<(TraceConfig, IdPattern, BTreeSet<String>), SetupError> {
    let config_path = root.join(TRACE_CONFIG);
    if !config_path.is_file() {
        return Err(SetupError {
            path: TRACE_CONFIG.to_string(),
            line: 0,
            message: "not found. Run clew from the project root, where trace.toml lives".into(),
        });
    }
    let config = TraceConfig::read(&config_path).map_err(|e| setup_error(TRACE_CONFIG, e))?;
    let pattern = IdPattern::parse(&config.id_pattern).map_err(|message| SetupError {
        path: TRACE_CONFIG.to_string(),
        line: 0,
        message: format!("id_pattern: {message}"),
    })?;
    let mut known = BTreeSet::new();
    for file in &config.requirements {
        let all = reqs::read_requirements(&root.join(file)).map_err(|e| SetupError {
            path: file.clone(),
            line: 0,
            message: e.to_string(),
        })?;
        known.extend(all.into_iter().map(|r| r.id));
    }
    Ok((config, pattern, known))
}

/// Runs every check on the project rooted at `root`.
// @implements CT-DEV-002, CT-DEV-004, CT-TRC-008
pub fn check_project(root: &Path) -> Result<Report, SetupError> {
    let (config, pattern, known) = load_ids(root)?;
    let mut checker = Checker {
        root: root.to_path_buf(),
        config,
        pattern,
        known,
        report: Report::default(),
    };
    checker.check_config()?;
    checker.check_sources()?;
    checker.check_deps()?;
    checker.check_adrs()?;
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

    /// Every file under the source roots, as `/`-separated paths relative to the root.
    fn source_files(&self) -> Result<Vec<String>, SetupError> {
        let mut files = Vec::new();
        let mut dirs: Vec<PathBuf> = self
            .config
            .source_roots
            .iter()
            .map(|r| self.root.join(r))
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
                    let relative = relative.strip_prefix(&self.root).unwrap_or(&relative);
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

    /// The syntax to scan a file with, or `None` when the file cannot be traced.
    fn syntax_for(&self, path: &str) -> Option<Syntax> {
        let syntax = Syntax::from_path(Path::new(path))?;
        let allowed = match syntax {
            Syntax::Rust => self.config.languages.iter().any(|l| l == "rust"),
            Syntax::TypeScript => self.config.languages.iter().any(|l| l == "typescript"),
            Syntax::Svelte => self.config.languages.iter().any(|l| l == "svelte"),
            Syntax::HashComments => self
                .config
                .file_level_allowed
                .iter()
                .any(|g| glob_matches(g, path)),
        };
        allowed.then_some(syntax)
    }

    fn check_sources(&mut self) -> Result<(), SetupError> {
        let files = self.source_files()?;
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
            let Some(syntax) = self.syntax_for(&path) else {
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
            if matches!(scope.kind, ScopeKind::Item | ScopeKind::Region) && lines > max {
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

    /// Checks the `serves` IDs of every `[[dep]]` in `deps.toml`.
    fn check_deps(&mut self) -> Result<(), SetupError> {
        let Ok(text) = std::fs::read_to_string(self.root.join(DEPS_FILE)) else {
            return Ok(());
        };
        let tables = parse_tables(&text).map_err(|e| setup_error(DEPS_FILE, e))?;
        for dep in tables.iter().filter(|t| t.array && t.name == "dep") {
            let serves = dep.list("serves").map_err(|e| setup_error(DEPS_FILE, e))?;
            let line = dep.get("serves").map_or(dep.line, |e| e.line);
            for id in serves {
                self.check_id(DEPS_FILE, line, id);
            }
        }
        Ok(())
    }

    /// Checks every word in an ADR that looks like a requirement ID.
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
            for (index, line) in text.lines().enumerate() {
                let words = line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'));
                for word in words.filter(|w| !prefix.is_empty() && w.starts_with(&prefix)) {
                    self.check_id(&name, index + 1, word);
                }
            }
        }
        Ok(())
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

// @verifies CT-DEV-002, CT-DEV-004
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
            "requirement <'CT-DEV-002'> a : R {\n}\nrequirement <'CT-DEV-004'> b : R {\n}\n",
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
    }
}
