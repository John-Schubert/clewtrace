# Clewtrace phases 2 and 3: draft plan for review (v2)

Drafted in chat on 2026-10-24, revised 2026-10-30. Sources: the phase 2 planning summary (main
at 5bce363), `model/requirements.sysml`, `docs/PLAN.md`, `CLAUDE.md`, and the product spec.

Status: the six changes listed under "Changes in v2" were approved by John in chat on
2026-10-30. The rest of the plan is still a draft. Flow: a Code session reviews it for gaps and
sizing, John posts his approvals as PR comments, then task 2.0 applies the approved requirement
and ADR changes.

## Changes in v2 (approved by John in chat, 2026-10-30)

1. **No polling file-watcher debounce.** Task 2.10 uses notify alone with a one-shot timer.
   notify-debouncer-* is not allowed: its background thread wakes on a timer forever
   (checked in notify-debouncer-full 0.7.0: `loop { ... thread::sleep(tick) ... }`), which
   breaks CT-EFF-008. No fallback to notify's polling watcher. Corrected notify's license to
   CC0-1.0 (checked on crates.io, 2026-10-30).
2. **Installer size checked on every PR from task 2.3,** not only at release, since 2.3 links
   clew-core (SQLite, parser, tree-sitter grammars) into the app for the first time.
3. **Local data (CT-SEC-003):** the recent-projects list stores paths only, with "Clear recent"
   and a note in the docs; a CLAUDE.md rule keeps project content out of web view storage; the
   autosave temp file sits in the same folder as its target.
4. **CT-SEC-002 offline edition stays in phase 3** instead of moving to phase 5. Task 3.13
   closes it with an inspection checklist, and task 2.0 adds the rule that any future network
   code goes behind a Cargo feature that is off by default.
5. **Code signing works offline:** staple the macOS notarization ticket, and timestamp every
   Windows signature.
6. **Dependency feature settings pinned:** rfd with `default-features = false` and `gtk3` only;
   rust_xlsxwriter and ed25519-dalek with default features off; a test that quick-xml never
   fetches or expands an external entity. The release attaches the attestation bundle so
   air-gapped customers can verify it.

## Summary

- Phase 2 becomes **Modeling MVP**: open a project, see errors, browse, follow traces, edit
  SysML text with live validation, autosave. Read-only browsing works after task 2.7
  (milestone A). John models in Clewtrace after task 2.10 (milestone B), 11 sessions in.
- Phase 3 becomes **Requirements product and pilot readiness**: forms, trace matrix,
  exchange formats, licensing and trial, release evidence, offline edition checklist,
  conformance, reproducible builds, code signing.
- Requirement changes: 3 move from phase 3 to 2, 15 move from 2 to 3, 3 optionally move to
  phases 4 and 5, 2 new requirements, 3 rewordings. Details in section 3.
- Size: phase 2 is 15 tasks plus the gate. Phase 3 is 15 tasks plus code signing and the gate.
  6 of the 30 tasks are marked Opus.

---

## 1. The six decisions

### Decision 1: CT-PLT-001 ARM64

**Recommendation: approve the rewording. Move the waiver's end to Gate 2 and build the Linux
ARM64 installers in task 2.14, the last task before the gate.**

- GitHub's arm64 hosted runners (`ubuntu-22.04-arm`, `ubuntu-24.04-arm`, `windows-11-arm`)
  became generally available for public repositories in August 2025 and are free there. Linux
  ARM64 deb and rpm can build natively on `ubuntu-22.04-arm`, the same glibc baseline as the
  x64 build. No cross-compiling, no new action.
- The universal DMG already covers Apple silicon. The rewording just says so.
- Windows ARM64 should stay optional. Tauri's documented ARM64 path is NSIS only, and at
  least one Tauri project got an ARM64 MSI building on `windows-11-arm` only after
  pre-downloading WiX and NSIS by hand. Windows on ARM runs the x64 MSI under emulation.
- Building first would cost a session before any modeling work. Extending the waiver one
  phase keeps the record honest and the path to modeling short.

### Decision 2: ADR updates

**Recommendation: approve both, as docs-only work in task 2.0.**

- ADR-0001: add "Status after phase 1" (parser, model, index, trace graph, coverage and
  impact reports, reproducible reports, coverage gate, perf job) and add GitHub Actions step
  scanning to the "Code scanning" row.
- ADR-0002: replace the old gate rule with your Gate 1 rule (finished phase gap = error,
  current phase gap = warning, Gate N needs zero warnings) and list the phase 1 additions
  (Markdown `@implements`, workflow `@verifies`, waivers with `part`, ADRs must cite a
  requirement).
