//! Performance probes, compiled only with the `perf-probe` Cargo feature (never in release
//! builds). With `CLEW_PROBE=idle` or `CLEW_PROBE=script` in the environment the app measures
//! itself, prints one JSON line on stdout, and exits. All timing and memory reading happens
//! here, on the Rust side. The web view runs only a small script this module injects, so the
//! UI bundle holds no probe code. The script reports back by changing `document.title`.
//!
//! - Time to an interactive window: from the first line of `main` to the second painted
//!   frame after the page loaded.
//! - Idle memory: the resident memory of the app and the processes below it (the web view's
//!   helper processes), read from `/proc` after the window has been idle for `IDLE_SETTLE`.
//!   Windows and macOS read memory from a CI workflow step instead.
//! - `script` mode also runs a fixed list of actions through the command palette, which
//!   runs them in the action registry, and logs the time from each to the next painted frame.
//! - `views` mode opens the project given on the command line and runs `VIEW_STEPS`: the
//!   coverage filters, two searches, selecting a hit, following a trace link, and going
//!   back (CT-EFF-005, CT-EFF-006). Each step is timed from its start to the painted frame
//!   after its result is on screen.

use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use clew_core::json::quote;
use tauri::Manager;

/// Titles of the registered actions the `script` mode runs, in order (task 2.4 registry).
// @implements CT-EFF-003
pub const SCRIPT: [&str; 5] = [
    "Theme: dark",
    "Theme: light",
    "Go to Problems",
    "Go to Main view",
    "Theme: follow the OS setting",
];

/// One timed step of the `views` mode: an optional action run through the command palette,
/// then some script that does what a user's hand does (typing, clicking), then a wait until
/// the result is on screen.
// @implements CT-EFF-005, CT-EFF-006
#[derive(Clone, Copy, Debug)]
pub struct Step<'a> {
    /// What the report calls the step. It starts with `Coverage: ` and `Navigate: ` (a
    /// budget of 100 ms), `Select: `, or `Search: ` (200 ms).
    pub title: &'a str,
    /// The title of a registered action to run first, or empty for none.
    pub action: &'a str,
    /// Script that runs after the action. `probe` is an object that keeps values between steps.
    pub drive: &'a str,
    /// A JavaScript expression that is true once the step's result is on screen.
    pub done: &'a str,
}

/// The state of the trace panel when the trace of an element has loaded: it names the
/// element, lists rows, and holds no "Loading..." row.
// @implements CT-EFF-005
const TRACE_LOADED: &str = "(() => { const p = document.querySelector('#pane-trace [data-element]'); return p !== null && p.querySelector('[role=option]') !== null && !p.textContent.includes('Loading'); })()";

