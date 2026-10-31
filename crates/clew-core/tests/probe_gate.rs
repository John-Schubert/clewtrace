//! The app's performance probes (`perf-probe` feature, app/src-tauri/src/probe.rs) must not
//! reach a shipped build. The budgets in CT-EFF-002 and CT-EFF-003 are for the app users
//! install, so probe code in it would be measured and shipped by mistake.

use std::fs;
use std::path::Path;

// @verifies CT-EFF-002, CT-EFF-003
fn read(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The features `default` turns on in a Cargo.toml, or `None` when it has no `default`.
// @verifies CT-EFF-002, CT-EFF-003
fn default_features(manifest: &str) -> Option<String> {
    let features = manifest.split("[features]").nth(1)?;
    let features = features.split("\n[").next()?;
    features
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("default"))
        .map(str::to_string)
}

// @verifies CT-EFF-002, CT-EFF-003
#[test]
fn the_probe_feature_exists_and_is_not_a_default() {
    let manifest = read("app/src-tauri/Cargo.toml");
    assert!(
        manifest.contains("perf-probe ="),
        "the app has no perf-probe feature"
    );
    let default = default_features(&manifest).expect("the app lists its default features");
    assert!(
        !default.contains("perf-probe"),
        "perf-probe is a default feature: {default}"
    );
    assert!(
        default_features("[features]\ndefault = [\"perf-probe\"]\n")
            .is_some_and(|d| d.contains("perf-probe"))
    );
}

// @verifies CT-EFF-002, CT-EFF-003
#[test]
fn release_builds_pass_no_features() {
    let release = read(".github/workflows/release.yml");
    for (n, line) in release.lines().enumerate() {
        let code = line.split('#').next().unwrap_or("");
        for word in ["--features", "-F ", "--all-features", "perf-probe"] {
            assert!(
                !code.contains(word),
                "release.yml line {} passes `{word}`: {line}",
                n + 1
            );
        }
    }
}