- New ADRs later in this plan: ADR-0004 app data layer (2.3), ADR-0005 incremental live
  validation (2.9), ADR-0006 license file format (3.9).

### Decision 3: ed25519-dalek and BSD-3-Clause

**Recommendation: approve a per-crate exception for BSD-3-Clause, limited to ed25519-dalek and
the crates it brings in under that license. Do not add BSD-3-Clause to the shipped tier as a
whole. Record the decision now, add the dependency in task 3.9.**

- BSD-3-Clause is permissive. It needs the copyright notice shipped, which `clew notices`
  already does for every shipped crate.
- ed25519-dalek (3.0.0) and curve25519-dalek (5.0.0) are both BSD-3-Clause (checked on
  crates.io, 2026-10-30). `subtle` may be too. The Code session takes the exact list from
  cargo-deny when 3.9 adds it.
- Verifying a signature needs no random numbers, so the app uses default features off and
  never touches the OS random generator. Only the separate signing tool, which never ships,
  needs one.
- The alternatives cost more: ring has mixed licenses including OpenSSL-derived code, plus C
  and assembly that complicate cross builds; aws-lc-rs needs a C build and is much larger.
- A per-crate exception keeps the policy tight. A blanket BSD-3-Clause allowance would let
  any BSD crate in without a decision.

### Decision 4: which open follow-ups become phase 2 tasks

**Recommendation: fold the ones that matter for the app or for correctness into tasks; defer
the rest with a reason.**

| Follow-up | Where | Why |
|---|---|---|
| SQLite busy_timeout | 2.10 | App and CLI will share the index |
| Parser recursion depth limit | 2.12 | Files from other people |
| Unreadable subfolder stops `clew index` | 2.12 | Same |
| Project-root lookup for `clew reqs` | 2.1 | App and CLI need one project opener |
| Phase given only as a definition default is not read | 2.1 | Gate correctness bug |
| Ambiguous name takes first match silently | 2.1 | You will see these while modeling |
| Only ScalarValues is built in | 2.1 | Make other library imports one warning, not an error per name |
| satisfy and verify targets not checked to be requirements | 2.1 | Live validation should catch this |
| Short name used by two requirements links to the first | 2.1 | Report as an error |
| clew-cli main.rs at 50.7% test coverage | 2.1 | Needed before the coverage floor bites |
| Scanner impl over max_scope_lines | 2.2 | Same file as the Svelte work |
| HTML has no scanner | 2.2 | Small, same task |
| Measure CT-EFF-002, -003, -005 | 2.5, 2.6, 2.7 | Due in phase 2 |
| Search has no caller | 2.7 | Search box in the app |
| WebKitGTK apt download (27 minutes) | 2.0 | Slows every PR; actions/cache is already approved |
| 16 shipped packages with no license file | 2.14 | Notices completeness |
| Windows and Linux ARM64 installers | 2.14 | Decision 1 |
| Report verify cannot say which file changed; non-UTF-8 not hashed | 3.5 | Goes with the report work |
| Launching installed DMG, deb, rpm in CI | 3.12 | Goes with release evidence |
| Lexer keyword list from memory | 3.14 | Goes with conformance |
| Defer: links not stored in the index | later | The app holds the graph in memory (137 MB at 10,000 requirements) |
| Defer: reports parse text on every run | later | 554 ms at 10,000 requirements is fine for the CLI |
| Defer: `check --self` reads the project twice | later | No budget at risk |
| Defer: `clew impact` misses document `@implements` | later | Cosmetic |
| Defer: symbolic links under source roots | later | None in the repo |
| Defer: expression parser, assert, metadata, aliases | phase 4+ | Grows only through requirements |
| Defer: built-in libraries not hashed | keep | Fixed by tool version, which is recorded |

### Decision 5: Svelte scanner

**Recommendation: do not stay regions-only. In task 2.2, scan each `<script lang="ts">` block
with the TypeScript grammar you already ship, offsetting line numbers. Markup stays covered by
the component's tag or regions. No new dependency, no Svelte grammar.**

- Phase 2 adds roughly 15 to 25 components. Region tags around whole components are the
  "rubber stamp" risk the spec warns about, and retagging later costs more than doing it now.
- No Svelte grammar also means no extra grammar tables in the installer.
- Pair it with a CLAUDE.md rule: keep component logic in plain `.ts` modules and keep
  components thin. That gives exact item spans and easier tests.

### Decision 6: minimum test coverage

