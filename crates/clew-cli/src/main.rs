use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use clew_core::check::{self, Report, Severity};
use clew_core::commit;
use clew_core::index::Index;
use clew_core::json;
use clew_core::notices::{self, Package};
use clew_core::report::{self, Project};
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
    /// Index every .sysml file under the current folder into .clewtrace/index.db,
    /// re-parsing only the files that changed. Exits 0 when clean, 1 when a file has
    /// syntax errors or cannot be read, and 2 when the index cannot be opened or written
    Index {
        /// Drop the index and rebuild it from the text
        #[arg(long)]
        rebuild: bool,
    },
    /// Report on the trace graph of the model and code under the current folder
    Report {
        #[command(subcommand)]
        command: ReportCommand,
    },
    /// List every element and code scope reachable from one element through trace links,
    /// upstream (what it traces to) and downstream (what traces to it). Exits 0 when clean,
    /// 1 when the element is not found or files have problems, and 2 when it cannot run
    Impact {
        /// A requirement ID such as CT-TRC-005, or a qualified name such as Design::Engine
        element: String,
    },
    /// Check third-party licenses (npm tiers, recorded scopes), and with --out write the
    /// license notices file, the SBOM, and the source of each shipped MPL-2.0 crate.
    /// Runs `cargo metadata` and reads app/package-lock.json and app/node_modules.
    /// Exits 0 when clean, 1 on any error, and 2 when the check cannot run
    Notices {
        /// Folder to write THIRD-PARTY-NOTICES.txt, sbom.cdx.json, and mpl-sources/ into
        #[arg(long)]
        out: Option<PathBuf>,
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

// @implements CT-TRC-008, CT-TRC-004
#[derive(Subcommand)]
enum ReportCommand {
    /// Requirements lacking a parent, an implementation, or a verification, and model
    /// elements traced to no requirement. Exits 0 when clean, 1 when files have problems,
    /// and 2 when the report cannot run
    Coverage,
}

// @implements CT-TRC-008
fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Reqs { command } => run_reqs(command),
        Command::Check { self_check: _ } => run_check(),
        Command::CheckCommit { file } => run_check_commit(&file),
        Command::Index { rebuild } => run_index(rebuild),
        Command::Report { command } => run_report(command),
        Command::Impact { element } => run_impact(&element),
        Command::Notices { out } => run_notices(out.as_deref()),
    }
}

// @implements CT-TRC-008, CT-TRC-004, CT-TRC-005, CT-USE-007
/// Reads the project under the current folder and prints its problems, or the exit code
/// when it cannot be read.
fn load_project() -> Result<Project, ExitCode> {
    match report::load_project(Path::new(".")) {
        Ok(project) => {
            for problem in &project.problems {
                println!("{problem}");
            }
            Ok(project)
        }
        Err(e) => {
            eprintln!("clew: {e}");
            Err(ExitCode::from(2))
        }
    }
}

