# Build plan

Each task is sized for one Claude Code cloud session on the Pro plan. Do them in order. Tick a
task only when its tests pass, `clew check --self` passes, and the work is committed and pushed
with a `Refs:` trailer. Note follow-ups under the task.

## Phase 0: Foundation

Exit gate: an empty app installs on Windows, macOS, and Linux from CI-built installers, and CI
blocks a commit that adds an untraced line or lacks a `Refs:` trailer.

- [x] **0.1 Repository skeleton.** Confirm `LICENSE`, `LICENSING.md`, and `CONTRIBUTING.md` are
      present. Add `.gitignore`, `rust-toolchain.toml`, and a Cargo workspace with empty
      `crates/clew-core` and `crates/clew-cli`. `clew --version` prints the version.
      Refs: CT-DEV-009, CT-DEV-013, CT-TRC-008
      Follow-ups:
      - `clew check --self` does not exist yet (task 0.4), so this task was not gate-checked.
      - `--version` parses arguments with the standard library. Revisit when `clap` is
        approved and added for later subcommands. Done in 0.2: John approved `clap` on
        2026-10-21 and `--version` now uses it.
- [x] **0.2 Requirement ID reader (v0).** In `clew-core`, read `model/requirements.sysml` and
      extract each requirement's short name, declared name, `doc` text, `verificationMethod`,
      and `phase`. This is a deliberately small reader for the shapes used in that file; the
      real parser replaces it in task 1.2. Add `clew reqs list` and `clew reqs show <ID>`.
      Test against the real file: 74 requirements, no duplicates.
      Refs: CT-DEV-001, CT-TRC-001
      Follow-ups:
      - `clew check --self` does not exist yet (task 0.4), so this task was not gate-checked.
      - `clew reqs` reads `model/requirements.sysml` relative to the current directory. A
        project-root lookup belongs with project opening in a later task.
      - `deps.toml` lists clap's transitive crates too, since its header says CI checks every
        crate in `Cargo.lock`. The task 0.4 checker should match that.
- [x] **0.3 Tag scanner (v0).** Use tree-sitter (Rust and TypeScript grammars) to find item
      spans, read `@implements`, `@verifies`, `@begin`/`@end`, and `@file` tags, and compute
      which lines are traced. Svelte files use `@begin`/`@end` regions in v0. Build fixtures
      covering every rule in `docs/TRACEABILITY.md`, including failure cases.
      Refs: CT-DEV-002, CT-TRC-006
      Follow-ups:
      - `clew check --self` does not exist yet (task 0.4), so this task was not gate-checked.
        A test in `scan.rs` scans every `.rs` file under `crates/` and requires no untraced
        lines, which caught `lib.rs` (it now uses regions).
      - John approved tree-sitter, tree-sitter-rust, and tree-sitter-typescript on 2026-10-21.
        tree-sitter builds with default features off.
      - The scanner checks ID shape only loosely. Task 0.4 checks IDs against `id_pattern`
        and the model, and applies `max_scope_lines` and `[[exclude]]` from `trace.toml`.
      - The Svelte example in `docs/TRACEABILITY.md` leaves its `<script>` and `</script>`
        lines untraced. Decide whether to exempt those lines or fix the example.
      - `trace.toml` allows `*.json` at file level, but JSON has no comments, so the scanner
        has no `@file` reader for it. Decide how JSON files are traced, or excluded.
      - Rust items also include type aliases, `macro_rules!`, `extern` blocks, and trait
        method signatures. TypeScript `@verifies` also takes a call statement such as
        `describe(...)` as a test block. Add these to the tag table in `docs/TRACEABILITY.md`.
