//! The app's own performance budgets, measured by the app (src/probe.rs): time to an
//! interactive window (CT-EFF-003) and idle memory with an empty project (CT-EFF-002).
//! Needs a display, so CI runs it under `xvfb-run` inside the systemd-run limits:
//! `cargo test --release -p clewtrace --features perf-probe --test perf_probe -- --ignored --nocapture`
//! Windows and macOS memory is measured by a CI workflow step instead (ci.yml).

use std::process::Command;

use clew_core::json::{Json, parse};

/// CT-EFF-003: the median time to an interactive window is within 1.5 s on the CI runner.
// @verifies CT-EFF-003
const MAX_INTERACTIVE_MS: f64 = 1500.0;

/// CT-EFF-003: launches that are run and not counted, so the cold start (first read of the
/// binary and the web view libraries from disk) does not decide the result. The cold start is
/// held to CT-EFF-011 on the reference machine instead.
// @verifies CT-EFF-003
const WARMUP_LAUNCHES: usize = 1;

/// CT-EFF-003: launches whose median time is checked against the budget.
// @verifies CT-EFF-003
const MEASURED_LAUNCHES: usize = 3;

/// CT-EFF-002 on Linux: total proportional set size (PSS) of the app and its web view
/// processes when idle with an empty project. Windows and macOS are checked by ci.yml.
// @verifies CT-EFF-002
const MAX_TOTAL_PSS_BYTES: u64 = 216 * 1_000_000;

/// CT-EFF-010 on Linux: the app's own process, in PSS.
// @verifies CT-EFF-010
const MAX_APP_PSS_BYTES: u64 = 93 * 1_000_000;

/// The number under `key`, or a message saying what the report held.
// @verifies CT-EFF-002, CT-EFF-003
fn number(json: &Json, key: &str) -> Result<f64, String> {
    match json.get(key) {
        Some(Json::Num(text)) => text.parse().map_err(|_| format!("{key} is `{text}`")),
        _ => Err(format!("the report has no number `{key}`")),
    }
}

/// The median of the values, or `None` for none. An even count gives the mean of the middle two.
// @verifies CT-EFF-003
fn median(values: &[f64]) -> Option<f64> {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    match sorted.len() {
        0 => None,
        n if n % 2 == 1 => Some(sorted[mid]),
        _ => Some((sorted[mid - 1] + sorted[mid]) / 2.0),
    }
}

/// What is wrong with the median time to an interactive window, empty when it meets the budget.
// @verifies CT-EFF-003
fn start_failures(median_ms: f64) -> Vec<String> {
    if median_ms > MAX_INTERACTIVE_MS {
        vec![format!(
            "CT-EFF-003: the median time to an interactive window was {median_ms} ms, over the {MAX_INTERACTIVE_MS} ms budget"
        )]
    } else {
        Vec::new()
    }
}

/// What is wrong with the idle memory in a probe report, empty when it meets the budgets.
// @verifies CT-EFF-002, CT-EFF-010
fn memory_failures(report: &str) -> Vec<String> {
    let json = match parse(report) {
        Ok(json) => json,
        Err(e) => return vec![format!("the report is not JSON: {e:?}")],
    };
    let memory = json.get("idle_memory");
    let limits = [
        (
            "pss_total_bytes",
            MAX_TOTAL_PSS_BYTES,
            "CT-EFF-002",
            "idle PSS of the app and web view",
        ),
        (
            "app_pss_bytes",
            MAX_APP_PSS_BYTES,
            "CT-EFF-010",
            "idle PSS of the app process",
        ),
    ];
    let mut failures = Vec::new();
    for (key, max, id, what) in limits {
        match memory.map(|m| number(m, key)) {
            Some(Ok(bytes)) if bytes > max as f64 => failures.push(format!(
                "{id}: {what} was {:.0} MB, over the {} MB budget",
                bytes / 1e6,
                max / 1_000_000
            )),
            Some(Ok(_)) => {}
            Some(Err(e)) => failures.push(e),
            None => failures.push("the report has no idle_memory".into()),
        }
    }
    failures
}

// @verifies CT-EFF-003
#[test]
fn the_median_ignores_one_slow_launch_and_handles_any_count() {
    assert_eq!(median(&[]), None);
    assert_eq!(median(&[5.0]), Some(5.0));
    assert_eq!(median(&[900.0, 3800.0, 700.0]), Some(900.0));
    assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), Some(2.5));
}

// @verifies CT-EFF-003
#[test]
fn the_start_check_passes_on_the_budget_and_fails_one_unit_over() {
    assert!(start_failures(1500.0).is_empty());
    let failures = start_failures(1500.1);
    assert_eq!(failures.len(), 1);
    assert!(failures[0].starts_with("CT-EFF-003"));
    assert_eq!((WARMUP_LAUNCHES, MEASURED_LAUNCHES), (1, 3));
}

// @verifies CT-EFF-002, CT-EFF-010
#[test]
fn the_memory_check_passes_within_budget_and_names_each_requirement_over_it() {
    let report = |total: u64, app: u64| {
        format!(
            r#"{{"time_to_interactive_ms":1.0,"idle_memory":{{"pss_total_bytes":{total},"app_pss_bytes":{app}}}}}"#
        )
    };
    // Exactly on every budget passes.
    assert!(memory_failures(&report(216_000_000, 93_000_000)).is_empty());
    // One unit over each budget fails, and each failure names its requirement.
    let failures = memory_failures(&report(216_000_001, 93_000_001));
    assert_eq!(failures.len(), 2, "{failures:?}");
    assert!(failures[0].starts_with("CT-EFF-002"));
    assert!(failures[1].starts_with("CT-EFF-010"));
    assert_eq!(memory_failures("not json").len(), 1);
    assert_eq!(memory_failures("{}").len(), 2);
}

/// Launches the app once with the probe script and returns its one-line report.
// @verifies CT-EFF-002, CT-EFF-003
fn launch() -> String {
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
    report.to_string()
}

// @verifies CT-EFF-002, CT-EFF-003, CT-EFF-010
#[test]
#[ignore = "needs a display and the perf CI limits: run under xvfb-run, see the module docs"]
fn the_app_meets_its_start_and_idle_memory_budgets() {
    for _ in 0..WARMUP_LAUNCHES {
        println!("warm-up launch, not counted:");
        launch();
    }
    let mut times = Vec::new();
    let mut failures = Vec::new();
    for _ in 0..MEASURED_LAUNCHES {
        let report = launch();
        let json = parse(&report).expect("the report is JSON");
        match number(&json, "time_to_interactive_ms") {
            Ok(ms) => times.push(ms),
            Err(e) => failures.push(e),
        }
        // Windows and macOS memory is checked by ci.yml.
        if cfg!(target_os = "linux") {
            failures.extend(memory_failures(&report));
        }
    }
    let median_ms = median(&times).expect("a measured launch reported a time");
    println!("median time to interactive over {MEASURED_LAUNCHES} launches: {median_ms} ms");
    failures.extend(start_failures(median_ms));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
