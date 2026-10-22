//! Reader for `trace.toml` and `deps.toml`.
//! It reads only the TOML subset those files use: `[table]` and `[[array]]` headers, and
//! `key = value` lines whose value is a basic string, an integer, or a one-line array of
//! basic strings. Anything else is an error with its line number.

use std::fmt;
use std::path::Path;

// @implements CT-DEV-002, CT-DEV-004
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Str(String),
    Int(i64),
    List(Vec<String>),
}

// @implements CT-DEV-002, CT-DEV-004
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub value: Value,
    pub line: usize,
}

/// One table. The unnamed table before the first header has an empty name.
// @implements CT-DEV-002, CT-DEV-004
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub name: String,
    pub array: bool,
    pub line: usize,
    pub entries: Vec<Entry>,
}

// @implements CT-DEV-002, CT-DEV-004, CT-USE-007
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub line: usize,
    pub message: String,
}

// @implements CT-DEV-002, CT-DEV-004, CT-USE-007
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

// @implements CT-DEV-002, CT-DEV-004
impl Table {
    pub fn get(&self, key: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.key == key)
    }

    fn missing(&self, key: &str) -> ConfigError {
        let header = if self.array {
            format!("[[{}]]", self.name)
        } else {
            format!("[{}]", self.name)
        };
        ConfigError {
            line: self.line,
            message: format!("{header} has no `{key}`. Add `{key} = ...` to it"),
        }
    }

    fn wrong_type(entry: &Entry, want: &str) -> ConfigError {
        ConfigError {
            line: entry.line,
            message: format!("`{}` must be {want}", entry.key),
        }
    }

    pub fn string(&self, key: &str) -> Result<&str, ConfigError> {
        let entry = self.get(key).ok_or_else(|| self.missing(key))?;
        match &entry.value {
            Value::Str(s) => Ok(s),
            _ => Err(Table::wrong_type(entry, "a string")),
        }
    }

    pub fn int(&self, key: &str) -> Result<i64, ConfigError> {
        let entry = self.get(key).ok_or_else(|| self.missing(key))?;
        match entry.value {
            Value::Int(n) => Ok(n),
            _ => Err(Table::wrong_type(entry, "an integer")),
        }
    }

    pub fn list(&self, key: &str) -> Result<&[String], ConfigError> {
        let entry = self.get(key).ok_or_else(|| self.missing(key))?;
        match &entry.value {
            Value::List(items) => Ok(items),
            _ => Err(Table::wrong_type(entry, "an array of strings")),
        }
    }
}

// @implements CT-DEV-002, CT-DEV-004, CT-USE-007
pub fn parse_tables(text: &str) -> Result<Vec<Table>, ConfigError> {
    let mut tables = vec![Table {
        name: String::new(),
        array: false,
        line: 1,
        entries: Vec::new(),
    }];
    for (index, raw) in text.lines().enumerate() {
        let line_no = index + 1;
        let error = |message: &str| ConfigError {
            line: line_no,
            message: message.to_string(),
        };
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            let (array, rest) = match rest.strip_prefix('[') {
                Some(rest) => (true, rest),
                None => (false, rest),
            };
            let close = if array { "]]" } else { "]" };
            let (name, after) = rest
                .split_once(close)
                .ok_or_else(|| error("table header is not closed"))?;
            if !after.trim().is_empty() && !after.trim().starts_with('#') {
                return Err(error("unexpected text after the table header"));
            }
            let name = name.trim();
            if !array && tables.iter().any(|t| !t.array && t.name == name) {
                return Err(error(&format!("table [{name}] is defined twice")));
            }
            tables.push(Table {
                name: name.to_string(),
                array,
                line: line_no,
                entries: Vec::new(),
            });
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| error("expected `key = value`"))?;
        let key = key.trim();
        if key.is_empty()
            || !key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(error(&format!("`{key}` is not a plain key")));
        }
        let (value, rest) = parse_value(value.trim()).map_err(|m| error(&m))?;
        if !rest.is_empty() && !rest.starts_with('#') {
            return Err(error("unexpected text after the value"));
        }
        let table = tables
            .last_mut()
            .expect("the unnamed table is always present");
        if table.get(key).is_some() {
            return Err(error(&format!("`{key}` is set twice in one table")));
        }
        table.entries.push(Entry {
            key: key.to_string(),
            value,
            line: line_no,
        });
    }
    Ok(tables)
}

