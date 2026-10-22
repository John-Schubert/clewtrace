//! Reproducible reports: every report starts with what made it, and `clew report verify`
//! makes it again and compares.
//!
//! The header names the command, the Git commit of the project, the tool version, and a
//! SHA-256 of the inputs. The inputs hash is the SHA-256 of a manifest with one line per
//! file the report read as text, sorted by path: the file's SHA-256 in lowercase hex, two
//! spaces, and its path with `/` separators, as `sha256sum` prints it.

use std::path::Path;
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::check::SetupError;
use crate::report;

// @implements CT-TRC-009
/// The tool and version that made a report.
pub const TOOL: &str = concat!("clew ", env!("CARGO_PKG_VERSION"));

// @implements CT-TRC-009
/// The help for a file that does not start with a report header.
const NOT_A_REPORT: &str = "not a clew report: it does not start with the Report, Commit, \
     Tool, and Inputs lines and a blank line\n  help: give a file saved from \
     `clew report coverage` or `clew impact`, unchanged";

// @implements CT-TRC-009
/// A report that can be made again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportKind {
    Coverage,
    /// The impact of the element with this requirement ID or qualified name.
    Impact(String),
}

// @implements CT-TRC-009
impl ReportKind {
    /// The command that makes this report.
    pub fn command(&self) -> String {
        match self {
            ReportKind::Coverage => "clew report coverage".to_string(),
            ReportKind::Impact(element) => format!("clew impact {element}"),
        }
    }

    fn parse(command: &str) -> Option<ReportKind> {
        if command == "clew report coverage" {
            return Some(ReportKind::Coverage);
        }
        let element = command.strip_prefix("clew impact ")?;
        (!element.is_empty()).then(|| ReportKind::Impact(element.to_string()))
    }
}

// @implements CT-TRC-009
/// The header of a report: what made it, from which inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    pub report: String,
    /// The commit SHA, with `, with uncommitted changes` when the work tree differs from
    /// it, or `none` outside a Git repository or when Git cannot run.
    pub commit: String,
    pub tool: String,
    /// `sha256 <hex> over <n> files`.
    pub inputs: String,
}

// @implements CT-TRC-009
impl Provenance {
    fn text(&self) -> String {
        format!(
            "Report: {}\nCommit: {}\nTool: {}\nInputs: {}\n\n",
            self.report, self.commit, self.tool, self.inputs
        )
    }

    /// Splits a report into its header and the rest.
    fn parse(text: &str) -> Option<(Provenance, &str)> {
        let mut rest = text;
        let mut field = |key: &str| {
            let (line, after) = rest.split_once('\n')?;
            rest = after;
            line.strip_prefix(key).map(str::to_string)
        };
        let provenance = Provenance {
            report: field("Report: ")?,
            commit: field("Commit: ")?,
            tool: field("Tool: ")?,
            inputs: field("Inputs: ")?,
        };
        Some((provenance, rest.strip_prefix('\n')?))
    }
}

// @implements CT-TRC-009
/// A freshly made report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generated {
    pub provenance: Provenance,
    /// The file problems, then the report itself.
    pub body: String,
    /// The project has file problems.
    pub problems: bool,
}

// @implements CT-TRC-009
impl Generated {
    /// The whole report: the header, a blank line, and the body.
    pub fn text(&self) -> String {
        self.provenance.text() + &self.body
    }
}

// @implements CT-TRC-009, CT-USE-007
/// Why a report cannot be made.
#[derive(Debug)]
pub enum GenerateError {
    /// The project cannot be read, for example without `trace.toml`.
    Setup(SetupError),
    /// The element an impact report names is not in the model.
    NotFound(String),
}

// @implements CT-TRC-009, CT-USE-007
impl std::fmt::Display for GenerateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenerateError::Setup(e) => write!(f, "{e}"),
            GenerateError::NotFound(name) => write!(
                f,
                "no element named {name}\n  help: give a requirement ID such as CT-TRC-005, \
                 or a qualified name such as Design::Engine"
            ),
        }
    }
}

// @implements CT-TRC-009, CT-TRC-004, CT-TRC-005
/// Makes a report of the project under `root`.
pub fn generate(root: &Path, kind: &ReportKind) -> Result<Generated, GenerateError> {
    let project = report::load_project(root).map_err(GenerateError::Setup)?;
    let report = match kind {
        ReportKind::Coverage => {
            let coverage = report::coverage(&project.model, &project.graph);
            report::coverage_text(&project.model, &coverage)
        }
        ReportKind::Impact(name) => match report::find_element(&project, name) {
            Some(element) => report::impact_text(&project.model, &project.graph, element),
            None => return Err(GenerateError::NotFound(name.clone())),
        },
    };
    let mut body = String::new();
    for problem in &project.problems {
        body.push_str(problem);
        body.push('\n');
    }
    body.push_str(&report);
    Ok(Generated {
        provenance: Provenance {
            report: kind.command(),
            commit: commit_of("git", root),
            tool: TOOL.to_string(),
            inputs: inputs_digest(&project.inputs),
        },
        body,
        problems: !project.problems.is_empty(),
    })
}

