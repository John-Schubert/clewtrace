# Review of the phase 2 and 3 plan (v2)

Reviewed 2026-10-30 against `main` at 5bce363. The plan under review is
`docs/planning/phase-2-3-plan.md`, added unchanged. This review changes no code, model,
ADR, `trace.toml`, `deps.toml`, or `docs/PLAN.md`.

Sources checked: `model/requirements.sysml`, `trace.toml`, `deps.toml`, `deny.toml`,
`CLAUDE.md`, ADR-0001 to ADR-0003, `docs/TRACEABILITY.md`, `docs/PLAN.md`,
`.github/workflows/ci.yml` and `release.yml`, `app/src-tauri/src/main.rs` (the no-network
tests), `crates/clew-core/src/check.rs` and `report.rs`, the `main` ruleset, recent CI run
timings from the GitHub API, crates.io (2026-10-30), and the Tauri source on its `dev`
branch (2026-10-30).

Not checked here: the Rust code could not be built on this machine, so `clew check --self`
on this branch runs in CI only. Items that need a web account or hardware are marked "not
verifiable here".

## 1. Numbers in the plan that check out

- Phase counts today: 16, 14, 29, 5, 7, 4 (75). After A to F with D: 16, 14, 16, 17, 9, 5
  (77). Without D: 16, 14, 16, 20, 7, 4 (77). All match the "Result" table.
- The phase 2 list after the moves is 16: CT-EFF-002 to -006 and -008, CT-USE-002, -003,
  -004, -005, -008, CT-MOD-003, -004, -005, CT-SEC-009, CT-USE-009.
- Task count: 2.0 to 2.14 is 15, 3.1 to 3.15 is 15, plus code signing and two gates.
  6 tasks are marked Opus. Milestone B is the 11th session (before the splits below).
- Every ID cited in the plan exists in the model except CT-SEC-009 and CT-USE-009. They are
  cited only by 2.10, 2.12, and 3.8, all after 2.0. No task before 2.0 cites them.
- Every phase 2 and phase 3 requirement (after the moves, with or without D) is cited by at
  least one task.
- The repo is public, so the free arm64 hosted runners apply.
- Total test line coverage 96.24% and main.rs at 50.7% match the phase 2 planning summary.
- crates.io licenses as stated: notify 9.0.0-rc.7 CC0-1.0, ed25519-dalek 3.0.0 and
  curve25519-dalek 5.0.0 BSD-3-Clause, `subtle` 2.6.1 BSD-3-Clause (so it is on the list),
  quick-xml 0.42.0 MIT, rust_xlsxwriter 0.102.0 MIT OR Apache-2.0. Its zip dependency uses
  `deflate` only, which pulls zopfli (Apache-2.0), flate2 (MIT OR Apache-2.0), and zlib-rs
  (Zlib). All are on the shipped list.
- The other ed25519-dalek 3 crates are on the shipped list: ed25519, signature, zeroize,
  digest, cpufeatures (MIT OR Apache-2.0), and fiat-crypto (MIT OR Apache-2.0 OR
  BSD-1-Clause).

## 2. Section 6 "Check" results

| Check | Result |
|---|---|
| `perf-probe` cannot reach a release build | Partly verified. `release.yml` passes no `--features`, and `app/src-tauri/Cargo.toml` has no `[features]` yet. A Cargo feature that is off compiles no Rust code, so it adds no bytes (CT-EFF-001). Gap: any UI probe code in `app/src` is bundled by Vite whatever the Cargo feature says. See should fix S5. |
| Tauri per-user MSI with WiX 3 | Verified from the Tauri source. The bundler's `main.wxs` hardcodes `InstallScope="perMachine"` and installs under Program Files. The only way to change it is a custom template (`bundle.windows.wix.template`). Tauri's NSIS installer defaults to `currentUser`. See S14. |
| Time the PR deb build adds | Estimated from runs. The release `build (ubuntu-22.04)` job (deb and rpm, release profile, no cargo cache) took 3 min 47 s on 2026-10-21. Today's `check (ubuntu-latest)` takes 1.5 to 2.5 min. As a parallel job the deb check adds about 2 min of wall time per PR. Added to the check job, it adds about 3 min. |
| Does the 2.0 apt cache cover the deb job | Only if the cache key includes the runner image. CI uses `ubuntu-latest` (24.04) and the release uses `ubuntu-22.04`, so the packages differ. Also, the "slows every PR" premise is wrong: in 25 recent CI runs the apt step took 25 to 50 s, except 2 runs at about 26 and 27 min. See S11. |
| Artifact Signing applies an RFC 3161 timestamp | Not verifiable here. In the Tauri source, `timestampUrl` and `tsp` sit next to `signCommand`. Artifact Signing goes through `signCommand`, so the timestamp flags most likely have to be in that command. Confirm in the signing task. See S16. |
| Stapling on the universal DMG | Not verifiable here. A universal binary does not change stapling. Check whether Tauri notarizes the DMG or only the app inside it. See S16. |

