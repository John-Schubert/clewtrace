//! The Clewtrace desktop app. It opens one window and, when started with a folder path,
//! loads that project in the background and serves it to the UI in pages (ADR-0004). The
//! UI is embedded in the binary, and the only commands it may call are the ones granted in
//! capabilities/main.json.

// @begin CT-PLT-001
// Release builds on Windows open no console window next to the app window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// @end

// @begin CT-EFF-004, CT-EFF-005
mod commands;
mod data;
#[cfg(feature = "perf-probe")]
mod probe;
mod recent;

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;

use data::AppState;
// @end

// @begin CT-EFF-004, CT-EFF-005
// Tests in data.rs open the generated projects the perf harness uses.
#[cfg(test)]
#[path = "../../../crates/clew-core/tests/generate/mod.rs"]
mod generate;
// @end

/// The context the app is built with: tauri.conf.json and the capability files.
/// Generated once, because a second `generate_context!` in the same build defines the
/// embedded Info.plist symbol twice on macOS.
// @implements CT-PLT-001, CT-PLT-003
fn context() -> tauri::Context<tauri::Wry> {
    tauri::generate_context!()
}

/// The project folder given on the command line: the first argument that is not an
/// option. macOS may pass a `-psn_...` process serial number when started from Finder.
// @implements CT-EFF-004
fn project_arg(args: impl IntoIterator<Item = OsString>) -> Option<PathBuf> {
    args.into_iter()
        .find(|a| !a.to_string_lossy().starts_with('-'))
        .map(PathBuf::from)
}

/// Web view settings that lower idle memory, as environment variables WebKitGTK reads when
/// it starts. `has` says whether a variable is already set, which is left alone so a user
/// can override it; `set` sets one. The DMABUF renderer is off because it cost about 140 MB
/// of shared memory for no gain on a window of text and tables (task 2.5b, PLAN.md).
// @implements CT-EFF-002, CT-EFF-010
#[cfg(any(target_os = "linux", test))]
fn apply_web_view_settings(has: &dyn Fn(&str) -> bool, set: &mut dyn FnMut(&str, &str)) {
    for (name, value) in [("WEBKIT_DISABLE_DMABUF_RENDERER", "1")] {
        if !has(name) {
            set(name, value);
        }
    }
}

// @implements CT-PLT-001, CT-PLT-003, CT-EFF-004, CT-EFF-005, CT-EFF-008, CT-EFF-011, CT-EFF-002
fn main() {
    // Before any thread starts, since the environment is not safe to change after.
    #[cfg(target_os = "linux")]
    apply_web_view_settings(&|n| std::env::var_os(n).is_some(), &mut |n, v| {
        // SAFETY: main has not started a thread yet.
        unsafe { std::env::set_var(n, v) }
    });
    #[cfg(feature = "perf-probe")]
    let started = std::time::Instant::now();
    let state = Arc::new(AppState::default());
    // Loading starts before the window opens and runs beside it.
    if let Some(path) = project_arg(std::env::args_os().skip(1)) {
        state.open(path);
    }
    #[cfg_attr(not(feature = "perf-probe"), allow(unused_mut))]
    let mut context = context();
    #[cfg(feature = "perf-probe")]
    probe::prepare(&mut context);
    let builder = tauri::Builder::default();
    #[cfg(feature = "perf-probe")]
    let builder = builder.setup(move |app| probe::start(app, started));
    builder
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::project_status,
            commands::wait_for_project,
            commands::wait_for_disk_change,
            commands::tree_children,
            commands::element_detail,
            commands::element_links,
            commands::element_impact,
            commands::coverage_gaps,
            commands::source_lines,
            commands::source_text,
            commands::save_source,
            commands::validate_source,
            commands::revert_source,
            commands::editor_keywords,
            commands::problems,
            commands::search,
            commands::pick_folder,
            commands::open_project,
            commands::recent_projects,
            commands::clear_recent,
        ])
        .run(context)
        .expect("Clewtrace could not start its window");
}

// @verifies CT-SEC-001, CT-PLT-003, CT-EFF-004
#[cfg(test)]
mod tests {
    use super::{context, project_arg};