**Recommendation: set it now, at 95% of lines for clew-core and clew-cli, in task 2.0.**

- `cargo llvm-cov --fail-under-lines 95` in the coverage job. 95 is below today's 96.24% to
  absorb normal noise, but a session that skips tests will fail CI.
- Phase 2 adds a lot of core code (editing, autosave, incremental validation). A floor now
  costs nothing and protects that work.
- Add `app/src-tauri` to the measured set in 2.3, report-only until Gate 2.
- At each gate, raise the floor to the measured value rounded down to a whole percent. It
  only goes up, as you decided.
- UI TypeScript coverage (vitest) stays report-only through phase 2.

---

## 2. Phase 2 and 3 task list (PLAN.md format)

Replace the "Later phases" section of `docs/PLAN.md` with the block below, after approval.
The existing "Before the first business pilot" section moves inside phase 3 as the
code-signing item. IDs marked (new) exist only after task 2.0 adds them.

```markdown
## Phase 2: Modeling MVP

Exit gate: John models in Clewtrace on his 4 GB laptop. The app opens a project, shows parse
and name errors as he types, browses the model, follows trace links to code and tests, and
autosaves without loss. Every phase 2 budget passes in CI with the app measured, not only the
core library.

- [ ] **2.0 Phase 2 housekeeping.** Apply John's approved changes from the planning PR: the
      requirement moves, rewordings, and new requirements in `model/requirements.sysml`; the
      CT-PLT-001 ARM64 waiver end changed to Gate 2; ADR-0001 "status after phase 1" and
      workflow step scanning; ADR-0002 phase rule and phase 1 additions; ADR-0003 exceptions
      table records the approved ed25519-dalek exception as pending its dependency; clear the
      two stale follow-ups. Add to the CLAUDE.md golden rules: any future network code goes
      behind a Cargo feature that is off by default, so a build without it has no network
      code (CT-SEC-002). CI: coverage floor of 95% lines for clew-core and clew-cli, and an apt
      package cache for the WebKitGTK install. Replace the "Later phases" text.
      Refs: CT-DEV-001, CT-DEV-003, CT-DEV-010, CT-PLT-001, CT-SEC-002
      Model: Sonnet
- [ ] **2.1 Core fixes before the app.** A shared `Project::open(path)` that finds the project
      root (nearest folder with `trace.toml` or `.clewtrace/`, else the given folder), used by
      every `clew` command and later by the app. Read a phase given only as a default on a
      requirement definition. Warn on an ambiguous name. Report an import of a standard
      library package that is not built in (such as ISQ or SI) once, as a warning that its
      names are not checked, instead of one error per name. Check that satisfy and verify
      targets are requirements. Report a short name used by two requirements as an error.
      Move logic out of `clew-cli/src/main.rs` into tested functions.
      Refs: CT-TRC-008, CT-USE-007, CT-MOD-002, CT-DEV-003
      Model: Sonnet
- [ ] **2.2 Svelte script scanning.** Scan `<script lang="ts">` blocks with the TypeScript
      grammar, offsetting line numbers, so functions in components get item-level tags.
      Markup stays covered by the component tag or regions. Read `<!-- @file -->` in HTML
      files. Split `impl Scanner` under max_scope_lines. Update `docs/TRACEABILITY.md`, and add
      to CLAUDE.md: keep component logic in `.ts` modules and components thin.
      Refs: CT-DEV-002, CT-TRC-006
      Model: Sonnet
- [ ] **2.3 App data layer.** The Rust side of the app owns the open project (model, trace
      graph, coverage, index). Tauri commands return pages of rows: tree children, element
      detail, links to and from, problems, search hits. A typed client in
      `app/src/lib/api.ts`. Load on a background thread so the window stays responsive. Open a
      project from a folder dialog (rfd with `default-features = false` and the `gtk3`
      feature only, needs approval), a recent-projects list, or a path on the command line.
      The recent-projects list stores folder paths only, never project content, in the app
      config folder, with a "Clear recent" action, and `docs/INSTALL.md` says where it lives.
      Errors carry what, where, and how to fix. ADR-0004 records the command and paging
      design. Add `app/src-tauri` to the coverage job, report-only. Tests run the commands on
      the 205-requirement generated project. CI: build the deb on every pull request in the
      Ubuntu job and fail over 20 MB, the same check as the release workflow, since this task
      links clew-core into the app for the first time.
      Refs: CT-EFF-004, CT-EFF-005, CT-SEC-003, CT-MOD-001, CT-EFF-001
      Model: Opus
- [ ] **2.4 App shell.** Pane layout (model browser, main view, trace panel, problems). Light
      and dark themes from CSS tokens, following the OS setting, with a manual override. A
      command palette with an action registry that every later action joins. One shared
      virtual list component for any list that can pass 200 rows. Keyboard focus order and
      ARIA landmarks from the start. Adds vitest for UI tests (needs approval). Add to
      CLAUDE.md: the UI never puts project content in localStorage, sessionStorage, or
      IndexedDB; it asks the Rust core each time. A test fails on any use of those APIs in
      `app/src` outside UI preferences such as the theme.
      Refs: CT-USE-002, CT-USE-005, CT-USE-004, CT-EFF-005, CT-SEC-003
      Model: Sonnet
- [ ] **2.5 App performance probes.** A `perf-probe` Cargo feature, off in release builds (a
      test checks), that reports time to an interactive window and the resident memory of the
      app and its web view processes when idle with an empty project. The perf job measures on
      Linux under the existing systemd-run limits with xvfb; Windows and macOS jobs measure
      idle memory. CI fails on CT-EFF-002 and CT-EFF-003. `docs/verification/idle-cpu.md`
      gives the CT-EFF-008 analysis: every timer, listener, and thread, and why none polls. A
      test fails on `setInterval` in `app/src`. Later UI tasks add their probes here.
      Refs: CT-EFF-002, CT-EFF-003, CT-EFF-008, CT-EFF-009
      Model: Sonnet
- [ ] **2.6 Model browser and problems.** A lazy, virtualized tree of packages, definitions,
      and usages. Element detail: kind, qualified name, doc, attributes, file and line. A
      problems panel with every syntax, name, and trace problem as file:line:column with its
      fix hint; click to select. Probe: open the 10,000-requirement project in the app within
      3 s and 400 MB.
      Refs: CT-MOD-005, CT-USE-007, CT-EFF-004
      Model: Sonnet
- [ ] **2.7 Traces, impact, coverage, and search.** Trace panel for the selected element:
      links to and from, grouped by kind, including code tags with file and line; click to
      follow; back and forward. Impact view, upstream and downstream. Coverage view
      (no parent, no implementation, no verification, untraced elements) with filters. Search
      box over the FTS5 index with paged results. Read-only source view that opens a code file
      at the tagged line. Probes: select, filter, and navigate within 100 ms; search within
      200 ms, in the app.
      Refs: CT-TRC-005, CT-TRC-004, CT-EFF-005, CT-EFF-006
      Model: Sonnet
      Milestone A: Clewtrace can open and trace its own repo, read-only. Optional pre-release
      for a laptop check.
- [ ] **2.8 Text editor.** CodeMirror 6 for `.sysml` files (needs approval). Syntax
      highlighting from a keyword list generated from the Rust lexer, so the two cannot drift.
      Open a file at a position from the browser, problems, or trace panel. Explicit save
      through Rust with an atomic write: a temp file in the same folder as the target, flush,
      then rename. Never the system temp folder. Undo and redo, with tests.
      Refs: CT-MOD-004, CT-USE-003, CT-USE-008, CT-EFF-005, CT-SEC-003
      Model: Sonnet
- [ ] **2.9 Live validation.** After a short pause in typing (a one-shot timer reset by each
      keystroke, never a poll), send the buffer to the core. Re-parse only that file, then
      re-resolve names and rebuild the trace graph from the cached syntax trees of the other
      files. Return diagnostics as editor markers with help text, and refresh the browser,
      problems, and trace panel. Perf tests: an edit in a 10,000-line file is validated within
      1 s on the 4 GB, 2 CPU job, and the 10,000-requirement project stays within the
      CT-EFF-004 memory budget. ADR-0005 records the incremental design.
      Refs: CT-MOD-003, CT-MOD-005, CT-EFF-007, CT-EFF-005
      Model: Opus
- [ ] **2.10 Autosave, file watching, and concurrent use.** Autosave after a short pause and
      on blur and close, through the 2.8 atomic write. A test kills the writer mid-save and
      finds the old or the new content, never a torn file. Watch the project folder with OS
      file events through notify alone (needs approval, CC0-1.0 per-crate exception), using
      its recommended watcher only. Debounce with a one-shot timer that starts on the first
      event and ends after the burst; nothing runs while no files change. Never use notify's
      polling watcher or any notify-debouncer crate (its thread wakes on a timer forever,
      CT-EFF-008). If the OS runs out of file watches, report it with how to raise the limit
      instead of falling back to polling. On events: re-index changed files, reload clean
      buffers, and ask when a file with unsaved edits changed on disk (git pull, another
      editor). Set the SQLite busy_timeout, and test `clew index` running while the app holds
      the index. Add the file watcher to `docs/verification/idle-cpu.md`.
      Refs: CT-USE-008, CT-EFF-007, CT-EFF-008, CT-USE-009 (new)
      Model: Opus
      Milestone B: John models in Clewtrace. From here, edit `model/requirements.sysml` in the
      app; changes still go through pull requests.
- [ ] **2.11 Go to definition and completion.** F12 and Ctrl or Cmd+click go to the
      definition. Find references from the model and trace links. Completion for keywords,
      names in scope, and requirement short names after `satisfy` and `verify`.
      Refs: CT-MOD-004, CT-EFF-005
      Model: Sonnet
- [ ] **2.12 Hardening for files from other people.** A parser recursion depth limit with an
      actionable error. An unreadable subfolder is reported and skipped instead of stopping
      `clew index`. Very long lines, invalid UTF-8, and very large files are reported, not
      fatal. A fixed-seed stress test (deep nesting, random bytes, truncated files) runs in
      `cargo test` with no new dependency. Full fuzzing comes with the importers in 3.8.
      Refs: CT-SEC-009 (new), CT-USE-007
      Model: Sonnet
- [ ] **2.13 Accessibility and keyboard pass.** `docs/verification/accessibility.md`: WCAG 2.1
      AA checks for contrast in both themes, visible focus, keyboard paths through every pane,
      and screen reader labels, checked with Narrator on Windows and VoiceOver on macOS.
      `docs/verification/command-palette.md`: every action is in the palette. Fix what fails.
      Refs: CT-USE-004, CT-USE-002, CT-USE-005
      Model: Sonnet
- [ ] **2.14 Linux ARM64 installers and notices.** Build deb and rpm on `ubuntu-22.04-arm` in
      the release workflow with the same 20 MB check and checksums. Remove the CT-PLT-001
      ARM64 waiver. Fetch license files for the 16 shipped packages that publish none and add
      them to the notices file.
      Refs: CT-PLT-001, CT-EFF-001, CT-DEV-014
      Model: Sonnet
- [ ] **Gate 2 check.** `clew check --self` shows 0 errors and 0 phase 2 coverage warnings.
      The perf job passes every phase 2 budget with the app measured (CT-EFF-002 to CT-EFF-006,
      CT-MOD-003). John installs the release build on his Windows 10 laptop (4 GB), opens the
      Clewtrace repo, edits `model/requirements.sysml` with live errors, follows a requirement
      to its code and tests, ends the app in Task Manager during an edit and loses no saved
      work, and records idle memory including the WebView2 processes. The accessibility and
      command palette checklists pass. Raise the test coverage floor. Then set
      `current_phase = 3`.

## Phase 3: Requirements product and pilot readiness

Exit gate: a requirements engineer with no SysML knowledge can create, trace, and exchange
requirements. Licensing and the trial work fully offline and never lock anyone out of their
data. A signed release carries its full evidence, installs and runs on an offline machine, and
builds reproducibly. Ready for the first business pilot.

- [ ] **3.1 Requirement forms.** Create, edit, and delete a requirement through a form with the
      CT-TRC-001 fields (ID, title, statement, rationale, verification method, status, owner).
      The core makes minimal text edits that keep surrounding text and comments, and every save
      re-parses to the same fields (round-trip test). Form edits join undo, redo, and autosave.
      Refs: CT-USE-001, CT-USE-003, CT-TRC-001
      Model: Opus
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
      CSV here and to HTML in 3.5.
      Refs: CT-TRC-003, CT-EFF-005
      Model: Sonnet
- [ ] **3.5 HTML reports.** Coverage, impact, and the trace matrix export as self-contained,
      printable HTML with the provenance header, from the app and with `--html` on the CLI.
      `clew report verify` checks HTML reports too. Store per-file hashes so verify can name the
      file that changed, and hash non-UTF-8 files.
      Refs: CT-XCH-003, CT-TRC-009
      Model: Sonnet
- [ ] **3.6 CSV and XLSX.** CSV import and export of requirements with a standard-library RFC
      4180 reader and writer. Import goes through the 3.1 writer, so imported requirements are
      valid SysML. XLSX export of reports (rust_xlsxwriter with default features off, needs
      approval). Exported cells that start with `=`, `+`, `-`, or `@` are escaped so
      spreadsheets never run them.
      Refs: CT-XCH-002, CT-SEC-008
      Model: Sonnet
- [ ] **3.7 ReqIF 1.2.** Import and export requirements and trace links. Map ReqIF spec types to
      the requirement fields and keep unknown attributes. Round-trip test. XML through
      quick-xml (needs approval) with no DTD processing or entity expansion. A test imports a
      file whose DOCTYPE declares an external entity and confirms nothing is fetched or
      expanded.
      Refs: CT-XCH-001, CT-SEC-008, CT-SEC-001
      Model: Opus
- [ ] **3.8 Importer fuzzing.** cargo-fuzz targets (needs approval, nightly toolchain, build-only,
      fuzz job only; release builds stay on the pinned stable toolchain) for the lexer and
      parser, CSV, and ReqIF, with seed corpora. A short fuzz run in CI. An inspection
      checklist that no importer executes content.
      Refs: CT-SEC-008, CT-SEC-009 (new)
      Model: Sonnet
- [ ] **3.9 Offline license file.** A signed license file (licensee, edition, seats, expiry,
      license ID) verified offline with ed25519-dalek (default features off, verify only, no OS
      random generator in the app) against a public key compiled into the app. A license
      status view. The signing tool is a separate crate that never ships, and the private key
      stays offline with John. ADR-0006 records the format.
      Refs: CT-PLT-005, CT-SEC-006, CT-SEC-001
      Model: Opus
- [ ] **3.10 Trial terms and read-only expiry.** First-run choice of personal, non-commercial
      use (no time limit) or commercial evaluation (90 days from the first commercial start).
      Remaining days shown in the status view. At expiry, editing, forms, and autosave turn
      off; browsing, search, reports, and every export keep working. Tests use an injected
      clock.
      Refs: CT-PLT-006, CT-PLT-007
      Model: Sonnet
- [ ] **3.11 Per-user install.** Windows installs per user without administrator rights; macOS
      runs from `~/Applications`; Linux documents the limits of deb and rpm. If WiX 3 cannot
      build a per-user MSI through Tauri, stop and ask John about an NSIS installer.
      Demonstration checklist.
      Refs: CT-PLT-002, CT-PLT-001
      Model: Sonnet
- [ ] **3.12 Release evidence.** Build provenance attestation (actions/attest-build-provenance,
      needs approval), the full trace report (`clew check --self` and `clew report coverage`
      with provenance) attached to each release, and a verification checklist. Attach the
      attestation bundle to the release, and the checklist explains how an air-gapped customer
      verifies the SHA-256 checksums and the attestation offline. CI launches the installed
      DMG, deb, and rpm as a smoke test.
      Refs: CT-DEV-006, CT-DEV-012, CT-PLT-001
      Model: Sonnet
- [ ] **3.13 SSDF, encryption, and offline edition docs.** `docs/SSDF.md` maps development
      practices to NIST SP 800-218. `docs/ENCRYPTION.md` explains BitLocker, FileVault, and
      LUKS, and how to enable FIPS-validated modes on each. `docs/verification/offline-edition.md`
      shows that every current build is the offline edition: it contains no network code, the
      no-network tests prove it on every pull request, and the CLAUDE.md rule puts any future
      network code behind a Cargo feature that is off by default. Inspection checklists for
      all three.
      Refs: CT-DEV-011, CT-SEC-005, CT-SEC-002
      Model: Sonnet
- [ ] **3.14 Parser conformance.** Run the example models from the OMG SysML v2 release
      (EPL-2.0, used as test inputs only, fetched pinned by hash, never committed or shipped)
      and report results per release. Check the lexer keyword list against the spec's table.
      Refs: CT-MOD-007, CT-MOD-002
      Model: Sonnet
- [ ] **3.15 Reproducible builds.** CI builds the same tag twice and compares every artifact.
      Document each exception and why.
      Refs: CT-DEV-007, CT-DEV-012
      Model: Sonnet
- [ ] **Code-signed installers.** Every installer is code-signed before the first business
      pilot: Microsoft Artifact Signing (Basic, about $10 a month) for the Windows MSI and the
      Apple Developer Program ($99 a year) to sign and notarize the macOS DMG. Free
      open-source signing programs do not apply, because Clewtrace uses the Business Source
      License (John, 2026-10-21). So that signed installers work offline: timestamp every
      Windows signature, so it stays valid after the short-lived signing certificate expires,
      and staple the notarization ticket to the app and the DMG, so macOS needs no network to
      check it. Verify both on a machine with networking turned off.
      Refs: CT-PLT-001, CT-DEV-012, CT-PLT-003
      Model: Sonnet
- [ ] **Gate 3 check.** `clew check --self` shows 0 errors and 0 phase 3 coverage warnings.
      John installs the signed MSI per user on his laptop with no administrator prompt and with
      networking turned off. A new user finishes the first-run steps within 5 minutes. A
      license file from the signing tool is accepted offline, and an expired test license
      leaves the project readable and exportable. Raise the test coverage floor. Then set
      `current_phase = 4`.

## Later phases

Phase 4 (CMMC pack) and phase 5 (diagrams, v1 import, API, AI assist) are broken into tasks
when the previous gate passes, using the phase column in `model/requirements.sysml`.
```