/// The steps `views` mode runs, in order. The project has requirements with no parent,
/// implementation, or verification, and elements traced to nothing, so every filter lists
/// a page. Selecting a search hit for `PF-00101` lets the next step follow its first trace
/// link (to the requirement it derives from) and the last go back.
// @implements CT-EFF-005, CT-EFF-006
pub const VIEW_STEPS: [Step<'static>; 8] = [
    Step {
        title: "Coverage: no verification",
        action: "Coverage: no verification",
        drive: "",
        done: "document.querySelector('[data-view=coverage][data-filter=\"no-verification\"][data-ready] [role=option]') !== null",
    },
    Step {
        title: "Coverage: no parent",
        action: "Coverage: no parent",
        drive: "",
        done: "document.querySelector('[data-view=coverage][data-filter=\"no-parent\"][data-ready] [role=option]') !== null",
    },
    Step {
        title: "Coverage: untraced",
        action: "Coverage: untraced",
        drive: "",
        done: "document.querySelector('[data-view=coverage][data-filter=\"untraced\"][data-ready] [role=option]') !== null",
    },
    Step {
        title: "Search: system budget",
        action: "Search the model",
        drive: "const box = document.querySelector('#search-input'); box.value = 'system budget'; box.dispatchEvent(new Event('input', { bubbles: true }));",
        done: "document.querySelector('[data-view=search][data-query=\"system budget\"][data-ready] [role=option]') !== null",
    },
    Step {
        title: "Search: PF-00101",
        action: "",
        drive: "const box = document.querySelector('#search-input'); box.value = 'PF-00101'; box.dispatchEvent(new Event('input', { bubbles: true }));",
        done: "document.querySelector('[data-view=search][data-query=\"PF-00101\"][data-ready] [role=option]') !== null",
    },
    Step {
        title: "Select: a search hit",
        action: "",
        drive: "document.querySelector('[data-view=search] [role=option]').click();",
        done: TRACE_LOADED,
    },
    Step {
        title: "Navigate: follow a trace link",
        action: "",
        drive: "probe.from = document.querySelector('#pane-trace [data-element]').dataset.element; document.querySelector('#pane-trace [role=option]').click();",
        done: "document.querySelector('#pane-trace [data-element]').dataset.element !== probe.from && (() => { const p = document.querySelector('#pane-trace [data-element]'); return p.querySelector('[role=option]') !== null && !p.textContent.includes('Loading'); })()",
    },
    Step {
        title: "Navigate: back",
        action: "Trace: back",
        drive: "",
        done: "document.querySelector('#pane-trace [data-element]').dataset.element === probe.from && (() => { const p = document.querySelector('#pane-trace [data-element]'); return p.querySelector('[role=option]') !== null && !p.textContent.includes('Loading'); })()",
    },
];

/// One process: its ID, its parent, its name, and its resident memory.
// @implements CT-EFF-002
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub rss_bytes: u64,
    /// Proportional set size: shared pages split among the processes that map them. Reported
    /// for information; the budget is on `rss_bytes`.
    pub pss_bytes: u64,
}

/// Memory of the app and of the processes below it.
// @implements CT-EFF-002, CT-EFF-010
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Memory {
    pub app_bytes: u64,
    pub webview_bytes: u64,
    pub pss_total_bytes: u64,
    /// The app process alone, in PSS (CT-EFF-010).
    pub app_pss_bytes: u64,
    pub processes: Vec<Proc>,
}

/// Everything one probe run measured.
// @implements CT-EFF-002, CT-EFF-003
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub interactive: Duration,
    pub memory: Option<Memory>,
    pub actions: Vec<(String, Duration)>,
}

/// The parent ID and name from the text of `/proc/<pid>/stat`. The name sits in
/// parentheses and may itself hold spaces and parentheses, so the last `)` ends it.
// @implements CT-EFF-002
// Only the Linux reader calls it; the tests run it everywhere.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_stat(text: &str) -> Option<(u32, String)> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    let name = text.get(open + 1..close)?;
    // After the name: state, then the parent ID.
    let ppid = text
        .get(close + 1..)?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;
    Some((ppid, name.to_string()))
}

/// The resident memory in bytes from the text of `/proc/<pid>/status` (`VmRSS`, in kB).
// @implements CT-EFF-002
// Only the Linux reader calls it; the tests run it everywhere.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_rss_bytes(text: &str) -> Option<u64> {
    let line = text.lines().find_map(|l| l.strip_prefix("VmRSS:"))?;
    let kb: u64 = line.split_whitespace().next()?.parse().ok()?;
    Some(kb * 1024)
}

/// The proportional memory in bytes from the text of `/proc/<pid>/smaps_rollup`.
// @implements CT-EFF-002
// Only the Linux reader calls it; the tests run it everywhere.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_pss_bytes(text: &str) -> Option<u64> {
    let line = text.lines().find_map(|l| l.strip_prefix("Pss:"))?;
    let kb: u64 = line.split_whitespace().next()?.parse().ok()?;
    Some(kb * 1024)
}

