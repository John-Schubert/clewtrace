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