**If you decline the optional moves (change D below),** those three requirements move to
phase 3 instead, and these tasks go at the end of phase 3:
3.16 Baselines (CT-TRC-007, Sonnet), 3.17 Change log (CT-SEC-004, Opus), and 3.18 Manual
update check (CT-PLT-004, Sonnet, behind the off-by-default network feature).

---

## 3. Requirement changes that need your approval

### A. Reword CT-PLT-001 (stays phase 0)

> CI shall produce installers for Windows 10 and newer (MSI) on x64, macOS 13 and newer (a
> universal DMG for x64 and Apple silicon), and Linux (deb, rpm) on x64 and ARM64. A Windows
> ARM64 installer is optional.

Waiver: change its end from "until phase 2" to "until Gate 2". Task 2.14 removes it.

### B. Move from phase 3 to phase 2 (the minimal editor)

| ID | Change |
|---|---|
| CT-MOD-003 fastDiagnostics | Move, and reword "within 1 s of saving a file" to "within 1 s of an edit or save in a file", since live validation works on the unsaved buffer |
| CT-MOD-004 textEditor | Move as is (highlighting, completion, go to definition) |
| CT-MOD-005 modelBrowser | Move as is |

### C. Move from phase 2 to phase 3 (after the editor)

CT-USE-001 formEditing, CT-USE-006 firstRun, CT-TRC-003 traceMatrix, CT-XCH-001 reqif,
CT-XCH-002 csvXlsx, CT-XCH-003 htmlReports, CT-SEC-002 offlineEdition, CT-SEC-005 osEncryption,
CT-SEC-008 safeImport, CT-PLT-002 perUserInstall, CT-PLT-005 offlineLicense,
CT-PLT-006 trialTerms, CT-PLT-007 noDataLockout, CT-DEV-006 releaseEvidence, CT-DEV-011 ssdf.

