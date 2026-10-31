# ADR-0001: Core technology stack

- Status: Accepted
- Date: 2026-10-20
- Updated: 2026-10-21 (task 0.10, reviewed against what was built in phase 0);
  2026-10-22 (task 1.8, performance gate); 2026-10-31 (task 2.0, phase 1 status and the
  phase 2 and 3 plan)
- Serves: CT-EFF-001, CT-EFF-002, CT-EFF-004, CT-EFF-008, CT-PLT-001, CT-PLT-005, CT-MOD-001,
  CT-TRC-008, CT-USE-004, CT-SEC-001, CT-SEC-006, CT-DEV-012, CT-DEV-014, CT-EFF-009

## Context

Clewtrace must install on Windows 10+, macOS 13+, and mainstream Linux, stay under 20 MB per
installer, idle under 200 MB of memory (250 MB on Linux), and handle 10,000 requirements in
400 MB on a 4 GB, 2-core machine. The same trace logic must run in the desktop app and in CI.

## Decision

| Layer | Choice |
| --- | --- |
| App shell | Tauri 2, using the OS web view (WebView2, WKWebView, WebKitGTK) |
| Core | One Rust library, `clew-core`, shared by the app and the `clew` CLI |
| UI | Svelte 5 with TypeScript, runes syntax only |
| Text editor | CodeMirror 6 |
| SysML v2 parser | Hand-written recursive descent in Rust, with error recovery |
| Storage | `.sysml` text files as truth; SQLite (rusqlite, bundled) with FTS5 as a rebuildable index |
| Code scanning | tree-sitter grammars for Rust and TypeScript; GitHub Actions workflow steps carry `@verifies`; each Svelte `<script lang="ts">` block is scanned with the TypeScript grammar, and markup and HTML files use `@begin`/`@end` regions or one `@file` tag |
| File watching | OS file events through `notify` (recommended watcher only, no polling watcher, no debouncer crate); a one-shot timer ends a burst of events |
| Licensing | Ed25519-signed license file verified offline with `ed25519-dalek` |
| Diagrams | Deferred to phase 5; SVG generated in Rust, layout chosen in a later ADR |
| Installers | MSI (Windows), universal DMG (macOS), deb and rpm (Linux), built by CI from a signed tag |
| Config files | `trace.toml` and `deps.toml` read by a small std-only TOML reader in `clew-core` |

## Status of the stack after phase 0

Built so far: the Tauri 2 shell (default features off, `wry` only, no plugins), the
`clew-core` library and `clew` CLI (`check --self`, `check-commit`, `notices`, `reqs`), the
Svelte 5 UI shell, the tree-sitter tag scanner, and the CI and release workflows.
Not built yet, still decided as above: the SysML v2 parser (phase 1, replacing the v0
requirement reader), the SQLite index, CodeMirror, file watching, and license file checks.
The Rust toolchain is pinned in `rust-toolchain.toml`.

## Status of the stack after phase 1

Built since phase 0: the hand-written SysML v2 parser with error recovery (replacing the v0
requirement reader), the model with name resolution, the SQLite index with FTS5 and
incremental re-indexing, the trace graph (derives, refines, satisfies, verifies, implements,
evidences), the coverage and impact reports, reproducible reports with `clew report verify`,
the phase-aware requirement coverage gate, and the performance job on 4 GB and 2 CPUs.
Still to build, decided as above: CodeMirror, file watching, license file checks, and the
desktop app's use of the core (phase 2).
Phases 2 and 3 are planned in `docs/PLAN.md`. ADR-0004 (app data layer, task 2.3a),
ADR-0005 (incremental live validation, task 2.9a), and ADR-0006 (license file format,
task 3.9) will record the design of those parts.

## Installers (John, 2026-10-21, task 0.9 decisions)

- **No AppImage.** An AppImage bundles GTK and WebKitGTK, which are LGPL and never allowed
  in shipped code (ADR-0003), and it is far over the 20 MB budget. The deb and rpm depend on
  the distribution's own GTK and WebKitGTK packages, so no LGPL code is distributed.
  CT-PLT-001 now names Linux (deb, rpm).
- **WebView2 install mode is `skip`.** The MSI never downloads anything during install
  (CT-SEC-001). Windows 11 and updated Windows 10 already have the WebView2 runtime.
  Air-gapped machines install Microsoft's offline WebView2 runtime first, as described in
  `docs/INSTALL.md`.
