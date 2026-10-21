use std::path::Path;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
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

// @implements CT-TRC-008, CT-TRC-001
fn phase_text(r: &Requirement) -> String {
    r.phase.map_or_else(|| "-".to_string(), |p| p.to_string())
}