That is 15. CT-SEC-002 is new to this list in v2: it stays visible in phase 3, closed by the
3.13 checklist, instead of moving to phase 5.

Staying in phase 2: CT-EFF-002, -003, -004, -005, -006, -008 and CT-USE-002, -003, -004,
-005, -008. Budgets and keyboard, theme, and accessibility basics are far cheaper to build in
from the start than to retrofit.

Licensing still lands before the first business pilot, because Gate 3 is the pilot gate.

The air-gap rules are not weakened by any move: CT-SEC-001 (no telemetry, no network) and
CT-PLT-003 (runs fully offline) are phase 0 requirements, and the no-network tests run on
every pull request.

### D. Optional, beyond phases 2 and 3: move three more out

You allowed moves between phases 2 and 3 only, so these need a separate yes or no. If you say
no, move them to phase 3 instead (a phase 2 to 3 move), so they still come after the editor.

| ID | To | Why |
|---|---|---|
| CT-TRC-007 baselines | phase 4 | Audit feature that pairs with the CMMC pack |
| CT-SEC-004 changeLog | phase 4 | Same: assessors use the change history |
| CT-PLT-004 manualUpdates | phase 5 | An update check is network code; none exists yet |

### E. Amend CT-USE-001 (moves to phase 3 per C)