// @implements CT-TRC-009, CT-SEC-006
/// `sha256 <hex> over <n> files`, the SHA-256 of the manifest of `inputs`, each a path and
/// the SHA-256 of the file's bytes.
fn inputs_digest(inputs: &[(String, Vec<u8>)]) -> String {
    let mut sorted: Vec<&(String, Vec<u8>)> = inputs.iter().collect();
    sorted.sort();
    let mut manifest = String::new();
    for (path, sha256) in &sorted {
        manifest.push_str(&format!("{}  {path}\n", hex(sha256)));
    }
    let digest = Sha256::digest(manifest.as_bytes());
    format!("sha256 {} over {} files", hex(&digest), sorted.len())
}

// @implements CT-TRC-009
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// @implements CT-TRC-009
/// The commit `git` reports for the repository holding `root`, noting uncommitted changes,
/// including untracked files, or `none`.
fn commit_of(git: &str, root: &Path) -> String {
    let run = |args: &[&str]| {
        let output = Command::new(git).args(args).current_dir(root).output();
        output.ok().filter(|o| o.status.success())
    };
    let Some(head) = run(&["rev-parse", "HEAD"]) else {
        return "none".to_string();
    };
    let sha = String::from_utf8_lossy(&head.stdout).trim().to_string();
    let dirty = run(&["status", "--porcelain"]).is_some_and(|o| !o.stdout.is_empty());
    if dirty {
        format!("{sha}, with uncommitted changes")
    } else {
        sha
    }
}

// @implements CT-TRC-009
/// What comparing a saved report with a fresh run found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Verdict {
    /// Why the report does not match, each with a `help:` line. Empty when it matches.
    pub differences: Vec<String>,
    /// Facts that do not stop a match, such as a different commit with identical inputs.
    pub notes: Vec<String>,
}

// @implements CT-TRC-009, CT-USE-007
/// Makes the report saved as `recorded` again from the project under `root` and compares
/// the tool version, the inputs hash, and every line after the header. A byte order mark
/// and Windows line endings in the saved file are ignored. `Err` means the comparison
/// cannot run: the file is not a report, or the project cannot be read.
pub fn verify(root: &Path, recorded: &str) -> Result<Verdict, String> {
    let recorded = recorded.strip_prefix('\u{feff}').unwrap_or(recorded);
    let recorded = recorded.replace("\r\n", "\n");
    let (old, old_body) = Provenance::parse(&recorded).ok_or(NOT_A_REPORT)?;
    let kind = ReportKind::parse(&old.report).ok_or(NOT_A_REPORT)?;
    let mut verdict = Verdict::default();
    let fresh = match generate(root, &kind) {
        Ok(fresh) => fresh,
        Err(GenerateError::Setup(e)) => return Err(e.to_string()),
        Err(GenerateError::NotFound(name)) => {
            verdict.differences.push(format!(
                "the report names {name}, which is not in the model now\n  help: \
                 check out the recorded commit, or restore the files the report was made from"
            ));
            return Ok(verdict);
        }
    };
    let new = &fresh.provenance;
    if old.commit != new.commit {
        verdict.notes.push(format!(
            "recorded at commit {}, now at {}",
            old.commit, new.commit
        ));
    }
    if old.tool != new.tool {
        verdict.differences.push(format!(
            "recorded with {}, now running {}\n  help: verify with the recorded tool version",
            old.tool, new.tool
        ));
    }
    if old.inputs != new.inputs {
        verdict.differences.push(format!(
            "the inputs differ: recorded {}, now {}\n  help: check out the recorded \
             commit, or restore the files the report was made from",
            old.inputs, new.inputs
        ));
    }
    let mut old_lines = old_body.lines();
    let mut new_lines = fresh.body.lines();
    // The header and its blank line take the first five lines.
    for number in 6.. {
        let (was, now) = (old_lines.next(), new_lines.next());
        if was.is_none() && now.is_none() {
            break;
        }
        if was != now {
            let show = |line: Option<&str>| line.map_or("the end".into(), |l| format!("`{l}`"));
            verdict.differences.push(format!(
                "line {number} differs from a fresh run: recorded {}, now {}\n  help: \
                 a report with the same inputs and tool is the same; restore the \
                 report as it was saved",
                show(was),
                show(now)
            ));
            break;
        }
    }
    Ok(verdict)
}

