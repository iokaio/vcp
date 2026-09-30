// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
//! Synthetic retained-state smoke against final production artifacts. This is
//! neither first-run onboarding nor distinct-build upgrade qualification.
#[path = "support/local_fixture.rs"]
mod local_fixture;
use std::{path::PathBuf, process::Command, time::Duration};
use vcp_store::BackendKind;

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
        let output = tokio::task::spawn_blocking(move || command.output().unwrap())
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "Private smoke retained at {}; synthetic inputs retained at {}. {} {}",
            private.display(),
            fixture_root.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
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