> Users shall create, edit, and delete requirements **and their trace links** through forms
> without writing SysML text, and each form save shall produce valid SysML v2 text.

Without this, task 3.2 (link forms) has no requirement to trace to.

### F. Two new requirements (phase 2)

```sysml
requirement <'CT-SEC-009'> robustInput : ProductRequirement {
    doc /* The application and CLI shall reject malformed, deeply nested, or oversized input files with an actionable error, and shall not crash, hang, or exceed their memory budgets. */
    attribute :>> verificationMethod = "Test";
    attribute :>> phase = 2;
}

requirement <'CT-USE-009'> concurrentUse : ProductRequirement {
    doc /* The application and the CLI shall work on the same project at the same time without errors or lost work. */
    attribute :>> verificationMethod = "Test";
    attribute :>> phase = 2;
}
```

CT-SEC-009 goes in the Security package, CT-USE-009 in Usability. They give the parser depth
limit (2.12) and the SQLite busy_timeout (2.10) an honest requirement to trace to, instead of
stretching CT-SEC-008 (importers) or CT-USE-008 (autosave).

### Result

| Phase | Before | After (A to F, with D) | After (without D) |
|---|---|---|---|
| 0 | 16 | 16 | 16 |
| 1 | 14 | 14 | 14 |
| 2 | 29 | 16 | 16 |
| 3 | 5 | 17 | 20 |
| 4 | 7 | 9 | 7 |
| 5 | 4 | 5 | 4 |
| Total | 75 | 77 | 77 |