## 3. Sizing

"Fits" means one Pro-plan session on the model marked: roughly 800 lines or less of new
code and tests, about 12 files or fewer, and one CI area to iterate on. These are estimates,
not measurements. Splits are in S1.

| Task | Files it touches | Fits? |
|---|---|---|
| 2.0 Housekeeping (Sonnet) | `model/requirements.sysml`, `trace.toml` (waiver text), ADR-0001, -0002, -0003, `CLAUDE.md`, `docs/TRACEABILITY.md`, `docs/PLAN.md`, `ci.yml` | Yes. Docs and YAML. The apt cache may take a few CI runs. |
| 2.1 Core fixes (Sonnet) | new `clew-core/src/project.rs`, `lib.rs`, `reqs.rs`, `model.rs`, `trace.rs`, `check.rs`, `report.rs`, `index.rs`, `clew-cli/src/main.rs`, `clew-cli/tests/*`, `docs/MODELING.md` | No. Seven unrelated fixes plus a refactor of the 455-line `main.rs`. Split into 2.1a and 2.1b. |
| 2.2 Svelte scanning (Sonnet) | `clew-core/src/scan.rs` (1,384 lines), `config.rs`, `trace.toml`, `docs/TRACEABILITY.md`, ADR-0001, ADR-0002, `CLAUDE.md`, `app/src/App.svelte`, `app/index.html` | Yes, tight. The `impl Scanner` split is the risky part. |
| 2.3 App data layer (Opus) | `app/src-tauri/Cargo.toml`, `src/main.rs` plus new `commands.rs`, `project.rs`, `recent.rs`, `app/src/lib/api.ts`, `deps.toml`, ADR-0004, `docs/INSTALL.md`, `ci.yml` (two jobs), no-network test | No. Split into 2.3a and 2.3b, and move the PR size job into 2.0 (S10). |
| 2.4 App shell (Sonnet) | `App.svelte`, 4 to 6 new components, `app/src/lib/actions.ts`, theme CSS, `package.json`, `vite.config.ts`, `deps.toml`, `CLAUDE.md`, `ci.yml` (vitest step), storage test | Tight. Fits if the virtual list stays minimal. Split if the first attempt overruns. |
| 2.5 Perf probes (Sonnet) | `app/src-tauri/Cargo.toml` `[features]`, new `probe.rs`, `ci.yml` (perf job, Windows and macOS steps), `docs/verification/idle-cpu.md`, two tests | Tight. CI on three OSes is the risk. Fits with S5 and S6 applied. |
| 2.6 Browser and problems (Sonnet) | 3 components, `api.ts`, `src-tauri` commands, a probe | Yes, once 2.3 exists. |
| 2.7 Traces, impact, coverage, search (Sonnet) | 5 views, navigation history, `api.ts`, commands, source view, 2 probes | No. Split into 2.7a and 2.7b. |
| 2.8 Text editor (Sonnet) | `package.json`, `deps.toml`, `Editor.svelte`, SysML language mode, new `clew-core/src/io.rs`, a save command, undo tests | Yes, if the keyword list is served by a command at runtime instead of generated at build time. |
| 2.9 Live validation (Opus) | `model.rs` (1,106 lines), `trace.rs`, a parse cache, the editor debounce, lint markers, panel refresh, `perf.rs`, ADR-0005 | No. Split into 2.9a (core, Opus) and 2.9b (app wiring, Sonnet). |
| 2.10 Autosave, watching, concurrency (Opus) | autosave, a crash test, a watcher module, debounce, reload and conflict prompt UI, `index.rs` busy_timeout, a concurrency test, `deps.toml`, `deny.toml`, ADR-0003, `idle-cpu.md` | No. Split into 2.10a and 2.10b, both Opus. |
| 2.11 Go to definition, completion (Sonnet) | `model.rs` position lookup, 2 commands, editor extensions | Yes. |
| 2.12 Hardening (Sonnet) | `parser.rs`, `lexer.rs`, `index.rs`, `io.rs`, a new stress test | Yes. |
| 2.13 Accessibility (Sonnet) | 2 checklists, fixes across components, a contrast test | Yes for the session. Narrator and VoiceOver need John (M5). |
| 2.14 Linux ARM64 and notices (Sonnet) | `release.yml`, the PR size job, `trace.toml` (remove the waiver), `notices.rs`, license texts | Yes, if the PR size job from S10 can build on `ubuntu-22.04-arm`. Otherwise the arm64 build is only tried on a release tag. |
| 3.1 Requirement forms (Opus) | new `clew-core/src/edit.rs`, round-trip tests, form components, undo and autosave hooks | No. Split into 3.1a (writer, Opus) and 3.1b (forms, Sonnet). |
| 3.2 Link forms (Sonnet) | `edit.rs`, pickers, search | Yes. |
| 3.3 First-run sample (Sonnet) | a sample project resource, `tauri.conf.json` resources, first-run state, guide UI, checklist | Yes. The timed run with a new user is John's. |
| 3.4 Trace matrix (Sonnet) | a core matrix query, a 2D virtual grid, CSV export | Yes, tight. |
| 3.5 HTML reports (Sonnet) | `report.rs`, `reproduce.rs`, `clew-cli/src/main.rs`, app export | Yes, tight. |
| 3.6 CSV and XLSX (Sonnet) | new `csv.rs`, an import through `edit.rs`, xlsx export, formula escaping, `deps.toml`, no-network test | Yes, tight. |
| 3.7 ReqIF (Opus) | new `reqif.rs` (import, export, mapping), round-trip and XXE tests, `deps.toml` | No. Split into 3.7a (export, Sonnet) and 3.7b (import, Opus). |
| 3.8 Fuzzing (Sonnet) | a new fuzz crate, `Cargo.toml` (workspace exclude), corpora, `ci.yml`, checklist, `deps.toml` | Yes. |
| 3.9 License file (Opus) | new `license.rs`, a signing tool crate, status view, ADR-0006, `deps.toml`, `deny.toml`, ADR-0003, sha2 bump | Yes, tight. |
| 3.10 Trial and expiry (Sonnet) | `license.rs`, first-run choice, status view, editing gates, clock injection | Yes. |
| 3.11 Per-user install (Sonnet) | a custom WiX template or the NSIS config, `tauri.release.conf.json`, `trace.toml`, checklist | Yes, but it will likely stop for John's choice (S14). Ask that early. |
| 3.12 Release evidence (Sonnet) | `release.yml` (attest, trace report, smoke launch), checklist, `deps.toml` | Tight. The release workflow only runs on a signed tag, so testing is slow. |
| 3.13 SSDF, encryption, offline docs (Sonnet) | 3 docs, 3 checklists | Yes. |
| 3.14 Conformance (Sonnet) | a CI job, a runner, a report | Yes. |
| 3.15 Reproducible builds (Sonnet) | `release.yml` or a new workflow, a docs page | Yes for "measure and document exceptions". Not for making the MSI and DMG byte-identical. |
| Code signing (Sonnet) | `release.yml`, `tauri.release.conf.json`, checklist | Yes, after John has the accounts and has set the CI secrets. |

