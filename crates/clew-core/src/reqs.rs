//! Requirement reader: the requirements in a SysML v2 file, read with the parser.
//!
//! A requirement is a requirement usage with a short name, which is its ID:
//! `requirement <'CT-EFF-001'> installerSize : ProductRequirement { ... }`. Its first `doc`
//! is the statement, and the attributes `verificationMethod` and `phase` are read from
//! their values.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fmt;
use std::path::Path;

use crate::model::{self, Model};
use crate::parse::ast::{Kind, Member, Pos, QualifiedName, Usage, UsageKind};
use crate::parse::parser::{self, SyntaxError};

// @implements CT-DEV-001, CT-TRC-001
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    pub id: String,
    pub name: String,
    pub doc: String,
    pub verification_method: Option<String>,
    pub phase: Option<u8>,
    pub line: usize,
}

// @implements CT-TRC-008, CT-TRC-001
impl Requirement {
    /// The line `clew reqs list` prints: ID, name, verification method, and phase.
    pub fn list_line(&self) -> String {
        format!(
            "{}  {}  {}  phase {}",
            self.id,
            self.name,
            self.verification_method.as_deref().unwrap_or("-"),
            self.phase_text()
        )
    }

    /// The text `clew reqs show` prints: the fields, where it is defined in `path`, and
    /// its doc.
    pub fn show_text(&self, path: &str) -> String {
        format!(
            "{} {}\nVerification: {}\nPhase: {}\nLocation: {path}:{}\n\n{}\n",
            self.id,
            self.name,
            self.verification_method.as_deref().unwrap_or("-"),
            self.phase_text(),
            self.line,
            self.doc
        )
    }

    /// The phase as a number, or `-` when the requirement has none.
    fn phase_text(&self) -> String {
        self.phase
            .map_or_else(|| "-".to_string(), |p| p.to_string())
    }
}

// @implements CT-DEV-001, CT-TRC-001, CT-USE-007
#[derive(Debug)]
pub enum ReadError {
    Io {
        path: String,
        error: std::io::Error,
    },
    /// Syntax errors, unresolved names, duplicate IDs, and bad attribute values, sorted by
    /// position.
    Invalid {
        path: String,
        errors: Vec<SyntaxError>,
    },
}

// @implements CT-DEV-001, CT-TRC-001, CT-USE-007
impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::Io { path, error } => write!(f, "{path}: {error}"),
            ReadError::Invalid { path, errors } => {
                let rendered: Vec<String> = errors.iter().map(|e| e.render(path)).collect();
                write!(f, "{}", rendered.join("\n"))
            }
        }
    }
}

// @implements CT-DEV-001, CT-TRC-001
impl std::error::Error for ReadError {}

// @implements CT-DEV-001, CT-TRC-001, CT-MOD-001
pub fn read_requirements(path: &Path) -> Result<Vec<Requirement>, ReadError> {
    let label = path.display().to_string();
    let text = std::fs::read_to_string(path).map_err(|error| ReadError::Io {
        path: label.clone(),
        error,
    })?;
    parse_requirements(&text, &label)
}

/// Reads the requirements in `text`. `path` names the file in error messages. Names are
/// resolved only when the file has no syntax errors, since a member dropped by error
/// recovery would make names that cite it look unresolved.
// @implements CT-DEV-001, CT-TRC-001, CT-MOD-001, CT-MOD-002, CT-USE-007
pub fn parse_requirements(text: &str, path: &str) -> Result<Vec<Requirement>, ReadError> {
    let parsed = parser::parse(text);
    let mut errors = parsed.errors;
    if errors.is_empty() {
        let model = Model::build(&[(path, &parsed.members)]);
        // Warnings do not stop the reader, and duplicate IDs are reported below.
        let errors_only = (model.errors.into_iter())
            .filter(|e| !e.warning && !e.error.message.starts_with(model::DUPLICATE_ID));
        errors.extend(errors_only.map(|e| e.error));
    }
    let mut found: Vec<(Requirement, Pos)> = Vec::new();
    let defaults = PhaseDefaults::new([parsed.members.as_slice()]);
    collect(&parsed.members, &defaults, &mut found, &mut errors);
    // The line each ID is first used on, so each requirement is checked in one lookup.
    let mut first_lines: HashMap<&str, usize> = HashMap::with_capacity(found.len());
    for (req, pos) in &found {
        match first_lines.entry(&req.id) {
            Entry::Vacant(slot) => {
                slot.insert(req.line);
            }
            Entry::Occupied(first) => errors.push(SyntaxError {
                line: pos.line,
                column: pos.column,
                message: format!(
                    "duplicate requirement ID {}, first used on line {}",
                    req.id,
                    first.get()
                ),
                help: "give each requirement its own ID",
            }),
        }
    }
    if errors.is_empty() {
        return Ok(found.into_iter().map(|(req, _)| req).collect());
    }
    errors.sort_by_key(|e| (e.line, e.column));
    Err(ReadError::Invalid {
        path: path.to_string(),
        errors,
    })
}