- [x] **0.4 `clew check --self`.** Combine 0.2 and 0.3 with `trace.toml`: report untraced lines,
      unknown IDs, and oversized scopes with file:line output and a nonzero exit code on error.
      Refs: CT-DEV-002, CT-DEV-004, CT-TRC-008, CT-USE-007
      Note: `deps.toml` lists only direct dependencies (John, 2026-10-21). Indirect crates are
      covered by CI license checks and the release SBOM. Update the `deps.toml` header to say
      so and drop clap's helper entries.
      Follow-ups:
      - Done with John's 0.3 decisions: bare Svelte `<script>`/`<style>` wrapper lines are
        exempt, `*.json` is out of `file_level_allowed` (JSON files need `[[exclude]]`), and
        the extra Rust and TypeScript item kinds are in the tag table.
      - `trace.toml` and `deps.toml` are read by a small standard-library reader for the TOML
        subset they use (one-line arrays of strings, basic strings, integers). No TOML crate
        is planned in `deps.toml`, so none was added. Swap in a crate if the files outgrow it.
      - `id_pattern` supports literals, `(a|b)`, `[0-9]` classes, and `{n}` only, for the same
        reason: no regex crate is planned.
      - Also checks IDs in `deps.toml` `serves` lists and in `docs/adr/*.md`. Commit trailers
        are left to 0.5 and requirement coverage and waivers to 1.9.
      - Two existing scopes in `scan.rs` warn as over 200 lines: `impl Scanner` (346) and its
        `mod tests` (464). Warnings do not fail the gate. Split `impl Scanner` in a later
        touch of that file; a long test module may need a rule decision.
      - `scan.rs` still has its own test that every `.rs` file under `crates/` is traced. It
        overlaps `clew check --self` now; removing it needs John's approval.
      - Symlinks under source roots are not followed or checked.
- [x] **0.5 `clew check-commit`.** Validate the `Refs:` trailer and its IDs. Add
      `scripts/install-hooks.sh` and `scripts/install-hooks.ps1` that install it as a
      `commit-msg` hook.
      Refs: CT-DEV-005
      - `clew check-commit <file>` takes one message file and exits 0, 1, or 2 like
        `clew check --self`. CI in 0.6 can loop over PR commits with
        `git log --format=%B -n1 <sha> > msg.txt`.
      - `Refs:` must be in the last paragraph and not the summary line. `#` lines and
        text below git's scissors line are ignored. The key is case-sensitive.
      - The hook uses `$CLEW`, then `clew` on PATH, then `cargo run`, and blocks the commit
        when none is found. The `.ps1` installer was not run here (no PowerShell in the
        cloud session). Check it once on Windows. Done in 0.6: the Windows CI job runs
        `scripts\install-hooks.ps1` and checks that the hook blocks a commit without a
        `Refs:` trailer and allows one with it.
- [x] **0.6 CI workflow.** GitHub Actions on Ubuntu, Windows, and macOS: fmt, clippy, tests,
      `clew check --self`, `clew check-commit` on each pull request commit, and cargo-deny for
      licenses and advisories. Every dependency so far is listed in `deps.toml`, and CI fails
      when one is missing.
      Refs: CT-DEV-004, CT-DEV-008
      Follow-ups:
      - `.github/workflows/ci.yml` runs fmt, clippy, tests, `clew check --self`, and the commit
        hook test on all three OSes. `clew check-commit` on each PR commit and cargo-deny run
        on Ubuntu. Not yet run on GitHub when this was written; check the first PR run.
      - `clew check --self` now fails when a direct dependency in any `Cargo.toml` has no
        `cargo` entry in `deps.toml`. Path and `workspace = true` dependencies are skipped.
        package.json is not read yet; add that in 0.7 when `app/` gets one.
      - John's 2026-10-21 decisions applied: `max_scope_lines` skips scopes with only
        `@verifies` tags (rule 4 updated), and the `scan.rs` test that duplicated the gate is
        removed. `impl Scanner` in `scan.rs` still warns at 346 lines.
      - cargo-deny 0.20.2 is installed from its release binary, pinned by SHA-256.
        actions/checkout is pinned by commit. Both are in `deps.toml` as ecosystem `ci`.
      - `deny.toml` allows MIT, Apache-2.0, Unicode-3.0, and Zlib (foldhash, a build-time
        crate under tree-sitter). John to confirm the allowlist. Add BSD-3-Clause with
        ed25519-dalek.
      - No build cache in CI, to avoid another action. Add one if CI time becomes a problem.
