<!-- @verifies CT-SEC-001, CT-PLT-003 -->
# No-network check

Verification method: Test, backed by this written check. Repeat before each release and
whenever a capability, a Tauri feature, or a dependency changes. Record the result below.

## Automated tests

`cargo test -p clewtrace` runs the `no_network` module in `app/src-tauri/src/main.rs`:

- `capabilities_grant_no_network_access`: every capability in `app/src-tauri/capabilities/`
  and any inlined in `tauri.conf.json` is local, has no `remote` URLs, and grants only
  permissions on the `OFFLINE_PERMISSIONS` allowlist.
- `windows_load_only_bundled_pages`: no window loads an external URL.
- `cargo_features_enable_no_network_access`: reads the app's `Cargo.toml` and
  `crates/clew-core/Cargo.toml`, which the app links. `tauri`, `tauri-build`, `rusqlite`,
  `tree-sitter`, `sha2`, and `rfd` keep default features off and enable only allowlisted
  features (`wry`, `config-json`, `bundled`, `gtk3`). No dependency is a known network crate (HTTP clients,
  TLS stacks, sockets) or a `tauri-plugin-*` crate.
- `resolved_desktop_trees_have_no_network_crates`: `cargo tree -p clewtrace -e normal` for
  Windows, macOS, and Linux on x64 and ARM64 lists no crate from `NETWORK_CRATES` and no
  `tauri-plugin-*` crate, so an indirect or target-specific network dependency fails too.
- `capability_check_rejects_network_grants`,
  `manifest_check_rejects_network_features_and_crates`,
  `manifest_check_holds_the_core_crates_to_their_pins`, and
  `tree_check_rejects_network_crates_and_plugins` prove the checks fail on bad input.

## Written check

- [x] `app/src-tauri/capabilities/` grants only `core:app:allow-name`,
      `core:app:allow-version`, and the `allow-` permissions of the app's own commands
      (`project_status`, `wait_for_project`, `tree_children`, `element_detail`,
      `element_links`, `problems`, `search`; task 2.3a, ADR-0004; and `pick_folder`,
      `open_project`, `recent_projects`, `clear_recent`; task 2.3b). None reaches the
      network: the app commands read the open project and its local SQLite index, show
      the OS folder dialog, and read or write the recent-projects file only.
- [x] `tauri.conf.json` configures no plugins and no `remote` capability.
- [x] The CSP allows only `'self'` and IPC (`connect-src ipc: http://ipc.localhost`).
      `http://ipc.localhost` is the Tauri IPC channel on Windows, not a network host.
- [x] `cargo tree -p clewtrace -e normal` for Windows, macOS, and Linux targets lists no
      `reqwest`, `hyper`, or TLS crate. Tauri pulls `reqwest` only on Android and iOS.
      Automated since task 0.9 by `resolved_desktop_trees_have_no_network_crates`.
- [x] The Windows installer does not download WebView2 (`webviewInstallMode` is `skip` in
      `tauri.release.conf.json`).
- [x] Every permission added to `OFFLINE_PERMISSIONS` since the last record was reviewed
      against the Tauri docs and grants no network access.
- [ ] Before release: run the installed app with the network disconnected and confirm it
      starts and works (CT-PLT-003), and watch it with a firewall log or `lsof -i` /
      `netstat` to confirm it opens no connection (CT-SEC-001).

## Record

| Date | Commit | Result | Checked by |
| --- | --- | --- | --- |
| 2026-10-21 | task 0.8 | Pass, automated and written checks; release run pending | Claude Code |
| 2026-10-21 | task 0.9 | Pass, resolved-tree check added for six desktop targets | Claude Code |
| 2026-10-31 | task 2.3a | Pass, seven read-only app command permissions reviewed; clew-core manifest checked | Claude Code |
| 2026-10-31 | task 2.3b | Pass, four folder and recent-list permissions reviewed; rfd 0.17.2 (gtk3 only) added, no network crate in the six desktop trees | Claude Code |