## 4. Findings

### Blocking

**B1. CT-EFF-008 cannot be verified the way 2.5 plans, so Gate 2 cannot pass.**
CT-EFF-008 is verified by Analysis. A `docs/verification/` file only counts for Inspection
or Demonstration (`CHECKLIST_METHODS` in `report.rs`, rule in `docs/TRACEABILITY.md`).
So `idle-cpu.md` will not count, and CT-EFF-008 stays a phase 2 warning. The fix keeps
the analysis document and adds a real test.

In 2.5, replace:

> `docs/verification/idle-cpu.md`
> gives the CT-EFF-008 analysis: every timer, listener, and thread, and why none polls. A
> test fails on `setInterval` in `app/src`. Later UI tasks add their probes here.

with:

> `docs/verification/idle-cpu.md`
> gives the CT-EFF-008 analysis: every timer, listener, and thread, and why none polls. It
> supports the review but does not count as verification, since checklists count only for
> Inspection and Demonstration. The verification is a test tagged `@verifies CT-EFF-008`. It
> fails on `setInterval`, `requestAnimationFrame` loops, `thread::sleep`, and
> `recv_timeout` in first-party code outside an allowlist in the test, where each entry
> gives its reason. Later UI tasks add their probes here.

The other choice is to change CT-EFF-008's method to Inspection (decision 17).