/// The requirements in parsed members, at any depth, in file order. Bad attribute values
/// are skipped: the model reader and `clew check --self` report them. A phase is read
/// from the definitions in `members` only; use `requirements_with_defaults` for a project.
// @implements CT-DEV-003, CT-TRC-004
pub fn requirements_in(members: &[Member]) -> Vec<Requirement> {
    requirements_with_defaults(members, &PhaseDefaults::default())
}

/// Like `requirements_in`, with a phase that a requirement lacks read from the requirement
/// definition it is typed by, such as one in another file.
// @implements CT-TRC-001, CT-MOD-002
pub fn requirements_with_defaults(
    members: &[Member],
    defaults: &PhaseDefaults,
) -> Vec<Requirement> {
    let mut found = Vec::new();
    collect(members, defaults, &mut found, &mut Vec::new());
    found.into_iter().map(|(req, _)| req).collect()
}

/// The phase that requirement definitions give as a default, by definition name. A
/// requirement with no phase of its own takes the one of its type, or of what its type
/// specializes. A name used by two definitions gives no default.
// @implements CT-TRC-001, CT-MOD-002
#[derive(Debug, Clone, Default)]
pub struct PhaseDefaults {
    /// The default phase and the names the definition specializes, by definition name.
    definitions: HashMap<String, Option<Definition>>,
}

// @implements CT-TRC-001, CT-MOD-002
/// A requirement definition's default phase and the names it specializes.
type Definition = (Option<u8>, Vec<String>);

// @implements CT-TRC-001, CT-MOD-002
impl PhaseDefaults {
    /// Collects the requirement definitions in each of `files`, at any depth.
    pub fn new<'a>(files: impl IntoIterator<Item = &'a [Member]>) -> PhaseDefaults {
        let mut defaults = PhaseDefaults::default();
        for members in files {
            defaults.add(members);
        }
        defaults
    }

    fn add(&mut self, members: &[Member]) {
        for member in members {
            if let Member::Definition(d) = member
                && d.kind == Kind::Requirement
                && let Some(name) = &d.name
            {
                let (phase, _) = read_phase(&d.members, &mut Vec::new());
                let parents = (d.specializes.iter().filter_map(last_segment))
                    .map(str::to_string)
                    .collect();
                match self.definitions.entry(name.clone()) {
                    Entry::Vacant(slot) => {
                        slot.insert(Some((phase, parents)));
                    }
                    Entry::Occupied(mut slot) => {
                        slot.insert(None);
                    }
                }
            }
            self.add(member.members());
        }
    }

    /// The default phase of the first of `types` that has one, directly or through what it
    /// specializes.
    fn phase_of(&self, types: &[QualifiedName]) -> Option<u8> {
        let mut pending: Vec<&str> = types.iter().filter_map(last_segment).collect();
        let mut seen = 0;
        while let Some(name) = pending.pop() {
            // The chain is short; the count only stops a specialization cycle.
            seen += 1;
            if seen > self.definitions.len() + types.len() {
                break;
            }
            if let Some(Some((phase, parents))) = self.definitions.get(name) {
                if phase.is_some() {
                    return *phase;
                }
                pending.extend(parents.iter().map(String::as_str));
            }
        }
        None
    }
}

// @implements CT-TRC-001, CT-MOD-002
fn last_segment(name: &QualifiedName) -> Option<&str> {
    name.segments.last().map(String::as_str)
}

