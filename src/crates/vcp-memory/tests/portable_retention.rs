// SPDX-License-Identifier: Apache-2.0
#[path = "../../vcp-store/tests/common/mod.rs"]
mod common;

use common::*;
use vcp_domain::{
    artifact::{ArtifactDescriptor, CaptureState},
    retention_selector::{Criterion, Selector, Tree},
    task::TaskState,
    *,
};
use vcp_memory::{
    access::Access,
    retention::{self, Action},
};
use vcp_store::{
    artifact::ArtifactWriter,
    contract::*,
    keys::{LocalKeys, RecoveryDirectory},
    portable_snapshot::Archive,
    snapshot_jobs::{Jobs, Stage},
    vault_crypto::PrivateStaging,
    vault_publish::{Checkpoint, LocalTrust, Vault},
    BackendKind, Store,
};

#[tokio::test]
async fn snapshot_obligations_block_physical_cleanup_and_stale_publication_after_purge() {
    retention_case(false).await;
}

#[tokio::test]
async fn admitted_copy_finishes_after_purge_and_remains_a_retained_backup_obligation() {
    retention_case(true).await;
}

async fn retention_case(admitted: bool) {
    const MARKER: &[u8] = b"portable-retention-private-source-93f8d";
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canonical");
        let paths = ["jobs", "stage", "vault", "recovery"].map(|s| temp.path().join(s));
        for path in &paths {
            std::fs::create_dir(path).unwrap();
        }
        let forbidden = vec![paths[2].clone()];
        let jobs = Jobs::open(&paths[0], &forbidden).unwrap();
        let staging = PrivateStaging::open(&paths[1], &forbidden).unwrap();
        let recovery = RecoveryDirectory::open(&paths[3], &forbidden).unwrap();
        let keys = LocalKeys::generate().unwrap();
        let exported = keys.export_recovery(&recovery).unwrap();
        let keys = keys.verify_recovery(&exported).unwrap();
        let ws = workspace();
        let mut trust = LocalTrust::enroll(
            &keys,
            ws.id.clone(),
            "a".repeat(64),
            Checkpoint {
                sequence: 0,
                deletion: 0,
                parent: None,
            },
        )
        .unwrap();
        let mut store = Store::open(&root, backend, &forbidden).await.unwrap();
        let mut initial = initial();
        // Terminal history is eligible for erasure; no live execution is waived.
        let mut terminal = task();
        terminal.state = TaskState::Cancelled;
        for mutation in &mut initial.mutations {
            if let Mutation::Put { record, .. } = mutation {
                if record.collection == Collection::Task {
                    *record = Record::typed(
                        Collection::Task,
                        terminal.scope.task.as_str(),
                        ws.id.clone(),
                        Revision::ZERO,
                        &terminal,
                    )
                    .unwrap();
                }
            }
        }
        store.transact(initial).await.unwrap();
        let mut writer = store.spool().create(spec()).unwrap();
        writer.write_chunk(MARKER).unwrap();
        let descriptor = writer.finalize().unwrap();
        drop(writer);
        store
            .transact(attach(store.state(), descriptor.clone(), None))
            .await
            .unwrap();
        let snapshot = store.snapshot().unwrap();
        let original = Archive::capture(&store, &snapshot, &ws.id, &|| false).unwrap();
        assert_eq!(
            original.retained_artifact(&descriptor.spec.id).unwrap(),
            MARKER
        );
        drop(snapshot);
        drop(original);

        let operation = CommandId::new();
        let capture = jobs
            .begin(&mut store, operation.clone(), &ws.id, &trust)
            .await
            .unwrap();
        let prepared = jobs.prepare(&store, capture, &|| false).unwrap();
        let job = jobs
            .accept_prepared(&mut store, &ws.id, prepared)
            .await
            .unwrap();
        let encrypted = jobs
            .encrypt(&job, &trust, &keys, &staging, &|| false)
            .unwrap();
        jobs.accept_encrypted(&mut store, &ws.id, encrypted)
            .await
            .unwrap();
        let ready = Jobs::inspect(&store, &operation, &ws.id).unwrap();
        let admitted_copy = if admitted {
            let evidence = jobs.prepare_admission(&ready, &trust).unwrap();
            Some(
                jobs.admit_prepared(&mut store, &ws.id, &trust, evidence)
                    .await
                    .unwrap(),
            )
        } else {
            None
        };
        // Keep independently prepared evidence to prove stale policy is checked
        // again at canonical admission, rather than trusted from preparation.
        let stale_evidence = if admitted {
            None
        } else {
            Some(jobs.prepare_admission(&ready, &trust).unwrap())
        };
        // All in-memory snapshot handles are gone. The durable job alone pins
        // the old payload across owner restart and a physical replay-base rewrite.
        store.close().await.unwrap();
        let mut store = Store::open(&root, backend, &forbidden).await.unwrap();
        let access = Access {
            workspace: ws.id.clone(),
            actor: ActorId::parse("human").unwrap(),
            authority: ws.authority,
            read: true,
            write: true,
            tasks: None,
        };
        let preview = retention::preview(
            &store,
            &access,
            Selector {
                schema_version: 1,
                tree: Tree::Match(Criterion::Task(terminal.scope.task.clone())),
            },
            Action::Purge,
            Timestamp::new(1000),
        )
        .unwrap();
        assert!(preview.protected.is_empty());
        assert!(preview.backup_copies.contains(&operation.to_string()));
        let applied = retention::apply(&mut store, &access, &preview, Timestamp::new(1001))
            .await
            .unwrap();
        assert!(applied.logical_unavailable);
        let cleanup = retention::cleanup(&mut store, &access, &applied.id, Timestamp::new(1002))
            .await
            .unwrap();
        assert!(cleanup.rewrite_complete);
        assert!(!cleanup.local_cleanup_complete);
        assert!(!cleanup.cleanup.as_ref().unwrap().pinned.is_empty());
        let erased: ArtifactDescriptor = store
            .state()
            .record(Collection::Artifact, descriptor.spec.id.as_str(), &ws.id)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(erased.state, CaptureState::Purged);
        let before = store.state().watermark;
        assert!(jobs
            .admit(&mut store, &operation, &ws.id, &trust)
            .await
            .is_err());
        assert_eq!(store.state().watermark, before);
        assert_eq!(
            Jobs::inspect(&store, &operation, &ws.id).unwrap().stage,
            if admitted {
                Stage::Admitted
            } else {
                Stage::CiphertextReady
            }
        );
        assert_eq!(std::fs::read_dir(&paths[2]).unwrap().count(), 0);
        if let Some(proof) = stale_evidence {
            assert!(jobs
                .admit_prepared(&mut store, &ws.id, &trust, proof)
                .await
                .is_err());
            assert_eq!(store.state().watermark, before);
        }
        if let Some((job, mut ciphertext, permit)) = admitted_copy {
            let vault = Vault::open(
                &paths[2],
                &[paths[0].clone(), paths[1].clone(), paths[3].clone()],
            )
            .unwrap();
            let store_ref = &mut store;
            let receipt = vault
                .publish_recorded(
                    &mut ciphertext,
                    &permit,
                    job.copy_identity(),
                    |identity| async {
                        jobs.record_copy(store_ref, &operation, &ws.id, identity)
                            .await?;
                        Ok(())
                    },
                    &|| false,
                    &|_| {},
                )
                .await
                .unwrap();
            // Trust advances only from proof of the exact existing ciphertext.
            let verified = vault.reconcile(&ciphertext, &permit).unwrap();
            assert_eq!(verified.operation, receipt.operation);
            jobs.complete(&mut store, &ws.id, &verified).await.unwrap();
            let revision = trust.configuration().revision;
            trust
                .advance_after_publication(&verified, revision)
                .unwrap();
            assert!(Jobs::checkpoint_matches(&store, &operation, &ws.id, &trust).unwrap());
            drop(ciphertext);
        }

        let released = jobs
            .release(&mut store, &operation, &ws.id, true)
            .await
            .unwrap();
        assert!(!released.active);
        if admitted {
            assert_eq!(released.stage, Stage::Published);
            assert!(released.publication.is_some());
            assert!(released.copy_identity().is_some());
            assert!(released.pins.is_empty());
        }
        let cleaned = retention::cleanup(&mut store, &access, &applied.id, Timestamp::new(1003))
            .await
            .unwrap();
        assert!(cleaned.local_cleanup_complete, "{cleaned:?}");
        assert!(cleaned.backup_copies.contains(&operation.to_string()));
        store.close().await.unwrap();
        let mut store = Store::open(&root, backend, &forbidden).await.unwrap();
        let snapshot = store.snapshot().unwrap();
        let fresh = Archive::capture(&store, &snapshot, &ws.id, &|| false).unwrap();
        assert!(fresh.retained_artifact(&descriptor.spec.id).is_err());
        for bytes in fresh.payloads().unwrap().values() {
            assert!(!bytes.windows(MARKER.len()).any(|window| window == MARKER));
        }
        drop(snapshot);
        let capture = jobs
            .begin(&mut store, CommandId::new(), &ws.id, &trust)
            .await
            .unwrap();
        assert_eq!(capture.job().deletion, cleaned.deletion.get());
        let prepared = jobs.prepare(&store, capture, &|| false).unwrap();
        jobs.accept_prepared(&mut store, &ws.id, prepared)
            .await
            .unwrap();
        store.close().await.unwrap();
    }
}
