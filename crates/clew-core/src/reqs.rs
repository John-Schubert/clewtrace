//! Requirement reader (v0) for the shapes used in `model/requirements.sysml`.
//! The full SysML v2 parser replaces it in task 1.2.

use std::fmt;
use std::path::Path;

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

// @implements CT-DEV-001, CT-TRC-001
#[derive(Debug)]
pub enum ReadError {
    Io(std::io::Error),
    Syntax {
        line: usize,
        message: String,
    },
    DuplicateId {
        id: String,
        line: usize,
        first_line: usize,
    },
}

// @implements CT-DEV-001, CT-TRC-001
impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::Io(e) => write!(f, "{e}"),
            ReadError::Syntax { line, message } => write!(f, "line {line}: {message}"),
            ReadError::DuplicateId {
                id,
                line,
                first_line,
            } => {
                write!(
                    f,
                    "line {line}: duplicate requirement ID {id} (first at line {first_line})"
                )
            }
        }
    }
}

// @implements CT-DEV-001, CT-TRC-001
impl std::error::Error for ReadError {}

// @implements CT-DEV-001, CT-TRC-001
pub fn read_requirements(path: &Path) -> Result<Vec<Requirement>, ReadError> {
    let text = std::fs::read_to_string(path).map_err(ReadError::Io)?;
    parse_requirements(&text)
}

// @implements CT-DEV-001, CT-TRC-001
pub fn parse_requirements(text: &str) -> Result<Vec<Requirement>, ReadError> {
    let mut reqs: Vec<Requirement> = Vec::new();
    // The open requirement and its brace depth.
    let mut current: Option<(Requirement, usize)> = None;
    // Doc text gathered so far while a multi-line `doc /* ... */` is open.
    let mut open_doc: Option<Vec<String>> = None;

    for (index, raw) in text.lines().enumerate() {
        let line_no = index + 1;
        let line = raw.trim();

        if let Some(parts) = open_doc.as_mut() {
            let body = line.strip_prefix('*').unwrap_or(line);
            match body.split_once("*/") {
                Some((before, _)) => {
                    parts.push(before.trim().to_string());
                    if let Some((req, _)) = current.as_mut() {
                        req.doc = join_doc(parts);
                    }
                    open_doc = None;
                }
                None => parts.push(body.trim().to_string()),
            }
            continue;
        }

        let Some((req, depth)) = current.as_mut() else {
            if let Some(rest) = line.strip_prefix("requirement <'") {
                let req = parse_header(rest, line_no)?;
                if let Some(first) = reqs.iter().find(|r| r.id == req.id) {
                    return Err(ReadError::DuplicateId {
                        id: req.id,
                        line: line_no,
                        first_line: first.line,
                    });
                }
                current = Some((req, 1));
            }
            continue;
        };

        if let Some(rest) = line.strip_prefix("doc /*") {
            match rest.split_once("*/") {
                Some((before, _)) => req.doc = before.trim().to_string(),
                None => open_doc = Some(vec![rest.trim().to_string()]),
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("attribute :>>")
            && let Some((key, value)) = rest.split_once('=')
        {
            let value = value.trim().trim_end_matches(';').trim();
            match key.trim() {
                "verificationMethod" => {
                    req.verification_method = Some(value.trim_matches('"').to_string());
                }
                "phase" => {
                    req.phase = Some(value.parse().map_err(|_| ReadError::Syntax {
                        line: line_no,
                        message: format!("phase is not a number: {value}"),
                    })?);
                }
                _ => {}
            }
        }
        *depth += line.matches('{').count();
        *depth = depth.saturating_sub(line.matches('}').count());
        if *depth == 0 {
            reqs.extend(current.take().map(|(req, _)| req));
        }
    }

    match current {
        Some((req, _)) => Err(ReadError::Syntax {
            line: req.line,
            message: format!("requirement {} is not closed", req.id),
        }),
        None => Ok(reqs),
    }
}

// Reads `ID'> name : Type {` after the `requirement <'` prefix.
// @implements CT-DEV-001, CT-TRC-001
fn parse_header(rest: &str, line_no: usize) -> Result<Requirement, ReadError> {
    let syntax = |message: &str| ReadError::Syntax {
        line: line_no,
        message: message.to_string(),
    };
    let (id, rest) = rest
        .split_once("'>")
        .ok_or_else(|| syntax("short name is not closed"))?;
    if !rest.trim_end().ends_with('{') {
        return Err(syntax("requirement header must end with '{'"));
    }
    let name = rest
        .trim_start()
        .split(|c: char| c.is_whitespace() || c == ':' || c == '{')
        .next()
        .unwrap_or_default();
    Ok(Requirement {
        id: id.to_string(),
        name: name.to_string(),
        doc: String::new(),
        verification_method: None,
        phase: None,
        line: line_no,
    })
}

// @implements CT-DEV-001, CT-TRC-001
fn join_doc(parts: &[String]) -> String {
    parts
        .iter()
        .filter(|p| !p.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
}

// @verifies CT-DEV-001, CT-TRC-001
#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"package P {
    requirement def ProductRequirement {
        doc /* A definition, not a requirement. */
        attribute phase : Integer;
    }
    package Group {
        requirement <'CT-EFF-001'> installerSize : ProductRequirement {
            doc /* Each installer shall be 20 MB or smaller. */
            attribute :>> verificationMethod = "Test";
            attribute :>> phase = 0;
        }

        requirement <'CT-USE-002'> palette : ProductRequirement {
            doc /* A command palette shall
             * open with one shortcut. */
            attribute :>> verificationMethod = "Demonstration";
            attribute :>> phase = 2;
        }
    }
}
"#;

    // @verifies CT-DEV-001, CT-TRC-001
    #[test]
    fn reads_fields_of_each_requirement() {
        let reqs = parse_requirements(FIXTURE).unwrap();
        assert_eq!(reqs.len(), 2);
        assert_eq!(
            reqs[0],
            Requirement {
                id: "CT-EFF-001".into(),
                name: "installerSize".into(),
                doc: "Each installer shall be 20 MB or smaller.".into(),
                verification_method: Some("Test".into()),
                phase: Some(0),
                line: 7,
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
    }

    // @verifies CT-DEV-001, CT-TRC-001
    #[test]
    fn rejects_duplicate_ids() {
        let text = "requirement <'CT-EFF-001'> a : R {\n}\nrequirement <'CT-EFF-001'> b : R {\n}\n";
        match parse_requirements(text) {
            Err(ReadError::DuplicateId {
                id,
                line,
                first_line,
            }) => {
                assert_eq!((id.as_str(), line, first_line), ("CT-EFF-001", 3, 1));
            }
            other => panic!("expected duplicate error, got {other:?}"),
        }
    }

    // @verifies CT-DEV-001, CT-TRC-001
    #[test]
    fn rejects_unclosed_requirement() {
        let text = "requirement <'CT-EFF-001'> a : R {\n    doc /* never closed */\n";
        assert!(matches!(
            parse_requirements(text),
            Err(ReadError::Syntax { line: 1, .. })
        ));
    }

    // @verifies CT-DEV-001, CT-TRC-001
    #[test]
    fn reads_the_real_requirements_file() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../model/requirements.sysml");
        let reqs = read_requirements(&path).unwrap();
        assert_eq!(reqs.len(), 74);
        let mut ids: Vec<&str> = reqs.iter().map(|r| r.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 74);
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
}
