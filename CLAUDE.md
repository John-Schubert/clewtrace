# Clewtrace: instructions for Claude Code

Clewtrace is a local-first, installable SysML v2 and requirements traceability tool for
Windows 10+, macOS 13+, and mainstream Linux. It must run well on a 4 GB RAM laptop, work
fully offline, and help defense contractors trace NIST SP 800-171 (CMMC) controls.
The full product spec lives in the "Clewtrace Product Specification" doc; the requirements
in `model/requirements.sysml` are the authority once code exists.

## Core tenets (in tie-break order)

1. **Traceability.** Every line of first-party code traces to a requirement. No exceptions.
2. **Efficiency.** Small installer, low memory, fast start, no idle CPU. Budgets are in the
   CT-EFF requirements and are enforced by tests.
3. **Usability.** A requirements engineer is productive without learning SysML syntax.

## Golden rules

- **Never write untraced code.** Tag every function, type, impl block, or UI component with
  `@implements CT-XXX-NNN` in the comment directly above it. Tag tests with `@verifies`.
  Module-level code that is not an item goes in `@begin` / `@end` regions. Config and CI files
  get one `@file` tag in their first comment. Grammar: `docs/TRACEABILITY.md`.
- **Only cite real IDs.** IDs must exist in `model/requirements.sysml`. Never invent one.
- **Tag honestly.** A tag means "this code exists to satisfy that requirement." Never add a
  tag just to make the checker pass. If code serves no requirement, it should not exist.
- **New behavior needs a requirement first.** If a task needs behavior no requirement covers,
  stop and propose a new requirement (ID, statement, verification method, phase) to John.
  Add it to `model/requirements.sysml` only after he approves.
- **Every commit carries a trailer:** `Refs: CT-XXX-NNN[, CT-YYY-NNN]`. Use `.gitmessage`.
- **Commit as John** (CT-DEV-013). Before the first commit of a session, run
  `git config user.name "John Schubert"` and `git config user.email "johnschubert@yahoo.com"`.
  Do not add a `Co-Authored-By` trailer.
- **Tests first.** Write the `@verifies` test, watch it fail, then implement.
- **No new dependency** without an entry in `deps.toml` (license, why, requirement served).
  A dependency that changes the architecture also needs an ADR in `docs/adr/`.
- **No network code, no telemetry** (CT-SEC-001). Do not add HTTP clients, analytics, crash
  reporters, activation servers, or update checkers unless a task names the requirement that
  allows it. Licensing works offline from a signed file (CT-PLT-005).
- **Network code is opt-in** (CT-SEC-002). Any future network code goes behind a Cargo
  feature that is off by default, so a build without it has no network code. Every current
  build is the offline edition.
- **No cryptography of our own** (CT-SEC-006). Hash with `sha2`; verify license signatures
  with `ed25519-dalek`. Never hand-roll crypto.
- **Never lock users out of their data** (CT-PLT-007). An expired license disables editing
  only; viewing and export always work.
- **Do not merge outside pull requests** (CT-DEV-013).

## Efficiency rules for code

- Event-driven, never polling. Use OS file events for watching.
- Stream large files; do not load whole projects into the web view.
- The Rust core owns data. The UI asks for pages of rows, not whole tables.
- Virtualize every list or table that can exceed 200 rows.
- Prefer the standard library. Each dependency must earn its bytes.
- When a change could affect a CT-EFF budget, run the perf tests before committing.

## Stack and layout

- Rust core library `crates/clew-core` (parse, scan, index, trace, io, audit, license modules).
- CLI `crates/clew-cli` producing the `clew` binary. CI runs `clew check --self`.
- Desktop app `app/`: Tauri 2 (`app/src-tauri`) with a Svelte 5 + TypeScript UI (`app/src`)
  and the CodeMirror 6 editor. SQLite (rusqlite, bundled) with FTS5 for the index.
- **Svelte 5 runes syntax only** (`$state`, `$derived`, `$effect`, `$props`). Never use
  Svelte 4 patterns such as `export let`, `$:` reactive statements, or `createEventDispatcher`.
- Keep component logic in `.ts` modules and components thin, so most UI code is scanned
  as plain TypeScript with item-level tags.
- Source of truth: `.sysml` text files. The index in `.clewtrace/` is a rebuildable cache.
- Decisions: `docs/adr/`. Plan and task list: `docs/PLAN.md`.

## Commands

- Build and test core: `cargo build`, `cargo test`
- Perf tests (10,000 requirements, 50,000 links):
  `cargo test --release -p clew-core --test perf -- --ignored --nocapture`
- Trace gate: `cargo run -p clew-cli -- check --self` (alias `clew check --self` once installed)
- Commit message check: `cargo run -p clew-cli -- check-commit .git/COMMIT_EDITMSG`
- App dev: `cd app && npm install && npm run tauri dev`
- Lint: `cargo clippy -- -D warnings`, `cargo fmt --check`, `npm run check` in `app/`
- Rust builds can take several minutes. Run long builds with a 10-minute timeout.

## How to work (keeps usage low on the Pro plan)

- Work happens in Claude Code cloud sessions at claude.ai/code against the GitHub repo.
- One task from `docs/PLAN.md` per session. Start a new session for each task.
- Read only the files the task needs. Look up a requirement with
  `grep -A3 "CT-XXX-NNN" model/requirements.sysml` instead of reading the whole file.
- Keep plans short: five bullets or fewer, then do the work.
- Stop and ask John before: architecture changes, new dependencies, new requirements,
  deleting tests, or anything that weakens a CI gate.
- Before ending a session: tests pass, `clew check --self` passes, work committed and pushed,
  and the task ticked in `docs/PLAN.md` with any follow-ups noted under it.

## Writing style for docs and messages

Plain, short sentences. Use commas or hyphens, never em dashes.
