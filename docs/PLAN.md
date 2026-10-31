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
- [x] **0.7 Empty desktop app.** Tauri 2 with Svelte 5 (runes syntax) + TypeScript showing a
      window with the app name and version. Lock down Tauri capabilities: no HTTP, shell, or
      updater plugins, and a strict content security policy. Every source file tagged. The
      cloud environment's setup script installs the Tauri Linux packages (see README).
      Refs: CT-PLT-001, CT-PLT-003, CT-SEC-001
      Follow-ups:
      - `app/src-tauri` is a workspace member, but `default-members` leaves it out, so plain
        `cargo build` and `cargo test` stay light. CI builds the UI, then runs clippy, tests,
        and `cargo build -p clewtrace` on all three OSes. `npm run tauri dev` runs the app.
      - Tauri features: `wry` only (no compression, dynamic ACL, X11, or D-Bus). Revisit
        `compression` in 0.9 against the CT-EFF-001 installer budget.
      - Capability `main` grants only `core:app:allow-name` and `core:app:allow-version`.
        No plugins. Two tests in `app/src-tauri/src/main.rs` check the CSP and that no
        plugin is configured; 0.8 adds the Cargo feature and capability checks.
      - Vite 7 and vite-plugin-svelte 6, not Vite 8: Vite 8 depends on lightningcss (MPL-2.0).
        TypeScript 6, since svelte-check does not support 7 yet.
      - actions/cache is the CI build cache. Swatinem/rust-cache is LGPL-3.0, so it was not used.
      - `clew check --self` now also reads `package.json` (npm deps must be in `deps.toml`).
        The trace root is `app/src-tauri` instead of `app/src-tauri/src`, so `build.rs` and its
        `Cargo.toml` are checked too. Files in `app/` itself (`index.html`, `vite.config.ts`,
        `package.json`, `tsconfig.json`) are tagged but outside any source root; HTML has no
        scanner yet.
      - cargo-deny fails on the Tauri tree. Needs John (see the PR): MPL-2.0 crates and the
        unmaintained proc-macro-error advisory. `Apache-2.0 WITH LLVM-exception` was added.
      - Bundle identifier was `com.clewtrace.desktop`. John set it to
        `io.github.john-schubert.clewtrace` (2026-10-21), changed in 0.8.
      - No npm license check runs in CI yet. Today the npm tree is MIT, ISC, Apache-2.0, and
        BSD-3-Clause only.
- [x] **0.8 No-network test.** A test that fails if the Tauri capability files or Cargo
      features enable any network access, plus a written check in
      `docs/verification/no-network.md`.
      Refs: CT-SEC-001, CT-PLT-003
      - Tests are in the `no_network` module of `app/src-tauri/src/main.rs`. Capabilities:
        every permission must be on a reviewed offline allowlist, no `remote` URLs, all local.
        Windows load only bundled pages. `Cargo.toml`: Tauri crates keep default features off
        and enable only allowlisted features, and no dependency is a known network crate or
        a `tauri-plugin-*`. Each dependency must sit on one line so the check can read it.
      - The bundle identifier is now `io.github.john-schubert.clewtrace` (John, 2026-10-21).
      - Follow-up: the check reads direct dependencies only. Tauri pulls `reqwest` on Android
        and iOS only, not on desktop. A resolved-tree check (`cargo tree` per desktop target)
        in CI would also catch a network crate added indirectly. Candidate for 0.9.
- [x] **0.9 Release workflow.** On a signed tag, CI builds MSI, DMG, AppImage, deb, and rpm,
      fails if any installer exceeds 20 MB, and attaches SHA-256 checksums. It also generates a
      third-party license notices file from the Cargo and npm dependency trees, ships it inside
      each installer, and attaches it to the release. Add an inspection checklist for it in
      `docs/verification/`. Attach the unmodified source of each shipped MPL-2.0 crate
      (`scope = "shipped"` in `deps.toml`) to the release, and list crate, version, and source
      link in the notices file. CI checks npm production dependencies against the shipped
      license tier and dev dependencies against the build-only tier (ADR-0003). The SBOM
      marks each dependency as shipped or build-only.
      Refs: CT-PLT-001, CT-EFF-001, CT-DEV-012, CT-DEV-014
      - `.github/workflows/release.yml` runs on a `v*` tag. It checks the tag is annotated,
        signed by a key in `.github/release-signers` (SSH) or `.github/release-keys.asc`
        (OpenPGP), on `main`, and matches the version. It then builds MSI (Windows), a
        universal DMG (macOS), and deb and rpm (Ubuntu 22.04), fails over 20,000,000 bytes,
        and attaches everything to a draft release with `SHA256SUMS.txt`. John publishes
        the draft after `docs/verification/release.md`. No new actions: it uses `gh`.
      - `clew notices [--out DIR]` (`crates/clew-core/src/notices.rs`) writes the notices
        file, a CycloneDX SBOM, and `mpl-sources/`. Shipped crates are the ones
        `cargo tree -e normal,no-proc-macro` lists for any of six desktop targets; license
        data comes from `cargo metadata`. `cargo metadata` alone is not enough: it merges
        build-script features into the normal graph and marks cssparser and selectors as
        shipped. npm `dev` packages are build-only, all others shipped.
      - CI runs `clew notices` on all three OSes: npm production packages against the
        shipped tier, dev packages against the build-only tier, and each `scope` in
        `deps.toml` against what really ships.
      - svelte moved from devDependencies to dependencies: its runtime is compiled into the
        web bundle, so it and its dependencies ship (all MIT or Apache-2.0).
      - 0.8 follow-up closed: `resolved_desktop_trees_have_no_network_crates` in the
        `no_network` module runs `cargo tree -e normal` for six desktop targets and fails on
        any `NETWORK_CRATES` entry or `tauri-plugin-*` crate.
      - `tauri.release.conf.json` turns bundling on, bundles the notices file, and sets
        WebView2 install mode to `skip`, so the MSI never downloads WebView2. Needs John.
      - Not built yet, needs John: AppImage (bundles GTK and WebKitGTK, which are LGPL and
        far over 20 MB). The MSI is built with WiX, which tauri-cli downloads; WiX is
        MS-RL, not on the build-only list.
      - Not covered yet: Windows and Linux ARM64 installers, code signing of the MSI and
        DMG (needs certificates), and 16 packages with no license file in their package
        (objc2 family, webview2-com, block2, dlopen2, tree-sitter-typescript, is-reference,
        locate-character): the notices file names their license and repository only.