/// `root` and every process below it, whatever the depth.
// @implements CT-EFF-002
pub fn descendants(procs: &[Proc], root: u32) -> Vec<Proc> {
    let mut found: Vec<Proc> = procs.iter().filter(|p| p.pid == root).cloned().collect();
    let mut next = 0;
    while next < found.len() {
        let parent = found[next].pid;
        found.extend(procs.iter().filter(|p| p.ppid == parent).cloned());
        next += 1;
    }
    found
}

/// The memory of `root` (the app) and its descendants (the web view), or `None` when `root`
/// is not in the table.
// @implements CT-EFF-002, CT-EFF-010
pub fn memory_of(procs: &[Proc], root: u32) -> Option<Memory> {
    let processes = descendants(procs, root);
    let app = processes.iter().find(|p| p.pid == root)?;
    let app_bytes = app.rss_bytes;
    let webview_bytes = processes.iter().map(|p| p.rss_bytes).sum::<u64>() - app_bytes;
    let pss_total_bytes = processes.iter().map(|p| p.pss_bytes).sum();
    Some(Memory {
        app_bytes,
        webview_bytes,
        pss_total_bytes,
        app_pss_bytes: app.pss_bytes,
        processes,
    })
}

/// The report as one line of JSON.
// @implements CT-EFF-002, CT-EFF-003, CT-EFF-010
pub fn report_json(report: &Report) -> String {
    let ms = |d: &Duration| format!("{:.1}", d.as_secs_f64() * 1000.0);
    let mut out = format!(r#"{{"time_to_interactive_ms":{}"#, ms(&report.interactive));
    if let Some(m) = &report.memory {
        let _ = write!(
            out,
            r#","idle_memory":{{"app_bytes":{},"webview_bytes":{},"total_bytes":{},"pss_total_bytes":{},"app_pss_bytes":{},"processes":["#,
            m.app_bytes,
            m.webview_bytes,
            m.app_bytes + m.webview_bytes,
            m.pss_total_bytes,
            m.app_pss_bytes
        );
        for (i, p) in m.processes.iter().enumerate() {
            let comma = if i > 0 { "," } else { "" };
            let _ = write!(
                out,
                r#"{comma}{{"pid":{},"name":{},"rss_bytes":{},"pss_bytes":{}}}"#,
                p.pid,
                quote(&p.name),
                p.rss_bytes,
                p.pss_bytes
            );
        }
        out.push_str("]}");
    }
    out.push_str(r#","actions":["#);
    for (i, (title, took)) in report.actions.iter().enumerate() {
        let comma = if i > 0 { "," } else { "" };
        let _ = write!(
            out,
            r#"{comma}{{"title":{},"ms":{}}}"#,
            quote(title),
            ms(took)
        );
    }
    out.push_str("]}");
    out
}

/// The script the web view runs for one step: opens the palette, types the action title,
/// presses Enter, and sets the document title to `clew-probe:done:<step>` after the second
/// painted frame. A failure sets `clew-probe:error:...` instead.
// @implements CT-EFF-003
pub fn step_script(title: &str, step: usize) -> String {
    step_script_for(
        &Step {
            title: "",
            action: title,
            drive: "",
            done: "true",
        },
        step,
    )
}

/// The script for a `Step`: its action through the palette (if any), its drive, then a wait
/// for its `done` test, then the title change after the second painted frame.
// @implements CT-EFF-003, CT-EFF-005, CT-EFF-006
pub fn step_script_for(step: &Step, index: usize) -> String {
    let action = if step.action.is_empty() {
        String::new()
    } else {
        let title = quote(step.action);
        format!(
            r#"window.dispatchEvent(new KeyboardEvent('keydown', {{ key: 'p', ctrlKey: true, shiftKey: true, bubbles: true }}));
    await turn();
    const input = document.querySelector('[role=combobox]');
    input.value = {title};
    input.dispatchEvent(new Event('input', {{ bubbles: true }}));
    await turn();
    input.closest('[role=dialog]').dispatchEvent(new KeyboardEvent('keydown', {{ key: 'Enter', bubbles: true }}));
    await turn();"#
        )
    };
    let (drive, done) = (step.drive, step.done);
    format!(
        r#"(async () => {{
  try {{
    const turn = () => new Promise((resolve) => setTimeout(resolve, 0));
    const waitFor = (test) => new Promise((resolve) => {{
      if (test()) {{ resolve(); return; }}
      const observer = new MutationObserver(() => {{ if (test()) {{ observer.disconnect(); resolve(); }} }});
      observer.observe(document.body, {{ childList: true, subtree: true, characterData: true, attributes: true }});
    }});
    const probe = (window.__clewProbe = window.__clewProbe || {{}});
    {action}
    {drive}
    await waitFor(() => {done});
    requestAnimationFrame(() => requestAnimationFrame(() => {{ document.title = 'clew-probe:done:{index}'; }}));
  }} catch (e) {{
    document.title = 'clew-probe:error:' + e;
  }}
}})();"#
    )
}

