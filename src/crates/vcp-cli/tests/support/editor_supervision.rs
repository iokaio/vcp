// SPDX-License-Identifier: Apache-2.0
//! Shared bounded final-artifact editor runner; preserves the native idle grace.
use codex_utils_pty::JobObject;
use std::{
    fs,
    io::Read,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::process::Command;

const DIAGNOSTIC_BYTES: usize = 64 * 1024;

fn ordinary_diagnostic_path(path: &Path) -> bool {
    path.is_absolute()
        && path.ancestors().all(|part| {
            fs::symlink_metadata(part).is_ok_and(|metadata| metadata.file_attributes() & 0x400 == 0)
        })
}

fn diagnostic_mode(mode: &str) -> &'static str {
    match mode {
        "observer" => "observer",
        "install" => "install",
        "restart" => "restart",
        "failed-update" => "failed-update",
        "missing" => "missing",
        "reconnect" => "reconnect",
        "restricted" => "restricted",
        "trusted" => "trusted",
        "finish" => "finish",
        _ => "unknown",
    }
}

fn output_diagnostic(path: &Path, observation: bool) -> serde_json::Value {
    let capture = (|| -> std::io::Result<_> {
        if !ordinary_diagnostic_path(path) {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        // Open the leaf itself rather than following a newly substituted link.
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(0x0020_0000)
            .open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        let length = metadata.len();
        let mut bytes = Vec::new();
        file.take((DIAGNOSTIC_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        Ok((length, bytes))
    })();
    let Ok((length, mut bytes)) = capture else {
        return serde_json::json!({"capture":"unavailable"});
    };
    let truncated = length > DIAGNOSTIC_BYTES as u64 || bytes.len() > DIAGNOSTIC_BYTES;
    bytes.truncate(DIAGNOSTIC_BYTES);
    let mut summary = serde_json::json!({
        "capture":"available", "bytes":length, "inspected_bytes":bytes.len(),
        "inspected_sha256":vcp_protocol::digest_bytes(&bytes), "truncated":truncated
    });
    // Output and exception strings can contain state, paths, credentials or
    // recovery keys. Recognize fixed markers only; never echo their text.
    let text = String::from_utf8_lossy(&bytes);
    let markers: Vec<_> = [
        ("child_exit_mismatch", "Candidate process returned "),
        (
            "existing_registration",
            "Existing registered VCP must be preserved",
        ),
        ("artifact_changed", "Final artifact bytes changed"),
        ("observer_failed", "Installed observer failed"),
        ("deadline", "deadline"),
        ("time_or_output_limit", "exceeded time or output limit"),
        ("assertion", "AssertionError"),
        ("missing_file", "ENOENT"),
        ("permission_denied", "EACCES"),
        ("no_space", "ENOSPC"),
    ]
    .into_iter()
    .filter_map(|(label, marker)| text.contains(marker).then_some(label))
    .collect();
    summary["recognized_markers"] = if markers.is_empty() {
        serde_json::json!(["unknown"])
    } else {
        serde_json::json!(markers)
    };
    if observation {
        let parsed = (!truncated)
            .then(|| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .flatten()
            .filter(serde_json::Value::is_object);
        summary["observation_format"] = serde_json::json!(if truncated {
            "oversized"
        } else if parsed.is_some() {
            "json-object"
        } else {
            "unavailable"
        });
        if let Some(value) = parsed {
            let mut safe = serde_json::Map::new();
            if let Some(status @ ("pass" | "fail")) = value["status"].as_str() {
                safe.insert("status".into(), status.into());
            }
            if let Some(mode) = value["mode"].as_str() {
                safe.insert("mode".into(), diagnostic_mode(mode).into());
            }
            for field in [
                "observer",
                "developmentPathAbsent",
                "development_path_absent",
                "native_payload_verified",
                "uninstalled",
                "actual_editor_trusted",
                "final_observer",
            ] {
                if let Some(boolean) = value[field].as_bool() {
                    safe.insert(field.into(), boolean.into());
                }
            }
            for field in [
                "model_calls",
                "forbidden_rpc_count",
                "context_attempts",
                "context_refused",
                "context_accepted",
            ] {
                if let Some(count) = value[field].as_u64().filter(|count| *count <= 1_000_000) {
                    safe.insert(field.into(), count.into());
                }
            }
            summary["reported_observation"] = safe.into();
        }
    }
    summary
}

fn emit_diagnostic(private: &Path, mode: &str, supervision: &serde_json::Value) {
    let mut safe_supervision = supervision.clone();
    safe_supervision["mode"] = diagnostic_mode(mode).into();
    let diagnostic = serde_json::json!({
        "schema":"vcp-editor-runner-diagnostic/1", "mode":diagnostic_mode(mode),
        "supervision":safe_supervision,
        "stdout":output_diagnostic(&private.join(format!("{mode}-runner.stdout")), true),
        "stderr":output_diagnostic(&private.join(format!("{mode}-runner.stderr")), false),
        "scope":"Unverified output markers and allowlisted observations; no raw output, state, paths or keys. Observation success still requires the caller's assertions."
    });
    // Candidate --nocapture retains this single bounded line on failure too.
    // Keep an identical private copy for focused regression and local review.
    eprintln!("VCP_EDITOR_DIAGNOSTIC {diagnostic}");
    assert!(
        ordinary_diagnostic_path(private),
        "Ordinary private diagnostic directory required"
    );
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(private.join(format!("{mode}-diagnostic.json")))
        .unwrap();
    serde_json::to_writer(file, &diagnostic).unwrap();
}

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
    emit_diagnostic(private, mode, &supervision);
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

#[cfg(test)]
mod diagnostic_tests {
    use super::*;

    #[test]
    fn allowlisted_observation_excludes_private_content_and_unrecognized_values() {
        let private = tempfile::tempdir().unwrap();
        let output = private.path().join("capture");
        fs::write(
            &output,
            serde_json::to_vec(&serde_json::json!({
                "status":"pass", "mode":"restart", "observer":true, "model_calls":0,
                "task":"private-task", "error":"private-key", "state":{"secret":"private-key"},
                "native_sha256":"private-key", "uninstalled":"private-key"
            }))
            .unwrap(),
        )
        .unwrap();
        let summary = output_diagnostic(&output, true);
        assert_eq!(
            summary["reported_observation"],
            serde_json::json!({
                "status":"pass", "mode":"restart", "observer":true, "model_calls":0
            })
        );
        assert!(!summary.to_string().contains("private-"));
        fs::write(
            &output,
            br#"{"status":"private-key","mode":"private-key","model_calls":"private-key"}"#,
        )
        .unwrap();
        assert_eq!(
            output_diagnostic(&output, true)["reported_observation"],
            serde_json::json!({"mode":"unknown"})
        );
    }

    #[test]
    fn malformed_missing_and_oversized_output_never_claim_an_observation() {
        let private = tempfile::tempdir().unwrap();
        let output = private.path().join("capture");
        assert_eq!(output_diagnostic(&output, true)["capture"], "unavailable");
        fs::write(&output, b"private-key ENOSPC malformed").unwrap();
        let malformed = output_diagnostic(&output, true);
        assert_eq!(malformed["observation_format"], "unavailable");
        assert_eq!(
            malformed["recognized_markers"],
            serde_json::json!(["no_space"])
        );
        assert!(malformed.get("reported_observation").is_none());
        let oversized = format!(
            "{{\"status\":\"pass\",\"private\":\"{}\"}}",
            "x".repeat(DIAGNOSTIC_BYTES)
        );
        fs::write(&output, oversized.as_bytes()).unwrap();
        let summary = output_diagnostic(&output, true);
        assert_eq!(summary["observation_format"], "oversized");
        assert_eq!(summary["inspected_bytes"], DIAGNOSTIC_BYTES);
        assert_eq!(summary["truncated"], true);
        assert!(summary.get("reported_observation").is_none());
        assert!(summary.to_string().len() < 1024);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn failed_runner_emits_bounded_diagnostic_without_disclosing_output() {
        let private = tempfile::tempdir().unwrap();
        let node = std::path::PathBuf::from(
            std::env::var_os("VCP_TEST_NODE").expect("pinned Node required"),
        );
        assert!(node.is_absolute() && node.is_file());
        let mut command = Command::new(node);
        command.args(["-e", "process.stdout.write(JSON.stringify({status:'fail',mode:'restart',observer:false,error:'NEVER_EXPORT_SYNTHETIC_KEY',task:'NEVER_EXPORT_PRIVATE_TASK'}));process.stderr.write('Candidate process returned 7: NEVER_EXPORT_SYNTHETIC_KEY');process.exitCode=7;"]);
        let root = private.path().to_owned();
        let failed =
            tokio::spawn(
                async move { editor_observation(&mut command, &root, &root, "restart").await },
            )
            .await;
        assert!(failed.is_err_and(|error| error.is_panic()));
        let bytes = fs::read(private.path().join("restart-diagnostic.json")).unwrap();
        let report: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(report["supervision"]["status"], "fail");
        assert_eq!(report["supervision"]["exit_code"], 7);
        assert_eq!(report["supervision"]["job_active_processes_zero"], true);
        assert_eq!(report["supervision"]["forced_descendant_cleanup"], false);
        assert_eq!(report["stdout"]["reported_observation"]["status"], "fail");
        assert_eq!(
            report["stderr"]["recognized_markers"],
            serde_json::json!(["child_exit_mismatch"])
        );
        assert!(!String::from_utf8_lossy(&bytes).contains("NEVER_EXPORT"));
        assert!(!String::from_utf8_lossy(&bytes).contains(private.path().to_str().unwrap()));
        assert!(bytes.len() < 8192);
        assert!(
            fs::read_to_string(private.path().join("restart-runner.stderr"))
                .unwrap()
                .contains("NEVER_EXPORT_SYNTHETIC_KEY")
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn redirected_capture_and_existing_diagnostic_never_read_or_overwrite_targets() {
        let private = tempfile::tempdir().unwrap();
        let target = private.path().join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("capture"), br#"{"status":"pass"}"#).unwrap();
        let alias = private.path().join("alias");
        let node = std::path::PathBuf::from(
            std::env::var_os("VCP_TEST_NODE").expect("pinned Node required"),
        );
        assert!(node.is_absolute() && node.is_file());
        assert!(Command::new(node)
            .creation_flags(0x0800_0000)
            .args([
                "-e",
                "require('node:fs').symlinkSync(process.argv[1],process.argv[2],'junction')"
            ])
            .arg(&target)
            .arg(&alias)
            .status()
            .await
            .unwrap()
            .success());
        assert_eq!(
            output_diagnostic(&alias.join("capture"), true)["capture"],
            "unavailable"
        );
        let sentinel = private.path().join("preserved");
        fs::write(&sentinel, b"preserve unrelated bytes").unwrap();
        fs::hard_link(&sentinel, private.path().join("restart-diagnostic.json")).unwrap();
        let result = std::panic::catch_unwind(|| {
            emit_diagnostic(
                private.path(),
                "restart",
                &serde_json::json!({"status":"fail"}),
            )
        });
        assert!(
            result.is_err(),
            "Existing diagnostic destination must be refused"
        );
        assert_eq!(fs::read(&sentinel).unwrap(), b"preserve unrelated bytes");
        fs::remove_dir(alias).unwrap();
    }
}
