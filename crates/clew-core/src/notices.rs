//! `clew notices`: the third-party license notices file, the SBOM, and the license checks
//! for npm packages (ADR-0003). Cargo crates come from `cargo metadata` for each desktop
//! target, npm packages from `app/package-lock.json`. Each package is either shipped
//! (linked into the app or CLI, or in the web bundle) or build-only.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::check::{DEPS_FILE, Finding, Report, Severity};
use crate::config::parse_tables;
use crate::json::{self, Json};

/// The targets Clewtrace ships installers for. A crate ships if it is linked on any of them.
// @implements CT-DEV-014, CT-PLT-001
pub const DESKTOP_TARGETS: [&str; 6] = [
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
];

/// The npm lockfile and installed packages of the web UI, relative to the project root.
// @implements CT-DEV-014
pub const NPM_LOCK: &str = "app/package-lock.json";
// @implements CT-DEV-014
pub const NPM_MODULES: &str = "app/node_modules";

/// Licenses allowed for shipped code without a per-package exception (ADR-0003).
// @implements CT-DEV-008, CT-DEV-014
pub const SHIPPED_LICENSES: [&str; 4] = ["MIT", "Apache-2.0", "Unicode-3.0", "Zlib"];

/// Licenses allowed for build-only code without asking (ADR-0003).
// @implements CT-DEV-008, CT-DEV-014
pub const BUILD_LICENSES: [&str; 9] = [
    "MIT",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Zlib",
    "Unicode-3.0",
    "CC0-1.0",
    "Apache-2.0 WITH LLVM-exception",
];

// @implements CT-DEV-014
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ecosystem {
    Cargo,
    Npm,
}

// @implements CT-DEV-014
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    Shipped,
    BuildOnly,
}

// @implements CT-DEV-014
impl Ecosystem {
    pub fn name(self) -> &'static str {
        match self {
            Ecosystem::Cargo => "cargo",
            Ecosystem::Npm => "npm",
        }
    }
}

// @implements CT-DEV-014
impl Scope {
    pub fn name(self) -> &'static str {
        match self {
            Scope::Shipped => "shipped",
            Scope::BuildOnly => "build-only",
        }
    }
}

/// One third-party package. `dir` is where its files are installed, `source` is a link to
/// its published source, and `archive` is the unmodified published archive, when known.
// @implements CT-DEV-014
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub ecosystem: Ecosystem,
    pub name: String,
    pub version: String,
    pub license: String,
    pub scope: Scope,
    pub repository: Option<String>,
    pub source: Option<String>,
    pub dir: Option<PathBuf>,
    pub archive: Option<PathBuf>,
    pub license_texts: Vec<String>,
}

// @implements CT-DEV-014
impl Package {
    pub fn label(&self) -> String {
        format!("{} {}", self.name, self.version)
    }

    pub fn purl(&self) -> String {
        match self.ecosystem {
            Ecosystem::Cargo => format!("pkg:cargo/{}@{}", self.name, self.version),
            Ecosystem::Npm => format!(
                "pkg:npm/{}@{}",
                self.name.replacen('@', "%40", 1),
                self.version
            ),
        }
    }

    /// True when the package is shipped under a license that needs its source made
    /// available, which today means MPL-2.0 (ADR-0003).
    pub fn needs_source(&self) -> bool {
        self.scope == Scope::Shipped
            && self.license.contains("MPL-2.0")
            && !license_allows(&self.license, &SHIPPED_LICENSES)
    }
}

/// True when the SPDX expression can be satisfied using only `allowed` licenses.
/// Supports `AND`, `OR`, `WITH`, parentheses, and the older `/` for OR. An expression
/// that does not parse is never allowed.
// @implements CT-DEV-008, CT-DEV-014
pub fn license_allows(expression: &str, allowed: &[&str]) -> bool {
    let spaced = expression
        .replace('(', " ( ")
        .replace(')', " ) ")
        .replace('/', " OR ");
    let tokens: Vec<&str> = spaced.split_whitespace().collect();
    let mut pos = 0;
    match spdx_or(&tokens, &mut pos, allowed) {
        Some(ok) if pos == tokens.len() => ok,
        _ => false,
    }
}

// @implements CT-DEV-008, CT-DEV-014
fn spdx_or(tokens: &[&str], pos: &mut usize, allowed: &[&str]) -> Option<bool> {
    let mut ok = spdx_and(tokens, pos, allowed)?;
    while tokens.get(*pos) == Some(&"OR") {
        *pos += 1;
        ok |= spdx_and(tokens, pos, allowed)?;
    }
    Some(ok)
}

// @implements CT-DEV-008, CT-DEV-014
fn spdx_and(tokens: &[&str], pos: &mut usize, allowed: &[&str]) -> Option<bool> {
    let mut ok = spdx_term(tokens, pos, allowed)?;
    while tokens.get(*pos) == Some(&"AND") {
        *pos += 1;
        ok &= spdx_term(tokens, pos, allowed)?;
    }
    Some(ok)
}