---

## 4. Dependencies that need your approval (at the task that adds them)

Each gets a `deps.toml` entry with license, reason, requirement, and the feature settings
below. Licenses marked "checked" were read from crates.io on 2026-10-30; the Code session
confirms each against the shipped tier before adding it.

| Task | Dependency | Scope | License | Settings and notes |
|---|---|---|---|---|
| 2.3 | rfd (folder dialog) | shipped | MIT (checked, 0.16.1) | `default-features = false`, `features = ["gtk3"]`. The defaults pull in async-std and the D-Bus portal stack you kept out in 0.7. Chosen over tauri-plugin-dialog so the "no tauri-plugin-*" check stays intact |
| 2.4 | vitest, plus jsdom or happy-dom | build-only | MIT | Needed for UI `@verifies` tests. Never ships |
| 2.5 | xvfb (apt) | CI tool | | Headless display for Linux app probes |
| 2.8 | @codemirror/state, view, commands, language, lint, autocomplete, search | shipped | MIT | Already in the stack in CLAUDE.md, but not yet in deps.toml. Bundled locally; the strict CSP blocks any outside load |
| 2.10 | notify (file events) | shipped | CC0-1.0 (checked, 9.0.0-rc.7) | Per-crate exception. Recommended watcher only, never the polling watcher. **notify-debouncer-full and notify-debouncer-mini are not allowed** (timer thread polls forever, CT-EFF-008) |
| 3.6 | rust_xlsxwriter | shipped | MIT OR Apache-2.0 (checked, 0.102.0) | `default-features = false`. Its only required dependency is zip with deflate |
| 3.7 | quick-xml | shipped | MIT (checked, 0.42.0) | No DTD processing; the external-entity test in 3.7 |
| 3.8 | cargo-fuzz, nightly toolchain | build-only | | Fuzz job only; releases stay on pinned stable |
| 3.9 | ed25519-dalek and its BSD-3-Clause crates | shipped | BSD-3-Clause (checked, 3.0.0; curve25519-dalek 5.0.0) | Per-crate exception per decision 3. Default features off, verify only |
| 3.11 | NSIS (only if needed) | build-only | | Only if a per-user MSI is not possible |
| 3.12 | actions/attest-build-provenance | CI | | Pin by commit like the other actions |

