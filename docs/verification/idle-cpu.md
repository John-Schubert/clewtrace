<!-- @verifies CT-EFF-008 -->
# Idle CPU analysis (CT-EFF-008)

CT-EFF-008 says the app uses no CPU while idle beyond responding to OS events. This page
lists every timer, listener, and thread in first-party code, and why none of them polls.
It supports the review. It is not the verification: that is the test
`first_party_code_does_not_poll` in `crates/clew-core/tests/idle_cpu.rs`, which fails on
`setInterval`, `requestAnimationFrame`, `thread::sleep`, and `recv_timeout` outside its
allowlist. Update this page when a task adds a timer, listener, or thread, and add the
task's probe to that test.

Last reviewed at task 2.5, against the app shell of task 2.4.

## Rust side (app/src-tauri)

| What | Where | Why it does not poll |
| --- | --- | --- |
| Tauri event loop (GTK, Win32, or Cocoa) | `main` | Blocks in the OS until an event arrives. Idle is a wait in the kernel. |
| `clew-load` thread | `data.rs`, `AppState::open` | Runs once to load a project, then ends. Started only when a project is opened. |
| Wait for the load to end | `data.rs`, `AppState::wait` | A `Condvar` wait. The load thread calls `notify_all` when it ends. No timeout. |
| Command threads | `commands.rs` | Tauri's thread pool runs a command when the UI calls it and parks the worker after. The UI calls `wait_for_project` once, not in a loop. |
| Folder dialog | `commands.rs`, `pick_folder` | `rfd` runs the OS dialog and returns when the user picks or cancels. |
| Recent-projects file | `recent.rs` | Read at start and written when the list changes. There is no file watcher. |

Probe threads (`probe.rs`) exist only in builds with the `perf-probe` feature, which release
builds never use (`probe_gate.rs` checks it). They sleep once for the idle settle time and
once for a watchdog. The allowlist in the test gives the same reason.

## UI side (app/src)

| What | Where | Why it does not poll |
| --- | --- | --- |
| `keydown` on the window | `App.svelte`, `<svelte:window>` | Runs when a key is pressed. |
| `change` on the dark-mode media query | `App.svelte`, `$effect` | Runs when the OS theme changes. Removed when the effect is destroyed. |
| Theme `$effect` | `App.svelte` | Re-runs only when `preference` or `systemDark` change. |
| Scroll handler | `VirtualList.svelte` | Runs on a scroll event and sets one value. |
| Focus on mount | `CommandPalette.svelte` | Runs once when the palette opens. |
| Event handlers on buttons and inputs | all components | Run on user input. |
| Autosave pause | `lib/autosave.ts`, `Autosaver` (task 2.10a) | A one-shot `setTimeout` that each edit restarts. It saves once, after the text has been still for 1 s, and leaves no timer behind. |
| Editor `blur` handler | `lib/editor.ts`, `editorState` (task 2.10a) | Runs when the editor loses focus, and saves once. |
| Window close request | `App.svelte`, `saveOnClose` (task 2.10a) | A Tauri event listener. Runs when the user closes the window, saves once, then closes it. |

There are no repeating timers, animation loops, `ResizeObserver`s, or CSS animations in the UI. The
web view is the OS's own (WebView2, WKWebView, WebKitGTK). It is idle when the page is.

## What this does not cover

- The OS web view's own helper processes. They are outside first-party code. Their idle
  memory is measured by the probes (task 2.5). Their idle CPU is not measured yet.
- Code added after task 2.5. Each later UI task adds its timers and listeners here, or
  adds a reason to the test allowlist.