/// Collects requirement usages with a short name, at any depth, in file order.
// @implements CT-DEV-001, CT-TRC-001
fn collect(
    members: &[Member],
    defaults: &PhaseDefaults,
    found: &mut Vec<(Requirement, Pos)>,
    errors: &mut Vec<SyntaxError>,
) {
    for member in members {
        if let Member::Usage(usage) = member
            && usage.kind == UsageKind::Of(Kind::Requirement)
            && let Some(id) = &usage.short_name
        {
            found.push((requirement(id, usage, defaults, errors), usage.pos));
        }
        collect(member.members(), defaults, found, errors);
    }
}

// @implements CT-DEV-001, CT-TRC-001, CT-USE-007
fn requirement(
    id: &str,
    usage: &Usage,
    defaults: &PhaseDefaults,
    errors: &mut Vec<SyntaxError>,
) -> Requirement {
    let mut req = Requirement {
        id: id.to_string(),
        name: usage.name.clone().unwrap_or_default(),
        doc: String::new(),
        verification_method: None,
        phase: None,
        line: usage.pos.line as usize,
    };
    for member in &usage.members {
        match member {
            Member::Comment(c) if c.is_doc && req.doc.is_empty() => {
                let lines: Vec<&str> = c.body.lines().filter(|l| !l.is_empty()).collect();
                req.doc = lines.join(" ");
            }
            Member::Usage(a) if a.kind == UsageKind::Of(Kind::Attribute) => {
                if let (Some("verificationMethod"), Some(v)) = (a.effective_name(), &a.value) {
                    req.verification_method = Some(v.text.clone());
                }
            }
            _ => {}
        }
    }
    let (phase, written) = read_phase(&usage.members, errors);
    req.phase = phase;
    if !written {
        req.phase = defaults.phase_of(&usage.types);
    }
    req
}

/// The phase an attribute in `members` gives, and whether one is written at all, so a bad
/// value does not fall back to a default. A bad value is added to `errors`.
// @implements CT-TRC-001, CT-USE-007
fn read_phase(members: &[Member], errors: &mut Vec<SyntaxError>) -> (Option<u8>, bool) {
    let mut phase = None;
    let mut written = false;
    for member in members {
        if let Member::Usage(a) = member
            && a.kind == UsageKind::Of(Kind::Attribute)
            && a.effective_name() == Some("phase")
            && let Some(v) = &a.value
        {
            written = true;
            match v.text.parse() {
                Ok(value) => phase = Some(value),
                Err(_) => errors.push(SyntaxError {
                    line: v.pos.line,
                    column: v.pos.column,
                    message: format!("phase is not a whole number from 0 to 255: {}", v.text),
                    help: "write the phase as a number, like `attribute :>> phase = 1;`",
                }),
            }
        }
    }
    (phase, written)
}