/// Parses a value at the start of `text`. Returns it and the trimmed text after it.
// @implements CT-DEV-002, CT-DEV-004
fn parse_value(text: &str) -> Result<(Value, &str), String> {
    if text.starts_with('"') {
        let (s, rest) = parse_string(text)?;
        return Ok((Value::Str(s), rest));
    }
    if let Some(mut rest) = text.strip_prefix('[') {
        let mut items = Vec::new();
        loop {
            rest = rest.trim_start();
            if let Some(after) = rest.strip_prefix(']') {
                return Ok((Value::List(items), after.trim()));
            }
            if !rest.starts_with('"') {
                return Err("arrays must hold strings and close on the same line".into());
            }
            let (item, after) = parse_string(rest)?;
            items.push(item);
            rest = after;
            if let Some(after) = rest.strip_prefix(',') {
                rest = after;
            } else if !rest.starts_with(']') {
                return Err("expected `,` or `]` in the array".into());
            }
        }
    }
    let end = text
        .find(|c: char| c.is_whitespace() || c == '#')
        .unwrap_or(text.len());
    let number = text[..end].replace('_', "");
    number
        .parse()
        .map(|n| (Value::Int(n), text[end..].trim()))
        .map_err(|_| format!("`{}` is not a string, integer, or array", &text[..end]))
}

/// Parses a basic string with `\"` and `\\` escapes. Returns it and the trimmed rest.
// @implements CT-DEV-002, CT-DEV-004
fn parse_string(text: &str) -> Result<(String, &str), String> {
    let mut out = String::new();
    let mut chars = text.char_indices().skip(1);
    while let Some((at, c)) = chars.next() {
        match c {
            '"' => return Ok((out, text[at + 1..].trim())),
            '\\' => match chars.next() {
                Some((_, '"')) => out.push('"'),
                Some((_, '\\')) => out.push('\\'),
                _ => return Err("only `\\\"` and `\\\\` escapes are supported".into()),
            },
            _ => out.push(c),
        }
    }
    Err("string is not closed on the same line".into())
}

/// The `[coverage]` key that lists the top-level requirement definitions.
// @implements CT-TRC-004, CT-DEV-003
pub const TOP_LEVEL_DEFINITIONS: &str = "top_level_definitions";

// @implements CT-DEV-002, CT-DEV-004
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exclude {
    pub path: String,
    pub reason: String,
    pub requirement: String,
    pub line: usize,
}

/// A `[[waiver]]` in `trace.toml`: why a due requirement may lack an implementation or a
/// verification, who approved that, and when. `status` is `approved` or `proposed`; a
/// proposed waiver has an empty `approved_by` until John approves it.
// @implements CT-DEV-003, CT-DEV-004
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiver {
    pub id: String,
    pub reason: String,
    pub status: String,
    pub approved_by: String,
    pub date: String,
    pub line: usize,
}

/// The settings in `trace.toml` that `clew check --self` and `clew report` use.
// @implements CT-DEV-002, CT-DEV-004, CT-TRC-004
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceConfig {
    pub requirements: Vec<String>,
    pub id_pattern: String,
    pub current_phase: i64,
    pub source_roots: Vec<String>,
    pub languages: Vec<String>,
    pub file_level_allowed: Vec<String>,
    pub max_scope_lines: usize,
    /// Qualified names of the requirement definitions whose usages need no parent in
    /// `clew report coverage`, from `[coverage] top_level_definitions`. Empty if not set.
    pub top_level_definitions: Vec<String>,
    /// The line of `top_level_definitions`, or 0 if it is not set.
    pub top_level_line: usize,
    pub excludes: Vec<Exclude>,
    pub waivers: Vec<Waiver>,
}

