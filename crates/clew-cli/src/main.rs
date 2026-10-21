use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use clew_core::check::{self, Report, Severity};
use clew_core::commit;
use clew_core::reqs::{self, Requirement};

// @implements CT-DEV-001
const REQUIREMENTS_PATH: &str = "model/requirements.sysml";

// @implements CT-TRC-008
#[derive(Parser)]
#[command(name = "clew", version, about = "Clewtrace requirements traceability")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

// @implements CT-TRC-008
#[derive(Subcommand)]
enum Command {
    /// Work with the requirements in model/requirements.sysml
    Reqs {
        #[command(subcommand)]
        command: ReqsCommand,
    },
    /// Check trace tags: untraced lines, unknown IDs, and oversized scopes.
    /// Exits 0 when clean, 1 on any error, and 2 when the check cannot run
    Check {
        /// Check this repository against its own trace.toml, from the repository root
        #[arg(long = "self", required = true)]
        self_check: bool,
    },
    /// Check that a commit message ends with a Refs: trailer naming real requirement IDs.
    /// Exits 0 when clean, 1 on any error, and 2 when the check cannot run
    CheckCommit {
        /// The commit message file, for example .git/COMMIT_EDITMSG
        file: PathBuf,
    },
}

// @implements CT-TRC-008, CT-TRC-001
#[derive(Subcommand)]
enum ReqsCommand {
    /// List every requirement: ID, name, verification method, and phase
    List,
    /// Show one requirement in full
    Show {
        /// Requirement ID, for example CT-TRC-008
        id: String,
    },
}

// @implements CT-TRC-008
fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Reqs { command } => run_reqs(command),
        Command::Check { self_check: _ } => run_check(),
        Command::CheckCommit { file } => run_check_commit(&file),
    }
}

// @implements CT-TRC-008, CT-TRC-001
fn run_reqs(command: ReqsCommand) -> ExitCode {
    let all = match reqs::read_requirements(Path::new(REQUIREMENTS_PATH)) {
        Ok(all) => all,
        Err(e) => {
            eprintln!("clew: {REQUIREMENTS_PATH}: {e}");
            return ExitCode::FAILURE;
        }
    };
    match command {
        ReqsCommand::List => {
            for r in &all {
                println!(
                    "{}  {}  {}  phase {}",
                    r.id,
                    r.name,
                    r.verification_method.as_deref().unwrap_or("-"),
                    phase_text(r)
                );
            }
            ExitCode::SUCCESS
        }
        ReqsCommand::Show { id } => match all.iter().find(|r| r.id == id) {
            Some(r) => {
                println!("{} {}", r.id, r.name);
                println!(
                    "Verification: {}",
                    r.verification_method.as_deref().unwrap_or("-")
                );
                println!("Phase: {}", phase_text(r));
                println!("Location: {REQUIREMENTS_PATH}:{}", r.line);
                println!();
                println!("{}", r.doc);
                ExitCode::SUCCESS
            }
            None => {
                eprintln!("clew: no requirement with ID {id}");
                ExitCode::FAILURE
            }
        },
    }
}

// @implements CT-TRC-008, CT-DEV-002, CT-DEV-004, CT-USE-007
fn run_check() -> ExitCode {
    let report = match check::check_project(Path::new(".")) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("clew: {e}");
            return ExitCode::from(2);
        }
    };
    print_report("clew check", &report)
}

// @implements CT-DEV-005, CT-USE-007
fn run_check_commit(file: &Path) -> ExitCode {
    let label = file.display().to_string();
    let message = match fs::read_to_string(file) {
        Ok(message) => message,
        Err(e) => {
            eprintln!("clew: {label}: {e}");
            return ExitCode::from(2);
        }
    };
    match commit::check_commit_message(Path::new("."), &label, &message) {
        Ok(report) => print_report("clew check-commit", &report),
        Err(e) => {
            eprintln!("clew: {e}");
            ExitCode::from(2)
        }
    }
}

/// Prints each finding and a summary line, and maps errors to the exit code.
// @implements CT-DEV-004, CT-DEV-005, CT-USE-007
fn print_report(command: &str, report: &Report) -> ExitCode {
    for finding in &report.findings {
        println!("{finding}");
    }
    let errors = report.count(Severity::Error);
    let warnings = report.count(Severity::Warning);
    let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
    println!(
        "{command}: {} checked, {}, {}",
        plural(report.files, "file"),
        plural(errors, "error"),
        plural(warnings, "warning")
    );
    if errors == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

// @implements CT-TRC-008, CT-TRC-001
fn phase_text(r: &Requirement) -> String {
    r.phase.map_or_else(|| "-".to_string(), |p| p.to_string())
}
