//! The app's own performance budgets, measured by the app (src/probe.rs): time to an
//! interactive window (CT-EFF-003) and idle memory with an empty project (CT-EFF-002).
//! Needs a display, so CI runs it under `xvfb-run` inside the systemd-run limits:
//! `cargo test --release -p clewtrace --features perf-probe --test perf_probe -- --ignored --nocapture`
//! Windows and macOS memory is measured by a CI workflow step instead (ci.yml).

use std::process::Command;

use clew_core::json::{Json, parse};

/// CT-EFF-003: the main window is interactive within 2 s of launch.
// @verifies CT-EFF-002, CT-EFF-003
const MAX_INTERACTIVE_MS: f64 = 2000.0;

/// CT-EFF-002: resident memory when idle with an empty project, app and web view together.
/// 250 MB on Linux (200 MB on Windows and macOS, checked by ci.yml).
// @verifies CT-EFF-002, CT-EFF-003
const MAX_IDLE_BYTES: u64 = 250 * 1_000_000;

/// The number under `key`, or a message saying what the report held.
// @verifies CT-EFF-002, CT-EFF-003
fn number(json: &Json, key: &str) -> Result<f64, String> {
    match json.get(key) {
        Some(Json::Num(text)) => text.parse().map_err(|_| format!("{key} is `{text}`")),
        _ => Err(format!("the report has no number `{key}`")),
    }
}

/// What is wrong with a probe report, empty when it meets the budgets.
// @verifies CT-EFF-002, CT-EFF-003
fn budget_failures(report: &str, check_memory: bool) -> Vec<String> {
    let json = match parse(report) {
        Ok(json) => json,
        Err(e) => return vec![format!("the report is not JSON: {e:?}")],
    };
    let mut failures = Vec::new();
    match number(&json, "time_to_interactive_ms") {
        Ok(ms) if ms > MAX_INTERACTIVE_MS => failures.push(format!(
            "CT-EFF-003: the window was interactive after {ms} ms, over the {MAX_INTERACTIVE_MS} ms budget"
        )),
        Ok(_) => {}
        Err(e) => failures.push(e),
    }
    if check_memory {
        match json.get("idle_memory").map(|m| number(m, "total_bytes")) {
            Some(Ok(bytes)) if bytes > MAX_IDLE_BYTES as f64 => failures.push(format!(
                "CT-EFF-002: idle resident memory was {:.0} MB, over the {} MB budget",
                bytes / 1e6,
                MAX_IDLE_BYTES / 1_000_000
            )),
            Some(Ok(_)) => {}
            Some(Err(e)) => failures.push(e),
            None => failures.push("the report has no idle_memory".into()),
        }
    }
    failures
}

// @verifies CT-EFF-002, CT-EFF-003
#[test]
fn the_budget_check_passes_within_budget_and_names_each_requirement_over_it() {
    let ok = r#"{"time_to_interactive_ms":900.0,"idle_memory":{"total_bytes":200000000}}"#;
    assert!(budget_failures(ok, true).is_empty());
    let slow = r#"{"time_to_interactive_ms":2000.1,"idle_memory":{"total_bytes":250000001}}"#;
    let failures = budget_failures(slow, true);
    assert_eq!(failures.len(), 2, "{failures:?}");
    assert!(failures[0].starts_with("CT-EFF-003"));
    assert!(failures[1].starts_with("CT-EFF-002"));
    assert_eq!(budget_failures(slow, false).len(), 1);
    assert_eq!(budget_failures("not json", true).len(), 1);
    assert_eq!(budget_failures("{}", true).len(), 2);
}

// @verifies CT-EFF-002, CT-EFF-003
#[test]
#[ignore = "needs a display and the perf CI limits: run under xvfb-run, see the module docs"]
fn the_app_meets_its_start_and_idle_memory_budgets() {
    let output = Command::new(env!("CARGO_BIN_EXE_clewtrace"))
        .env("CLEW_PROBE", "script")
        .output()
        .expect("the app starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the probe failed ({}). Is a display set? Run under xvfb-run.\n{stderr}",
        output.status
    );
    let report = stdout.lines().last().expect("the probe prints a report");
    println!("{report}");
    let json = parse(report).expect("the report is JSON");
    let actions = json.get("actions").map(Json::items).unwrap_or_default();
    assert_eq!(actions.len(), 5, "the scripted actions did not all run");
    let failures = budget_failures(report, cfg!(target_os = "linux"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