// @implements CT-DEV-008, CT-DEV-014
fn spdx_term(tokens: &[&str], pos: &mut usize, allowed: &[&str]) -> Option<bool> {
    let token = *tokens.get(*pos)?;
    *pos += 1;
    if token == "(" {
        let ok = spdx_or(tokens, pos, allowed)?;
        if tokens.get(*pos) != Some(&")") {
            return None;
        }
        *pos += 1;
        return Some(ok);
    }
    if matches!(token, ")" | "AND" | "OR" | "WITH") {
        return None;
    }
    let mut id = token.to_string();
    if tokens.get(*pos) == Some(&"WITH") {
        let exception = tokens.get(*pos + 1)?;
        id = format!("{token} WITH {exception}");
        *pos += 2;
    }
    Some(allowed.contains(&id.as_str()))
}

/// The `name version` pairs in `cargo tree --prefix none --format "{p}"` output. Lines look
/// like `tauri v2.12.4`, with ` (path)` for local crates and ` (*)` for repeats.
// @implements CT-DEV-014
pub fn tree_crates(text: &str) -> BTreeSet<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let name = words.next()?;
            let version = words.next()?.strip_prefix('v')?;
            Some((name.to_string(), version.to_string()))
        })
        .collect()
}

/// Third-party crates from `cargo metadata --filter-platform` output, one document per
/// desktop target. `shipped` holds the crates `cargo tree -e normal,no-proc-macro` lists
/// for any desktop target; every other crate is build-only. The scope comes from
/// `cargo tree` because `cargo metadata` merges the features build scripts enable into the
/// normal graph, which would mark build-time crates as shipped.
// @implements CT-DEV-014
pub fn cargo_packages(
    metadata: &[Json],
    shipped: &BTreeSet<(String, String)>,
) -> Result<Vec<Package>, String> {
    let mut all: BTreeMap<String, Package> = BTreeMap::new();
    for doc in metadata {
        let members: Vec<&str> = doc
            .get("workspace_members")
            .map(Json::items)
            .unwrap_or_default()
            .iter()
            .filter_map(Json::as_str)
            .collect();
        for p in doc.get("packages").map(Json::items).unwrap_or_default() {
            let id = p
                .get("id")
                .and_then(Json::as_str)
                .ok_or("package without id")?;
            if members.contains(&id) {
                continue;
            }
            let text = |key: &str| p.get(key).and_then(Json::as_str).map(str::to_string);
            let name = text("name").ok_or("package without name")?;
            let version = text("version").ok_or("package without version")?;
            let manifest = text("manifest_path").map(PathBuf::from);
            let registry = text("source").is_some_and(|s| s.contains("crates.io-index"));
            let key = (name.clone(), version.clone());
            let package = Package {
                ecosystem: Ecosystem::Cargo,
                license: text("license").unwrap_or_else(|| "NOASSERTION".to_string()),
                scope: if shipped.contains(&key) {
                    Scope::Shipped
                } else {
                    Scope::BuildOnly
                },
                repository: text("repository"),
                source: registry.then(|| {
                    format!("https://static.crates.io/crates/{name}/{name}-{version}.crate")
                }),
                dir: manifest
                    .as_deref()
                    .and_then(Path::parent)
                    .map(Path::to_path_buf),
                archive: manifest.as_deref().and_then(registry_archive),
                license_texts: Vec::new(),
                name,
                version,
            };
            all.entry(package.label()).or_insert(package);
        }
    }
    Ok(all.into_values().collect())
}

/// The `.crate` archive cargo keeps for a crate unpacked from a registry:
/// `<registry>/src/<index>/<name>-<version>/Cargo.toml` maps to
/// `<registry>/cache/<index>/<name>-<version>.crate`.
// @implements CT-DEV-014
fn registry_archive(manifest: &Path) -> Option<PathBuf> {
    let dir = manifest.parent()?;
    let index = dir.parent()?;
    let src = index.parent()?;
    if src.file_name()? != "src" {
        return None;
    }
    let file = format!("{}.crate", dir.file_name()?.to_str()?);
    Some(
        src.parent()?
            .join("cache")
            .join(index.file_name()?)
            .join(file),
    )
}

