// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
//! Synthetic retained-state smoke against final production artifacts. This is
//! neither first-run onboarding nor distinct-build upgrade qualification.
#[path = "support/hidden_process.rs"]
mod hidden_process;
#[path = "support/local_fixture.rs"]
mod local_fixture;
use codex_utils_pty::JobObject;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::process::Command;
use vcp_store::BackendKind;

async fn editor_observation(
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
    let mut child = hidden_process::spawn(&job, command)
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn editor_process_supervision_requires_natural_descendant_completion() {
    let node = PathBuf::from(std::env::var_os("VCP_TEST_NODE").expect("pinned Node required"));
    assert!(node.is_absolute() && node.is_file());
    let private = tempfile::tempdir().unwrap();
    let mut graceful = Command::new(&node);
    graceful.args(["-e", "require('node:child_process').spawn(process.execPath,['-e','setTimeout(()=>{},1000)'],{stdio:'ignore',windowsHide:true,detached:true}).unref();process.stdout.write('{}');"]);
    let began = Instant::now();
    let report =
        editor_observation(&mut graceful, private.path(), private.path(), "graceful").await;
    assert!(began.elapsed() >= Duration::from_millis(900));
    assert_eq!(report["process_tree"]["job_active_processes_zero"], true);
    assert_eq!(report["process_tree"]["forced_descendant_cleanup"], false);

    let root = private.path().to_owned();
    let mut retained = Command::new(&node);
    retained.args(["-e", "require('node:child_process').spawn(process.execPath,['-e','setTimeout(()=>{},120000)'],{stdio:'ignore',windowsHide:true,detached:true}).unref();process.stdout.write('{}');"]);
    let failed =
        tokio::spawn(
            async move { editor_observation(&mut retained, &root, &root, "retained").await },
        )
        .await;
    assert!(
        failed.is_err_and(|error| error.is_panic()),
        "Forced descendant cleanup must fail the observation"
    );
    let report: serde_json::Value = serde_json::from_slice(
        &fs::read(private.path().join("retained-supervision.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(report["status"], "fail");
    assert_eq!(report["job_active_processes_zero"], true);
    assert_eq!(report["forced_descendant_cleanup"], true);

    let root = private.path().to_owned();
    let mut oversized = Command::new(&node);
    oversized.args(["-e", "process.stdout.write(Buffer.alloc(5*1024*1024));"]);
    let failed =
        tokio::spawn(
            async move { editor_observation(&mut oversized, &root, &root, "oversized").await },
        )
        .await;
    assert!(
        failed.is_err_and(|error| error.is_panic()),
        "Output beyond the cap must fail the observation"
    );
    let report: serde_json::Value = serde_json::from_slice(
        &fs::read(private.path().join("oversized-supervision.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(report["status"], "fail");
    assert_eq!(report["job_active_processes_zero"], true);
    assert_eq!(report["limit_exceeded"], true);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "explicit final native/setup/VSIX receipts and pinned editor required"]
async fn final_installed_candidate_observes_synthetic_history_both_stores() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let mut fixture = local_fixture::Fixture::new(backend).await;
        // Setup can register an uninstall entry before a later verification
        // fails. Never let assertion unwinding delete its registered program
        // root, recovery logs or selected synthetic data directory.
        let fixture_root = fixture.preserve_private_root();
        let private = tempfile::tempdir().unwrap().keep();
        let mut command = Command::new("pwsh");
        command
            .args(["-NoProfile", "-File"])
            .arg(repo.join("scripts/release/editor-smoke.ps1"));
        for (flag, variable) in [
            ("-NativeResult", "VCP_BETA_NATIVE_RESULT"),
            ("-SetupResult", "VCP_BETA_SETUP_RESULT"),
            ("-VsixManifest", "VCP_BETA_VSIX_MANIFEST"),
            ("-Code", "VCP_TEST_CODE"),
        ] {
            command
                .arg(flag)
                .arg(std::env::var_os(variable).expect(variable));
        }
        command
            .arg("-Workspace")
            .arg(&fixture.workspace)
            .arg("-DataRoot")
            .arg(&fixture.data)
            .arg("-OutputRoot")
            .arg(private.join("editor-smoke"));
        let report = editor_observation(&mut command, &private, &fixture_root, "observer").await;
        assert_eq!(report["status"], "pass");
        assert_eq!(report["observer"], true);
        assert_eq!(report["task"], fixture.config.root_task.to_string());
        let store = fixture.reopen_within(Duration::from_secs(45)).await;
        local_fixture::assert_offline_paused(&store, &fixture.config);
        store.close().await.unwrap();
        eprintln!(
            "Final installed production artifact observer smoke passed for {backend:?}: {report}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "explicit final receipts, pinned editor and a fresh registration-free user required"]
async fn final_installed_candidate_editor_lifecycle_preserves_both_stores() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let mut fixture = local_fixture::Fixture::new(backend).await;
        let fixture_root = fixture.preserve_private_root();
        let private = tempfile::tempdir().unwrap().keep();
        assert!(!private.canonicalize().unwrap().starts_with(&repo));
        let root = private.join("editor-lifecycle");
        let sentinel = fixture.data.join("preserve-lifecycle-data");
        let sentinel_bytes = b"independent user data survives editor lifecycle";
        std::fs::write(&sentinel, sentinel_bytes).unwrap();
        let workspace_file = fixture.workspace.join("existing-user-work.txt");
        std::fs::write(&workspace_file, b"untouched human work\r\n").unwrap();
        let key = private.join("independent-recovery-key");
        let key_bytes = vcp_protocol::digest_bytes(vcp_domain::ActorId::new().as_str().as_bytes());
        std::fs::write(&key, key_bytes.as_bytes()).unwrap();
        let store = fixture.reopen().await;
        let before = store.state().clone();
        store.close().await.unwrap();
        let mut reports = Vec::new();
        for mode in [
            "install",
            "restart",
            "failed-update",
            "missing",
            "reconnect",
        ] {
            let mut command = Command::new("pwsh");
            command
                .args(["-NoProfile", "-File"])
                .arg(repo.join("scripts/release/editor-lifecycle.ps1"));
            for (flag, variable) in [
                ("-NativeResult", "VCP_BETA_NATIVE_RESULT"),
                ("-SetupResult", "VCP_BETA_SETUP_RESULT"),
                ("-VsixManifest", "VCP_BETA_VSIX_MANIFEST"),
                ("-Code", "VCP_TEST_CODE"),
            ] {
                command
                    .arg(flag)
                    .arg(std::env::var_os(variable).expect(variable));
            }
            command
                .arg("-Workspace")
                .arg(&fixture.workspace)
                .arg("-DataRoot")
                .arg(&fixture.data)
                .arg("-Scope")
                .arg(fixture.scope().to_string())
                .arg("-Task")
                .arg(fixture.config.root_task.as_str())
                .arg("-OutputRoot")
                .arg(&root)
                .arg("-Mode")
                .arg(mode);
            let mut report = editor_observation(&mut command, &private, &fixture_root, mode).await;
            assert_eq!(report["status"], "pass");
            assert_eq!(report["mode"], mode);
            assert_eq!(report["observer"], mode != "missing");
            assert_eq!(report["model_calls"], 0);
            if mode == "install" {
                assert_eq!(report["reload"]["observer"], true);
                assert_eq!(
                    report["incompatible_protocol"]["kind"],
                    "unsupported_version"
                );
            } else if mode == "failed-update" {
                assert_eq!(report["malformed_update"]["exit_code"], 1);
                assert_eq!(
                    report["malformed_update"]["original_archive_preserved"],
                    true
                );
            } else if mode == "reconnect" {
                assert_eq!(report["uninstalled"], true);
            }
            let store = fixture.reopen_within(Duration::from_secs(45)).await;
            local_fixture::assert_offline_paused(&store, &fixture.config);
            assert_eq!(
                store.state(),
                &before,
                "{mode} must preserve all canonical records, accounting, commands and history"
            );
            store.close().await.unwrap();
            assert_eq!(std::fs::read(&sentinel).unwrap(), sentinel_bytes);
            assert_eq!(
                std::fs::read(&workspace_file).unwrap(),
                b"untouched human work\r\n"
            );
            assert_eq!(std::fs::read(&key).unwrap(), key_bytes.as_bytes());
            report["canonical_state_preserved"] = true.into();
            report["independent_data_preserved"] = true.into();
            reports.push(report);
        }
        let report = serde_json::json!({
            "schema": "vcp-editor-lifecycle-observations/1",
            "status": "pass",
            "backend": format!("{backend:?}"),
            "observations": reports,
            "limitations": "Synthetic paused history; no first useful task, paid call, reviewed edit, distinct-version upgrade or human UI acceptance."
        });
        std::fs::write(
            root.join("observations.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        eprintln!("Final installed candidate lifecycle: {report}");
    }
}