/// How long the window sits idle before memory is read, so loading and first layout are done.
// @implements CT-EFF-002
const IDLE_SETTLE: Duration = Duration::from_secs(5);

/// A probe that has not finished by then failed: the page never loaded or a script threw.
// @implements CT-EFF-002, CT-EFF-003
const WATCHDOG: Duration = Duration::from_secs(60);

/// Every process this OS lets us read, with its parent and resident memory. Only Linux is
/// read here; Windows and macOS memory comes from the CI workflow, so this is empty there.
// @implements CT-EFF-002
#[cfg(target_os = "linux")]
fn read_processes() -> Vec<Proc> {
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    dir.filter_map(|entry| {
        let pid: u32 = entry.ok()?.file_name().to_str()?.parse().ok()?;
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
        let (ppid, name) = parse_stat(&stat)?;
        // Unreadable for other users' processes, which are never the app's own.
        let pss = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup"))
            .ok()
            .and_then(|t| parse_pss_bytes(&t));
        Some(Proc {
            pid,
            ppid,
            name,
            rss_bytes: parse_rss_bytes(&status)?,
            pss_bytes: pss.unwrap_or(0),
        })
    })
    .collect()
}

// @implements CT-EFF-002
#[cfg(not(target_os = "linux"))]
fn read_processes() -> Vec<Proc> {
    Vec::new()
}

/// How the app was asked to probe itself, from `CLEW_PROBE`.
// @implements CT-EFF-002, CT-EFF-003, CT-EFF-004
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// Report start time and idle memory.
    Idle,
    /// Also run `SCRIPT` and report the time of each action.
    Script,
    /// Open the project given on the command line, and report the time to its first tree
    /// row on screen and the memory then (CT-EFF-004).
    Project,
    /// Open the project given on the command line, then run `VIEW_STEPS` and report the time
    /// of each (CT-EFF-005, CT-EFF-006).
    Views,
}

// @implements CT-EFF-004, CT-EFF-005
impl Mode {
    /// Whether the mode waits for the project's first tree row before it is ready.
    fn opens_project(self) -> bool {
        matches!(self, Mode::Project | Mode::Views)
    }
}

// @implements CT-EFF-002, CT-EFF-003, CT-EFF-004
fn parse_mode(value: Option<&str>) -> Option<Mode> {
    match value {
        Some("idle") => Some(Mode::Idle),
        Some("script") => Some(Mode::Script),
        Some("project") => Some(Mode::Project),
        Some("views") => Some(Mode::Views),
        _ => None,
    }
}

// @implements CT-EFF-002, CT-EFF-003, CT-EFF-004
fn mode() -> Option<Mode> {
    parse_mode(std::env::var("CLEW_PROBE").ok().as_deref())
}