- **WiX Toolset 3** builds the MSI. tauri-cli downloads it at build time in CI; it is a
  build-only tool (ADR-0003, `deps.toml`).
- **Release builds come only from CI** on an annotated tag signed by a key in
  `.github/release-signers` (SSH) or `.github/release-keys.asc` (OpenPGP), on `main`.
  Each release attaches `SHA256SUMS.txt`, the third-party notices file, a CycloneDX SBOM,
  and the source of each shipped MPL-2.0 crate, all produced by `clew notices`.
- **Code signing** of the MSI and DMG is required before the first business pilot
  (`docs/PLAN.md`). Until then installers are unsigned and Windows SmartScreen and macOS
  Gatekeeper will warn.
- ARM64 installers (John, 2026-10-31): CT-PLT-001 names Linux deb and rpm on x64 and ARM64,
  and the universal DMG already covers Apple silicon. Task 2.14 builds the Linux ARM64
  packages on the `ubuntu-22.04-arm` runner, and the waiver for the ARM64 part ends at Gate 2.
  A Windows ARM64 installer is optional, because Windows on ARM runs the x64 MSI under
  emulation and Tauri's ARM64 path is NSIS only.

## Performance gate (John, 2026-10-22, task 1.8)

- The `perf (4 GB, 2 CPUs)` CI job runs the performance tests under
  `sudo systemd-run --scope -p MemoryMax=4G -p MemorySwapMax=0 -p AllowedCPUs=0-1`, not in a
  container. Containers set their limits through the same kernel control groups, so the
  limits are the same, and there is no container image to pin, download, or audit.
- It runs as root under the system manager, because a user manager may lack the memory
  and cpuset controllers and ignore the limits without an error. `AllowedCPUs` pins the
  tests to 2 cores, so they also see 2 cores; `CPUQuota` would only cap CPU time.
- The job reads the limits back from its cgroup, then runs a canary that writes 5 GiB and
  must be killed. It fails if either check does not hold.
- This covers the core and CLI budgets on Linux. The desktop app on a 4 GB Windows machine
  is still checked on John's laptop at each gate that changes the desktop app.

## Installer size and coverage in CI (John, 2026-10-31, task 2.0)

- The `installer-size` job builds the Linux deb on every pull request, on Ubuntu 22.04 with
  the release config and the notices file, and fails over 20 MB (CT-EFF-001).
- The `coverage` job fails below 95% of lines for clew-core and clew-cli. The minimum only
  goes up, at each gate.
- The WebKitGTK packages are cached from a folder under `$HOME`, keyed on the runner image
  and the package list, and the install step has a timeout.

## Development environment

- Coding and builds run in Claude Code cloud sessions (claude.ai/code): 4 vCPUs, 16 GB RAM,
  Rust preinstalled, included with the Pro plan. The 4 GB Pi 5 and 4 GB laptop are too small
  to compile the Tauri app comfortably.
- GitHub Actions builds all installers; the repository is public.
- John's Windows 10 laptop with 4 GB RAM is the reference test machine.

## Alternatives rejected

- **Electron:** bundles Chromium; installers over 100 MB and much higher memory. Fails CT-EFF-001 and CT-EFF-002.
- **Java or the OMG pilot implementation stack:** JVM memory overhead; the pilot code is EPL-2.0, so we write our own parser from the published specification instead of copying it.
- **Langium (TypeScript) parser:** keeps a JavaScript language runtime in memory and splits logic between UI and core.
- **React and React Flow:** larger runtime than Svelte; graph layout in JavaScript costs memory.
- **Monaco editor:** larger download and memory footprint than CodeMirror 6.

## Tradeoffs accepted by John (2026-10-20)

- Rust is harder for John to read than Python or TypeScript, so reviews lean on tests and
  Claude's explanations.
- The hand-written parser takes weeks longer than a generated one.
- Rust builds are slower than JavaScript builds.
- Svelte 5 is newer than React, so CLAUDE.md pins runes syntax to avoid mixed versions.
- Interactive diagram editing will need more custom work in phase 5.

## Consequences

- WebKitGTK on Linux uses more memory than the other web views, so Linux has its own budget.
- Windows 10 machines without the WebView2 runtime need it installed before Clewtrace. The
  MSI does not bundle or download it (install mode `skip`), which keeps the installer small
  and the install offline. `docs/INSTALL.md` describes the offline runtime step.
- Linux users need the distribution's WebKitGTK 4.1 package, which the deb and rpm declare
  as a dependency.
- CI caches the Rust target directory to keep build times down.
