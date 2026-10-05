// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
//! Actual native loading, SDK ownership and encrypted vault verification; no cloud service.
#[path = "support/sdk_execution_fixture.rs"]
mod execution_fixture;
use serde_json::{json, Value};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use vcp_cli::backup::{self, Backup, Keys};
use vcp_store::contract::CanonicalStore;
use vcp_store::{contract::Collection, BackendKind, Store};

async fn driver(input: Value) -> Value {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/sdk-ts/tests/native-backup-publisher.mjs");
    let bytes = serde_json::to_vec(&input).unwrap();
    let output = tokio::task::spawn_blocking(move || {
        let mut child = Command::new(std::env::var_os("VCP_TEST_NODE").unwrap())
            .arg(script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&bytes).unwrap();
        let until = Instant::now() + Duration::from_secs(120);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() > until {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                panic!(
                    "publisher SDK deadline: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        child.wait_with_output().unwrap()
    })
    .await
    .unwrap();
    assert!(
        output.status.success(),
        "publisher SDK failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["ok"], true);
    result
}
async fn reopen(path: &std::path::Path, backend: BackendKind) -> Store {
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        match Store::open(path, backend, &[]).await {
            Ok(store) => return store,
            Err(error) => {
                assert!(
                    Instant::now() < deadline,
                    "test-owned pipe host did not release store: {error}"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_sdk_publishes_encrypted_local_backup_and_recovers_observer_receipt() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        println!("publisher native backend: {backend:?}");
        let server = wiremock::MockServer::start().await;
        let fixture = execution_fixture::Fixture::new(&server.uri(), "complete");
        let git = PathBuf::from(std::env::var_os("VCP_TEST_GIT").unwrap());
        for args in [vec!["init", "--quiet"], vec!["add", "--", "value.txt"]] {
            assert!(Command::new(&git)
                .current_dir(&fixture.workspace)
                .args(args)
                .output()
                .unwrap()
                .status
                .success());
        }
        let entry = fixture.seed(backend).await;
        let workspace = PathBuf::from(&entry.config.binding.root);
        let recovery = fixture.data.join("native-publisher-recovery");
        let vault = fixture.data.join("native-publisher-vault");
        let staging = fixture.data.join("native-publisher-staging");
        for path in [&recovery, &vault, &staging] {
            std::fs::create_dir(path).unwrap();
        }
        let enrolled = backup::keys(
            &Keys::Create {
                recovery_dir: recovery.clone(),
                sync_root: vec![],
            },
            &fixture.data,
            &workspace,
            Some(&entry),
            None,
        )
        .unwrap();
        let key = PathBuf::from(enrolled["recovery_copy"].as_str().unwrap());
        backup::configure(
            &Backup::Configure {
                vault: vault.clone(),
                staging: staging.clone(),
                expected_revision: None,
                sync_root: vec![],
                automatic: true,
                manual_only: false,
            },
            &fixture.data,
            &workspace,
            &entry,
        )
        .unwrap();
        let forbidden = backup::forbidden(&workspace, &entry.config.canonical_root, &[]).unwrap();
        let recovery_directory = vcp_store::keys::RecoveryDirectory::open(
            key.parent().unwrap(),
            &[
                workspace.clone(),
                entry.config.canonical_root.clone(),
                vault.clone(),
                staging.clone(),
            ],
        )
        .unwrap();
        let copy = recovery_directory
            .open_copy(key.file_stem().unwrap().to_str().unwrap())
            .unwrap();
        // Preserve the independently enrolled predecessor before the publisher
        // advances its replay floor. Archive metadata never supplies trust.
        let predecessor = {
            let saved = vcp_store::trust_store::TrustStore::open(
                &backup::trust_path(&fixture.data, &entry.config.workspace),
                &forbidden,
            )
            .unwrap();
            let configuration = saved.trust().configuration();
            let verified = vcp_store::keys::LocalKeys::import(&copy)
                .unwrap()
                .verify_recovery(&copy)
                .unwrap();
            vcp_store::vault_publish::LocalTrust::enroll(
                &verified,
                configuration.workspace.clone(),
                configuration.lineage.clone(),
                configuration.checkpoint.clone(),
            )
            .unwrap()
        };
        // The actual native publisher must acquire the recovery file itself.
        drop(copy);
        drop(recovery_directory);
        let profile = fixture.data.join("native-publisher.json");
        std::fs::write(
            &profile,
            serde_json::to_vec(&json!({"version":1,"key":key,"git":git})).unwrap(),
        )
        .unwrap();
        let operation = vcp_domain::CommandId::new();
        let mut input = json!({"scenario":"publish","executable":env!("CARGO_BIN_EXE_vcp"),"workspace":fixture.workspace,"data":fixture.data,
            "root":entry.config.binding.root,"scope":{"workspace":entry.config.workspace,"session":entry.config.session},"profile":profile,"operation":operation});
        let result = driver(input.clone()).await;
        let store = reopen(&entry.config.canonical_root, backend).await;
        let jobs: Vec<vcp_store::snapshot_jobs::Job> = store
            .current()
            .records
            .values()
            .filter(|row| {
                row.collection == Collection::SnapshotPin
                    && row.value["document_type"] == "vcp_snapshot_job_v1"
            })
            .map(|row| row.decode().unwrap())
            .collect();
        assert_eq!(jobs.len(), 1);
        let job = &jobs[0];
        assert_eq!(job.id, operation);
        assert_eq!(job.stage, vcp_store::snapshot_jobs::Stage::Published);
        assert!(!job.active);
        assert_eq!(
            (&store.archive_state().await.unwrap())
                .commands
                .values()
                .filter(|r| r.command == operation)
                .count(),
            1
        );
        assert!(!store
            .current()
            .records
            .values()
            .any(|r| r.collection == Collection::Attempt));
        let object = vault.join(
            job.publication.as_ref().unwrap()["object"]
                .as_str()
                .unwrap(),
        );
        let encrypted = std::fs::read(&object).unwrap();
        assert!(encrypted.starts_with(b"age-encryption.org/v1\n"));
        let trust = vcp_store::trust_store::TrustStore::open(
            &backup::trust_path(&fixture.data, &entry.config.workspace),
            &forbidden,
        )
        .unwrap();
        let recovery_directory = vcp_store::keys::RecoveryDirectory::open(
            key.parent().unwrap(),
            &[
                workspace.clone(),
                entry.config.canonical_root.clone(),
                vault.clone(),
                staging.clone(),
            ],
        )
        .unwrap();
        let copy = recovery_directory
            .open_copy(key.file_stem().unwrap().to_str().unwrap())
            .unwrap();
        // Publication advances the independent replay floor. The current head is
        // verified against its pinned digest, not admitted again as a descendant.
        let manifest: vcp_store::vault_crypto::ManifestEnvelope = serde_json::from_value(
            serde_json::to_value(job).unwrap()["finalization"]["manifest"].clone(),
        )
        .unwrap();
        let commitment = vcp_store::vault_crypto::inspect_ciphertext(&object, &|| Ok(())).unwrap();
        let mut validated_head = None;
        for (name, authority, accepted) in [
            ("predecessor", &predecessor, true),
            ("current", trust.trust(), false),
        ] {
            let scratch = staging.join(name);
            std::fs::create_dir(&scratch).unwrap();
            let mut restore = vcp_store::restore_stage::Restore::begin(
                &scratch,
                &forbidden,
                vcp_domain::CommandId::new(),
                authority,
                commitment.sha256.clone(),
                commitment.bytes,
            )
            .unwrap();
            restore.acquire(&object, &|| false).unwrap();
            let validation = restore
                .authenticate(
                    authority,
                    &copy,
                    vcp_store::vault_crypto::Limits::default(),
                    &|| false,
                )
                .await;
            if accepted {
                validated_head = Some(validation.unwrap());
            } else {
                assert!(
                    validation.is_err(),
                    "current floor must reject its own snapshot as replay"
                );
            }
        }
        let verified = validated_head.unwrap();
        let manifest_bytes = vcp_protocol::canonical_bytes(&manifest).unwrap();
        assert_eq!(
            vcp_protocol::canonical_bytes(&verified.manifest()).unwrap(),
            manifest_bytes
        );
        let checkpoint = &trust.trust().configuration().checkpoint;
        assert_eq!(
            checkpoint.parent.as_deref(),
            Some(vcp_protocol::digest_bytes(&manifest_bytes).as_str())
        );
        assert_eq!(checkpoint.sequence, manifest.sequence());
        assert_eq!(checkpoint.deletion, manifest.deletion());
        assert_eq!(manifest.workspace(), &entry.config.workspace);
        assert!(
            vcp_lifecycle::foundation::backup::setup(&trust)
                .unwrap()
                .unwrap()
                .automatic,
            "explicit load does not rewrite native automatic preference"
        );
        drop(verified);
        drop(copy);
        drop(trust);
        store.close().await.unwrap();
        input["scenario"] = json!("reopen");
        input["receipt"] = result["receipt"].clone();
        driver(input).await;
        assert!(server.received_requests().await.unwrap().is_empty());
        assert_eq!(
            std::fs::read_to_string(fixture.workspace.join("value.txt")).unwrap(),
            "41\n"
        );
        println!("publisher evidence: encrypted+signed local vault verified; duplicate receipt1; observer ownership preserved; restart unavailable; remote unknown");
    }
}
