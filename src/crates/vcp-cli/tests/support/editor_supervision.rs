// SPDX-License-Identifier: Apache-2.0
//! Shared bounded final-artifact editor runner; preserves the native idle grace.
use codex_utils_pty::JobObject;
use std::{
    fs,
    path::Path,
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::process::Command;
pub async fn editor_observation(
    command: &mut Command,
    private: &Path,
    fixture_root: &Path,
    mode: &str,
) -> serde_json::Value {
    let stdout = private.join(format!("{mode}-runner.stdout"));
    let stderr = private.join(format!("{mode}-runner.stderr"));
    command
        .stdin(Stdio::null())
        .stdout(fs::File::create(&stdout).unwrap())
        .stderr(fs::File::create(&stderr).unwrap());
    let job = JobObject::create_without_breakaway().unwrap();
    let mut child = crate::hidden_process::spawn(&job, command)
        .await
        .unwrap_or_else(|error| {
            panic!(
                "Editor startup refused: {error}; private evidence {}; fixture {}",
                private.display(),
                fixture_root.display()
            )
        });
    let began = Instant::now();
    let output_bytes =
        || fs::metadata(&stdout).unwrap().len() + fs::metadata(&stderr).unwrap().len();
    let mut exceeded = false;
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if began.elapsed() >= Duration::from_secs(600) || output_bytes() > 4 * 1024 * 1024 {
            exceeded = true;
            break None;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    // The local server deliberately survives its initiating bridge, but does
    // not request job breakaway (windows_launch.rs). Preserve its normal
    // 30-second observer idle grace before requiring the entire tree to stop.
    let grace = Instant::now();
    while !exceeded
        && job.active_process_count().unwrap() != 0
        && grace.elapsed() < Duration::from_secs(45)
    {
        if output_bytes() > 4 * 1024 * 1024 {
            exceeded = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let forced = job.active_process_count().unwrap() != 0;
    if forced {
        job.terminate().unwrap();
    }
    let cleanup = Instant::now();
    while job.active_process_count().unwrap() != 0 && cleanup.elapsed() < Duration::from_secs(10) {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let stopped = job.active_process_count().unwrap() == 0;
    let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
    exceeded |= output_bytes() > 4 * 1024 * 1024;
    let passed = status.is_some_and(|status| status.success())
        && stopped
        && !forced
        && !exceeded
        && output_bytes() <= 4 * 1024 * 1024;
    let supervision = serde_json::json!({
        "schema": "vcp-editor-process-observation/1", "status": if passed { "pass" } else { "fail" },
        "mode": mode, "runner_deadline_seconds": 600, "owner_idle_grace_seconds": 45,
        "runner_wall_ms": began.elapsed().as_millis(), "exit_code": status.and_then(|status| status.code()),
        "output_limit_bytes": 4 * 1024 * 1024, "output_bytes": output_bytes(),
        "job_active_processes_zero": stopped, "forced_descendant_cleanup": forced, "limit_exceeded": exceeded
    });
    fs::write(
        private.join(format!("{mode}-supervision.json")),
        serde_json::to_vec_pretty(&supervision).unwrap(),
    )
    .unwrap();
    assert!(passed, "Editor {mode} failed or did not drain its complete process tree; registered installation/logs retained at {}; synthetic inputs at {}", private.display(), fixture_root.display());
    let mut report: serde_json::Value = serde_json::from_slice(&fs::read(&stdout).unwrap())
        .unwrap_or_else(|error| {
            panic!(
                "Editor {mode} returned invalid JSON: {error}; private stdout at {}",
                stdout.display()
            )
        });
    report["process_tree"] = supervision;
    report
}