/// The script the web view runs when the page has loaded. It sets the document title to
/// `clew-probe:ready` after the second painted frame. With a project, it first waits for
/// the model browser to hold a row, so the time covers loading the project and drawing it.
// @implements CT-EFF-003, CT-EFF-004
pub fn ready_script(project: bool) -> String {
    let frames = "requestAnimationFrame(() => requestAnimationFrame(() => { document.title = 'clew-probe:ready'; }));";
    if !project {
        return frames.to_string();
    }
    format!(
        r#"(() => {{
  const shown = () => {{
    const row = document.querySelector('#pane-browser [role=option]');
    return row !== null && !row.textContent.includes('Loading');
  }};
  if (shown()) {{ {frames} return; }}
  new MutationObserver((_, observer) => {{
    if (shown()) {{ observer.disconnect(); {frames} }}
  }}).observe(document.body, {{ childList: true, subtree: true, characterData: true }});
}})();"#
    )
}

/// What has been measured so far.
// @implements CT-EFF-002, CT-EFF-003
struct Run {
    started: Instant,
    interactive: Option<Duration>,
    memory: Option<Memory>,
    step: usize,
    step_started: Instant,
    actions: Vec<(String, Duration)>,
    /// The title and script of each step this mode runs.
    steps: Vec<(String, String)>,
}

/// With a probe requested, the window is created in `start` instead of from the config, so
/// the title and page-load hooks can be attached to it.
// @implements CT-EFF-003
pub fn prepare(context: &mut tauri::Context<tauri::Wry>) {
    if mode().is_some() {
        for window in &mut context.config_mut().app.windows {
            window.create = false;
        }
    }
}

/// The titles and scripts of the steps a mode runs after the window is ready: none for
/// `idle` and `project`.
// @implements CT-EFF-003, CT-EFF-005, CT-EFF-006
fn steps_of(mode: Mode) -> Vec<(String, String)> {
    match mode {
        Mode::Script => (SCRIPT.iter().enumerate())
            .map(|(i, title)| (title.to_string(), step_script(title, i)))
            .collect(),
        Mode::Views => (VIEW_STEPS.iter().enumerate())
            .map(|(i, step)| (step.title.to_string(), step_script_for(step, i)))
            .collect(),
        Mode::Idle | Mode::Project => Vec::new(),
    }
}

/// Opens the window with the probe hooks when a probe was requested; otherwise does nothing,
/// since the window came from the config.
// @implements CT-EFF-002, CT-EFF-003, CT-EFF-004
pub fn start(app: &tauri::App, started: Instant) -> Result<(), Box<dyn std::error::Error>> {
    let Some(mode) = mode() else {
        return Ok(());
    };
    let config = app.config().app.windows[0].clone();
    let run = Arc::new(Mutex::new(Run {
        started,
        interactive: None,
        memory: None,
        step: 0,
        step_started: started,
        actions: Vec::new(),
        steps: steps_of(mode),
    }));
    let finished = Arc::new(AtomicBool::new(false));

    let on_title = {
        let run = Arc::clone(&run);
        let finished = Arc::clone(&finished);
        move |window: tauri::WebviewWindow, title: String| {
            let Some(event) = title.strip_prefix("clew-probe:") else {
                return;
            };
            on_event(&run, &finished, &window, event);
        }
    };
    let on_load = move |window: tauri::WebviewWindow,
                        payload: tauri::webview::PageLoadPayload<'_>| {
        if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
            // Ready when the second frame after the load has been painted (with a project,
            // after its first tree row is on screen).
            let _ = window.eval(ready_script(mode.opens_project()));
        }
    };
    tauri::WebviewWindowBuilder::from_config(app.handle(), &config)?
        .on_document_title_changed(on_title)
        .on_page_load(on_load)
        .build()?;

    let handle = app.handle().clone();
    let watched = Arc::clone(&run);
    std::thread::Builder::new()
        .name("clew-probe-watchdog".into())
        .spawn(move || {
            std::thread::sleep(WATCHDOG);
            if !finished.load(Ordering::SeqCst) {
                let guard = watched.lock().unwrap_or_else(PoisonError::into_inner);
                let waiting = guard
                    .steps
                    .get(guard.step)
                    .map_or("the page", |s| s.0.as_str());
                eprintln!("probe failed: no result within {WATCHDOG:?}, waiting on `{waiting}`");
                handle.exit(1);
            }
        })?;
    Ok(())
}