// @verifies CT-TRC-009
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    const CONFIG: &str = "[project]\nrequirements = [\"model/reqs.sysml\"]\n\
                          id_pattern = \"CT-[A-Z]{3}-[0-9]{3}\"\ncurrent_phase = 1\n\
                          [coverage]\nsource_roots = [\"src\"]\nlanguages = [\"rust\"]\n\
                          file_level_allowed = []\nmax_scope_lines = 200\n";

    const REQS: &str = "package Reqs {\n    requirement <'CT-TRC-001'> top;\n    requirement <'CT-TRC-002'> \
         child;\n    #derivation dependency child to top;\n}\n";

    /// A project folder with `trace.toml`, one model file, and one code file.
    fn project(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join("clew-reproduce-tests").join(name);
        let _ = fs::remove_dir_all(&root);
        let files = [
            ("trace.toml", CONFIG),
            ("model/reqs.sysml", REQS),
            (
                "src/lib.rs",
                "// @verifies CT-TRC-002\n#[test]\nfn t() {}\n",
            ),
        ];
        for (path, text) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        root
    }

    fn made(root: &Path, kind: &ReportKind) -> Generated {
        generate(root, kind).unwrap()
    }

    // @verifies CT-TRC-009
    #[test]
    fn a_report_starts_with_its_command_commit_tool_and_inputs() {
        let root = project("header");
        let report = made(&root, &ReportKind::Coverage);
        let text = report.text();
        let (provenance, body) = Provenance::parse(&text).unwrap();
        assert_eq!(provenance, report.provenance);
        assert_eq!(body, report.body);
        assert_eq!(provenance.report, "clew report coverage");
        assert_eq!(
            provenance.tool,
            format!("clew {}", env!("CARGO_PKG_VERSION"))
        );
        assert!(provenance.inputs.ends_with(" over 3 files"), "{text}");
        assert!(
            body.starts_with("Requirements without a parent (1):\n"),
            "{text}"
        );
        assert!(!report.problems);
        let impact = made(&root, &ReportKind::Impact("CT-TRC-001".into()));
        assert_eq!(impact.provenance.report, "clew impact CT-TRC-001");
        assert!(
            impact
                .body
                .starts_with("CT-TRC-001 Reqs::top  model/reqs.sysml:2\n")
        );
        let missing = ReportKind::Impact("Nowhere".into());
        let err = generate(&root, &missing).unwrap_err().to_string();
        assert!(
            err.starts_with("no element named Nowhere\n  help: "),
            "{err}"
        );
    }

    // @verifies CT-TRC-009, CT-SEC-006
    #[test]
    fn the_inputs_hash_is_the_sha256_of_a_sorted_manifest() {
        // The same as `sha256sum` over the manifest "<sha256 of ''>  a\n<sha256 of abc>  b\n".
        let empty = Sha256::digest(b"").to_vec();
        let abc = Sha256::digest(b"abc").to_vec();
        let inputs = [("b".to_string(), abc), ("a".to_string(), empty)];
        assert_eq!(
            inputs_digest(&inputs),
            "sha256 bf6e2103d6cbead28abb7d3fec372bc25d03a35b1647e83e090248eb08d0adae over 2 \
             files"
        );
        assert_eq!(
            inputs_digest(&[]),
            "sha256 e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 over 0 \
             files"
        );
    }

    // @verifies CT-TRC-009
    #[test]
    fn records_the_commit_and_uncommitted_changes() {
        let root = project("git");
        assert_eq!(commit_of("clew-no-such-git", &root), "none");
        let git = |args: &[&str]| {
            let status = Command::new("git")
                .args(["-c", "user.name=T", "-c", "user.email=t@example.com"])
                .args(["-c", "commit.gpgsign=false"])
                .args(args)
                .current_dir(&root)
                .output()
                .unwrap();
            assert!(status.status.success(), "{status:?}");
            String::from_utf8(status.stdout).unwrap()
        };
        git(&["init", "-q"]);
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "x"]);
        let head = git(&["rev-parse", "HEAD"]).trim().to_string();
        assert_eq!(head.len(), 40);
        assert_eq!(commit_of("git", &root), head);
        fs::write(root.join("src/lib.rs"), "fn changed() {}\n").unwrap();
        assert_eq!(
            commit_of("git", &root),
            format!("{head}, with uncommitted changes")
        );
    }

    // @verifies CT-TRC-009
    #[test]
    fn a_saved_report_matches_a_fresh_run() {
        let root = project("matches");
        for kind in [
            ReportKind::Coverage,
            ReportKind::Impact("Reqs::child".into()),
        ] {
            let text = made(&root, &kind).text();
            assert_eq!(verify(&root, &text).unwrap(), Verdict::default());
            // A byte order mark and Windows line endings, as some editors save it.
            let windows = format!("\u{feff}{}", text.replace('\n', "\r\n"));
            assert_eq!(verify(&root, &windows).unwrap(), Verdict::default());
        }
    }

    // @verifies CT-TRC-009
    #[test]
    fn a_different_commit_with_the_same_inputs_is_only_a_note() {
        let root = project("commit");
        let text = made(&root, &ReportKind::Coverage).text();
        let text = text.replacen("Commit: none\n", "Commit: 0123abc\n", 1);
        let verdict = verify(&root, &text).unwrap();
        assert!(verdict.differences.is_empty(), "{verdict:?}");
        assert_eq!(verdict.notes, ["recorded at commit 0123abc, now at none"]);
    }

    // @verifies CT-TRC-009, CT-USE-007
    #[test]
    fn reports_changed_inputs_and_the_first_changed_line() {
        let root = project("inputs");
        let text = made(&root, &ReportKind::Coverage).text();
        fs::write(root.join("src/lib.rs"), "fn t() {}\n").unwrap();
        let verdict = verify(&root, &text).unwrap();
        assert_eq!(verdict.differences.len(), 2, "{verdict:?}");
        assert!(verdict.differences[0].starts_with("the inputs differ: recorded sha256 "));
        assert!(
            verdict.differences[1].starts_with(
                "line 11 differs from a fresh run: recorded `Requirements without a \
                 verification (1):`, now `Requirements without a verification (2):`\n"
            ),
            "{verdict:?}"
        );
        assert!(verdict.differences.iter().all(|d| d.contains("\n  help: ")));
    }

    // @verifies CT-TRC-009, CT-USE-007
    #[test]
    fn reports_a_changed_tool_or_body() {
        let root = project("tampered");
        let text = made(&root, &ReportKind::Coverage).text();
        let old_tool = text.replacen(&format!("Tool: {TOOL}\n"), "Tool: clew 0.0.1\n", 1);
        let verdict = verify(&root, &old_tool).unwrap();
        assert_eq!(
            verdict.differences,
            [format!(
                "recorded with clew 0.0.1, now running {TOOL}\n  help: verify with the \
                 recorded tool version"
            )]
        );
        let edited = text.replacen("(1):\n", "(0):\n", 1);
        let verdict = verify(&root, &edited).unwrap();
        assert_eq!(verdict.differences.len(), 1);
        assert!(
            verdict.differences[0].starts_with(
                "line 6 differs from a fresh run: recorded `Requirements without a parent \
                 (0):`, now `Requirements without a parent (1):`\n"
            ),
            "{verdict:?}"
        );
        let short = text.trim_end().rsplit_once('\n').unwrap().0.to_string() + "\n";
        let verdict = verify(&root, &short).unwrap();
        assert!(
            verdict.differences[0].contains(": recorded the end, now `clew report coverage:"),
            "{verdict:?}"
        );
    }

    // @verifies CT-TRC-009, CT-USE-007
    #[test]
    fn an_impact_element_that_is_gone_differs() {
        let root = project("gone");
        let text = made(&root, &ReportKind::Impact("CT-TRC-002".into())).text();
        fs::write(root.join("model/reqs.sysml"), "package Reqs;\n").unwrap();
        let verdict = verify(&root, &text).unwrap();
        assert_eq!(verdict.differences.len(), 1);
        assert!(
            verdict.differences[0]
                .starts_with("the report names CT-TRC-002, which is not in the model now\n"),
            "{verdict:?}"
        );
    }

    // @verifies CT-TRC-009, CT-USE-007
    #[test]
    fn cannot_verify_a_file_that_is_not_a_report_or_a_project_that_cannot_be_read() {
        let root = project("not-a-report");
        let text = made(&root, &ReportKind::Coverage).text();
        for bad in [
            "",
            "hello\n",
            "Report: clew report coverage\nCommit: none\nTool: clew\nInputs: x\nbody\n",
            "Report: clew\nCommit: none\nTool: clew\nInputs: x\n\nbody\n",
            "Report: clew impact \nCommit: none\nTool: clew\nInputs: x\n\nbody\n",
        ] {
            let err = verify(&root, bad).unwrap_err();
            assert!(err.starts_with("not a clew report: "), "{err}");
            assert!(err.contains("\n  help: "), "{err}");
        }
        fs::remove_file(root.join("trace.toml")).unwrap();
        let err = verify(&root, &text).unwrap_err();
        assert!(err.starts_with("trace.toml: not found."), "{err}");
    }
}
