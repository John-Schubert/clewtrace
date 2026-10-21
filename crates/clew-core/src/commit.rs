//! `clew check-commit`: checks that a commit message ends with a `Refs:` trailer naming
//! real requirement IDs. Rules: docs/TRACEABILITY.md, section "Commits".

use std::collections::BTreeSet;
use std::path::Path;

use crate::check::{Finding, IdPattern, Report, SetupError, Severity, load_ids};

/// Git's scissors line. Everything below it is dropped from the message.
// @implements CT-DEV-005
const SCISSORS: &str = "# ------------------------ >8 ------------------------";

// @implements CT-DEV-005
const TRAILER_KEY: &str = "Refs:";

/// Checks the commit message in `message` against the project rooted at `root`.
/// `label` is the path printed in findings.
// @implements CT-DEV-005, CT-USE-007
pub fn check_commit_message(root: &Path, label: &str, message: &str) -> Result<Report, SetupError> {
    let (config, pattern, known) = load_ids(root)?;
    let findings = commit_findings(label, message, &config.id_pattern, &pattern, &known);
    Ok(Report { files: 1, findings })
}

/// The findings for one message. Lines starting with `#` are comments, as in git, and
/// the trailer block is the last paragraph after the summary.
// @implements CT-DEV-005, CT-USE-007
fn commit_findings(
    label: &str,
    message: &str,
    pattern_text: &str,
    pattern: &IdPattern,
    known: &BTreeSet<String>,
) -> Vec<Finding> {
    let error = |line: usize, message: String, help: &str| Finding {
        path: label.to_string(),
        line,
        severity: Severity::Error,
        message,
        help: help.to_string(),
    };
    // Paragraphs of (1-based line number, text), with comment lines removed.
    let mut paragraphs: Vec<Vec<(usize, &str)>> = vec![Vec::new()];
    for (row, raw) in message.lines().enumerate() {
        if raw.trim_end() == SCISSORS {
            break;
        }
        let text = raw.trim_end();
        if text.starts_with('#') {
            continue;
        }
        if text.trim().is_empty() {
            if paragraphs.last().is_some_and(|p| !p.is_empty()) {
                paragraphs.push(Vec::new());
            }
        } else if let Some(last) = paragraphs.last_mut() {
            last.push((row + 1, text));
        }
    }
    paragraphs.retain(|p| !p.is_empty());

    let missing_help = "end the message with a blank line and `Refs: CT-XXX-NNN`, naming the requirement this change serves";
    let refs_line = |p: &&Vec<(usize, &str)>| p.iter().any(|(_, t)| t.starts_with(TRAILER_KEY));
    let trailers = match paragraphs.split_first() {
        None => return vec![error(0, "commit message is empty".into(), missing_help)],
        Some((_, rest)) => rest.last(),
    };
    let Some(trailers) = trailers.filter(refs_line) else {
        let message = match paragraphs.iter().find(refs_line) {
            Some(p) => {
                let (line, _) = p.iter().find(|(_, t)| t.starts_with(TRAILER_KEY)).unwrap();
                return vec![error(
                    *line,
                    "`Refs:` trailer is not in the last paragraph".into(),
                    "move `Refs:` to the end of the message, after a blank line, with the other trailers",
                )];
            }
            None => "commit message has no `Refs:` trailer".to_string(),
        };
        return vec![error(0, message, missing_help)];
    };

    let mut findings = Vec::new();
    for &(line, text) in trailers {
        let Some(value) = text.strip_prefix(TRAILER_KEY) else {
            continue;
        };
        let ids: Vec<&str> = value.split(',').map(str::trim).collect();
        if ids.iter().all(|id| id.is_empty()) {
            findings.push(error(
                line,
                "`Refs:` trailer names no requirement".into(),
                "list at least one ID, for example `Refs: CT-DEV-005`",
            ));
            continue;
        }
        for id in ids {
            if !pattern.matches(id) {
                findings.push(error(
                    line,
                    format!("`{id}` does not match id_pattern {pattern_text}"),
                    "separate IDs with commas and write each as in model/requirements.sysml, for example CT-DEV-005",
                ));
            } else if !known.contains(id) {
                findings.push(error(
                    line,
                    format!("unknown requirement ID {id}"),
                    "cite an ID that exists in model/requirements.sysml (`clew reqs list`)",
                ));
            }
        }
    }
    findings
}

