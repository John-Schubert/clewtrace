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
- `cargo_features_enable_no_network_access`: `tauri` and `tauri-build` keep default features
  off and enable only allowlisted features (`wry`, `config-json`). No dependency is a known
  network crate (HTTP clients, TLS stacks, sockets) or a `tauri-plugin-*` crate.
- `capability_check_rejects_network_grants` and
  `manifest_check_rejects_network_features_and_crates` prove both checks fail on bad input.

## Written check

- [x] `app/src-tauri/capabilities/` grants only `core:app:allow-name` and
      `core:app:allow-version`. Neither reaches the network.
- [x] `tauri.conf.json` configures no plugins and no `remote` capability.
- [x] The CSP allows only `'self'` and IPC (`connect-src ipc: http://ipc.localhost`).
      `http://ipc.localhost` is the Tauri IPC channel on Windows, not a network host.
- [x] `cargo tree -p clewtrace -e normal` for Windows, macOS, and Linux targets lists no
      `reqwest`, `hyper`, or TLS crate. Tauri pulls `reqwest` only on Android and iOS.
- [x] Every permission added to `OFFLINE_PERMISSIONS` since the last record was reviewed
      against the Tauri docs and grants no network access.
- [ ] Before release: run the installed app with the network disconnected and confirm it
      starts and works (CT-PLT-003), and watch it with a firewall log or `lsof -i` /
      `netstat` to confirm it opens no connection (CT-SEC-001).

## Record

| Date | Commit | Result | Checked by |
| --- | --- | --- | --- |
| 2026-10-21 | task 0.8 | Pass, automated and written checks; release run pending | Claude Code |