    // @verifies CT-EFF-004
    #[test]
    fn the_project_is_the_first_argument_that_is_not_an_option() {
        let args = |list: &[&str]| list.iter().map(Into::into).collect::<Vec<_>>();
        assert_eq!(project_arg(args(&[])), None);
        assert_eq!(project_arg(args(&["-psn_0_1"])), None);
        assert_eq!(
            project_arg(args(&["-psn_0_1", "my project", "x"])),
            Some("my project".into())
        );
    }

    // @verifies CT-SEC-001, CT-PLT-003
    #[test]
    fn content_security_policy_allows_only_bundled_content_and_ipc() {
        let context = context();
        let security = &context.config().app.security;
        let csp = security.csp.as_ref().expect("tauri.conf.json sets a csp");
        let csp = csp.to_string();
        let directives: Vec<&str> = csp.split(';').map(str::trim).collect();
        for want in [
            "default-src 'self'",
            "script-src 'self'",
            "connect-src ipc: http://ipc.localhost",
            "object-src 'none'",
            "base-uri 'none'",
            "form-action 'none'",
        ] {
            assert!(directives.contains(&want), "csp lacks `{want}`: {csp}");
        }
        let banned = [
            "'unsafe-inline'",
            "'unsafe-eval'",
            "https:",
            "http:",
            "*",
            "data:",
        ];
        for source in directives.iter().flat_map(|d| d.split_whitespace().skip(1)) {
            assert!(!banned.contains(&source), "csp allows `{source}`: {csp}");
        }
        assert!(security.freeze_prototype, "freezePrototype is off");
        let global = context.config().app.with_global_tauri;
        assert!(!global, "withGlobalTauri is on");
    }

    // @verifies CT-SEC-001
    #[test]
    fn no_plugins_are_configured() {
        assert!(context().config().plugins.0.is_empty());
    }
}

// @verifies CT-SEC-001, CT-PLT-003
#[cfg(test)]
mod no_network {
    use super::{apply_web_view_settings, context};
    use std::collections::BTreeSet;
    use std::path::Path;
    use std::process::Command;
    use tauri::utils::acl::capability::{Capability, CapabilityFile};
    use tauri::utils::config::{CapabilityEntry, WebviewUrl};

    /// Permissions reviewed as granting no network access. Adding one here needs the same
    /// review, recorded in docs/verification/no-network.md.
    /// The `allow-` permissions of the app's own commands (build.rs) only read the project
    /// the app has open, show the folder dialog, read and write the recent-projects file, or
    /// save an existing `.sysml` file in the open project; none of them opens a connection.
    /// The `core:event` listen and unlisten permissions and `core:window:allow-destroy` let
    /// the page hear its own window's close request and then close the window, so the
    /// editor saves first (CT-USE-008). They pass events inside the app only.
    const OFFLINE_PERMISSIONS: [&str; 25] = [
        "core:app:allow-name",
        "core:app:allow-version",
        "core:event:allow-listen",
        "core:event:allow-unlisten",
        "core:window:allow-destroy",
        "allow-project-status",
        "allow-wait-for-project",
        "allow-wait-for-disk-change",
        "allow-tree-children",
        "allow-element-detail",
        "allow-element-links",
        "allow-element-impact",
        "allow-coverage-gaps",
        "allow-source-lines",
        "allow-source-text",
        "allow-save-source",
        "allow-validate-source",
        "allow-revert-source",
        "allow-editor-keywords",
        "allow-problems",
        "allow-search",
        "allow-pick-folder",
        "allow-open-project",
        "allow-recent-projects",
        "allow-clear-recent",
    ];

    /// Features each pinned crate may enable, with its default features off. None of them
    /// pulls in a TLS stack, an HTTP client, or a plugin. The Tauri crates are in the app's
    /// manifest; the others are in clew-core's, which the app links. notify's
    /// `macos_fsevent` selects the macOS file event API, which is local only.
    const ALLOWED_FEATURES: [(&str, &[&str]); 7] = [
        ("tauri", &["wry"]),
        ("tauri-build", &["config-json"]),
        ("rfd", &["gtk3"]),
        ("rusqlite", &["bundled"]),
        ("tree-sitter", &[]),
        ("sha2", &[]),
        ("notify", &["macos_fsevent"]),
    ];