// @implements CT-DEV-002, CT-DEV-003, CT-DEV-004, CT-USE-007, CT-TRC-004
impl TraceConfig {
    pub fn parse(text: &str) -> Result<TraceConfig, ConfigError> {
        let tables = parse_tables(text)?;
        let table = |name: &str| {
            tables
                .iter()
                .find(|t| !t.array && t.name == name)
                .ok_or_else(|| ConfigError {
                    line: 1,
                    message: format!("no [{name}] table. Add one"),
                })
        };
        let project = table("project")?;
        let coverage = table("coverage")?;
        let max = coverage.int("max_scope_lines")?;
        let max_scope_lines = usize::try_from(max)
            .ok()
            .filter(|&n| n > 0)
            .ok_or_else(|| ConfigError {
                line: coverage.get("max_scope_lines").map_or(1, |e| e.line),
                message: "`max_scope_lines` must be a positive integer".into(),
            })?;
        let top_level_definitions = match coverage.get(TOP_LEVEL_DEFINITIONS) {
            Some(_) => coverage.list(TOP_LEVEL_DEFINITIONS)?.to_vec(),
            None => Vec::new(),
        };
        let top_level_line = coverage.get(TOP_LEVEL_DEFINITIONS).map_or(0, |e| e.line);
        let mut excludes = Vec::new();
        for t in tables.iter().filter(|t| t.array && t.name == "exclude") {
            excludes.push(Exclude {
                path: t.string("path")?.to_string(),
                reason: t.string("reason")?.to_string(),
                requirement: t.string("requirement")?.to_string(),
                line: t.line,
            });
        }
        let mut waivers = Vec::new();
        for t in tables.iter().filter(|t| t.array && t.name == "waiver") {
            waivers.push(Waiver {
                id: t.string("id")?.to_string(),
                reason: t.string("reason")?.to_string(),
                status: t.string("status")?.to_string(),
                approved_by: t.string("approved_by")?.to_string(),
                date: t.string("date")?.to_string(),
                line: t.line,
            });
        }
        Ok(TraceConfig {
            requirements: project.list("requirements")?.to_vec(),
            id_pattern: project.string("id_pattern")?.to_string(),
            current_phase: project.int("current_phase")?,
            source_roots: coverage.list("source_roots")?.to_vec(),
            languages: coverage.list("languages")?.to_vec(),
            file_level_allowed: coverage.list("file_level_allowed")?.to_vec(),
            max_scope_lines,
            top_level_definitions,
            top_level_line,
            excludes,
            waivers,
        })
    }

    pub fn read(path: &Path) -> Result<TraceConfig, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|e| ConfigError {
            line: 0,
            message: e.to_string(),
        })?;
        TraceConfig::parse(&text)
    }
}

// @verifies CT-DEV-002, CT-DEV-004
#[cfg(test)]
mod tests {
    use super::*;

    // @verifies CT-DEV-002, CT-DEV-004
    #[test]
    fn reads_the_toml_subset() {
        let text = "\
# comment
top = 1
[a]
s = \"x \\\"q\\\" # not a comment\" # comment
n = 1_000
l = [\"p\", \"q\",]
e = []
[[dep]]
name = \"one\"
[[dep]]
name = \"two\"
";
        let tables = parse_tables(text).unwrap();
        assert_eq!(tables.len(), 4);
        assert_eq!(tables[0].int("top"), Ok(1));
        assert_eq!(tables[1].string("s"), Ok("x \"q\" # not a comment"));
        assert_eq!(tables[1].int("n"), Ok(1000));
        assert_eq!(tables[1].list("l").unwrap(), ["p", "q"]);
        assert!(tables[1].list("e").unwrap().is_empty());
        assert_eq!((tables[3].name.as_str(), tables[3].array), ("dep", true));
        assert_eq!(tables[3].string("name"), Ok("two"));
    }

    // @verifies CT-DEV-004, CT-USE-007
    #[test]
    fn errors_name_the_line() {
        let cases = [
            ("[a\n", 1, "not closed"),
            ("[a]\nx\n", 2, "key = value"),
            ("[a]\nx = \"open\n", 2, "not closed"),
            ("[a]\nx = [\"a\",\n", 2, "same line"),
            ("[a]\nx = true\n", 2, "not a string"),
            ("[a]\nx = 1\nx = 2\n", 3, "set twice"),
            ("[a]\n[a]\n", 2, "defined twice"),
            ("[a]\nx = 1 y\n", 2, "after the value"),
        ];
        for (text, line, words) in cases {
            let err = parse_tables(text).unwrap_err();
            assert_eq!(err.line, line, "{text:?}");
            assert!(err.message.contains(words), "{text:?}: {}", err.message);
        }
    }

    // @verifies CT-DEV-002, CT-DEV-004
    #[test]
    fn reads_the_real_trace_toml() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../trace.toml");
        let config = TraceConfig::read(&path).unwrap();
        assert_eq!(config.requirements, ["model/requirements.sysml"]);
        assert_eq!(config.max_scope_lines, 200);
        assert!(config.languages.contains(&"rust".to_string()));
        assert!(!config.file_level_allowed.contains(&"*.json".to_string()));
        assert_eq!(
            config.top_level_definitions,
            ["ClewtraceRequirements::ProductRequirement"]
        );
        assert!(config.top_level_line > 0);
    }