- [x] **0.10 Decisions recorded.** Review ADR-0001, ADR-0002, and ADR-0003 against what was
      built, and update them. Fill `deps.toml` completely.
      Refs: CT-DEV-008, CT-DEV-010, CT-CMP-008
      - Applied John's 0.9 release decisions: AppImage dropped (CT-PLT-001 now says Linux
        (deb, rpm)), WebView2 install mode stays `skip` with the offline runtime step in
        `docs/INSTALL.md`, and WiX Toolset 3 (MS-RL) recorded as a build-only CI tool.
      - ADR-0001: installers, WebView2, what is built so far, Svelte scanned by regions only.
        ADR-0002: as-built checker, trailer rules, JSON excludes, checklists. ADR-0003:
        two-tier policy, per-crate exceptions table, MS-RL for WiX only, `clew notices`,
        advisory ignore, code-signing gate.
      - `deps.toml`: every entry has a `scope`, checked by `clew notices`; added the Rust
        toolchain, Node.js, `gh`, and WiX as CI tools.
      - `docs/verification/release.md` notes that the first release needs John's SSH key in
        `.github/release-signers` (the reviewer adds it).
      - Follow-up: fetch license files for the shipped packages that publish none (the objc2
        family, webview2-com, block2, dlopen2, tree-sitter-typescript, is-reference,
        locate-character) and add them to the notices file.
      - Follow-up: ADR-0002 is still "Proposed"; John to decide whether to mark it Accepted.
