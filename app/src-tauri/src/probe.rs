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
    let title = quote(title);
    format!(
        r#"(async () => {{
  try {{
    const turn = () => new Promise((resolve) => setTimeout(resolve, 0));
    window.dispatchEvent(new KeyboardEvent('keydown', {{ key: 'p', ctrlKey: true, shiftKey: true, bubbles: true }}));
    await turn();
    const input = document.querySelector('[role=combobox]');
    input.value = {title};
    input.dispatchEvent(new Event('input', {{ bubbles: true }}));
    await turn();
    input.closest('[role=dialog]').dispatchEvent(new KeyboardEvent('keydown', {{ key: 'Enter', bubbles: true }}));
    requestAnimationFrame(() => requestAnimationFrame(() => {{ document.title = 'clew-probe:done:{step}'; }}));
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
// @implements CT-EFF-002, CT-EFF-003
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Report start time and idle memory.
    Idle,
    /// Also run `SCRIPT` and report the time of each action.
    Script,
}

// @implements CT-EFF-002, CT-EFF-003
fn mode() -> Option<Mode> {
    match std::env::var("CLEW_PROBE").as_deref() {
        Ok("idle") => Some(Mode::Idle),
        Ok("script") => Some(Mode::Script),
        _ => None,
    }
}

/// What has been measured so far.
// @implements CT-EFF-002, CT-EFF-003
struct Run {
    mode: Mode,
    started: Instant,
    interactive: Option<Duration>,
    memory: Option<Memory>,
    step: usize,
    step_started: Instant,
    actions: Vec<(String, Duration)>,
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

/// Opens the window with the probe hooks when a probe was requested; otherwise does nothing,
/// since the window came from the config.
// @implements CT-EFF-002, CT-EFF-003
pub fn start(app: &tauri::App, started: Instant) -> Result<(), Box<dyn std::error::Error>> {
    let Some(mode) = mode() else {
        return Ok(());
    };
    let config = app.config().app.windows[0].clone();
    let run = Arc::new(Mutex::new(Run {
        mode,
        started,
        interactive: None,
        memory: None,
        step: 0,
        step_started: started,
        actions: Vec::new(),
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
    let on_load = |window: tauri::WebviewWindow, payload: tauri::webview::PageLoadPayload<'_>| {
        if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
            // Ready when the second frame after the load has been painted.
            let _ = window.eval(
                "requestAnimationFrame(() => requestAnimationFrame(() => { document.title = 'clew-probe:ready'; }));",
            );
        }
    };
    tauri::WebviewWindowBuilder::from_config(app.handle(), &config)?
        .on_document_title_changed(on_title)
        .on_page_load(on_load)
        .build()?;

    let handle = app.handle().clone();
    std::thread::Builder::new()
        .name("clew-probe-watchdog".into())
        .spawn(move || {
            std::thread::sleep(WATCHDOG);
            if !finished.load(Ordering::SeqCst) {
                eprintln!("probe failed: no result within {WATCHDOG:?}");
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
                if guard.mode == Mode::Script {
                    run_step(&mut guard, &window);
                } else {
                    finish(&guard, &finished, &window);
                }
            });
    } else if event.starts_with("done:") {
        let step = guard.step;
        let took = guard.step_started.elapsed();
        guard.actions.push((SCRIPT[step].to_string(), took));
        guard.step += 1;
        if guard.step < SCRIPT.len() {
            run_step(&mut guard, window);
        } else {
            finish(&guard, finished, window);
        }
    }
}

// @implements CT-EFF-003
fn run_step(run: &mut Run, window: &tauri::WebviewWindow) {
    run.step_started = Instant::now();
    let _ = window.eval(step_script(SCRIPT[run.step], run.step));
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

// @verifies CT-EFF-002, CT-EFF-003, CT-EFF-010
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