/// Third-party packages from a `package-lock.json` (version 2 or 3). Packages marked `dev`
/// are build-only; everything else, including `devOptional`, counts as shipped.
// @implements CT-DEV-014
pub fn npm_packages(lock: &Json, modules_parent: &Path) -> Vec<Package> {
    let mut all: BTreeMap<String, Package> = BTreeMap::new();
    let entries = lock.get("packages").map(Json::fields).unwrap_or_default();
    for (key, p) in entries {
        let Some(at) = key.rfind("node_modules/") else {
            continue;
        };
        if p.get("link").and_then(Json::as_bool) == Some(true) {
            continue;
        }
        let name = key[at + "node_modules/".len()..].to_string();
        let text = |key: &str| p.get(key).and_then(Json::as_str).map(str::to_string);
        let Some(version) = text("version") else {
            continue;
        };
        let dev = p.get("dev").and_then(Json::as_bool) == Some(true);
        let package = Package {
            ecosystem: Ecosystem::Npm,
            license: text("license").unwrap_or_else(|| "NOASSERTION".to_string()),
            scope: if dev {
                Scope::BuildOnly
            } else {
                Scope::Shipped
            },
            repository: None,
            source: text("resolved"),
            dir: Some(modules_parent.join(key)),
            archive: None,
            license_texts: Vec::new(),
            name,
            version,
        };
        let entry = all
            .entry(package.label())
            .or_insert_with(|| package.clone());
        if package.scope == Scope::Shipped {
            entry.scope = Scope::Shipped;
        }
    }
    all.into_values().collect()
}

