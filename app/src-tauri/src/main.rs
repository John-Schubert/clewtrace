//! The Clewtrace desktop app. For now it opens one window that shows the app name and
//! version. The UI is embedded in the binary, and the only commands it may call are the
//! ones granted in capabilities/main.json.

// @begin CT-PLT-001
// Release builds on Windows open no console window next to the app window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// @end

/// The context the app is built with: tauri.conf.json and the capability files.
/// Generated once, because a second `generate_context!` in the same build defines the
/// embedded Info.plist symbol twice on macOS.
// @implements CT-PLT-001, CT-PLT-003
fn context() -> tauri::Context<tauri::Wry> {
    tauri::generate_context!()
}

// @implements CT-PLT-001, CT-PLT-003
fn main() {
    tauri::Builder::default()
        .run(context())
        .expect("Clewtrace could not start its window");
}

// @verifies CT-SEC-001, CT-PLT-003
#[cfg(test)]
mod tests {
    use super::context;

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
    use super::context;
    use std::path::Path;
    use tauri::utils::acl::capability::{Capability, CapabilityFile};
    use tauri::utils::config::{CapabilityEntry, WebviewUrl};

    /// Permissions reviewed as granting no network access. Adding one here needs the same
    /// review, recorded in docs/verification/no-network.md.
    const OFFLINE_PERMISSIONS: [&str; 2] = ["core:app:allow-name", "core:app:allow-version"];

    /// Features each Tauri crate may enable. None of them pulls in a TLS stack, an HTTP
    /// client, or a plugin.
    const ALLOWED_FEATURES: [(&str, &[&str]); 2] =
        [("tauri", &["wry"]), ("tauri-build", &["config-json"])];

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
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let manifest = std::fs::read_to_string(&path).expect("Cargo.toml is readable");
        let problems = manifest_problems(&manifest);
        assert!(problems.is_empty(), "{}", problems.join("\n"));
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
}