- [x] **Gate 0 check.** John installs the release build on his Windows 10 laptop (4 GB) and
      confirms it runs; Mac and Linux installers are confirmed by CI smoke tests. Confirm CI
      rejects an untraced line and a commit without a trailer. Then set `current_phase = 1`
      in `trace.toml`.
      Record (2026-10-21):
      - v0.1.0 was signed with John's release key, and the release workflow verified the tag and
        built the MSI (2.7 MB), universal DMG (5.2 MB), deb, and rpm (2.6 MB each), all under
        20 MB. The MSI and the notices file match `SHA256SUMS.txt`.
      - John installed the MSI on his Windows 10 laptop (4 GB) in under a minute, after
        installing the WebView2 runtime, which the laptop lacked. The app and its desktop
        shortcut opened and showed the name and version. The Clewtrace process used about
        3 MB. The WebView2 processes are counted separately.
      - CI rejected an untraced function (PR #14, `lib.rs:23 ... not inside a tagged scope`, on
        all three OSes) and a commit without a trailer (PR #15). The ruleset blocked both
        merges. Both were closed unmerged.
      Follow-ups:
      - Mac and Linux are covered by CI building the installers and running the app tests.
        Launching the installed DMG, deb, and rpm in CI is not automated yet.
      - Measure idle memory including the WebView2 processes against CT-EFF-002 (task 1.8).

## Phase 1: Trace engine and CLI

Exit gate: Clewtrace traces itself with 100% line coverage, and every phase 0 and phase 1
requirement is implemented and verified or formally waived.

- [x] **1.1 Lexer.** SysML v2 tokens for the CT-MOD-002 subset, with exact line and column.
      Refs: CT-MOD-002, CT-USE-007
      Done in `crates/clew-core/src/parse/lexer.rs`. Follow-ups: the parser (1.2) prefixes
      lexer errors with the file path; the keyword list is the SysML v2 reserved words as
      recalled, so check it against the spec's keyword table when the parser grows past the
      subset.
- [x] **1.2 Parser.** Recursive descent with error recovery for packages, imports, comments
      and doc, part, attribute, port, and requirement definitions and usages, short names,
      satisfy, and verify. Replace the v0 reader from task 0.2. Fixture tests per construct.
      Refs: CT-MOD-001, CT-MOD-002, CT-USE-007
      Done in `crates/clew-core/src/parse/ast.rs` and `parse/parser.rs`; `reqs.rs` now reads
      requirements through the parser, with the same `clew reqs` output as the v0 reader.
      Errors carry file, line, column, and help, and recovery skips to the next `;`, balanced
      `{ }`, or member keyword. Follow-ups: feature values are kept as typed literals or raw
      expression text, so a real expression parser is needed when calculations arrive;
      `assert` / `not satisfy`, `locale`, metadata, and other SysML members are reported as
      not supported yet; `subject` is parsed because requirement bodies use it.
- [x] **1.3 Model and name resolution.** Resolve qualified names and report unresolved
      references with file, line, and column.
      Refs: CT-MOD-002, CT-USE-007
      Done in `crates/clew-core/src/model.rs`. `Model::build` puts every file under one root
      namespace and resolves import targets, specializations, types, subsets, redefines,
      satisfy, verify, and comment `about` names. Lookup searches owned, inherited, and
      imported members, then each enclosing namespace, and honors public, protected, and
      private visibility and public versus private imports. `reqs.rs` now resolves names when
      a file has no syntax errors, so `clew reqs` and `clew check --self` report unresolved
      names in the requirements model. Follow-ups: only `ScalarValues` of the standard
      library is built in (as attribute defs), so other library packages report as
      unresolved; ambiguous names take the first match without a warning; filtered imports,
      aliases, and conjugation are not supported yet.
- [x] **1.4 Index.** SQLite (bundled) with FTS5 in `.clewtrace/index.db`; rebuild from text,
      and re-index only changed files.
      Refs: CT-MOD-001, CT-EFF-007, CT-SEC-003
      Done in `crates/clew-core/src/index.rs` and `clew index [--rebuild]`. Every `.sysml`
      file under the project root is indexed, skipping folders that start with `.`, folders
      with a `CACHEDIR.TAG` (such as `target`), and symbolic links. A file is re-parsed only
      when its size or modification time changed and its SHA-256 differs. Packages,
      definitions, and usages are stored with kind, names, qualified name, position, and doc
      text, and `Index::search` pages through FTS5 matches. A corrupt or outdated database is
      replaced on open. Follow-ups: rows come from each file's syntax tree alone, so links and
      resolved names wait for the trace graph (1.5); search has no CLI or UI caller yet, and
      its 200 ms budget (CT-EFF-006, phase 2) is for the harness in 1.8 to measure.
- [x] **1.5 Trace graph.** Link types derives, refines, satisfies, verifies, implements, and
      evidences, from both the model and code tags.
      Refs: CT-TRC-002, CT-TRC-006
      Done in `crates/clew-core/src/trace.rs`. `TraceGraph::build` takes a `Model` and the tag
      scans of the code. `satisfy R by x` links x to R, and `satisfy R` and `verify R` link
      their owner to R. The parser now reads `#derivation dependency a to b;` (a derives from
      b), `#refinement dependency a to b;` (a refines b), and `#evidence dependency a to b;`
      (a evidences b), with an optional `<'id'> name from` and lists on both sides; a plain
      `dependency` or other metadata is a syntax error. `@implements` and `@verifies` scopes
      link to the requirement whose short name they cite. Links are looked up by the element
      at either end. A tag citing no requirement in the model, and a top-level `satisfy` or
      `verify` with nothing to own it, are reported with a `help:` line. `clew index` also
      keeps each file's syntax errors and reports them on every run (review of PR #20).
      Follow-ups: the `#derivation`/`#refinement`/`#evidence` syntax awaits John's approval;
      targets of satisfy and verify are not checked to be requirements; a short name used by
      two requirements links to the first; links are not stored in the index yet, and the
      graph has no CLI caller until 1.6.
- [x] **1.6 Coverage and impact.** `clew report coverage` and `clew impact <ID>`.
      Refs: CT-TRC-004, CT-TRC-005, CT-DEV-003
      Done in `crates/clew-core/src/report.rs`, with `TraceGraph::reachable` in `trace.rs`.
      Both commands read every `.sysml` file and the code tags under the source roots, and
      print file problems first (exit 1). The coverage report lists requirements without a
      parent (derives from or refines a requirement, or nested in one), an implementation
      (implements or satisfies), or a verification, and definitions and usages linked to no
      requirement in either direction, counting an owner's or member's link. Impact walks
      links breadth first, upstream and downstream, and shows each link that reached an
      element. John approved the link syntax (PR #21): the `Clewtrace` library defines
      `#derivation` and `#evidence` as `metadata def`, is built in next to `ScalarValues`,
      and is published as `library/Clewtrace.sysml`; docs in `docs/MODELING.md`.
      Follow-ups: waivers and phase filtering wait for the gate in 1.9, and the report counts
      model `satisfy` and `verify` as well as code tags; checklist verification in
      `docs/verification/` is not read yet; the model is parsed from the text on each run,
      not read from the index; the `sysml_files` error path in `load_project` has no test,
      since it needs a folder that cannot be read.
- [x] **1.7 Reproducible reports.** Every report records the Git commit, tool version, and a
      SHA-256 of its inputs; `clew report verify` regenerates and compares.
      Refs: CT-TRC-009, CT-SEC-006
      Done in `crates/clew-core/src/reproduce.rs`. `clew report coverage` and `clew impact`
      start with `Report`, `Commit`, `Tool`, and `Inputs` lines and a blank line, then the
      file problems and the report. The commit comes from running `git rev-parse HEAD` and
      `git status --porcelain`, and is `none` without Git. The inputs hash is the SHA-256
      (sha2) of a `sha256sum`-style manifest of every file the report read as text.
      `clew report verify <file>` makes the report again and compares the tool, the inputs
      hash, and every line after the header; a different commit is only a note. Exits 0, 1,
      and 2 as usual. Docs in `docs/MODELING.md`.
      Follow-ups: the report records only the combined inputs hash, so verify cannot say
      which file changed; a file that is not UTF-8 is reported as a problem but its bytes
      are not hashed; the built-in libraries are fixed by the tool version, not hashed.
- [x] **1.8 Performance harness.** A generator for a 10,000-requirement, 50,000-link test
      project and a CI job, limited by `systemd-run` to 4 GB RAM with no swap and 2 CPU cores,
      that checks the budgets due so far. (Was "a container"; changed by John, 2026-10-22,
      see ADR-0001.)
      Refs: CT-EFF-009, CT-EFF-004, CT-EFF-007
      Done in `crates/clew-core/tests/perf.rs`. The generator writes 100 requirements per
      file, one in ten top level, each satisfied, verified, implemented, and tested, with the
      rest of the links as derivations. `cargo test` runs it at 205 requirements; the full
      size is an ignored test that the new `perf (4 GB, 2 CPUs)` CI job runs in release mode
      under `systemd-run` with MemoryMax=4G, no swap, and AllowedCPUs=0-1, after checking the
      limits applied, and a 5 GiB canary run under the same limits must be killed. A budget
      fails the job only when its requirement's phase has started;
      the rest print as not due. First local run (2 CPUs): full index 896 ms, incremental
      10 ms with 1 file re-parsed (CT-EFF-007), load and coverage 450 ms (CT-EFF-004, 3 s),
      search 8 ms (CT-EFF-006, 200 ms), peak 132 MB (CT-EFF-004, 400 MB).
      Also in this PR: `top_level_definitions` in trace.toml (John, PR #22), so this repo's
      coverage report lists 0 requirements without a parent, and the duplicate-ID check in
      `parse_requirements` uses a map instead of comparing every pair.
      John approved `systemd-run` instead of a container and the new CT-EFF-009 text
      (2026-10-22, ADR-0001).
      Follow-ups: CT-EFF-004 is measured on the core library only, since the app does not
      open projects until phase 2; peak memory is read only on Linux (task 2.5 adds
      Windows and macOS idle memory). `perf (4 GB, 2 CPUs)` is in the `main` ruleset now.
- [x] **1.9 Requirement coverage gate.** Turn on the phase-aware requirement coverage check
      in CI and add waivers only with John's approval.
      Refs: CT-DEV-003, CT-DEV-004
      Done in `crates/clew-core/src/check.rs` and `report.rs`. `clew check --self`, which CI
      already runs, now fails on each requirement due in a started phase (or with no phase)
      that has no implementation or no verification and no waiver, using the same coverage
      as `clew report coverage`. Inspection and Demonstration requirements are verified by
      `docs/verification/` checklists. `[[waiver]]` in `trace.toml` records the ID, reason,
      status (`approved` or `proposed`), approver, and date; a proposed waiver is a warning,
      and `clew report coverage` lists waived requirements in their own section. Each ADR
      must cite a requirement (CT-DEV-010). New checklist `docs/verification/no-cert-claim.md`
      (CT-CMP-008). Format in `docs/TRACEABILITY.md`.
      With John's decisions on PR #25: a Markdown document may `@implements` an Inspection or
      Demonstration requirement (LICENSING.md for CT-CMP-008, CONTRIBUTING.md for
      CT-DEV-013), and a workflow step whose failure fails the build may carry `@verifies`
      (the release workflow's Installer size step for CT-EFF-001 and CT-PLT-001). A waiver
      may name a `part`, which excuses no gap. Final waivers: one, approved by John on
      2026-10-22, for the ARM64 part of CT-PLT-001 until phase 2. No proposed waivers.
      ADR-0002 now says the coverage check is on (John's decision 3 on PR #25).
      Follow-ups: `check --self` reads the project twice, once for line tags and once for
      coverage; a phase set only as a default on a requirement definition is not read, so
      such requirements count as having no phase and are always due; `clew impact` does not
      show document `@implements` links, since they are not in the trace graph; the
      Scanner impl in `scan.rs` is still over max_scope_lines (a warning before this task).
- [x] **Gate 1 check.** `clew check --self` shows 100% line coverage and no uncovered phase 0
      or phase 1 requirements. Set `current_phase = 2`.
      Passed on 2026-10-22. At `current_phase = 1`, `clew check --self` reported 36 files
      checked, 0 errors (so 0 untraced lines), 0 phase 1 coverage warnings, and 2 other
      warnings (the oversized Scanner impl in `scan.rs` and the `app/src-tauri/gen/**`
      exclude that matches no file in a clean checkout). `clew report coverage` listed 75
      requirements, 75 with a parent, and 1 partly waived (the ARM64 part of CT-PLT-001); every
      requirement it lists without an implementation or verification is from phase 2 or
      later. Then `current_phase = 2`: 0 errors and 28 phase 2 coverage warnings. The
      performance budgets due in phase 2 pass. Test coverage of clew-core and clew-cli
      (cargo-llvm-cov 0.9.1, report only): 96.24% of lines.
      John's decisions for Gate 1 (2026-10-22):
      1. Phase rule: a coverage gap in a requirement from a finished phase, or with no phase,
         is an error; a gap in a current-phase requirement is a warning; a Gate N check
         needs zero such warnings before `current_phase` is raised. Performance budgets stay
         enforced from the start of their phase.
      2. "100% line coverage" means trace coverage, as `clew check --self` enforces. Test
         coverage is measured by cargo-llvm-cov in the `coverage (report only)` CI job, and
         is report-only until John sets a minimum, which then only goes up.
      3. No laptop test at Gate 1, since it does not change the desktop app. ADR-0001 now
         says the laptop check runs at each gate that changes the desktop app.
      Follow-ups: both were cleared in task 2.0: the CT-PLT-001 ARM64 waiver now ends at
      Gate 2, and ADR-0002 states the Gate 1 phase rule.

## Phase 2: Modeling MVP

Exit gate: John models in Clewtrace on his 4 GB laptop. The app opens a project, shows parse
and name errors as he types, browses the model, follows trace links to code and tests, and
autosaves without loss. Every phase 2 budget passes in CI with the app measured, not only the
core library.

Planned in `docs/planning/phase-2-3-plan.md` and reviewed in `docs/planning/phase-2-3-review.md`.
John approved the plan and the review's splits and fixes on PR #27 (2026-10-31). A task that
fails twice moves to Opus. A dependency marked "PR #27" was approved in intent there; the task
still adds its `deps.toml` entry with the settings shown, and confirms its license against the
tier in ADR-0003 before adding it. Phase 2 has 20 tasks plus the gate. Milestone A is after
2.7b, and Milestone B (John models in Clewtrace) is after 2.10b, the 16th session.

- [x] **2.0 Phase 2 housekeeping.** Applied John's decisions on PR #27.
      Requirements in `model/requirements.sysml`: CT-MOD-003 (reworded to "an edit or save"),
      CT-MOD-004, and CT-MOD-005 moved to phase 2; 15 requirements moved to phase 3
      (CT-USE-001, -006, CT-TRC-003, CT-XCH-001 to -003, CT-SEC-002, -005, -008, CT-PLT-002,
      -005 to -007, CT-DEV-006, -011); CT-TRC-007 and CT-SEC-004 moved to phase 4 and
      CT-PLT-004 to phase 5; CT-PLT-001 reworded (universal DMG, Linux deb and rpm on x64 and
      ARM64, Windows ARM64 optional); CT-USE-001 now includes trace links; CT-SEC-002 reworded
      to "any network code behind an off-by-default build option"; CT-SEC-009 and CT-USE-009
      added. Phase counts are now 16, 14, 16, 17, 9, 5 (77 requirements). The CT-PLT-001 waiver now ends at Gate 2.
      ADR-0001 has "Status after phase 1", workflow step scanning, the ARM64 decision, and the
      new CI jobs. ADR-0002 has the Gate 1 phase rule and the phase 1 additions. ADR-0003
      records the pending exceptions (ed25519-dalek and its BSD-3-Clause crates, notify,
      inotify and inotify-sys, libfuzzer-sys) and the EPL-2.0 test-input rule. CLAUDE.md has the
      rule that network code goes behind an off-by-default Cargo feature (CT-SEC-002). CI: the
      `coverage` job fails under 95% of lines for clew-core and clew-cli; a WebKitGTK apt cache
      kept under `$HOME`, keyed on the runner image and package list, with a step timeout; and
      a parallel `installer-size` job on `ubuntu-22.04` that builds the deb with
      `tauri.release.conf.json` and fails over 20 MB, so the size is recorded before 2.3a links
      clew-core into the app.
      Refs: CT-DEV-001, CT-DEV-003, CT-DEV-010, CT-PLT-001, CT-SEC-002, CT-EFF-001
      Follow-ups:
      - John to add `coverage` and `installer-size` to the "CI must pass" ruleset. The job
        `coverage (report only)` no longer exists.
      - The CI changes could not run in the cloud session. Check the first PR run of the apt
        cache, the coverage floor, and the `installer-size` job.
      - The review counted 22 phase 2 tasks. After its five phase 2 splits there are 20; the
        two other splits (3.1 and 3.7) are in phase 3.
- [x] **2.1a Project opener and CLI tests.** A shared `Project::open(path)` that finds the
      project root (nearest folder with `trace.toml` or `.clewtrace/`, else the given
      folder), used by every `clew` command and later by the app. Move logic out of
      `clew-cli/src/main.rs` (50.7% test coverage) into tested functions.
      Refs: CT-TRC-008, CT-USE-007, CT-DEV-003
      Model: Sonnet
      Done in `crates/clew-core/src/project.rs`: `find_root` and `Project::open`, and a `root`
      field on `Project`. Every `clew` command except `notices` now finds the root from the
      current folder; docs in `docs/MODELING.md`. Moved out of `main.rs` into tested core
      functions: the `reqs` output (`Requirement::list_line`, `show_text`), the report
      summary line (`Report::summary`), `notices::read_packages` (with an injected cargo
      runner) and `notices::write_outputs`. `main.rs` test coverage went from 50.7% to
      66.7% of lines; the total is 97.2%.
      Follow-ups:
      - The remaining untested `main.rs` code is `clew notices` and its cargo runner, and
        the error branches of the other commands. A `notices` test needs `npm ci` and six
        `cargo metadata` runs, so CI is its only check.
      - Test fixtures that must have no project above them moved from `target/tmp` (inside the
        repo) to the system temp folder, since the root lookup would find the repo's
        `trace.toml`.
      - `Project::open` has no caller yet but the tests; the app uses it in 2.3a.
- [x] **2.1b Model checks.** Read a phase given only as a default on a requirement
      definition. Warn on an ambiguous name. Report an import of a standard library package
      that is not built in (such as ISQ or SI) once, as a warning that its names are not
      checked, instead of one error per name. Check that satisfy and verify targets are
      requirements. Report a short name used by two requirements as an error.
      Refs: CT-MOD-002, CT-TRC-001, CT-TRC-002, CT-DEV-003, CT-USE-007
      Model: Sonnet
      Done in `model.rs` and `reqs.rs`: `ModelError` has a `warning` flag, and
      `Project::warnings` holds the warnings (they print in reports but do not set the
      problems exit code). `PhaseDefaults` reads a phase from the requirement definition,
      across files. Documented in `docs/MODELING.md`.
      Follow-ups:
      - The list of standard library package names in `model.rs` (`STANDARD_PACKAGES`) is
        a fixed list. A qualified use such as `ISQ::mass` with no import is still an error.
      - A satisfy or verify of a requirement definition is accepted, not only a usage.
      - Ambiguity is checked for owned members only, not for two imports that supply the
        same name.
- [x] **2.2 Svelte script scanning.** Scan `<script lang="ts">` blocks with the TypeScript
      grammar, offsetting line numbers, so functions in components get item-level tags.
      Markup stays covered by the component tag or regions. Read `<!-- @file -->` in HTML
      files. Add `html` to `languages` and `app/index.html` to the source roots in
      `trace.toml`. Split `impl Scanner` under max_scope_lines. Update
      `docs/TRACEABILITY.md`, the "Code scanning" row of ADR-0001, and "As built" in
      ADR-0002, and add to CLAUDE.md: keep component logic in `.ts` modules and components
      thin.
      Refs: CT-DEV-002, CT-TRC-006
      Model: Sonnet
      Done in `scan.rs`: `Syntax::Html`, and `Scanner::svelte` parses `lang="ts"` script
      blocks with the TypeScript grammar. The file is masked (other text becomes spaces,
      newlines kept), so line numbers and byte offsets need no offsetting. `impl Scanner`
      is split into five blocks, each under 200 lines.
      Follow-ups:
      - A script block without `lang="ts"` still allows regions only.
      - A `<script` inside an HTML comment or string is read as a tag.
- [x] **2.3a App data layer.** The Rust side of the app owns the open project (model, trace
      graph, coverage, index). Tauri commands return pages of rows: tree children, element
      detail, links to and from, problems, search hits. A typed client in
      `app/src/lib/api.ts`. Load on a background thread so the window stays responsive.
      Open a project from a path on the command line. Errors carry what, where, and how to
      fix. ADR-0004 records the command and paging design. Extend the no-network manifest
      check to read `crates/clew-core/Cargo.toml` too. Add `app/src-tauri` to the coverage
      job, report-only until Gate 2. Tests run the commands on the 205-requirement generated
      project.
      Refs: CT-EFF-004, CT-EFF-005, CT-MOD-001, CT-EFF-001, CT-SEC-001
      Model: Opus
      Done in `app/src-tauri/src/data.rs` (the open project, pages of at most 200 rows, JSON
      replies built with `clew_core::json::quote`, so no serde) and `commands.rs` (seven
      thin commands on Tauri's thread pool). `wait_for_project` blocks on a condition
      variable, so the UI never polls. `build.rs` lists the commands in an app manifest and
      `capabilities/main.json` grants them. The perf generator moved to
      `crates/clew-core/tests/generate/mod.rs` so the app tests share it.
      `ElementKind::label` in clew-core gives the keywords. The window shows the project
      status line until the 2.4 shell.
      Follow-ups:
      - John to accept ADR-0004 (status Proposed).
      - The CI coverage step for `app/src-tauri` could not run in the cloud session. Check
        it on the first PR run.
      - Element IDs are model indexes and change on reload. Revisit in 2.10.
      - `api.ts` has no unit tests until vitest arrives in 2.4.
- [ ] **2.3b Opening projects.** A folder dialog (rfd 0.17.2 with `default-features = false`
      and the `gtk3` feature only, MIT, PR #27; add `("rfd", &["gtk3"])` to the no-network
      test's allowed features). A recent-projects list that stores folder paths only, never
      project content, in the app config folder, with a "Clear recent" action.
      `docs/INSTALL.md` says where it lives.
      Refs: CT-SEC-003, CT-USE-007, CT-EFF-005
      Model: Sonnet
- [ ] **2.4 App shell.** Pane layout (model browser, main view, trace panel, problems). Light
      and dark themes from CSS tokens, following the OS setting, with a manual override. A
      command palette with an action registry that every later action joins. One shared
      virtual list component for any list that can pass 200 rows. Keyboard focus order and
      ARIA landmarks from the start. Adds vitest with happy-dom or jsdom for UI tests
      (build-only, MIT, PR #27). Add to CLAUDE.md: the UI never puts project content in
      localStorage, sessionStorage, or IndexedDB; it asks the Rust core each time. A test
      fails on any use of those APIs in `app/src` outside UI preferences such as the theme.
      Split into two sessions if the first attempt overruns.
      Refs: CT-USE-002, CT-USE-005, CT-USE-004, CT-EFF-005, CT-SEC-003
      Model: Sonnet
- [ ] **2.5 App performance probes.** A `perf-probe` Cargo feature, off in release builds (a
      test checks that `release.yml` passes no `--features` and that `default` does not
      include it). All timing and memory reading is on the Rust side, so no probe code is in
      the UI bundle. It reports time to an interactive window and the resident memory of the
      app and its web view processes when idle with an empty project. The perf job measures
      on Linux under the existing systemd-run limits; run `xvfb-run` inside the `systemd-run`
      scope, since `sudo` drops `DISPLAY` (xvfb is an apt CI tool, PR #27). Windows and macOS
      jobs measure idle memory from a workflow step (PowerShell `Get-Process` summing the app
      and its `msedgewebview2` children, and `ps -o rss` on macOS), so no new crate is
      needed. CI fails on CT-EFF-002 and CT-EFF-003. `docs/verification/idle-cpu.md` gives the
      CT-EFF-008 analysis: every timer, listener, and thread, and why none polls. It supports
      the review but does not count as verification, since checklists count only for
      Inspection and Demonstration. The verification is a test tagged `@verifies CT-EFF-008`.
      It fails on `setInterval`, `requestAnimationFrame` loops, `thread::sleep`, and
      `recv_timeout` in first-party code outside an allowlist in the test, where each entry
      gives its reason. Later UI tasks add their probes here. With `perf-probe` on, the app
      can run a fixed script of actions through the 2.4 action registry, log the time from
      each action to the next painted frame, and exit. No new dependency.
      Refs: CT-EFF-002, CT-EFF-003, CT-EFF-008, CT-EFF-009
      Model: Sonnet
- [ ] **2.6 Model browser and problems.** A lazy, virtualized tree of packages, definitions,
      and usages. Element detail: kind, qualified name, doc, attributes, file and line. A
      problems panel with every syntax, name, and trace problem as file:line:column with its
      fix hint; click to select. Probe: open the 10,000-requirement project in the app within
      3 s and 400 MB.
      Refs: CT-MOD-005, CT-USE-007, CT-EFF-004
      Model: Sonnet
- [ ] **2.7a Traces and impact.** Trace panel for the selected element: links to and from,
      grouped by kind, including code tags with file and line; click to follow; back and
      forward. Impact view, upstream and downstream. Probe: select and navigate within
      100 ms in the app.
      Refs: CT-TRC-005, CT-TRC-006, CT-EFF-005
      Model: Sonnet
- [ ] **2.7b Coverage, search, and source view.** Coverage view (no parent, no
      implementation, no verification, untraced elements) with filters. Search box over the
      FTS5 index with paged results. Read-only source view that opens a code file at the
      tagged line. Probes: filter within 100 ms; search within 200 ms, in the app.
      Refs: CT-TRC-004, CT-EFF-005, CT-EFF-006
      Model: Sonnet
      Milestone A: Clewtrace can open and trace its own repo, read-only. Optional
      pre-release for a laptop check.
- [ ] **2.8 Text editor.** CodeMirror 6 for `.sysml` files (@codemirror/state, view,
      commands, language, lint, autocomplete, search; MIT, bundled locally, PR #27).
      Syntax highlighting from a keyword list that a core command serves from the Rust lexer
      at run time, so the two cannot drift. Open a file at a position from the browser,
      problems, or trace panel. Explicit save through Rust with an atomic write in a new
      `clew-core/src/io.rs`: a temp file in the same folder as the target, flush, then
      rename. Never the system temp folder. Undo and redo, with tests.
      Refs: CT-MOD-004, CT-USE-003, CT-USE-008, CT-EFF-005, CT-SEC-003
      Model: Sonnet
- [ ] **2.9a Incremental validation in the core.** An API that takes one file's new text,
      re-parses only that file, then re-resolves names and rebuilds the trace graph from the
      cached syntax trees of the other files. Perf tests: an edit in a 10,000-line file is
      validated within 1 s on the 4 GB, 2 CPU job, and the 10,000-requirement project stays
      within the CT-EFF-004 memory budget. ADR-0005 records the incremental design.
      Refs: CT-MOD-003, CT-EFF-007, CT-EFF-004
      Model: Opus
- [ ] **2.9b Live validation in the app.** After a short pause in typing (a one-shot timer
      reset by each keystroke, never a poll), send the buffer to the 2.9a API. Return
      diagnostics as editor markers with help text, and refresh the browser, problems, and
      trace panel.
      Refs: CT-MOD-003, CT-MOD-005, CT-EFF-005
      Model: Sonnet
- [ ] **2.10a Autosave and concurrent use.** Autosave after a short pause and on blur and
      close, through the 2.8 atomic write. A test kills the writer mid-save and finds the
      old or the new content, never a torn file. Set the SQLite busy_timeout, and test
      `clew index` running while the app holds the index.
      Refs: CT-USE-008, CT-USE-009
      Model: Opus
- [ ] **2.10b File watching.** Watch the project folder with OS file events through notify
      8.2.1 alone, using its recommended watcher only (CC0-1.0 for notify and ISC for
      inotify and inotify-sys as per-crate exceptions, PR #27). Debounce with a one-shot
      timer that starts on the first event and ends after the burst; nothing runs while no
      files change. Never use notify's polling watcher or any notify-debouncer crate (its
      thread wakes on a timer forever, CT-EFF-008). If the OS runs out of file watches,
      report it with how to raise the limit instead of falling back to polling. On events:
      re-index changed files, reload clean buffers, and ask when a file with unsaved edits
      changed on disk (git pull, another editor). Add the file watcher to
      `docs/verification/idle-cpu.md`.
      Refs: CT-EFF-007, CT-EFF-008, CT-USE-008, CT-USE-009
      Model: Opus
      Milestone B: John models in Clewtrace. From here, edit `model/requirements.sysml` in
      the app; changes still go through pull requests.
- [ ] **2.11 Go to definition and completion.** F12 and Ctrl or Cmd+click go to the
      definition. Find references from the model and trace links. Completion for keywords,
      names in scope, and requirement short names after `satisfy` and `verify`.
      Refs: CT-MOD-004, CT-EFF-005
      Model: Sonnet
- [ ] **2.12 Hardening for files from other people.** A parser recursion depth limit with an
      actionable error. An unreadable subfolder is reported and skipped instead of stopping
      `clew index`, with a test of the `sysml_files` error path in `load_project`. Very long
      lines, invalid UTF-8, and very large files are reported, not fatal. A fixed-seed stress
      test (deep nesting, random bytes, truncated files) runs in `cargo test` with no new
      dependency. Full fuzzing comes with the importers in 3.8.
      Refs: CT-SEC-009, CT-USE-007
      Model: Sonnet
- [ ] **2.13 Accessibility and keyboard pass.** `docs/verification/accessibility.md`: WCAG 2.1
      AA checks for contrast in both themes, visible focus, keyboard paths through every pane,
      and screen reader labels, with Narrator on Windows and VoiceOver on macOS checked by
      John and recorded in the checklist. `docs/verification/command-palette.md`: every
      action is in the palette. `docs/verification/themes.md`: light and dark themes,
      following the OS setting and the manual override. `docs/verification/editor.md`:
      highlighting, completion, go to definition, and the browser staying in step with
      edits. Fix what fails.
      Refs: CT-USE-004, CT-USE-002, CT-USE-005, CT-MOD-004, CT-MOD-005
      Model: Sonnet
- [ ] **2.14 Linux ARM64 installers and notices.** Build deb and rpm on `ubuntu-22.04-arm` in
      the release workflow with the same 20 MB check and checksums. Add an `ubuntu-22.04-arm`
      entry to the `installer-size` job, so the arm64 build is tested on every pull request
      and not only on a release tag. Remove the CT-PLT-001 ARM64 waiver. Fetch license files
      for the 16 shipped packages that publish none and add them to the notices file.
      Refs: CT-PLT-001, CT-EFF-001, CT-DEV-014
      Model: Sonnet
- [ ] **Gate 2 check.** `clew check --self` shows 0 errors and 0 phase 2 coverage warnings
      (CT-MOD-003, -004, -005, CT-SEC-009, and CT-USE-009 are phase 2 warnings from task 2.0
      until their tasks land). The perf job passes every phase 2 budget with the app measured
      (CT-EFF-002 to CT-EFF-006, CT-MOD-003). John installs the release build on his Windows 10
      laptop (4 GB), opens the Clewtrace repo, edits `model/requirements.sysml` with live
      errors, follows a requirement to its code and tests, ends the app in Task Manager during
      an edit and loses no saved work, and records idle memory including the WebView2
      processes. The accessibility, command palette, themes, and editor checklists pass. Raise
      the test coverage minimum to the measured value rounded down, and make the `app/src-tauri`
      coverage enforced. Then set `current_phase = 3`.

## Phase 3: Requirements product and pilot readiness

Exit gate: a requirements engineer with no SysML knowledge can create, trace, and exchange
requirements. Licensing and the trial work fully offline and never lock anyone out of their
data. A signed release carries its full evidence, installs and runs on an offline machine, and
builds reproducibly. Ready for the first business pilot. Phase 3 has 17 tasks plus code
signing and the gate.

- [ ] **3.1a Minimal-edit writer.** The core makes minimal text edits that create, change,
      and delete a requirement with the CT-TRC-001 fields (ID, title, statement, rationale,
      verification method, status, owner), keeping surrounding text and comments. Every
      write re-parses to the same fields (round-trip test over the repo's own model and
      the generated project).
      Refs: CT-USE-001, CT-TRC-001
      Model: Opus
- [ ] **3.1b Requirement forms.** Create, edit, and delete a requirement through a form on
      the 3.1a writer. Form edits join undo, redo, and autosave.
      `docs/verification/form-editing.md` is the Demonstration checklist.
      Refs: CT-USE-001, CT-USE-003, CT-USE-008
      Model: Sonnet
- [ ] **3.2 Trace link forms.** Add and remove satisfy, verify, derivation, refinement, and
      evidence links with pickers and search, through the same minimal-edit writer.
      Refs: CT-USE-001, CT-TRC-002
      Model: Sonnet
- [ ] **3.3 First-run sample project.** A bundled sample opens on first launch, with short
      guided steps to create and trace a first requirement. Demonstration checklist timed with
      one new user, 5 minutes or less.
      Refs: CT-USE-006
      Model: Sonnet
- [ ] **3.4 Trace matrix.** Rows and columns from any two element sets (by package, kind, or
      ID pattern). Cells show link kinds. Filters. Virtualized in both directions. Export to
      CSV here and to HTML in 3.5. `docs/verification/trace-matrix.md` is the Demonstration
      checklist.
      Refs: CT-TRC-003, CT-EFF-005
      Model: Sonnet
- [ ] **3.5 HTML reports.** Coverage, impact, and the trace matrix export as self-contained,
      printable HTML with the provenance header, from the app and with `--html` on the CLI.
      `clew report verify` checks HTML reports too. Store per-file hashes so verify can name the
      file that changed, and hash non-UTF-8 files. Old reports still verify: `clew report
      verify` reads both header forms.
      Refs: CT-XCH-003, CT-TRC-009
      Model: Sonnet
- [ ] **3.6 CSV and XLSX.** CSV import and export of requirements with a standard-library RFC
      4180 reader and writer. Import goes through the 3.1a writer, so imported requirements
      are valid SysML. The user picks the target file; the default is a new file named after
      the CSV. XLSX export of reports (rust_xlsxwriter 0.102.0 with default features off,
      MIT OR Apache-2.0, PR #27; add `("rust_xlsxwriter", &[])` to the no-network test's
      allowed features). Exported cells that start with `=`, `+`, `-`, or `@` are escaped so
      spreadsheets never run them.
      Refs: CT-XCH-002, CT-SEC-008
      Model: Sonnet
- [ ] **3.7a ReqIF export.** Export requirements and trace links as ReqIF 1.2 through
      quick-xml 0.42.0 (MIT, PR #27). Map the requirement fields to ReqIF spec types.
      Refs: CT-XCH-001
      Model: Sonnet
- [ ] **3.7b ReqIF import.** Import requirements and trace links through the 3.1a writer.
      Map ReqIF spec types to the requirement fields and keep unknown attributes. Round-trip
      test with 3.7a. No DTD processing or entity expansion: a test imports a file whose
      DOCTYPE declares an external entity and confirms nothing is fetched or expanded.
      Refs: CT-XCH-001, CT-SEC-008, CT-SEC-001
      Model: Opus
- [ ] **3.8 Importer fuzzing.** cargo-fuzz targets (nightly toolchain, fuzz job only; NCSA
      for libfuzzer-sys is a build-only per-crate exception, PR #27; release builds stay on
      the pinned stable toolchain) in `crates/clew-core/fuzz`, listed under `exclude` in the
      workspace `Cargo.toml` so stable builds never see it. Targets for the lexer and parser,
      CSV, and ReqIF, with seed corpora. A short fuzz run in CI. An inspection checklist that
      no importer executes content.
      Refs: CT-SEC-008, CT-SEC-009
      Model: Sonnet
- [ ] **3.9 Offline license file.** First bump clew-core to sha2 0.11 in its own commit, with
      a test that report input hashes are unchanged, so only one sha2 ships. Then a signed
      license file (licensee, edition, seats, expiry, license ID) verified offline with
      ed25519-dalek 3 (default features off, verify only, no OS random generator in the app;
      BSD-3-Clause per-crate exceptions for ed25519-dalek, curve25519-dalek, and subtle, PR #27;
      add `("ed25519-dalek", &[])` to the no-network test's allowed features) against a public
      key compiled into the app. A license status view. The signing tool is a separate crate
      that never ships, and the private key stays offline with John. ADR-0006 records the
      format.
      Refs: CT-PLT-005, CT-SEC-006, CT-SEC-001
      Model: Opus
- [ ] **3.10 Trial terms and read-only expiry.** First-run choice of personal, non-commercial
      use (no time limit) or commercial evaluation (90 days from the first commercial start).
      Remaining days shown in the status view. At expiry, pending edits are saved first, then
      editing, forms, and autosave turn off; browsing, search, reports, and every export keep
      working. Tests use an injected clock.
      Refs: CT-PLT-006, CT-PLT-007
      Model: Sonnet
- [ ] **3.11 Per-user install.** Windows installs per user without administrator rights;
      macOS runs from `~/Applications`; Linux documents the limits of deb and rpm. Tauri's
      own WiX template is per-machine, so a per-user MSI needs a custom template set with
      `bundle.windows.wix.template`, which keeps the MSI (John chose this route, decision 22
      on PR #27). Add a `trace.toml` exclude or file type for the template. If the template
      cannot be made per-user, stop and ask John about NSIS, which would reword CT-PLT-001.
      Demonstration checklist.
      Refs: CT-PLT-002, CT-PLT-001
      Model: Sonnet
- [ ] **3.12 Release evidence.** Build provenance attestation (actions/attest-build-provenance,
      pinned by commit, PR #27), the full trace report (`clew check --self` and
      `clew report coverage` with provenance) attached to each release, and a verification
      checklist. Attach the attestation bundle to the release, and the checklist explains how
      an air-gapped customer verifies the SHA-256 checksums and the attestation offline. CI
      launches the installed DMG, deb, and rpm as a smoke test.
      Refs: CT-DEV-006, CT-DEV-012, CT-PLT-001
      Model: Sonnet
- [ ] **3.13 SSDF, encryption, and offline edition docs.** `docs/SSDF.md` maps development
      practices to NIST SP 800-218. `docs/ENCRYPTION.md` explains BitLocker, FileVault, and
      LUKS, and how to enable FIPS-validated modes on each. `docs/verification/offline-edition.md`
      shows that every current build is the offline edition: it contains no network code, the
      no-network tests prove it on every pull request, and `docs/OFFLINE-EDITION.md`
      (`<!-- @implements CT-SEC-002 -->`) states the rule that CLAUDE.md points to. The
      checklist explains the WebKitGTK network process and shows it opens no connections.
      Inspection checklists for all three.
      Refs: CT-DEV-011, CT-SEC-005, CT-SEC-002
      Model: Sonnet
- [ ] **3.14 Parser conformance.** Run the example models from the OMG SysML v2 release
      (EPL-2.0, test inputs only, fetched pinned by hash in a CI job, never committed or
      shipped; ADR-0003) and report results per release. Check the lexer keyword list against
      the spec's table.
      Refs: CT-MOD-007, CT-MOD-002
      Model: Sonnet
- [ ] **3.15 Reproducible builds.** CI builds the same tag twice and compares every artifact.
      Document each exception and why. Byte-identical MSI and DMG are not promised.
      Refs: CT-DEV-007, CT-DEV-012
      Model: Sonnet
- [ ] **Code-signed installers.** Every Windows and macOS installer is code-signed before the
      first business pilot. The Linux deb and rpm are verified by `SHA256SUMS.txt` and the
      3.12 attestation (John, decision 23, PR #27). Plan on Microsoft Artifact Signing (Basic,
      about $10 a month) for the Windows MSI and the Apple Developer Program ($99 a year) to
      sign and notarize the macOS DMG. Free open-source signing programs do not apply,
      because Clewtrace uses the Business Source License (John, 2026-10-21). So that signed
      installers work offline: timestamp every Windows signature, so it stays valid after the
      short-lived signing certificate expires, and staple the notarization ticket to the app
      and the DMG, so macOS needs no network to check it. Artifact Signing goes through
      Tauri's `signCommand`, so the RFC 3161 timestamp flags go in that command (Tauri's
      `timestampUrl` and `tsp` may not apply to it). A release step runs
      `signtool verify /pa /v` and fails if the signature has no timestamp. For macOS,
      notarize the DMG itself, not only the app inside, then `xcrun stapler staple` and
      `xcrun stapler validate` both. Verify both on a machine with networking turned off.
      John sets up the accounts and CI secrets first.
      Refs: CT-PLT-001, CT-DEV-012, CT-PLT-003
      Model: Sonnet
- [ ] **Gate 3 check.** `clew check --self` shows 0 errors and 0 phase 3 coverage warnings.
      John installs the signed MSI per user on his laptop with no administrator prompt and with
      networking turned off. A new user finishes the first-run steps within 5 minutes. A
      license file from the signing tool is accepted offline, and an expired test license
      leaves the project readable and exportable. Raise the test coverage minimum. Then set
      `current_phase = 4`.

## Deferred follow-ups

From the phase 2 planning (John, PR #27, decision 4). None puts a budget or a requirement at
risk.

- Links are not stored in the index: the app holds the graph in memory (132 MB peak at
  10,000 requirements, measured in 1.8).
- Reports parse the text on every run: 554 ms at 10,000 requirements is fine for the CLI.
- `check --self` reads the project twice: no budget at risk.
- `clew impact` misses document `@implements` links: cosmetic.
- Symbolic links under source roots: none in the repo.
- Expression parser, `assert`, metadata, aliases, filtered imports, and conjugation: phase 4
  or later, and they grow only through requirements.
- Built-in libraries are not hashed: they are fixed by the tool version, which reports record.

Phase 4 (CMMC pack, plus baselines CT-TRC-007 and the change log CT-SEC-004) and phase 5
(diagrams, v1 import, API, AI assist, plus manual update checks CT-PLT-004) are broken into
tasks when the previous gate passes, using the phase column in `model/requirements.sysml`.