// @verifies CT-DEV-005
#[cfg(test)]
mod tests {
    use super::*;

    // @verifies CT-DEV-005
    fn findings(message: &str) -> Vec<String> {
        let pattern = IdPattern::parse("CT-(DEV|TRC)-[0-9]{3}").unwrap();
        let known: BTreeSet<String> = ["CT-DEV-005", "CT-TRC-004"]
            .into_iter()
            .map(String::from)
            .collect();
        commit_findings("MSG", message, "CT-(DEV|TRC)-[0-9]{3}", &pattern, &known)
            .iter()
            .map(|f| f.to_string())
            .collect()
    }

    // @verifies CT-DEV-005
    #[test]
    fn accepts_a_refs_trailer_with_known_ids() {
        assert_eq!(
            findings("Add a thing\n\nWhy.\n\nRefs: CT-DEV-005, CT-TRC-004\n"),
            Vec::<String>::new()
        );
        assert_eq!(
            findings("Add a thing\r\n\r\nRefs: CT-DEV-005\r\nCo-Authored-By: A <a@b>\r\n"),
            Vec::<String>::new()
        );
    }

    // @verifies CT-DEV-005
    #[test]
    fn ignores_comments_and_text_below_the_scissors() {
        let message = "Add a thing\n\n# Summary line: imperative mood.\nRefs: CT-DEV-005\n# Please enter the commit message\n\n# ------------------------ >8 ------------------------\nRefs: CT-DEV-999\n";
        assert_eq!(findings(message), Vec::<String>::new());
    }

    // @verifies CT-DEV-005
    #[test]
    fn reports_a_missing_trailer() {
        let out = findings("Add a thing\n\nNo trailer here.\n");
        assert_eq!(out.len(), 1);
        assert!(
            out[0].starts_with("MSG: error: commit message has no `Refs:` trailer"),
            "{out:?}"
        );
        assert!(out[0].contains("help: "), "{out:?}");
        assert!(findings("# only a comment\n")[0].contains("commit message is empty"));
    }

    // @verifies CT-DEV-005
    #[test]
    fn reports_a_trailer_outside_the_last_paragraph() {
        let out = findings("Add a thing\n\nRefs: CT-DEV-005\n\nMore text.\n");
        assert_eq!(out.len(), 1);
        assert!(
            out[0].starts_with("MSG:3: error: `Refs:` trailer is not in the last paragraph"),
            "{out:?}"
        );
        // The summary line is not a trailer block.
        let out = findings("Refs: CT-DEV-005\n");
        assert!(out[0].contains("not in the last paragraph"), "{out:?}");
    }

    // @verifies CT-DEV-005
    #[test]
    fn reports_bad_and_unknown_ids_by_line() {
        let out = findings("Add a thing\n\nRefs: CT-DEV-005, CT-TRC-999, ct-dev-005\n");
        assert_eq!(
            out.iter()
                .map(|f| f.lines().next().unwrap())
                .collect::<Vec<_>>(),
            vec![
                "MSG:3: error: unknown requirement ID CT-TRC-999",
                "MSG:3: error: `ct-dev-005` does not match id_pattern CT-(DEV|TRC)-[0-9]{3}",
            ]
        );
        // The .gitmessage template's placeholder is not an ID.
        let out = findings("Add a thing\n\nRefs: CT-\n");
        assert!(out[0].contains("`CT-` does not match"), "{out:?}");
        let out = findings("Add a thing\n\nRefs:\n");
        assert!(out[0].contains("names no requirement"), "{out:?}");
    }
}