**B2. notify needs a second license exception, and the plan pins a release candidate.**
On Linux, notify depends on `inotify` 0.11.5 and `inotify-sys` 0.1.8, which are both ISC.
ISC is not on the shipped list in `deny.toml` or ADR-0003, so `cargo deny` fails in 2.10
with only the CC0-1.0 exception. Also, 9.0.0-rc.7 is a release candidate. The latest
stable release is 8.2.1 (CC0-1.0). Its default features are `macos_fsevent` only, which
is the recommended macOS watcher.

In section 4, replace the notify row with:

> | 2.10 | notify (file events) | shipped | CC0-1.0 (checked, 8.2.1 stable; 9.x is a release candidate) | Per-crate exceptions: CC0-1.0 for notify, ISC for inotify and inotify-sys (Linux only, checked 0.11.5 and 0.1.8). Default features (`macos_fsevent` only). Recommended watcher only, never the polling watcher. **notify-debouncer-full and notify-debouncer-mini are not allowed** (timer thread polls forever, CT-EFF-008) |

In 2.10, replace "(needs approval, CC0-1.0 per-crate exception)" with "(needs approval:
notify 8.2.1, per-crate exceptions CC0-1.0 for notify and ISC for inotify and inotify-sys)".

**B3. The 95% test coverage floor would not block anything.**
The `coverage (report only)` job is not in the `main` ruleset. The required checks are the
three `check` jobs, `cargo-deny`, and `perf (4 GB, 2 CPUs)`. A failing coverage job would
show red but would not stop a merge. `docs/TRACEABILITY.md` also says the job never fails
on the number.

In 2.0, replace "CI: coverage floor of 95% lines for clew-core and clew-cli, and an apt
package cache for the WebKitGTK install." with:

> CI: a coverage floor of 95% lines for clew-core and clew-cli
> (`cargo llvm-cov --fail-under-lines 95`). Rename the job from `coverage (report only)` to
> `coverage`, and update `docs/TRACEABILITY.md` to say a minimum is set and only goes up.
> John adds `coverage` to the "CI must pass" ruleset. Also an apt package cache for the
> WebKitGTK install (see the apt note under 2.0).

### Should fix

**S1. Split the tasks that will not fit one session.**
Replace 2.1, 2.3, 2.7, 2.9, 2.10, 3.1, and 3.7 with the blocks below. The rest of each
original text carries over unchanged where it is not listed.

```markdown
- [ ] **2.1a Project opener and CLI tests.** A shared `Project::open(path)` that finds the
      project root (nearest folder with `trace.toml` or `.clewtrace/`, else the given
      folder), used by every `clew` command and later by the app. Move logic out of
      `clew-cli/src/main.rs` into tested functions.
      Refs: CT-TRC-008, CT-USE-007, CT-DEV-003
      Model: Sonnet
- [ ] **2.1b Model checks.** Read a phase given only as a default on a requirement
      definition. Warn on an ambiguous name. Report an import of a standard library package
      that is not built in (such as ISQ or SI) once, as a warning that its names are not
      checked, instead of one error per name. Check that satisfy and verify targets are
      requirements. Report a short name used by two requirements as an error.
      Refs: CT-MOD-002, CT-TRC-001, CT-TRC-002, CT-DEV-003, CT-USE-007
      Model: Sonnet
- [ ] **2.3a App data layer.** The Rust side of the app owns the open project (model, trace
      graph, coverage, index). Tauri commands return pages of rows: tree children, element
      detail, links to and from, problems, search hits. A typed client in
      `app/src/lib/api.ts`. Load on a background thread so the window stays responsive.
      Open a project from a path on the command line. Errors carry what, where, and how to
      fix. ADR-0004 records the command and paging design. Extend the no-network manifest
      check to read `crates/clew-core/Cargo.toml` too. Add `app/src-tauri` to the coverage
      job, report-only. Tests run the commands on the 205-requirement generated project.
      Refs: CT-EFF-004, CT-EFF-005, CT-MOD-001, CT-EFF-001, CT-SEC-001
      Model: Opus
- [ ] **2.3b Opening projects.** A folder dialog (rfd with `default-features = false` and
      the `gtk3` feature only, needs approval; add `("rfd", &["gtk3"])` to the no-network
      test's allowed features). A recent-projects list that stores folder paths only, never
      project content, in the app config folder, with a "Clear recent" action.
      `docs/INSTALL.md` says where it lives.
      Refs: CT-SEC-003, CT-USE-007, CT-EFF-005
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
      Refs: CT-USE-008, CT-USE-009 (new)
      Model: Opus
- [ ] **2.10b File watching.** Watch the project folder with OS file events through notify
      alone (needs approval: notify 8.2.1, per-crate exceptions CC0-1.0 for notify and ISC
      for inotify and inotify-sys), using its recommended watcher only. Debounce with a
      one-shot timer that starts on the first event and ends after the burst; nothing runs
      while no files change. Never use notify's polling watcher or any notify-debouncer
      crate (its thread wakes on a timer forever, CT-EFF-008). If the OS runs out of file
      watches, report it with how to raise the limit instead of falling back to polling. On
      events: re-index changed files, reload clean buffers, and ask when a file with
      unsaved edits changed on disk (git pull, another editor). Add the file watcher to
      `docs/verification/idle-cpu.md`.
      Refs: CT-EFF-007, CT-EFF-008, CT-USE-008, CT-USE-009 (new)
      Model: Opus
      Milestone B: John models in Clewtrace. From here, edit `model/requirements.sysml` in
      the app; changes still go through pull requests.
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
- [ ] **3.7a ReqIF export.** Export requirements and trace links as ReqIF 1.2 through
      quick-xml (needs approval). Map the requirement fields to ReqIF spec types.
      Refs: CT-XCH-001
      Model: Sonnet
- [ ] **3.7b ReqIF import.** Import requirements and trace links through the 3.1a writer.
      Map ReqIF spec types to the requirement fields and keep unknown attributes. Round-trip
      test with 3.7a. No DTD processing or entity expansion: a test imports a file whose
      DOCTYPE declares an external entity and confirms nothing is fetched or expanded.
      Refs: CT-XCH-001, CT-SEC-008, CT-SEC-001
      Model: Opus
```