// @verifies CT-DEV-001, CT-TRC-001
#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"package P {
    private import ScalarValues::*;
    requirement def ProductRequirement {
        doc /* A definition, not a requirement. */
        attribute verificationMethod : String;
        attribute phase : Integer;
    }
    package Group {
        requirement <'CT-EFF-001'> installerSize : ProductRequirement {
            doc /* Each installer shall be 20 MB or smaller. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 0;
            doc /* A second doc is not the statement. */
        }

        requirement <'CT-USE-002'> palette : ProductRequirement {
            doc /* A command palette shall
             * open with one shortcut. */
            attribute :>> verificationMethod = "Demonstration";
            attribute :>> phase = 2;
            attribute other = 1;
            comment /* not the doc */
        }
        requirement unidentified : ProductRequirement;
        part p { requirement <'CT-DEV-001'>; }
    }
}
"#;

    // @verifies CT-DEV-001, CT-TRC-001
    #[test]
    fn reads_fields_of_each_requirement() {
        let reqs = parse_requirements(FIXTURE, "f.sysml").unwrap();
        assert_eq!(reqs.len(), 3);
        assert_eq!(
            reqs[0],
            Requirement {
                id: "CT-EFF-001".into(),
                name: "installerSize".into(),
                doc: "Each installer shall be 20 MB or smaller.".into(),
                verification_method: Some("Test".into()),
                phase: Some(0),
                line: 9,
            }
        );
        assert_eq!(reqs[1].id, "CT-USE-002");
        assert_eq!(
            reqs[1].doc,
            "A command palette shall open with one shortcut."
        );
        assert_eq!(
            reqs[1].verification_method.as_deref(),
            Some("Demonstration")
        );
        assert_eq!(reqs[1].phase, Some(2));
        // A nested requirement with an ID but no name, doc, or attributes.
        assert_eq!(reqs[2].id, "CT-DEV-001");
        assert_eq!(reqs[2].name, "");
        assert_eq!(
            (reqs[2].verification_method.clone(), reqs[2].phase),
            (None, None)
        );
    }

    // @verifies CT-DEV-003, CT-TRC-004
    #[test]
    fn requirements_in_skips_bad_values() {
        let text = "requirement <'CT-EFF-001'> a : R {\n    attribute :>> phase = \"one\";\n}\n";
        let parsed = parser::parse(text);
        let found = requirements_in(&parsed.members);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].id.as_str(), found[0].phase), ("CT-EFF-001", None));
        let parsed = parser::parse(FIXTURE);
        let ids: Vec<String> = requirements_in(&parsed.members)
            .into_iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(ids, ["CT-EFF-001", "CT-USE-002", "CT-DEV-001"]);
    }

    // @verifies CT-DEV-001, CT-TRC-001, CT-USE-007
    #[test]
    fn rejects_duplicate_ids() {
        let text = "requirement def R;\nrequirement <'CT-EFF-001'> a : R;\n  \
                    requirement <'CT-EFF-001'> b : R { }\n";
        let err = parse_requirements(text, "m.sysml").unwrap_err();
        assert_eq!(
            err.to_string(),
            "m.sysml:3:3: error: duplicate requirement ID CT-EFF-001, first used on line 2\n  \
             help: give each requirement its own ID"
        );
    }

    // @verifies CT-DEV-001, CT-TRC-001, CT-USE-007
    #[test]
    fn reports_every_error_with_path_line_and_column() {
        let text = "requirement <'CT-EFF-001'> a : R {\n    attribute :>> phase = \"one\";\n\
                    }\nrequirement <'CT-EFF-002'> b : R {\n    doc /* never closed */\n";
        let err = parse_requirements(text, "m.sysml").unwrap_err();
        let ReadError::Invalid { path, errors } = &err else {
            panic!("expected invalid, got {err:?}")
        };
        assert_eq!(path, "m.sysml");
        let found: Vec<(u32, u32)> = errors.iter().map(|e| (e.line, e.column)).collect();
        assert_eq!(found, [(2, 27), (4, 34)]);
        assert!(errors[0].message.contains("phase is not a whole number"));
        assert_eq!(errors[1].message, "`{` is never closed");
        let rendered = err.to_string();
        assert_eq!(rendered.lines().count(), 4, "{rendered}");
        assert!(rendered.starts_with("m.sysml:2:27: error: phase"));
        assert!(rendered.contains("\nm.sysml:4:34: error:"));
    }

    // @verifies CT-MOD-002, CT-USE-007
    #[test]
    fn reports_unresolved_names_only_when_the_syntax_is_clean() {
        let text = "requirement <'CT-EFF-001'> a : Missing {\n    attribute :>> phase = 1;\n}\n";
        let err = parse_requirements(text, "m.sysml").unwrap_err();
        let ReadError::Invalid { errors, .. } = &err else {
            panic!("expected invalid, got {err:?}")
        };
        let found: Vec<(u32, u32, &str)> = (errors.iter())
            .map(|e| (e.line, e.column, e.message.as_str()))
            .collect();
        assert_eq!(
            found,
            [
                (1, 32, "cannot resolve type `Missing`"),
                (2, 19, "cannot resolve redefined feature `phase`"),
            ]
        );
        let broken = format!("{text}part def {{\n");
        let err = parse_requirements(&broken, "m.sysml").unwrap_err();
        let ReadError::Invalid { errors, .. } = &err else {
            panic!("expected invalid, got {err:?}")
        };
        assert_eq!(errors.len(), 1, "{err}");
        assert_eq!(errors[0].line, 4);
    }

    // @verifies CT-DEV-001, CT-TRC-001, CT-USE-007
    #[test]
    fn missing_file_names_the_path() {
        let err = read_requirements(Path::new("no/such/file.sysml")).unwrap_err();
        assert!(matches!(err, ReadError::Io { .. }));
        assert!(err.to_string().starts_with("no/such/file.sysml: "), "{err}");
    }

    // @verifies CT-TRC-008, CT-TRC-001
    #[test]
    fn formats_a_requirement_for_the_cli() {
        let mut req = Requirement {
            id: "CT-DEV-001".to_string(),
            name: "reqsInRepo".to_string(),
            doc: "It shall.".to_string(),
            verification_method: Some("Inspection".to_string()),
            phase: Some(0),
            line: 7,
        };
        assert_eq!(
            req.list_line(),
            "CT-DEV-001  reqsInRepo  Inspection  phase 0"
        );
        assert_eq!(
            req.show_text("model/r.sysml"),
            "CT-DEV-001 reqsInRepo\nVerification: Inspection\nPhase: 0\n\
             Location: model/r.sysml:7\n\nIt shall.\n"
        );
        req.verification_method = None;
        req.phase = None;
        assert_eq!(req.list_line(), "CT-DEV-001  reqsInRepo  -  phase -");
        assert!(req.show_text("m").contains("Verification: -\nPhase: -\n"));
    }

    // @verifies CT-DEV-001, CT-TRC-001, CT-MOD-001
    #[test]
    fn reads_the_real_requirements_file() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../model/requirements.sysml");
        let reqs = read_requirements(&path).unwrap();
        assert_eq!(reqs.len(), 77);
        let mut ids: Vec<&str> = reqs.iter().map(|r| r.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 77);
        assert!(
            reqs.iter()
                .all(|r| !r.doc.is_empty() && r.verification_method.is_some() && r.phase.is_some())
        );
        let dev1 = reqs.iter().find(|r| r.id == "CT-DEV-001").unwrap();
        assert_eq!(dev1.name, "reqsInRepo");
        assert_eq!(dev1.verification_method.as_deref(), Some("Inspection"));
        assert_eq!(dev1.phase, Some(0));
        assert!(
            dev1.doc
                .starts_with("The product requirements shall be maintained")
        );
    }
    // @verifies CT-TRC-001, CT-MOD-002
    #[test]
    fn reads_a_phase_given_only_as_a_default_on_the_definition() {
        let text = "package P {\n  private import ScalarValues::*;\n  requirement def R { attribute phase : Integer = 2; }\n  \
                    requirement def Sub :> R;\n  requirement def Other { attribute phase = 4; }\n  \
                    requirement <'CT-EFF-001'> a : R;\n  \
                    requirement <'CT-EFF-002'> b : R { attribute :>> phase = 0; }\n  \
                    requirement <'CT-EFF-003'> c : Sub;\n  \
                    requirement <'CT-EFF-004'> d : Other;\n  \
                    requirement <'CT-EFF-005'> e;\n}\n";
        let reqs = parse_requirements(text, "m.sysml").unwrap();
        let phases: Vec<Option<u8>> = reqs.iter().map(|r| r.phase).collect();
        assert_eq!(phases, [Some(2), Some(0), Some(2), Some(4), None]);
    }

    // @verifies CT-TRC-001, CT-MOD-002
    #[test]
    fn a_default_phase_applies_across_files() {
        let defs = parser::parse("requirement def R { attribute phase : Integer = 3; }");
        let uses = parser::parse("requirement <'CT-EFF-001'> a : R;");
        let defaults = PhaseDefaults::new([defs.members.as_slice(), uses.members.as_slice()]);
        let found = requirements_with_defaults(&uses.members, &defaults);
        assert_eq!(found[0].phase, Some(3));
        assert_eq!(requirements_in(&uses.members)[0].phase, None);
    }

    // @verifies CT-TRC-001
    #[test]
    fn a_bad_phase_value_gets_no_default() {
        let text = "requirement def R { attribute phase : Integer = 2; }\n\
                    requirement <'CT-EFF-001'> a : R { attribute :>> phase = \"one\"; }\n";
        let parsed = parser::parse(text);
        let defaults = PhaseDefaults::new([parsed.members.as_slice()]);
        assert_eq!(
            requirements_with_defaults(&parsed.members, &defaults)[0].phase,
            None
        );
    }
}