CSV uses the standard library, so no crate.

---

## 5. Model use

- **Opus:** 2.3 data layer, 2.9 live validation, 3.9 license file (your list), plus three
  additions: 2.10 autosave and file watching (data-loss risk), 3.1 form writer (rewriting user
  text while keeping comments), 3.7 ReqIF (large, loosely specified format).
- **Sonnet, medium effort:** everything else. Any task that fails twice moves to Opus.

---

## 6. Notes for the Code review session

- **Sizing risks:** 2.1 (many small items), 2.3 (now also the PR size check), 2.9, 2.10, 3.1,
  and 3.7 are the most likely to overflow one session. Split any that will.
- **Check:** that `perf-probe` cannot reach a release build and does not affect CT-EFF-001;
  whether Tauri can build a per-user MSI with WiX 3; how long the PR deb build adds to the
  Ubuntu job, and whether the apt cache from 2.0 covers its packages; that Artifact Signing
  in CI applies an RFC 3161 timestamp; that stapling works on the universal DMG.
- **IDs:** CT-SEC-009 and CT-USE-009 do not exist until task 2.0. No task before 2.0 may
  cite them.
- **Gate effect:** after 2.0, CT-MOD-003, -004, and -005 become phase 2 coverage warnings,
  and the 15 moved requirements stop warning until phase 3.
- **Product spec drift** (fix in chat after approval): CT-PLT-001 still lists AppImage,
  CT-EFF-009 still says "a VM", the roadmap phase names and requirement phases need the
  changes above, and CT-SEC-002 shows phase 3.