/// The license, notice, and copying files at the top of a package directory, and every file
/// in a `LICENSES` folder there, in path order.
// @implements CT-DEV-014
pub fn read_license_texts(dir: &Path) -> Vec<String> {
    let is_license = |name: &str| {
        let upper = name.to_ascii_uppercase();
        ["LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE"]
            .iter()
            .any(|p| upper.starts_with(p))
    };
    let mut files = Vec::new();
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(d) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_file() && (d != dir || is_license(&name)) {
                files.push(entry.path());
            } else if kind.is_dir() && d == dir && name.eq_ignore_ascii_case("LICENSES") {
                dirs.push(entry.path());
            }
        }
    }
    files.sort();
    files
        .iter()
        .filter_map(|f| std::fs::read(f).ok())
        .map(|bytes| String::from_utf8_lossy(&bytes).trim_end().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// One `[[dep]]` entry of `deps.toml` that matters for the license checks.
// @implements CT-DEV-008, CT-DEV-014
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepEntry {
    pub name: String,
    pub ecosystem: String,
    pub license: String,
    pub scope: Option<String>,
    pub line: usize,
}

/// Reads the `[[dep]]` entries of `deps.toml`.
// @implements CT-DEV-008, CT-DEV-014
pub fn read_dep_entries(text: &str) -> Result<Vec<DepEntry>, String> {
    let tables = parse_tables(text).map_err(|e| format!("{DEPS_FILE}: {e}"))?;
    let mut entries = Vec::new();
    for dep in tables.iter().filter(|t| t.array && t.name == "dep") {
        let field = |key: &str| {
            dep.string(key)
                .map(str::to_string)
                .map_err(|e| format!("{DEPS_FILE}: {e}"))
        };
        entries.push(DepEntry {
            name: field("name")?,
            ecosystem: field("ecosystem")?,
            license: field("license")?,
            scope: dep.get("scope").and(field("scope").ok()),
            line: dep.line,
        });
    }
    Ok(entries)
}

/// License policy findings (ADR-0003). npm packages must fit their tier or have a
/// `deps.toml` exception with the same license and a scope that covers them. Cargo licenses
/// are checked by cargo-deny, so for crates this checks that each `scope` in `deps.toml`
/// matches what actually ships, and that every shipped MPL crate is recorded as shipped.
// @implements CT-DEV-008, CT-DEV-014, CT-USE-007
pub fn check_policy(packages: &[Package], deps: &[DepEntry]) -> Report {
    let mut report = Report::default();
    let mut push = |path: &str, line: usize, severity, message: String, help: &str| {
        report.findings.push(Finding {
            path: path.to_string(),
            line,
            severity,
            message,
            help: help.to_string(),
        });
    };
    let entry = |p: &Package| {
        deps.iter()
            .find(|d| d.ecosystem == p.ecosystem.name() && d.name == p.name)
    };
    for p in packages {
        let exception = entry(p).filter(|d| d.license == p.license);
        let scoped = exception.and_then(|d| d.scope.as_deref());
        if p.ecosystem == Ecosystem::Npm {
            let (tier, covered) = match p.scope {
                Scope::Shipped => (&SHIPPED_LICENSES[..], scoped == Some("shipped")),
                Scope::BuildOnly => (&BUILD_LICENSES[..], scoped.is_some()),
            };
            if !license_allows(&p.license, tier) && !covered {
                push(
                    NPM_LOCK,
                    0,
                    Severity::Error,
                    format!(
                        "{} npm package `{}` has license `{}`, outside the {} tier",
                        p.scope.name(),
                        p.label(),
                        p.license,
                        p.scope.name()
                    ),
                    "replace the package, or ask John for an exception and record it in deps.toml with its scope (ADR-0003)",
                );
            }
        }
        if p.needs_source() && scoped != Some("shipped") {
            push(
                DEPS_FILE,
                0,
                Severity::Error,
                format!(
                    "`{}` ships under `{}` but deps.toml does not record it as shipped",
                    p.label(),
                    p.license
                ),
                "after John approves it, add a [[dep]] entry with scope = \"shipped\" and the via path",
            );
        }
        if p.scope == Scope::Shipped && p.license_texts.is_empty() {
            let path = match p.ecosystem {
                Ecosystem::Cargo => "Cargo.lock",
                Ecosystem::Npm => NPM_LOCK,
            };
            push(
                path,
                0,
                Severity::Warning,
                format!(
                    "`{}` ({}) has no license file in its package",
                    p.label(),
                    p.license
                ),
                "check its repository and add the text to the notices by hand if its license requires one",
            );
        }
    }
    for d in deps.iter().filter(|d| d.scope.is_some()) {
        let found = packages
            .iter()
            .find(|p| p.ecosystem.name() == d.ecosystem && p.name == d.name);
        let Some(p) = found else {
            continue;
        };
        if d.scope.as_deref() != Some(p.scope.name()) {
            let severity = match p.scope {
                Scope::Shipped => Severity::Error,
                Scope::BuildOnly => Severity::Warning,
            };
            push(
                DEPS_FILE,
                d.line,
                severity,
                format!(
                    "`{}` is {}, but deps.toml says scope = {:?}",
                    p.label(),
                    p.scope.name(),
                    d.scope.as_deref().unwrap_or("")
                ),
                "correct the scope; a crate that ships needs John's approval under the shipped tier",
            );
        }
    }
    report.findings.sort();
    report
}

/// The notices file: every shipped package, the source of each MPL-2.0 package, and the
/// license texts, each identical text printed once with the packages that use it.
// @implements CT-DEV-014
pub fn notices_text(packages: &[Package]) -> String {
    let shipped: Vec<&Package> = packages
        .iter()
        .filter(|p| p.scope == Scope::Shipped)
        .collect();
    let mut out = String::new();
    out.push_str("Clewtrace third-party notices\n=============================\n\n");
    out.push_str(
        "Clewtrace includes the third-party packages listed below. Each keeps its own\n\
         license, reproduced in the License texts section. This file is generated by\n\
         `clew notices` from Cargo.lock and app/package-lock.json.\n\n",
    );
    out.push_str("Packages\n--------\n\n");
    for p in &shipped {
        out.push_str(&format!(
            "{}  {}  {}\n",
            p.ecosystem.name(),
            p.label(),
            p.license
        ));
    }
    let sourced: Vec<&&Package> = shipped.iter().filter(|p| p.needs_source()).collect();
    if !sourced.is_empty() {
        out.push_str("\nSource code of MPL-2.0 packages\n-------------------------------\n\n");
        out.push_str(
            "These packages are used unmodified. Their source is attached to each release\n\
             as the archive named below, and is also available from the source link.\n\n",
        );
        for p in sourced {
            let archive = p.archive.as_ref().and_then(|a| a.file_name()).map_or_else(
                || "(none)".to_string(),
                |f| f.to_string_lossy().into_owned(),
            );
            out.push_str(&format!(
                "{}\n  License: {}\n  Release attachment: {archive}\n  Source: {}\n",
                p.label(),
                p.license,
                p.source.as_deref().unwrap_or("(unknown)")
            ));
            if let Some(repo) = &p.repository {
                out.push_str(&format!("  Repository: {repo}\n"));
            }
            out.push('\n');
        }
    }
    out.push_str("\nLicense texts\n-------------\n");
    let mut by_text: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut missing = Vec::new();
    for p in &shipped {
        if p.license_texts.is_empty() {
            missing.push(p);
        }
        for text in &p.license_texts {
            by_text.entry(text).or_default().push(p.label());
        }
    }
    let mut groups: Vec<(&str, Vec<String>)> = by_text.into_iter().collect();
    groups.sort_by(|a, b| a.1.cmp(&b.1));
    for (text, users) in groups {
        out.push_str(&format!(
            "\n==== Used by: {} ====\n\n{text}\n",
            users.join(", ")
        ));
    }
    for p in missing {
        out.push_str(&format!(
            "\n==== {} ====\n\nLicensed under {}. The package includes no license file; see {}.\n",
            p.label(),
            p.license,
            p.repository
                .as_deref()
                .or(p.source.as_deref())
                .unwrap_or("its registry page")
        ));
    }
    out
}

/// A CycloneDX 1.6 SBOM of every third-party package. Shipped packages have scope
/// `required`, build-only ones `excluded`, and each carries a `clewtrace:scope` property.
// @implements CT-DEV-014
pub fn sbom_json(packages: &[Package], version: &str) -> String {
    let mut out = String::new();
    out.push_str(
        "{\n  \"bomFormat\": \"CycloneDX\",\n  \"specVersion\": \"1.6\",\n  \"version\": 1,\n",
    );
    out.push_str(&format!(
        "  \"metadata\": {{\n    \"component\": {{ \"type\": \"application\", \"bom-ref\": \"clewtrace\", \"name\": \"Clewtrace\", \"version\": {}, \"licenses\": [{{ \"expression\": \"BUSL-1.1\" }}] }}\n  }},\n",
        json::quote(version)
    ));
    out.push_str("  \"components\": [");
    for (i, p) in packages.iter().enumerate() {
        let cdx_scope = match p.scope {
            Scope::Shipped => "required",
            Scope::BuildOnly => "excluded",
        };
        let license = p.license.replace('/', " OR ");
        out.push_str(if i == 0 { "\n" } else { ",\n" });
        out.push_str(&format!(
            "    {{ \"type\": \"library\", \"bom-ref\": {purl}, \"name\": {}, \"version\": {}, \"purl\": {purl}, \"scope\": \"{cdx_scope}\", \"licenses\": [{{ \"expression\": {} }}], \"properties\": [{{ \"name\": \"clewtrace:scope\", \"value\": \"{}\" }}, {{ \"name\": \"clewtrace:ecosystem\", \"value\": \"{}\" }}] }}",
            json::quote(&p.name),
            json::quote(&p.version),
            json::quote(&license),
            p.scope.name(),
            p.ecosystem.name(),
            purl = json::quote(&p.purl()),
        ));
    }
    out.push_str("\n  ]\n}\n");
    out
}

// @implements CT-DEV-014
/// Runs `cargo` with the given arguments and returns what it printed.
pub type CargoRunner<'a> = &'a dyn Fn(&[&str]) -> Result<String, String>;

// @implements CT-DEV-014
/// Every third-party package: the crates from `cargo metadata` and `cargo tree` for each
/// desktop target, run through `run`, and the npm packages from the lockfile under `root`,
/// each with the license texts found in its installed folder.
pub fn read_packages(root: &Path, run: CargoRunner) -> Result<Vec<Package>, String> {
    let mut metadata = Vec::new();
    let mut shipped = BTreeSet::new();
    for target in DESKTOP_TARGETS {
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
        shipped.extend(tree_crates(&tree));
    }
    let mut packages = cargo_packages(&metadata, &shipped)?;
    let lock_path = root.join(NPM_LOCK);
    let lock_text =
        std::fs::read_to_string(&lock_path).map_err(|e| format!("{}: {e}", lock_path.display()))?;
    let lock = json::parse(&lock_text).map_err(|e| format!("{}: {e}", lock_path.display()))?;
    let modules = root.join(NPM_MODULES);
    if !modules.is_dir() {
        return Err(format!(
            "{} is missing. Run `npm ci` in app/ first",
            modules.display()
        ));
    }
    let app = modules.parent().unwrap_or(root);
    packages.extend(npm_packages(&lock, app));
    for p in &mut packages {
        if let Some(dir) = &p.dir {
            p.license_texts = read_license_texts(dir);
        }
    }
    Ok(packages)
}

// @implements CT-DEV-014
/// Writes `THIRD-PARTY-NOTICES.txt`, `sbom.cdx.json`, and a copy of the source archive of
/// each shipped package that needs one into `dir` and `dir/mpl-sources`.
pub fn write_outputs(dir: &Path, packages: &[Package], version: &str) -> Result<(), String> {
    let sources = dir.join("mpl-sources");
    std::fs::create_dir_all(&sources).map_err(|e| format!("{}: {e}", sources.display()))?;
    let write = |path: PathBuf, text: String| {
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
    };
    write(dir.join("THIRD-PARTY-NOTICES.txt"), notices_text(packages))?;
    write(dir.join("sbom.cdx.json"), sbom_json(packages, version))?;
    for p in packages.iter().filter(|p| p.needs_source()) {
        let archive = p
            .archive
            .as_ref()
            .filter(|a| a.is_file())
            .ok_or_else(|| format!("no source archive found for {}", p.label()))?;
        let name = archive.file_name().unwrap_or_default();
        std::fs::copy(archive, sources.join(name))
            .map_err(|e| format!("{}: {e}", archive.display()))?;
    }
    Ok(())
}

// @verifies CT-DEV-008, CT-DEV-014
#[cfg(test)]
mod tests {
    use super::*;

    // @verifies CT-DEV-014
    #[test]
    fn reads_cargo_and_npm_packages_through_the_runner() {
        let root = std::env::temp_dir().join(format!("clew-packages-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let run = |args: &[&str]| -> Result<String, String> {
            match args[0] {
                "metadata" => Ok(metadata_text()),
                _ => Ok("app v0.1.0\nc v1.0.0\n".to_string()),
            }
        };
        let missing = read_packages(&root, &run).unwrap_err();
        assert!(missing.contains("package-lock.json"), "{missing}");

        std::fs::create_dir_all(root.join(NPM_LOCK).parent().unwrap()).unwrap();
        std::fs::write(root.join(NPM_LOCK), "{ not json").unwrap();
        let broken = read_packages(&root, &run).unwrap_err();
        assert!(broken.contains("package-lock.json"), "{broken}");

        std::fs::write(
            root.join(NPM_LOCK),
            r#"{"packages": {"node_modules/tool": {"version": "1.0.0", "license": "ISC", "dev": true}}}"#,
        )
        .unwrap();
        let no_modules = read_packages(&root, &run).unwrap_err();
        assert!(no_modules.contains("npm ci"), "{no_modules}");

        let tool = root.join(NPM_MODULES).join("tool");
        std::fs::create_dir_all(&tool).unwrap();
        std::fs::write(tool.join("LICENSE"), "ISC text").unwrap();
        let packages = read_packages(&root, &run).unwrap();
        let names: Vec<&str> = packages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["a", "c", "tool"]);
        assert_eq!(packages[2].license_texts, ["ISC text"]);

        let failing = |_: &[&str]| -> Result<String, String> { Err("no cargo".to_string()) };
        assert_eq!(read_packages(&root, &failing).unwrap_err(), "no cargo");
        let garbled = |_: &[&str]| -> Result<String, String> { Ok("{ nope".to_string()) };
        let bad = read_packages(&root, &garbled).unwrap_err();
        assert!(bad.starts_with("cargo metadata: "), "{bad}");
        let _ = std::fs::remove_dir_all(&root);
    }

    // @verifies CT-DEV-014
    #[test]
    fn writes_the_outputs_and_copies_the_mpl_source() {
        let dir = std::env::temp_dir().join(format!("clew-outputs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("opt-1.0.crate");
        std::fs::write(&archive, "source").unwrap();
        let mut mpl = package(Ecosystem::Cargo, "opt", "MPL-2.0", Scope::Shipped);
        mpl.archive = Some(archive.clone());
        let out = dir.join("out");
        write_outputs(&out, &[mpl.clone()], "1.2.3").unwrap();
        let notices = std::fs::read_to_string(out.join("THIRD-PARTY-NOTICES.txt")).unwrap();
        let sbom = std::fs::read_to_string(out.join("sbom.cdx.json")).unwrap();
        let copied = std::fs::read_to_string(out.join("mpl-sources/opt-1.0.crate")).unwrap();
        assert!(notices.contains("opt"), "{notices}");
        assert!(sbom.contains("1.2.3"), "{sbom}");
        assert_eq!(copied, "source");

        mpl.archive = None;
        let err = write_outputs(&dir.join("out2"), &[mpl], "1.2.3").unwrap_err();
        assert!(err.contains("no source archive found for opt"), "{err}");
        let blocked = write_outputs(&archive.join("sub"), &[], "1.2.3").unwrap_err();
        assert!(blocked.contains("mpl-sources"), "{blocked}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // @verifies CT-DEV-008, CT-DEV-014
    #[test]
    fn license_expressions_follow_spdx() {
        let shipped = &SHIPPED_LICENSES[..];
        assert!(license_allows("MIT", shipped));
        assert!(license_allows("MIT OR Apache-2.0", shipped));
        assert!(license_allows("MIT/Apache-2.0", shipped));
        assert!(license_allows(
            "(MIT OR Apache-2.0) AND Unicode-3.0",
            shipped
        ));
        assert!(license_allows("0BSD OR MIT OR Apache-2.0", shipped));
        assert!(!license_allows("MIT AND BSD-3-Clause", shipped));
        assert!(!license_allows("ISC", shipped));
        assert!(!license_allows("MPL-2.0", shipped));
        assert!(!license_allows("GPL-3.0-only OR LGPL-2.1", &BUILD_LICENSES));
        assert!(license_allows(
            "Apache-2.0 WITH LLVM-exception",
            &BUILD_LICENSES
        ));
        assert!(!license_allows("Apache-2.0 WITH LLVM-exception", shipped));
        for broken in ["", "MIT OR", "(MIT", "MIT)", "AND MIT", "MIT WITH"] {
            assert!(!license_allows(broken, shipped), "`{broken}` allowed");
        }
    }

    /// `cargo metadata` output for a small workspace member `app` and crates `a` and `c`.
    fn metadata_text() -> String {
        let pkg = |name: &str, license: &str| {
            format!(
                r#"{{"id": "{name}-id", "name": "{name}", "version": "1.0.0", "license": "{license}",
                "source": "registry+https://github.com/rust-lang/crates.io-index",
                "manifest_path": "/home/u/.cargo/registry/src/index.crates.io-x/{name}-1.0.0/Cargo.toml"}}"#
            )
        };
        format!(
            r#"{{"packages": [{}, {}, {}], "workspace_members": ["app-id"]}}"#,
            pkg("app", "BUSL-1.1"),
            pkg("c", "MPL-2.0"),
            pkg("a", "MIT"),
        )
    }

    fn metadata() -> Json {
        json::parse(&metadata_text()).expect("sample metadata parses")
    }

    fn scopes(packages: &[Package]) -> Vec<(String, Scope)> {
        packages.iter().map(|p| (p.name.clone(), p.scope)).collect()
    }

    // @verifies CT-DEV-014
    #[test]
    fn crates_ship_when_cargo_tree_lists_them() {
        let tree = "app v0.1.0 (/src/app)\nc v1.0.0\nc v1.0.0 (*)\nother v2.0.0\n\n";
        let shipped = tree_crates(tree);
        assert_eq!(shipped.len(), 3);
        assert!(shipped.contains(&("c".to_string(), "1.0.0".to_string())));
        let packages = cargo_packages(&[metadata(), metadata()], &shipped).expect("reads");
        assert_eq!(
            scopes(&packages),
            [
                ("a".to_string(), Scope::BuildOnly),
                ("c".to_string(), Scope::Shipped),
            ]
        );
        let c = &packages[1];
        assert!(c.needs_source());
        assert_eq!(
            c.archive.as_deref(),
            Some(Path::new(
                "/home/u/.cargo/registry/cache/index.crates.io-x/c-1.0.0.crate"
            ))
        );
        assert_eq!(
            c.source.as_deref(),
            Some("https://static.crates.io/crates/c/c-1.0.0.crate")
        );
    }

    fn npm_lock() -> Json {
        json::parse(
            r#"{"lockfileVersion": 3, "packages": {
              "": {"name": "app", "license": "BUSL-1.1"},
              "node_modules/@scope/api": {"version": "2.0.0", "license": "MIT", "resolved": "https://registry.npmjs.org/@scope/api/-/api-2.0.0.tgz"},
              "node_modules/tool": {"version": "1.0.0", "license": "ISC", "dev": true},
              "node_modules/tool/node_modules/lib": {"version": "3.0.0", "license": "BSD-3-Clause", "dev": true},
              "node_modules/both": {"version": "1.0.0", "license": "MIT", "devOptional": true},
              "node_modules/linked": {"link": true, "resolved": "../x"}
            }}"#,
        )
        .expect("sample lock parses")
    }

    // @verifies CT-DEV-014
    #[test]
    fn npm_dev_packages_are_build_only() {
        let packages = npm_packages(&npm_lock(), Path::new("app"));
        assert_eq!(
            scopes(&packages),
            [
                ("@scope/api".to_string(), Scope::Shipped),
                ("both".to_string(), Scope::Shipped),
                ("lib".to_string(), Scope::BuildOnly),
                ("tool".to_string(), Scope::BuildOnly),
            ]
        );
        assert_eq!(packages[0].purl(), "pkg:npm/%40scope/api@2.0.0");
        assert_eq!(
            packages[2].dir.as_deref(),
            Some(Path::new("app/node_modules/tool/node_modules/lib"))
        );
    }

    fn package(ecosystem: Ecosystem, name: &str, license: &str, scope: Scope) -> Package {
        Package {
            ecosystem,
            name: name.to_string(),
            version: "1.0.0".to_string(),
            license: license.to_string(),
            scope,
            repository: Some(format!("https://example.com/{name}")),
            source: Some(format!(
                "https://static.crates.io/crates/{name}/{name}-1.0.0.crate"
            )),
            dir: None,
            archive: Some(PathBuf::from(format!("/cache/{name}-1.0.0.crate"))),
            license_texts: vec![format!("{license} text")],
        }
    }

    fn dep(name: &str, ecosystem: &str, license: &str, scope: Option<&str>) -> DepEntry {
        DepEntry {
            name: name.to_string(),
            ecosystem: ecosystem.to_string(),
            license: license.to_string(),
            scope: scope.map(str::to_string),
            line: 7,
        }
    }

    // @verifies CT-DEV-008, CT-DEV-014
    #[test]
    fn policy_checks_npm_tiers_and_recorded_scopes() {
        use Ecosystem::{Cargo, Npm};
        use Scope::{BuildOnly, Shipped};
        let packages = [
            package(Npm, "ok-shipped", "MIT OR Apache-2.0", Shipped),
            package(Npm, "isc-shipped", "ISC", Shipped),
            package(Npm, "isc-tool", "ISC", BuildOnly),
            package(Npm, "gpl-tool", "GPL-3.0", BuildOnly),
            package(Npm, "approved", "MPL-2.0", Shipped),
            package(Cargo, "option-ext", "MPL-2.0", Shipped),
            package(Cargo, "cssparser", "MPL-2.0", Shipped),
            package(Cargo, "selectors", "MPL-2.0", BuildOnly),
            package(Cargo, "unrecorded", "MPL-2.0", Shipped),
        ];
        let deps = [
            dep("approved", "npm", "MPL-2.0", Some("shipped")),
            dep("option-ext", "cargo", "MPL-2.0", Some("shipped")),
            dep("cssparser", "cargo", "MPL-2.0", Some("build-only")),
            dep("selectors", "cargo", "MPL-2.0", Some("build-only")),
        ];
        let report = check_policy(&packages, &deps);
        let messages: Vec<String> = report.findings.iter().map(|f| f.to_string()).collect();
        let joined = messages.join("\n");
        for want in [
            "app/package-lock.json: error: shipped npm package `isc-shipped 1.0.0` has license `ISC`, outside the shipped tier",
            "app/package-lock.json: error: build-only npm package `gpl-tool 1.0.0` has license `GPL-3.0`",
            "deps.toml:7: error: `cssparser 1.0.0` is shipped, but deps.toml says scope = \"build-only\"",
            "deps.toml: error: `unrecorded 1.0.0` ships under `MPL-2.0` but deps.toml does not record it as shipped",
        ] {
            assert!(joined.contains(want), "missing `{want}` in:\n{joined}");
        }
        assert_eq!(report.count(Severity::Error), 5, "{joined}");
        assert_eq!(report.count(Severity::Warning), 0, "{joined}");
    }

    // @verifies CT-DEV-014
    #[test]
    fn notices_list_shipped_packages_sources_and_shared_texts() {
        use Scope::{BuildOnly, Shipped};
        let mut a = package(Ecosystem::Cargo, "a", "MIT", Shipped);
        let mut b = package(Ecosystem::Npm, "b", "MIT", Shipped);
        a.license_texts = vec!["Same MIT text".to_string()];
        b.license_texts = vec!["Same MIT text".to_string()];
        let mpl = package(Ecosystem::Cargo, "option-ext", "MPL-2.0", Shipped);
        let mut bare = package(Ecosystem::Cargo, "bare", "Apache-2.0", Shipped);
        bare.license_texts.clear();
        let tool = package(Ecosystem::Cargo, "tool", "ISC", BuildOnly);
        let text = notices_text(&[a, b, mpl, bare, tool]);
        assert!(text.contains("cargo  a 1.0.0  MIT\n"));
        assert!(text.contains("npm  b 1.0.0  MIT\n"));
        assert!(!text.contains("tool"), "build-only package listed:\n{text}");
        assert!(text.contains(
            "option-ext 1.0.0\n  License: MPL-2.0\n  Release attachment: option-ext-1.0.0.crate\n  Source: https://static.crates.io/crates/option-ext/option-ext-1.0.0.crate\n"
        ));
        assert_eq!(text.matches("Same MIT text").count(), 1, "{text}");
        assert!(text.contains("==== Used by: a 1.0.0, b 1.0.0 ===="));
        assert!(text.contains("==== bare 1.0.0 ====\n\nLicensed under Apache-2.0. The package includes no license file; see https://example.com/bare.\n"));
    }

    // @verifies CT-DEV-014
    #[test]
    fn sbom_marks_each_package_shipped_or_build_only() {
        let packages = [
            package(Ecosystem::Cargo, "a", "MIT/Apache-2.0", Scope::Shipped),
            package(Ecosystem::Npm, "@s/t", "ISC", Scope::BuildOnly),
        ];
        let text = sbom_json(&packages, "0.1.0");
        let doc = json::parse(&text).expect("SBOM is valid JSON");
        assert_eq!(
            doc.get("bomFormat").and_then(Json::as_str),
            Some("CycloneDX")
        );
        let components = doc.get("components").map(Json::items).unwrap_or_default();
        assert_eq!(components.len(), 2);
        let field = |i: usize, key: &str| components[i].get(key).and_then(Json::as_str);
        assert_eq!(field(0, "purl"), Some("pkg:cargo/a@1.0.0"));
        assert_eq!(field(0, "scope"), Some("required"));
        assert_eq!(field(1, "purl"), Some("pkg:npm/%40s/t@1.0.0"));
        assert_eq!(field(1, "scope"), Some("excluded"));
        let license = components[0]
            .get("licenses")
            .map(Json::items)
            .unwrap_or_default()[0]
            .get("expression")
            .and_then(Json::as_str);
        assert_eq!(license, Some("MIT OR Apache-2.0"));
        let scope = components[1]
            .get("properties")
            .map(Json::items)
            .unwrap_or_default()[0]
            .get("value")
            .and_then(Json::as_str);
        assert_eq!(scope, Some("build-only"));
    }

    // @verifies CT-DEV-014
    #[test]
    fn license_files_are_read_from_the_package_folder() {
        let dir = std::env::temp_dir().join(format!("clew-notices-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("LICENSES")).expect("temp dir");
        std::fs::create_dir_all(dir.join("src")).expect("temp dir");
        std::fs::write(dir.join("LICENSE-MIT"), "MIT text\n\n").expect("write");
        std::fs::write(dir.join("license.md"), "Apache text").expect("write");
        std::fs::write(dir.join("LICENSES/BSD.txt"), "BSD text").expect("write");
        std::fs::write(dir.join("src/LICENSE"), "nested, ignored").expect("write");
        std::fs::write(dir.join("README.md"), "not a license").expect("write");
        let texts = read_license_texts(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(texts, ["MIT text", "BSD text", "Apache text"]);
    }
}