    // @verifies CT-TRC-004, CT-DEV-003
    #[test]
    fn top_level_definitions_are_optional_and_must_be_a_list() {
        let text = "[project]\nrequirements = []\nid_pattern = \"x\"\ncurrent_phase = 0\n\
                    [coverage]\nsource_roots = []\nlanguages = []\nfile_level_allowed = []\n\
                    max_scope_lines = 5\n";
        let config = TraceConfig::parse(text).unwrap();
        assert!(config.top_level_definitions.is_empty());
        assert_eq!(config.top_level_line, 0);
        let listed = format!("{text}top_level_definitions = [\"P::R\"]\n");
        let config = TraceConfig::parse(&listed).unwrap();
        assert_eq!(config.top_level_definitions, ["P::R"]);
        assert_eq!(config.top_level_line, 10);
        let wrong = format!("{text}top_level_definitions = \"P::R\"\n");
        let err = TraceConfig::parse(&wrong).unwrap_err();
        assert_eq!(err.line, 10);
        assert_eq!(
            err.message,
            "`top_level_definitions` must be an array of strings"
        );
    }

    // @verifies CT-DEV-003, CT-DEV-004, CT-USE-007
    #[test]
    fn reads_waivers_and_names_missing_keys() {
        let text = "[project]\nrequirements = []\nid_pattern = \"x\"\ncurrent_phase = 0\n\
                    [coverage]\nsource_roots = []\nlanguages = []\nfile_level_allowed = []\n\
                    max_scope_lines = 5\n[[waiver]]\nid = \"CT-DEV-003\"\nreason = \"Later\"\n\
                    status = \"approved\"\napproved_by = \"John Schubert\"\ndate = \"2026-10-22\"\n";
        let config = TraceConfig::parse(text).unwrap();
        assert_eq!(
            config.waivers,
            [Waiver {
                id: "CT-DEV-003".into(),
                reason: "Later".into(),
                status: "approved".into(),
                approved_by: "John Schubert".into(),
                date: "2026-10-22".into(),
                line: 10,
            }]
        );
        let err = TraceConfig::parse(&text.replace("date = \"2026-10-22\"\n", "")).unwrap_err();
        assert_eq!(err.line, 10);
        assert_eq!(
            err.message,
            "[[waiver]] has no `date`. Add `date = ...` to it"
        );
    }

    // @verifies CT-DEV-004, CT-USE-007
    #[test]
    fn config_errors_say_what_is_missing() {
        let err = TraceConfig::parse("[project]\n").unwrap_err();
        assert!(err.message.contains("[coverage]"), "{}", err.message);
        let text = "[project]\nrequirements = []\nid_pattern = \"x\"\ncurrent_phase = 0\n\
                    [coverage]\nsource_roots = []\nlanguages = []\nfile_level_allowed = []\n\
                    max_scope_lines = 0\n";
        let err = TraceConfig::parse(text).unwrap_err();
        assert_eq!(err.line, 9);
        let text = text.replace(
            "max_scope_lines = 0",
            "max_scope_lines = 5\n[[exclude]]\npath = \"x\"",
        );
        let err = TraceConfig::parse(&text).unwrap_err();
        assert_eq!(err.line, 10);
        assert!(
            err.message.contains("[[exclude]] has no `reason`"),
            "{}",
            err.message
        );
    }
}