After the splits, phase 2 has 22 tasks and Milestone B is the 16th session (2.0 to 2.10b).
Phase 3 has 17 tasks plus code signing.

**S2. 2.1 cites too few requirements.** A unique short name is CT-TRC-001 ("a unique ID").
Satisfy and verify targets being requirements is CT-TRC-002. The current Refs line would
make the tags dishonest or force them onto CT-MOD-002. S1 fixes this in 2.1b. If 2.1 is
not split, replace its Refs line with:
`Refs: CT-TRC-008, CT-USE-007, CT-MOD-002, CT-TRC-001, CT-TRC-002, CT-DEV-003`.

**S3. Three phase 2 Demonstration requirements have no named verification.** CT-USE-005
(themes), CT-MOD-004 (text editor), and CT-MOD-005 (model browser) need an `@verifies` test
or a checklist before Gate 2. In 2.13, replace:

> `docs/verification/command-palette.md`: every action is in the palette. Fix what fails.
> Refs: CT-USE-004, CT-USE-002, CT-USE-005

with:

> `docs/verification/command-palette.md`: every action is in the palette.
> `docs/verification/themes.md`: light and dark themes, following the OS setting and the
> manual override. `docs/verification/editor.md`: highlighting, completion, go to
> definition, and the browser staying in step with edits. Fix what fails.
> Refs: CT-USE-004, CT-USE-002, CT-USE-005, CT-MOD-004, CT-MOD-005

For phase 3, CT-TRC-003 (trace matrix) also needs one. In 3.4, add:
"`docs/verification/trace-matrix.md` is the Demonstration checklist."

**S4. CT-SEC-002's wording does not match the plan's approach.** The requirement says "A
build option shall produce an offline edition with all network code compiled out." The plan
(approved change 4) shows that every build is the offline edition and that future network
code sits behind an off-by-default feature. An inspector will ask where the build option
is. Also, putting the rule only in CLAUDE.md would make CLAUDE.md a hashed `@implements`
document. Proposed rewording (decision 18):

> Any network code shall be behind a build option that is off by default, so that a build
> without it is an offline edition with no network code.

In 3.13, replace "and the CLAUDE.md rule puts any future network code behind a Cargo
feature that is off by default" with "and `docs/OFFLINE-EDITION.md`
(`<!-- @implements CT-SEC-002 -->`) states the rule that CLAUDE.md points to".

**S5. Keep probe code out of the UI bundle.** A Cargo feature does not reach `app/src`. Any
TypeScript probe code ships in every installer. In 2.5, replace "A `perf-probe` Cargo
feature, off in release builds (a test checks), that reports" with:

> A `perf-probe` Cargo feature, off in release builds (a test checks that `release.yml`
> passes no `--features` and that `default` does not include it). All timing and memory
> reading is on the Rust side, so no probe code is in the UI bundle. It reports

**S6. Nothing drives the UI in CI for the in-app latency probes.** 2.6 and 2.7 ask for
select, filter, navigate, and search times "in the app", but no task adds a way to click
through the UI headless. WebDriver (tauri-driver) would be a new dependency and does not
support macOS. In 2.5, after "Later UI tasks add their probes here.", add:

> With `perf-probe` on, the app can run a fixed script of actions through the 2.4 action
> registry, log the time from each action to the next painted frame, and exit. No new
> dependency.

And because the existing peak memory reader works on Linux only, replace "Windows and
macOS jobs measure idle memory" with:

> Windows and macOS jobs measure idle memory from a workflow step (PowerShell `Get-Process`
> summing the app and its `msedgewebview2` children, and `ps -o rss` on macOS), so no new
> crate is needed. On Linux, run `xvfb-run` inside the `systemd-run` scope, since `sudo`
> drops `DISPLAY`.

**S7. Enforce the pinned feature settings.** The no-network test's `ALLOWED_FEATURES` covers
only `tauri` and `tauri-build`, and it reads only `app/src-tauri/Cargo.toml`. The approved
pins for ed25519-dalek and rust_xlsxwriter go in `crates/clew-core/Cargo.toml`, which no
test reads. 2.3a in S1 extends the check. In 3.6, add: "Add `("rust_xlsxwriter", &[])` to
the no-network test's allowed features." In 3.9, add: "Add `("ed25519-dalek", &[])` to the
no-network test's allowed features."

**S8. ed25519-dalek 3 brings a second copy of sha2.** ed25519-dalek 3.0.0 needs sha2 0.11.
clew-core uses sha2 0.10.9. Both would ship, along with two copies of digest and its
helpers. In 3.9, add:

> First bump clew-core to sha2 0.11 in its own commit, with a test that report input hashes
> are unchanged, so only one sha2 ships.

**S9. Update the ADR text that 2.2 makes wrong.** ADR-0001's "Code scanning" row and
ADR-0002's "As built in phase 0" both say Svelte files use regions only. `app/index.html`
is outside every source root, so reading `<!-- @file -->` there also needs a `trace.toml`
change. In 2.2, replace "Update `docs/TRACEABILITY.md`, and add" with:

> Add `html` to `languages` and `app/index.html` to the source roots in `trace.toml`.
> Update `docs/TRACEABILITY.md`, the "Code scanning" row of ADR-0001, and "As built" in
> ADR-0002, and add

**S10. Add the PR installer size check in 2.0, on the release image.** The CI `check` job
runs on `ubuntu-latest` (24.04). The release deb is built on `ubuntu-22.04` with
`tauri.release.conf.json` and the notices file. A deb from 24.04 can differ in size. Doing
it in 2.0 records the size before 2.3a links clew-core, and still meets approved change 2.
Remove the last two sentences of 2.3 ("CI: build the deb ... for the first time."). In 2.0,
add:

> CI: a parallel `installer-size` job on `ubuntu-22.04` runs `clew notices`, builds the deb
> with `tauri.release.conf.json`, and fails over 20 MB, using the same check as
> `release.yml`. John adds it to the "CI must pass" ruleset. This records the size before
> 2.3a links clew-core into the app for the first time.

In 2.14, add: "Add an `ubuntu-22.04-arm` entry to the `installer-size` job, so the arm64
build is tested on every pull request and not only on a release tag."

**S11. Make the apt fix match the problem.** In 25 recent CI runs the apt step took 25 to
50 s, and 2 runs took about 26 and 27 min. It does not slow every PR. `actions/cache` cannot
write to the root-owned `/var/cache/apt/archives`. In decision 4's table, replace "Slows
every PR; actions/cache is already approved" with "2 of 25 recent runs took about 26 min;
actions/cache is already approved". In 2.0, replace "an apt package cache for the WebKitGTK
install" with:

> an apt package cache for the WebKitGTK install: download the `.deb` files into a folder
> under `$HOME`, cache it with actions/cache keyed on the runner image version and the
> package list, and install from it. Add `timeout-minutes` to the step so a slow mirror
> fails fast instead of holding the job for half an hour.