// @implements CT-EFF-002, CT-EFF-003
fn on_event(
    run: &Arc<Mutex<Run>>,
    finished: &Arc<AtomicBool>,
    window: &tauri::WebviewWindow,
    event: &str,
) {
    let mut guard = run.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(error) = event.strip_prefix("error:") {
        eprintln!("probe failed in the web view: {error}");
        window.app_handle().exit(1);
    } else if event == "ready" {
        guard.interactive = Some(guard.started.elapsed());
        let (run, window) = (Arc::clone(run), window.clone());
        let finished = Arc::clone(finished);
        // Idle for a while, then read memory. A timer thread, since the window has nothing to
        // wait for.
        let _ = std::thread::Builder::new()
            .name("clew-probe-idle".into())
            .spawn(move || {
                std::thread::sleep(IDLE_SETTLE);
                let mut guard = run.lock().unwrap_or_else(PoisonError::into_inner);
                guard.memory = memory_of(&read_processes(), std::process::id());
                if !guard.steps.is_empty() {
                    run_step(&mut guard, &window);
                } else {
                    finish(&guard, &finished, &window);
                }
            });
    } else if event.starts_with("done:") {
        let step = guard.step;
        let took = guard.step_started.elapsed();
        let title = guard.steps[step].0.clone();
        guard.actions.push((title, took));
        guard.step += 1;
        if guard.step < guard.steps.len() {
            run_step(&mut guard, window);
        } else {
            finish(&guard, finished, window);
        }
    }
}

// @implements CT-EFF-003
fn run_step(run: &mut Run, window: &tauri::WebviewWindow) {
    run.step_started = Instant::now();
    let _ = window.eval(run.steps[run.step].1.clone());
}

// @implements CT-EFF-002, CT-EFF-003
fn finish(run: &Run, finished: &AtomicBool, window: &tauri::WebviewWindow) {
    let report = Report {
        interactive: run.interactive.unwrap_or_default(),
        memory: run.memory.clone(),
        actions: run.actions.clone(),
    };
    println!("{}", report_json(&report));
    finished.store(true, Ordering::SeqCst);
    window.app_handle().exit(0);
}

