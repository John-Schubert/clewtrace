# ADR-0004: App data layer, commands, and paging

- Status: Proposed (task 2.3a, for John to accept)
- Date: 2026-10-31
- Serves: CT-EFF-004, CT-EFF-005, CT-MOD-001, CT-USE-007, CT-SEC-001, CT-EFF-001

## Context

The desktop app needs the open project in the UI: a model tree, element detail, the links
to and from an element, the problems list, and search. A project can hold 10,000
requirements and 50,000 links (CT-EFF-004). Every view must respond within 100 ms
(CT-EFF-005). Sending whole tables to the web view would cost memory twice and make the
UI slow on a 4 GB laptop. CLAUDE.md says the Rust core owns the data and the UI asks for
pages of rows.

## Decision

- **The Rust side owns the open project.** `app/src-tauri/src/data.rs` holds the
  `Project` from `clew-core` (model and trace graph), its coverage, and the SQLite search
  index. The UI keeps no copy of project data.
- **Load on a background thread.** `AppState::open` starts a `clew-load` thread and returns
  at once. The window opens while the project loads. A later open replaces an earlier one
  that is still running.
- **No polling.** The UI calls `wait_for_project` once. It blocks on a condition variable
  on Tauri's thread pool and answers when the load ends. `project_status` answers at once.
- **One command per kind of page.** `tree_children`, `element_detail`, `element_links`,
  `problems`, and `search`, in `app/src-tauri/src/commands.rs`. Each is a thin wrapper
  over `data.rs`, which has no Tauri types, so tests call it directly. Every command runs
  on Tauri's thread pool, never on the main thread.
- **Pages.** A page is `{offset, total, more, rows}` with at most `MAX_PAGE` (200) rows,
  the size above which CLAUDE.md asks the UI to virtualize a list. A larger limit is
  clamped. Search does not count its hits, since that costs a second query, so its
  `total` is null and `more` says whether another page follows.
- **Element IDs** are indexes into the loaded model. They stay valid until the project is
  opened again, which is enough until live reload (2.10) needs stable IDs.
- **Replies.** Commands reply with JSON text built with `clew_core::json::quote`, so the
  app needs no serialization crate. Data commands reply `{"ok": value}` or
  `{"error": {"what", "where", "help"}}` (CT-USE-007). `app/src/lib/api.ts` is the typed
  client; it throws a `ClewError` for an error reply.
- **Access control.** `build.rs` lists the commands in a tauri-build app manifest, so each
  is denied unless a capability grants its `allow-` permission. `capabilities/main.json`
  grants the seven to the main window. They only read the open project, so they are on the
  no-network test's offline list (CT-SEC-001).
- **Opening.** The first command-line argument that is not an option is the project
  folder. `Project::open` finds the project root from it. The folder dialog and recent
  projects come in 2.3b.
- **No-network check.** The manifest check reads `crates/clew-core/Cargo.toml` too, and
  holds `rusqlite` (`bundled` only), `tree-sitter`, and `sha2` to default features off.

## Alternatives rejected

- **serde and serde_json as direct dependencies.** Tauri already builds them, but the
  replies are flat and `clew-core` already writes JSON. A direct dependency would need a
  `deps.toml` entry and earns little.
- **Tauri events for load progress.** Listening needs the `core:event` permissions in the
  capability file. One blocking `wait_for_project` call does the same with no new grant.
- **Sending the whole model to the UI.** Simple, but it doubles memory and breaks
  CT-EFF-004 and CT-EFF-005 at full size.

## Consequences

- Linking `clew-core` adds SQLite and the tree-sitter grammars to the app binary. The
  `installer-size` CI job checks the deb stays at 20 MB or less (CT-EFF-001).
- A new command needs four changes: `commands.rs`, `generate_handler!` in `main.rs`,
  `COMMANDS` in `build.rs`, and its `allow-` permission in `capabilities/main.json` and in
  `OFFLINE_PERMISSIONS` after review.
- Element IDs are not stable across reloads. 2.10 revisits this.