**S12. Fuzzing needs one more license decision and a workspace rule.** libfuzzer-sys 0.4.13
is "(MIT OR Apache-2.0) AND NCSA". NCSA is not on the build-only list. The fuzz crate needs
nightly, so it must stay out of the workspace members, or `cargo test --workspace` on
stable breaks. In 3.8, replace "cargo-fuzz targets (needs approval, nightly toolchain,
build-only, fuzz job only; release builds stay on the pinned stable toolchain)" with:

> cargo-fuzz targets (needs approval, including NCSA for libfuzzer-sys as a build-only
> per-crate exception; nightly toolchain, fuzz job only; release builds stay on the pinned
> stable toolchain) in `crates/clew-core/fuzz`, listed under `exclude` in the workspace
> `Cargo.toml` so stable builds never see it

**S13. Expiry must not drop unsaved edits.** CT-USE-008 says no saved work is lost, and
CT-PLT-007 says nobody is locked out. In 3.10, replace "At expiry, editing, forms, and
autosave turn off;" with:

> At expiry, pending edits are saved first, then editing, forms, and autosave turn off;

**S14. Decide the per-user installer route early.** Tauri's WiX template is per-machine. A
per-user MSI needs a custom WiX template. That template is an XML file Clewtrace must
maintain against Tauri updates, and it needs a `trace.toml` exclude or a new file-level
type. Tauri's NSIS installer is per-user by default, but switching would change CT-PLT-001's
"MSI". In 3.11, replace "If WiX 3 cannot build a per-user MSI through Tauri, stop and ask
John about an NSIS installer." with:

> Tauri's own WiX template is per-machine, so a per-user MSI needs a custom template set
> with `bundle.windows.wix.template`. Use the route John chose in the planning PR
> (decision 22).

**S15. Phase 3 has no Linux installer signing.** "Every installer is code-signed" covers
only the MSI and DMG in the task text. In the code-signing task, replace "Every installer
is code-signed before the first business pilot:" with:

> Every Windows and macOS installer is code-signed before the first business pilot. The
> Linux deb and rpm are verified by `SHA256SUMS.txt` and the 3.12 attestation, unless John
> chooses GPG signing (decision 23).

**S16. Spell out the offline signing checks.** In the code-signing task, replace "Verify
both on a machine with networking turned off." with:

> Artifact Signing goes through Tauri's `signCommand`, so the RFC 3161 timestamp flags go
> in that command (Tauri's `timestampUrl` and `tsp` may not apply to it). A release step
> runs `signtool verify /pa /v` and fails if the signature has no timestamp. For macOS,
> notarize the DMG itself, not only the app inside, then `xcrun stapler staple` and
> `xcrun stapler validate` both. Verify both on a machine with networking turned off.

### Minor

**M1. Name the two stale follow-ups in 2.0.** Replace "clear the two stale follow-ups" with
"clear the two Gate 1 follow-ups in `docs/PLAN.md` (the waiver says "until phase 2", and
ADR-0002 states the old gate rule)".

**M2. Decision 4's table misses three follow-ups.** Add these rows:

> | `sysml_files` error path in `load_project` has no test | 2.12 | Same as the unreadable subfolder |
> | Peak memory read only on Linux | 2.5 | Windows and macOS idle memory |
> | Defer: filtered imports and conjugation | phase 4+ | Grows only through requirements |

Also drop the stale "`perf (4 GB, 2 CPUs)` is not a required check" follow-up from 1.8. It
is in the ruleset now.

**M3. Gate effect note.** In section 6, replace "after 2.0, CT-MOD-003, -004, and -005
become phase 2 coverage warnings" with "after 2.0, CT-MOD-003, -004, -005, CT-SEC-009,
and CT-USE-009 become phase 2 coverage warnings, along with any other phase 2 requirement
still lacking an implementation or a verification".

**M4. rfd version.** crates.io lists 0.17.2 as the latest stable release (0.16.1 is the
latest upload). In 0.17, the default features are `xdg-portal` (pollster) and `wayland`,
not async-std. The pinned settings are still right. In section 4, replace "MIT (checked,
0.16.1)" with "MIT (checked, 0.17.2)", and replace "The defaults pull in async-std and the
D-Bus portal stack" with "The defaults pull in the XDG portal and Wayland crates".

**M5. Screen reader checks need John.** A cloud session cannot run Narrator or VoiceOver.
In 2.13, replace "checked with Narrator on Windows and VoiceOver on macOS" with "with
Narrator on Windows and VoiceOver on macOS checked by John and recorded in the checklist".