    /// The manifests of the crates the app is built from, relative to app/src-tauri.
    const MANIFESTS: [&str; 2] = ["Cargo.toml", "../../crates/clew-core/Cargo.toml"];

    /// Crates that open network connections or exist only to serve them.
    const NETWORK_CRATES: [&str; 16] = [
        "reqwest",
        "hyper",
        "ureq",
        "curl",
        "isahc",
        "attohttpc",
        "surf",
        "h2",
        "h3",
        "quinn",
        "tungstenite",
        "tokio-tungstenite",
        "native-tls",
        "rustls",
        "openssl",
        "socket2",
    ];

    /// The desktop targets Clewtrace ships for: Windows, macOS, and Linux on x64 and ARM64.
    const DESKTOP_TARGETS: [&str; 6] = [
        "x86_64-pc-windows-msvc",
        "aarch64-pc-windows-msvc",
        "x86_64-apple-darwin",
        "aarch64-apple-darwin",
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
    ];

    /// Problems in `cargo tree --prefix none --format {p}` output for one target, empty when
    /// no crate in the resolved tree is a known network crate or a Tauri plugin.
    fn tree_problems(target: &str, tree: &str) -> Vec<String> {
        let names: BTreeSet<&str> = tree
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .collect();
        names
            .into_iter()
            .filter(|name| NETWORK_CRATES.contains(name) || name.starts_with("tauri-plugin"))
            .map(|name| format!("{target}: `{name}` is in the resolved dependency tree"))
            .collect()
    }