- [ ] **0.7 Empty desktop app.** Tauri 2 with Svelte 5 (runes syntax) + TypeScript showing a
      window with the app name and version. Lock down Tauri capabilities: no HTTP, shell, or
      updater plugins, and a strict content security policy. Every source file tagged. The
      cloud environment's setup script installs the Tauri Linux packages (see README).
      Refs: CT-PLT-001, CT-PLT-003, CT-SEC-001
- [ ] **0.8 No-network test.** A test that fails if the Tauri capability files or Cargo
      features enable any network access, plus a written check in
      `docs/verification/no-network.md`.
      Refs: CT-SEC-001, CT-PLT-003
- [ ] **0.9 Release workflow.** On a signed tag, CI builds MSI, DMG, AppImage, deb, and rpm,
      fails if any installer exceeds 20 MB, and attaches SHA-256 checksums.
      Refs: CT-PLT-001, CT-EFF-001, CT-DEV-012
- [ ] **0.10 Decisions recorded.** Review ADR-0001, ADR-0002, and ADR-0003 against what was
      built, and update them. Fill `deps.toml` completely.
      Refs: CT-DEV-008, CT-DEV-010, CT-CMP-008
- [ ] **Gate 0 check.** John installs the release build on his Windows 10 laptop (4 GB) and
      confirms it runs; Mac and Linux installers are confirmed by CI smoke tests. Confirm CI
      rejects an untraced line and a commit without a trailer. Then set `current_phase = 1`
      in `trace.toml`.

## Phase 1: Trace engine and CLI

Exit gate: Clewtrace traces itself with 100% line coverage, and every phase 0 and phase 1
requirement is implemented and verified or formally waived.

- [ ] **1.1 Lexer.** SysML v2 tokens for the CT-MOD-002 subset, with exact line and column.
      Refs: CT-MOD-002, CT-USE-007
- [ ] **1.2 Parser.** Recursive descent with error recovery for packages, imports, comments
      and doc, part, attribute, port, and requirement definitions and usages, short names,
      satisfy, and verify. Replace the v0 reader from task 0.2. Fixture tests per construct.
      Refs: CT-MOD-001, CT-MOD-002, CT-USE-007
- [ ] **1.3 Model and name resolution.** Resolve qualified names and report unresolved
      references with file, line, and column.
      Refs: CT-MOD-002, CT-USE-007
- [ ] **1.4 Index.** SQLite (bundled) with FTS5 in `.clewtrace/index.db`; rebuild from text,
      and re-index only changed files.
      Refs: CT-MOD-001, CT-EFF-007, CT-SEC-003
- [ ] **1.5 Trace graph.** Link types derives, refines, satisfies, verifies, implements, and
      evidences, from both the model and code tags.
      Refs: CT-TRC-002, CT-TRC-006
- [ ] **1.6 Coverage and impact.** `clew report coverage` and `clew impact <ID>`.
      Refs: CT-TRC-004, CT-TRC-005, CT-DEV-003
- [ ] **1.7 Reproducible reports.** Every report records the Git commit, tool version, and a
      SHA-256 of its inputs; `clew report verify` regenerates and compares.
      Refs: CT-TRC-009, CT-SEC-006
- [ ] **1.8 Performance harness.** A generator for a 10,000-requirement, 50,000-link test
      project and a CI job running in a container limited to 4 GB RAM and 2 CPUs that checks
      the budgets due so far.
      Refs: CT-EFF-009, CT-EFF-004, CT-EFF-007
- [ ] **1.9 Requirement coverage gate.** Turn on the phase-aware requirement coverage check
      in CI and add waivers only with John's approval.
      Refs: CT-DEV-003, CT-DEV-004
- [ ] **Gate 1 check.** `clew check --self` shows 100% line coverage and no uncovered phase 0
      or phase 1 requirements. Set `current_phase = 2`.

## Later phases

Phase 2 (desktop MVP, including offline license files, trial terms, and read-only expiry per
CT-PLT-005 to CT-PLT-007), phase 3 (SysML v2 editor), phase 4 (CMMC pack), and phase 5
(diagrams, v1 import, API, AI assist) are broken into tasks when the previous gate passes,
using the phase column in `model/requirements.sysml`.
