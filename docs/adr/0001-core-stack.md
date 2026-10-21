# ADR-0001: Core technology stack

- Status: Accepted
- Date: 2026-10-20
- Serves: CT-EFF-001, CT-EFF-002, CT-EFF-004, CT-EFF-008, CT-PLT-001, CT-PLT-005, CT-MOD-001,
  CT-TRC-008, CT-USE-004, CT-SEC-006

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
| Code scanning | tree-sitter grammars for Rust, TypeScript, and Svelte |
| File watching | OS file events (no polling) |
| Licensing | Ed25519-signed license file verified offline with `ed25519-dalek` |
| Diagrams | Deferred to phase 5; SVG generated in Rust, layout chosen in a later ADR |

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
- Windows 10 machines without the WebView2 runtime need it installed; the MSI uses the
  bootstrapper, which keeps the installer small but needs a one-time download or an offline
  runtime package on air-gapped machines. Document both paths.
- CI caches the Rust target directory to keep build times down.
