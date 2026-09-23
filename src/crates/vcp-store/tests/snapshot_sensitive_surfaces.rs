// SPDX-License-Identifier: Apache-2.0
mod common;
use common::*;
use std::collections::BTreeMap;
use vcp_domain::CommandId;
use vcp_store::{
    artifact::ArtifactWriter,
    contract::CanonicalStore,
    keys::{LocalKeys, RecoveryDirectory},
    snapshot_jobs::Jobs,
    vault_publish::{Checkpoint, LocalTrust},
    BackendKind, Store,
};

#[tokio::test]
async fn private_snapshot_staging_excludes_key_material_but_retains_scoped_ordinary_text() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let mut temp = tempfile::tempdir().unwrap();
        temp.disable_cleanup(true);
        eprintln!(
            "p803-sensitive-stage backend={backend:?} retained_root={}",
            temp.path().display()
        );
        let stage = temp.path().join("private-staging");
        let recovery = temp.path().join("independent-recovery");
        let canonical = temp.path().join("canonical");
        std::fs::create_dir(&stage).unwrap();
        std::fs::create_dir(&recovery).unwrap();
        std::fs::create_dir(&canonical).unwrap();
        let local = LocalKeys::generate().unwrap();
        let copy = local
            .export_recovery(
                &RecoveryDirectory::open(&recovery, &[stage.clone(), canonical.clone()]).unwrap(),
            )
            .unwrap();
        // Disposable synthetic identity; values never enter failure messages.
        let material = std::fs::read_to_string(copy.path()).unwrap();
        let age = material
            .lines()
            .find(|line| line.starts_with("AGE-SECRET-KEY-"))
            .unwrap();
        let writer = material
            .lines()
            .find_map(|line| line.strip_prefix("# VCP writer Ed25519 "))
            .unwrap();
        let verified = local.verify_recovery(&copy).unwrap();
        let trust = LocalTrust::enroll(
            &verified,
            workspace().id,
            "a".repeat(64),
            Checkpoint {
                sequence: 0,
                deletion: 0,
                parent: None,
            },
        )
        .unwrap();
        let jobs = Jobs::open(&stage, &[recovery, canonical.clone()]).unwrap();
        let mut store = Store::open(&canonical, backend, &[]).await.unwrap();
        store.transact(initial()).await.unwrap();
        let ordinary = b"sk-synthetic-ordinary-authorized-source-p803";
        let mut capture = store.spool().create(spec()).unwrap();
        capture.write_chunk(ordinary).unwrap();
        let descriptor = capture.finalize().unwrap();
        drop(capture);
        store
            .transact(attach(store.state(), descriptor, None))
            .await
            .unwrap();
        let operation = CommandId::new();
        let captured = jobs
            .begin(&mut store, operation.clone(), &workspace().id, &trust)
            .await
            .unwrap();
        let prepared = jobs.prepare(&store, captured, &|| false).unwrap();
        let staged = std::fs::read(stage.join(format!("{operation}.archive"))).unwrap();
        assert!(!staged.is_empty());
        // The staging file encodes bytes as JSON arrays: searching only its raw
        // file bytes would give a vacuous marker-absence result.
        let payloads: BTreeMap<String, Vec<u8>> = serde_json::from_slice(&staged).unwrap();
        assert!(!payloads.is_empty());
        let mut ordinary_retained = false;
        for bytes in payloads.values() {
            for secret in [age, writer, "AGE-SECRET-KEY-", "VCP writer Ed25519"] {
                assert!(
                    !bytes
                        .windows(secret.len())
                        .any(|part| part == secret.as_bytes()),
                    "typed key material reached a staged payload"
                );
            }
            ordinary_retained |= bytes.windows(ordinary.len()).any(|part| part == ordinary);
        }
        assert!(
            ordinary_retained,
            "authorized ordinary source is retained exactly"
        );
        let accepted = jobs
            .accept_prepared(&mut store, &workspace().id, prepared)
            .await
            .unwrap();
        assert_eq!(
            accepted.archive_digest,
            Some(vcp_protocol::digest_bytes(&staged))
        );
        let serialized = serde_json::to_vec(store.state()).unwrap();
        for secret in [age, writer] {
            assert!(
                !serialized
                    .windows(secret.len())
                    .any(|part| part == secret.as_bytes()),
                "typed key material reached canonical snapshot receipts"
            );
        }
        jobs.release(&mut store, &operation, &workspace().id, true)
            .await
            .unwrap();
        assert!(!stage.join(format!("{operation}.archive")).exists());
        store.close().await.unwrap();
    }
}