// @verifies CT-EFF-002, CT-EFF-003, CT-EFF-004, CT-EFF-010
#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, ppid: u32, name: &str, kb: u64) -> Proc {
        Proc {
            pid,
            ppid,
            name: name.into(),
            rss_bytes: kb * 1024,
            pss_bytes: kb * 512,
        }
    }

    // @verifies CT-EFF-004
    #[test]
    fn the_project_ready_script_waits_for_the_first_tree_row_to_be_painted() {
        let script = ready_script(true);
        assert!(script.contains("#pane-browser [role=option]"));
        assert!(script.contains("MutationObserver"));
        assert!(script.contains("clew-probe:ready"));
        // Without a project the window is ready two frames after the page loads.
        let plain = ready_script(false);
        assert!(!plain.contains("MutationObserver"));
        assert!(plain.contains("clew-probe:ready"));
    }

    // @verifies CT-EFF-004
    #[test]
    fn the_project_mode_is_named_by_the_environment() {
        assert_eq!(parse_mode(Some("project")), Some(Mode::Project));
        assert_eq!(parse_mode(Some("idle")), Some(Mode::Idle));
        assert_eq!(parse_mode(Some("other")), None);
        assert_eq!(parse_mode(None), None);
    }

    // @verifies CT-EFF-002
    #[test]
    fn smaps_rollup_gives_proportional_memory_in_bytes() {
        let text = "00400000-ff 0 [rollup]\nRss:\t 2000 kB\nPss:\t  800 kB\nPss_Anon:\t 1 kB\n";
        assert_eq!(parse_pss_bytes(text), Some(800 * 1024));
        assert_eq!(parse_pss_bytes("Rss: 5 kB\n"), None);
    }

    // @verifies CT-EFF-002
    #[test]
    fn stat_gives_the_parent_and_a_name_with_spaces_and_parentheses() {
        assert_eq!(
            parse_stat("42 (clewtrace) S 7 42 42 0 -1 4194560"),
            Some((7, "clewtrace".into()))
        );
        assert_eq!(
            parse_stat("43 (Web (Content) x) R 42 43 43 0 -1 0"),
            Some((42, "Web (Content) x".into()))
        );
        assert_eq!(parse_stat("garbage"), None);
        assert_eq!(parse_stat("1 (a) S notanumber"), None);
    }

    // @verifies CT-EFF-002
    #[test]
    fn status_gives_resident_memory_in_bytes() {
        let text = "Name:\tx\nVmPeak:\t 9 kB\nVmRSS:\t  1234 kB\nThreads:\t3\n";
        assert_eq!(parse_rss_bytes(text), Some(1234 * 1024));
        assert_eq!(parse_rss_bytes("Name:\tkernel\n"), None);
    }

    // @verifies CT-EFF-002
    #[test]
    fn descendants_follow_the_tree_to_any_depth_and_skip_other_trees() {
        let procs = [
            proc(1, 0, "init", 10),
            proc(10, 1, "clewtrace", 100),
            proc(11, 10, "bwrap", 5),
            proc(12, 11, "WebKitWebProcess", 300),
            proc(20, 1, "other", 999),
        ];
        let ids: Vec<u32> = descendants(&procs, 10).iter().map(|p| p.pid).collect();
        assert_eq!(ids, [10, 11, 12]);
    }

    // @verifies CT-EFF-002
    #[test]
    fn memory_splits_the_app_from_its_web_view() {
        let procs = [
            proc(10, 1, "clewtrace", 100),
            proc(12, 10, "WebKitWebProcess", 300),
            proc(13, 10, "WebKitNetworkProcess", 50),
            proc(20, 1, "other", 999),
        ];
        let memory = memory_of(&procs, 10).expect("the app is in the table");
        assert_eq!(memory.app_bytes, 100 * 1024);
        assert_eq!(memory.webview_bytes, 350 * 1024);
        assert_eq!(memory.pss_total_bytes, 450 * 512);
        assert_eq!(memory.app_pss_bytes, 100 * 512);
        assert_eq!(memory.processes.len(), 3);
        assert_eq!(memory_of(&procs, 99), None);
    }

    // @verifies CT-EFF-002, CT-EFF-003
    #[test]
    fn the_report_is_one_line_of_json_with_the_numbers_the_ci_gate_reads() {
        let report = Report {
            interactive: Duration::from_millis(812),
            memory: Some(memory_of(&[proc(10, 1, "clewtrace", 1)], 10).unwrap()),
            actions: vec![("Theme: dark".into(), Duration::from_micros(12_340))],
        };
        let text = report_json(&report);
        assert!(!text.contains('\n'));
        let json = clew_core::json::parse(&text).expect("the report is valid JSON");
        let ms = |key: &str| match json.get(key) {
            Some(clew_core::json::Json::Num(n)) => n.clone(),
            other => panic!("{key} is {other:?}"),
        };
        assert_eq!(ms("time_to_interactive_ms"), "812.0");
        let memory = json.get("idle_memory").expect("idle_memory");
        assert!(memory.get("total_bytes").is_some());
        assert!(memory.get("pss_total_bytes").is_some());
        assert!(memory.get("app_pss_bytes").is_some());
        assert_eq!(json.get("actions").unwrap().items().len(), 1);
    }

    // @verifies CT-EFF-003
    #[test]
    fn a_report_without_memory_still_has_the_timings() {
        let report = Report {
            interactive: Duration::from_millis(5),
            memory: None,
            actions: vec![],
        };
        let json = clew_core::json::parse(&report_json(&report)).unwrap();
        assert!(json.get("time_to_interactive_ms").is_some());
        assert!(json.get("idle_memory").is_none());
    }

    // @verifies CT-EFF-003
    #[test]
    fn the_step_script_quotes_the_title_and_ends_on_a_painted_frame() {
        let script = step_script("He said \"hi\"", 3);
        assert!(script.contains(r#""He said \"hi\"""#), "{script}");
        assert!(script.contains("clew-probe:done:3"));
        assert!(script.contains("clew-probe:error:"));
        assert_eq!(script.matches("requestAnimationFrame").count(), 2);
    }

    // @verifies CT-EFF-005, CT-EFF-006
    #[test]
    fn the_views_mode_is_named_by_the_environment_and_opens_a_project() {
        assert_eq!(parse_mode(Some("views")), Some(Mode::Views));
        assert!(Mode::Views.opens_project());
        assert!(Mode::Project.opens_project());
        assert!(!Mode::Script.opens_project());
    }

    // @verifies CT-EFF-005, CT-EFF-006
    #[test]
    fn a_view_step_runs_its_action_then_its_drive_then_waits_for_its_done_test() {
        let step = Step {
            title: "Search: \"x\"",
            action: "Search the model",
            drive: "probe.ran = 1;",
            done: "probe.ran === 1",
        };
        let script = step_script_for(&step, 4);
        let at = |needle: &str| script.find(needle).unwrap_or_else(|| panic!("no {needle}"));
        assert!(at("\"Search the model\"") < at("probe.ran = 1;"));
        assert!(at("probe.ran = 1;") < at("probe.ran === 1"));
        assert!(at("probe.ran === 1") < at("clew-probe:done:4"));
        assert_eq!(script.matches("requestAnimationFrame").count(), 2);
        assert!(script.contains("clew-probe:error:"));
        // No action: the palette is not opened.
        let plain = step_script_for(&Step { action: "", ..step }, 0);
        assert!(!plain.contains("role=combobox"));
    }

    // @verifies CT-EFF-005, CT-EFF-006
    #[test]
    fn the_view_steps_filter_search_select_and_navigate() {
        let titles: Vec<&str> = VIEW_STEPS.iter().map(|s| s.title).collect();
        assert!(
            titles.iter().any(|t| t.starts_with("Coverage: ")),
            "{titles:?}"
        );
        assert!(
            titles.iter().any(|t| t.starts_with("Search: ")),
            "{titles:?}"
        );
        assert!(
            titles.iter().any(|t| t.starts_with("Select: ")),
            "{titles:?}"
        );
        assert!(
            titles.iter().any(|t| t.starts_with("Navigate: ")),
            "{titles:?}"
        );
    }

    // @verifies CT-EFF-005, CT-EFF-006
    #[test]
    fn every_view_step_action_is_a_registered_palette_title() {
        let app = include_str!("../../src/App.svelte");
        let filters = include_str!("../../src/lib/find.ts");
        for step in VIEW_STEPS.iter().filter(|s| !s.action.is_empty()) {
            // `Coverage: <filter>` actions are built from the filter labels in find.ts.
            let registered = match step.action.strip_prefix("Coverage: ") {
                Some(filter) => {
                    app.contains("title: `Coverage: ${f.label.toLowerCase()}`")
                        && filters
                            .lines()
                            .any(|l| l.to_lowercase().contains(&format!("label: '{filter}'")))
                }
                None => app.contains(&format!("title: '{}'", step.action)),
            };
            assert!(
                registered,
                "App.svelte registers no action titled {}",
                step.action
            );
        }
    }

    // @verifies CT-EFF-003
    #[test]
    fn every_scripted_action_is_a_registered_palette_title() {
        let app = include_str!("../../src/App.svelte");
        for title in SCRIPT {
            // `Go to <pane>` actions are built from the pane titles.
            let wanted = match title.strip_prefix("Go to ") {
                Some(pane) => format!("'{pane}'"),
                None => format!("title: '{title}'"),
            };
            assert!(
                app.contains(&wanted),
                "App.svelte registers no action titled {title}"
            );
        }
    }
}