    /// Every capability: the files in capabilities/ and any inlined in tauri.conf.json.
    fn capabilities() -> Vec<Capability> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities");
        let mut all = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("capabilities/ is readable") {
            let path = entry.expect("capabilities/ entry is readable").path();
            let file = CapabilityFile::load(&path)
                .unwrap_or_else(|e| panic!("{} does not parse: {e}", path.display()));
            match file {
                CapabilityFile::Capability(c) => all.push(c),
                CapabilityFile::List(list) | CapabilityFile::NamedList { capabilities: list } => {
                    all.extend(list)
                }
            }
        }
        for entry in &context().config().app.security.capabilities {
            if let CapabilityEntry::Inlined(c) = entry {
                all.push(c.clone());
            }
        }
        all
    }

    /// Problems with one capability, empty when it grants no network access.
    fn capability_problems(c: &Capability) -> Vec<String> {
        let mut problems = Vec::new();
        if let Some(remote) = &c.remote {
            problems.push(format!(
                "capability `{}` lets remote URLs call the app: {:?}",
                c.identifier, remote.urls
            ));
        }
        if !c.local {
            problems.push(format!("capability `{}` is not local", c.identifier));
        }
        for permission in &c.permissions {
            let id = permission.identifier().get();
            if !OFFLINE_PERMISSIONS.contains(&id) {
                problems.push(format!(
                    "capability `{}` grants `{id}`, which is not reviewed as offline",
                    c.identifier
                ));
            }
        }
        problems
    }

    /// Problems in a Cargo.toml, empty when its dependencies cannot reach the network.
    /// Each dependency must fit on one line, so nothing hides in a multi-line table.
    fn manifest_problems(manifest: &str) -> Vec<String> {
        let mut problems = Vec::new();
        let mut in_deps = false;
        for (n, raw) in manifest.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') {
                in_deps = line
                    .trim_matches(|c| c == '[' || c == ']')
                    .ends_with("dependencies");
                if line.contains("dependencies.") {
                    problems.push(format!("line {}: write `{line}` as one line", n + 1));
                }
                continue;
            }
            if !in_deps {
                continue;
            }
            let Some((name, spec)) = line.split_once('=') else {
                problems.push(format!("line {}: cannot read `{line}`", n + 1));
                continue;
            };
            let name = name.trim().trim_matches('"');
            let spec = spec.trim();
            let package = field(spec, "package")
                .map(|p| p.trim_matches('"').to_string())
                .unwrap_or_else(|| name.to_string());
            if spec.starts_with('{') && !spec.ends_with('}') {
                problems.push(format!("line {}: put `{name}` on one line", n + 1));
                continue;
            }
            if NETWORK_CRATES.contains(&package.as_str()) || package.starts_with("tauri-plugin") {
                problems.push(format!("line {}: `{package}` reaches the network", n + 1));
                continue;
            }
            let Some((_, allowed)) = ALLOWED_FEATURES.iter().find(|(c, _)| *c == package) else {
                continue;
            };
            if field(spec, "default-features") != Some("false") {
                problems.push(format!(
                    "line {}: `{package}` keeps default features",
                    n + 1
                ));
            }
            for feature in features(spec) {
                if !allowed.contains(&feature.as_str()) {
                    problems.push(format!(
                        "line {}: `{package}` enables feature `{feature}`",
                        n + 1
                    ));
                }
            }
        }
        problems
    }

    /// The raw value of `key = value` inside an inline table, up to the next comma or brace.
    fn field<'a>(spec: &'a str, key: &str) -> Option<&'a str> {
        let body = spec.strip_prefix('{')?.strip_suffix('}')?;
        let mut rest = body;
        while let Some(at) = rest.find(key) {
            let before = &rest[..at];
            let after = rest[at + key.len()..].trim_start();
            let starts_key = before.trim_end().is_empty() || before.trim_end().ends_with(',');
            if let (true, Some(value)) = (starts_key, after.strip_prefix('=')) {
                let value = value.trim_start();
                let end = if value.starts_with('[') {
                    value.find(']').map_or(value.len(), |i| i + 1)
                } else {
                    value.find([',', '}']).unwrap_or(value.len())
                };
                return Some(value[..end].trim());
            }
            rest = &rest[at + key.len()..];
        }
        None
    }

    /// The names in `features = [...]` of an inline table.
    fn features(spec: &str) -> Vec<String> {
        field(spec, "features")
            .map(|list| {
                list.trim_matches(|c| c == '[' || c == ']')
                    .split(',')
                    .map(|f| f.trim().trim_matches('"').to_string())
                    .filter(|f| !f.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    // @verifies CT-SEC-001, CT-PLT-003
    #[test]
    fn capabilities_grant_no_network_access() {
        let all = capabilities();
        assert!(!all.is_empty(), "no capability found");
        let problems: Vec<String> = all.iter().flat_map(capability_problems).collect();
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    // @verifies CT-SEC-001, CT-PLT-003
    #[test]
    fn windows_load_only_bundled_pages() {
        let context = context();
        let config = context.config();
        for window in &config.app.windows {
            let url = &window.url;
            assert!(
                matches!(url, WebviewUrl::App(_)),
                "window `{}` loads `{url}`",
                window.label
            );
        }
    }

    // @verifies CT-SEC-001, CT-PLT-003
    #[test]
    fn cargo_features_enable_no_network_access() {
        let mut problems = Vec::new();
        for manifest in MANIFESTS {
            let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(manifest);
            let text = std::fs::read_to_string(&path).expect("manifest is readable");
            let found = manifest_problems(&text);
            problems.extend(found.into_iter().map(|p| format!("{manifest}: {p}")));
        }
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    /// Checks the dependency tree cargo resolves for each desktop target, so a network
    /// crate pulled in indirectly or by a target-specific dependency fails too.
    /// Build dependencies are left out: they run on the build machine and do not ship.
    // @verifies CT-SEC-001, CT-PLT-003
    #[test]
    fn resolved_desktop_trees_have_no_network_crates() {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let mut problems = Vec::new();
        for target in DESKTOP_TARGETS {
            let output = Command::new(&cargo)
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .args(["tree", "--locked", "-p", "clewtrace", "--edges", "normal"])
                .args(["--target", target, "--prefix", "none", "--format", "{p}"])
                .output()
                .expect("cargo runs");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "cargo tree for {target}: {stderr}");
            let tree = String::from_utf8_lossy(&output.stdout);
            assert!(
                tree.contains("tauri v"),
                "{target}: tree lacks tauri:\n{tree}"
            );
            problems.extend(tree_problems(target, &tree));
        }
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    // @verifies CT-SEC-001, CT-PLT-003
    #[test]
    fn tree_check_rejects_network_crates_and_plugins() {
        let tree = "clewtrace v0.1.0 (/src)\ntauri v2.12.4\nreqwest v0.12.0\n\
                    tauri-plugin-shell v2.0.0\nhyper v1.0.0 (*)\nserde v1.0.0\n";
        assert_eq!(
            tree_problems("t", tree),
            [
                "t: `hyper` is in the resolved dependency tree",
                "t: `reqwest` is in the resolved dependency tree",
                "t: `tauri-plugin-shell` is in the resolved dependency tree",
            ]
        );
    }

    // @verifies CT-SEC-001, CT-PLT-003
    #[test]
    fn capability_check_rejects_network_grants() {
        let file: CapabilityFile = r#"{
            "identifier": "net",
            "remote": { "urls": ["https://example.com"] },
            "permissions": ["core:app:allow-name", "http:default", "core:webview:allow-create-webview"]
        }"#
        .parse()
        .expect("sample capability parses");
        let CapabilityFile::Capability(c) = file else {
            panic!("sample is one capability");
        };
        let problems = capability_problems(&c);
        assert_eq!(problems.len(), 3, "{problems:?}");
    }

    // @verifies CT-SEC-001, CT-PLT-003
    #[test]
    fn manifest_check_holds_the_core_crates_to_their_pins() {
        let manifest = r#"
[dependencies]
rusqlite = { version = "0.40", default-features = false, features = ["bundled", "url"] }
sha2 = "0.10"
tree-sitter = { version = "0.27", default-features = false }
tree-sitter-rust = "0.24"
rfd = { version = "0.17", default-features = false, features = ["gtk3", "xdg-portal"] }
"#;
        assert_eq!(
            manifest_problems(manifest),
            [
                "line 3: `rusqlite` enables feature `url`",
                "line 4: `sha2` keeps default features",
                "line 7: `rfd` enables feature `xdg-portal`",
            ]
        );
    }

    // @verifies CT-SEC-001, CT-PLT-003
    #[test]
    fn manifest_check_rejects_network_features_and_crates() {
        let manifest = r#"
[dependencies]
tauri = { version = "2", default-features = false, features = ["wry", "rustls-tls"] }
tauri-build = { version = "2", features = ["config-json"] }
http-client = { version = "1", package = "reqwest" }
tauri-plugin-http = "2"
serde = { version = "1",
  features = ["derive"] }

[target.'cfg(windows)'.dependencies]
ureq = "3"
"#;
        let problems = manifest_problems(manifest);
        let joined = problems.join("\n");
        for want in [
            "`tauri` enables feature `rustls-tls`",
            "`tauri-build` keeps default features",
            "`reqwest` reaches the network",
            "`tauri-plugin-http` reaches the network",
            "put `serde` on one line",
            "`ureq` reaches the network",
        ] {
            assert!(joined.contains(want), "missing `{want}` in:\n{joined}");
        }
        assert!(manifest_problems("[dependencies]\ntauri = { version = \"2\", default-features = false, features = [\"wry\"] }\n").is_empty());
    }

    // @verifies CT-EFF-002, CT-EFF-010
    #[test]
    fn linux_web_view_settings_turn_off_the_dmabuf_renderer_and_keep_a_users_choice() {
        let mut set = Vec::new();
        apply_web_view_settings(&|_| false, &mut |k, v| {
            set.push((k.to_string(), v.to_string()))
        });
        assert_eq!(
            set,
            [(
                "WEBKIT_DISABLE_DMABUF_RENDERER".to_string(),
                "1".to_string()
            )]
        );
        let mut set = Vec::new();
        apply_web_view_settings(&|_| true, &mut |k, v| {
            set.push((k.to_string(), v.to_string()))
        });
        assert!(set.is_empty(), "a variable the user set was overwritten");
    }
}