// @implements CT-TRC-008, CT-TRC-004, CT-TRC-005
fn problems_code(project: &Project) -> ExitCode {
    if project.problems.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

// @implements CT-TRC-008, CT-TRC-004, CT-DEV-003
fn run_report(command: ReportCommand) -> ExitCode {
    let ReportCommand::Coverage = command;
    let project = match load_project() {
        Ok(project) => project,
        Err(code) => return code,
    };
    let coverage = report::coverage(&project.model, &project.graph);
    print!("{}", report::coverage_text(&project.model, &coverage));
    problems_code(&project)
}

// @implements CT-TRC-008, CT-TRC-005, CT-USE-007
fn run_impact(name: &str) -> ExitCode {
    let project = match load_project() {
        Ok(project) => project,
        Err(code) => return code,
    };
    let Some(element) = report::find_element(&project, name) else {
        eprintln!(
            "clew: no element named {name}\n  help: give a requirement ID such as CT-TRC-005, \
             or a qualified name such as Design::Engine"
        );
        return ExitCode::FAILURE;
    };
    print!(
        "{}",
        report::impact_text(&project.model, &project.graph, element)
    );
    problems_code(&project)
}

// @implements CT-TRC-008, CT-MOD-001, CT-EFF-007, CT-USE-007
fn run_index(rebuild: bool) -> ExitCode {
    let result = Index::open(Path::new(".")).and_then(|mut index| {
        let stats = if rebuild {
            index.rebuild()?
        } else {
            index.update()?
        };
        Ok((stats, index.element_count()?))
    });
    let (stats, elements) = match result {
        Ok(done) => done,
        Err(e) => {
            eprintln!("clew: {e}");
            return ExitCode::from(2);
        }
    };
    for problem in &stats.problems {
        println!("{problem}");
    }
    println!(
        "clew index: {} files, {} re-indexed, {} removed, {} elements, {} problems",
        stats.files,
        stats.parsed,
        stats.removed,
        elements,
        stats.problems.len()
    );
    if stats.problems.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

// @implements CT-TRC-008, CT-TRC-001
fn run_reqs(command: ReqsCommand) -> ExitCode {
    let all = match reqs::read_requirements(Path::new(REQUIREMENTS_PATH)) {
        Ok(all) => all,
        Err(e) => {
            eprintln!("clew: {e}");
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

// @implements CT-DEV-014, CT-DEV-008, CT-USE-007
fn run_notices(out: Option<&Path>) -> ExitCode {
    let packages = match read_packages() {
        Ok(packages) => packages,
        Err(e) => {
            eprintln!("clew: {e}");
            return ExitCode::from(2);
        }
    };
    let deps = match fs::read_to_string(check::DEPS_FILE)
        .map_err(|e| format!("{}: {e}", check::DEPS_FILE))
        .and_then(|text| notices::read_dep_entries(&text))
    {
        Ok(deps) => deps,
        Err(e) => {
            eprintln!("clew: {e}");
            return ExitCode::from(2);
        }
    };
    let mut report = notices::check_policy(&packages, &deps);
    report.files = 2;
    let shipped = packages
        .iter()
        .filter(|p| p.scope == notices::Scope::Shipped)
        .count();
    println!(
        "clew notices: {} packages, {shipped} shipped, {} build-only",
        packages.len(),
        packages.len() - shipped
    );
    let code = print_report("clew notices", &report);
    if code != ExitCode::SUCCESS {
        return code;
    }
    match out.map(|dir| write_notices(dir, &packages)) {
        Some(Err(e)) => {
            eprintln!("clew: {e}");
            ExitCode::from(2)
        }
        _ => code,
    }
}

/// Every third-party package, from `cargo metadata` for each desktop target and the npm
/// lockfile, with the license texts found in its installed folder.
// @implements CT-DEV-014
fn read_packages() -> Result<Vec<Package>, String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let run = |args: &[&str]| -> Result<String, String> {
        let output = std::process::Command::new(&cargo)
            .args(args)
            .output()
            .map_err(|e| format!("cannot run cargo: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("cargo {} failed:\n{stderr}", args.join(" ")));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    };
    let mut metadata = Vec::new();
    let mut shipped = std::collections::BTreeSet::new();
    for target in notices::DESKTOP_TARGETS {
        let text = run(&[
            "metadata",
            "--locked",
            "--format-version",
            "1",
            "--filter-platform",
            target,
        ])?;
        metadata.push(json::parse(&text).map_err(|e| format!("cargo metadata: {e}"))?);
        let tree = run(&[
            "tree",
            "--locked",
            "--workspace",
            "--edges",
            "normal,no-proc-macro",
            "--target",
            target,
            "--prefix",
            "none",
            "--format",
            "{p}",
        ])?;
        shipped.extend(notices::tree_crates(&tree));
    }
    let mut packages = notices::cargo_packages(&metadata, &shipped)?;
    let lock_text =
        fs::read_to_string(notices::NPM_LOCK).map_err(|e| format!("{}: {e}", notices::NPM_LOCK))?;
    let lock = json::parse(&lock_text).map_err(|e| format!("{}: {e}", notices::NPM_LOCK))?;
    let modules = Path::new(notices::NPM_MODULES);
    if !modules.is_dir() {
        return Err(format!(
            "{} is missing. Run `npm ci` in app/ first",
            notices::NPM_MODULES
        ));
    }
    let app = modules.parent().unwrap_or(Path::new("."));
    packages.extend(notices::npm_packages(&lock, app));
    for p in &mut packages {
        if let Some(dir) = &p.dir {
            p.license_texts = notices::read_license_texts(dir);
        }
    }
    Ok(packages)
}

/// Writes the notices file, the SBOM, and a copy of each shipped MPL-2.0 crate archive.
// @implements CT-DEV-014
fn write_notices(dir: &Path, packages: &[Package]) -> Result<(), String> {
    let sources = dir.join("mpl-sources");
    fs::create_dir_all(&sources).map_err(|e| format!("{}: {e}", sources.display()))?;
    let write = |path: PathBuf, text: String| {
        fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
    };
    write(
        dir.join("THIRD-PARTY-NOTICES.txt"),
        notices::notices_text(packages),
    )?;
    write(
        dir.join("sbom.cdx.json"),
        notices::sbom_json(packages, env!("CARGO_PKG_VERSION")),
    )?;
    for p in packages.iter().filter(|p| p.needs_source()) {
        let archive = p
            .archive
            .as_ref()
            .filter(|a| a.is_file())
            .ok_or_else(|| format!("no source archive found for {}", p.label()))?;
        let name = archive.file_name().unwrap_or_default();
        fs::copy(archive, sources.join(name)).map_err(|e| format!("{}: {e}", archive.display()))?;
    }
    println!("clew notices: wrote {}", dir.display());
    Ok(())
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