**M6. Per-file hashes change the report header.** In 3.5, add: "Old reports still verify:
`clew report verify` reads both header forms."

**M7. EPL-2.0 test inputs.** 3.14 fetches EPL-2.0 example models in CI. ADR-0003 has no
rule for test data that is never shipped or committed. That needs a yes or no (decision 21).

**M8. WebKitGTK's network process.** WebKitGTK starts a `WebKitNetworkProcess` even when
the app makes no connections. In 3.13, add: "The offline edition checklist explains the
WebKitGTK network process and shows it opens no connections."

**M9. CSV import target.** 3.6 does not say which `.sysml` file receives imported
requirements. In 3.6, add: "The user picks the target file. The default is a new file
named after the CSV."

**M10. The 137 MB figure.** Decision 4's table says the graph is 137 MB at 10,000
requirements. Task 1.8 recorded a 132 MB peak for load and coverage. Use the 1.8 figure,
or say where 137 MB was measured.

## 5. Decisions for John

Answer by number, for example "1 yes, 9 no, 22 NSIS".

1. Decision 1: reword CT-PLT-001 (change A), and change the waiver end to Gate 2, with
   task 2.14 removing it. Recommend yes.
2. Decision 2: the ADR-0001 and ADR-0002 updates, done in 2.0. Recommend yes.
3. Decision 3: BSD-3-Clause as per-crate exceptions for ed25519-dalek, curve25519-dalek,
   and subtle only, added in 3.9. Recommend yes.
4. Decision 4: the follow-up mapping, with M2 added. Recommend yes.
5. Decision 5: scan Svelte `<script lang="ts">` blocks in 2.2, with S9. Recommend yes.
6. Decision 6: a 95% line floor for clew-core and clew-cli in 2.0, with the job renamed to
   `coverage` and added to the ruleset (B3). Recommend yes.
7. Change B: move CT-MOD-003 (reworded to "an edit or save"), CT-MOD-004, and CT-MOD-005 to
   phase 2. Recommend yes.
8. Change C: move the 15 requirements to phase 3. Recommend yes.
9. Change D: CT-TRC-007 and CT-SEC-004 to phase 4, CT-PLT-004 to phase 5. If no, they go to
   phase 3 as tasks 3.16 to 3.18. Recommend yes.
10. Change E: amend CT-USE-001 to include trace links. Recommend yes.
11. Change F: add CT-SEC-009 and CT-USE-009 to phase 2. Recommend yes.
12. Opus for 2.10 (2.10a and 2.10b after the split). Recommend yes.
13. Opus for 3.1 (3.1a only after the split; 3.1b on Sonnet). Recommend yes.
14. Opus for 3.7 (3.7b only after the split; 3.7a on Sonnet). Recommend yes.
15. Split 2.1, 2.3, 2.7, 2.9, 2.10, 3.1, and 3.7 as in S1. Recommend yes.
16. notify 8.2.1 instead of 9.0.0-rc.7, with per-crate exceptions CC0-1.0 for notify and ISC
    for inotify and inotify-sys (B2). Recommend yes.
17. CT-EFF-008 verification: (a) a `@verifies` test as in B1, or (b) change its method to
    Inspection so a checklist counts. Recommend (a).
18. Reword CT-SEC-002 as in S4, done in 2.0 with the other requirement changes. Recommend
    yes.
19. A parallel `installer-size` job on `ubuntu-22.04` in 2.0, added to the ruleset (S10).
    Recommend yes.
20. Dependencies in section 4: approve them now, or at each task as the plan says. This
    covers rfd, vitest with happy-dom or jsdom, xvfb, CodeMirror, rust_xlsxwriter,
    quick-xml, cargo-fuzz, and actions/attest-build-provenance. Recommend "at each task",
    with this PR as the record of intent.
21. EPL-2.0 OMG example models, fetched pinned by hash in a CI job, never committed or
    shipped (3.14). Recommend yes.
22. Per-user Windows install: (a) a custom WiX template that keeps the MSI, or (b) NSIS,
    with CT-PLT-001 reworded. Recommend (a), and switch to (b) only if the template
    cannot be made per-user.
23. Linux installer signing: (a) checksums and attestation only, or (b) GPG-sign the rpm and
    deb as well. Recommend (a) for the pilot.
24. NCSA for libfuzzer-sys as a build-only per-crate exception (S12). Recommend yes.
25. Bump clew-core to sha2 0.11 in 3.9, before adding ed25519-dalek (S8). Recommend yes.
