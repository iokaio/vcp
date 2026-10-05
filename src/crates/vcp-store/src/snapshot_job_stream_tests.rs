// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    artifact::ArtifactWriter,
    keys::{LocalKeys, RecoveryDirectory},
    vault_publish::Checkpoint,
    BackendKind,
};
#[path = "../tests/common/mod.rs"]
mod common;

#[tokio::test]
async fn existing_job_stream_preparation_restart_encryption_and_full_archive_replay() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        for name in [
            "jobs",
            "stage",
            "vault",
            "recovery",
            "wire",
            "replay",
            "closure",
            "restore",
            "targets",
            "other-trust",
        ] {
            fs::create_dir(temp.path().join(name)).unwrap();
        }
        let forbidden = vec![temp.path().join("vault")];
        let jobs = Jobs::open(&temp.path().join("jobs"), &forbidden).unwrap();
        let staging = PrivateStaging::open(&temp.path().join("stage"), &forbidden).unwrap();
        let recovery = RecoveryDirectory::open(&temp.path().join("recovery"), &forbidden).unwrap();
        let keys = LocalKeys::generate().unwrap();
        let copy = keys.export_recovery(&recovery).unwrap();
        let keys = keys.verify_recovery(&copy).unwrap();
        let workspace = common::workspace().id;
        let trust = LocalTrust::enroll(
            &keys,
            workspace.clone(),
            "a".repeat(64),
            Checkpoint {
                sequence: 0,
                deletion: 0,
                parent: None,
            },
        )
        .unwrap();
        let root = temp.path().join("canonical");
        let mut store = Store::open(&root, backend, &[]).await.unwrap();
        let (_, initial) = crate::contract::State::default()
            .prepare(&common::initial())
            .unwrap();
        let original_initial = serde_json::to_vec_pretty(&initial).unwrap();
        store.transact_original(&original_initial).await.unwrap();
        let mut writer = store.spool().create(common::spec()).unwrap();
        for index in 0..81u8 {
            writer.write_chunk(&vec![index; 65536]).unwrap();
        }
        let artifact = writer.finalize().unwrap();
        drop(writer);
        store
            .transact(common::attach(
                &store.archive_state().await.unwrap(),
                artifact.clone(),
                None,
            ))
            .await
            .unwrap();
        let expected = store.archive_state().await.unwrap();
        let id = CommandId::new();
        let captured = Jobs::capture_inputs_inner(&store, &workspace, Default::default()).unwrap();
        let prepared = Jobs::prepare_inputs(captured, &|| false).await.unwrap();
        // Exercise the production insertion and stages with the new format;
        // public format selection changes only with its restore consumer.
        let capture = jobs
            .begin_prepared_format(
                &mut store,
                id.clone(),
                &workspace,
                &trust,
                prepared,
                ArchiveFormat::Stream,
            )
            .await
            .unwrap();
        assert!(jobs.prepare_detached(capture, &|| true).await.is_err());
        let job = Jobs::inspect(&store, &id, &workspace).unwrap();
        store.close().await.unwrap();
        let mut store = Store::open(&root, backend, &[]).await.unwrap();
        let capture = jobs.resume_capture(&store, &job).await.unwrap();
        let prepared = jobs.prepare_detached(capture, &|| false).await.unwrap();
        assert!(prepared.stream.as_ref().unwrap().payload.bytes > 4 * 1024 * 1024);
        // An interrupted stage commit reconciles exact deterministic wire bytes.
        let retried = jobs
            .prepare_detached(jobs.resume_capture(&store, &job).await.unwrap(), &|| false)
            .await
            .unwrap();
        assert_eq!(prepared.stream, retried.stream);
        let job = jobs
            .accept_prepared(&mut store, &workspace, prepared)
            .await
            .unwrap();
        assert!(jobs
            .encrypt(&job, &trust, &keys, &staging, &|| true)
            .is_err());
        let encrypted = jobs
            .encrypt(&job, &trust, &keys, &staging, &|| false)
            .unwrap();
        let repeated = jobs
            .encrypt(&job, &trust, &keys, &staging, &|| false)
            .unwrap();
        assert_eq!(
            canonical_bytes(&encrypted.finalization).unwrap(),
            canonical_bytes(&repeated.finalization).unwrap()
        );
        let job = jobs
            .accept_encrypted(&mut store, &workspace, encrypted)
            .await
            .unwrap();
        let object = jobs.reopen_ciphertext(&job).unwrap();
        assert_eq!(object.bytes(), job.finalization.as_ref().unwrap().bytes);
        drop(object);
        let descriptor = job.stream.as_ref().unwrap();
        let mut proof = trust
            .verify_stream_restore(
                &staging,
                &jobs.path(&id, "encrypted").unwrap().join("object.age"),
                &copy,
                crate::vault_crypto::stream::StreamLimits::for_payload(descriptor.payload.bytes)
                    .unwrap(),
                &|| Ok(()),
            )
            .unwrap();
        let wire_dir = Directory::open(&temp.path().join("wire"), &forbidden).unwrap();
        let replay_dir = Directory::open(&temp.path().join("replay"), &forbidden).unwrap();
        let closure_dir = Directory::open(&temp.path().join("closure"), &forbidden).unwrap();
        let mut wire_pages = crate::history_index::io::Files::new(&wire_dir);
        let mut replay_pages = crate::history_index::io::Files::new(&replay_dir);
        let mut closure_pages = crate::history_index::io::Files::new(&closure_dir);
        let untrusted = crate::portable_snapshot::wire::read(
            proof.reader().unwrap(),
            &descriptor.root,
            &descriptor.payload,
            &mut wire_pages,
            &|| Ok(()),
        )
        .await
        .unwrap();
        let restored = crate::portable_snapshot::complete::Archive::restore(
            untrusted,
            &mut wire_pages,
            &mut replay_pages,
            &mut closure_pages,
            &temp.path().join("restored-spool"),
            &forbidden,
            crate::artifact::DEFAULT_ARTIFACT_LIMIT,
            &workspace,
            &|| Ok(()),
        )
        .await
        .unwrap();
        assert_eq!(
            restored.owner.semantic().current(),
            &crate::CurrentState::from_state(&expected)
        );
        assert_eq!(
            restored
                .owner
                .semantic()
                .catalog()
                .legacy_digest(
                    &mut replay_pages,
                    restored.owner.semantic().current().into()
                )
                .await
                .unwrap(),
            job.state_digest
        );
        assert_eq!(restored.spool.inspect(&artifact.spec.id).unwrap(), artifact);
        // The existing public restore operation imports the signed stream into
        // native originals, sanitizes authority and independently cold reopens.
        let operation = CommandId::new();
        let finalized = job.finalization.as_ref().unwrap();
        let mut restore = crate::restore_stage::Restore::begin(
            &temp.path().join("restore"),
            &forbidden,
            operation.clone(),
            &trust,
            finalized.sha256.clone(),
            finalized.bytes,
        )
        .unwrap();
        let cipher_path = jobs.path(&id, "encrypted").unwrap().join("object.age");
        restore.acquire(&cipher_path, &|| false).unwrap();
        let other_keys = LocalKeys::generate().unwrap();
        let other_copy = other_keys.export_recovery(&recovery).unwrap();
        let other_keys = other_keys.verify_recovery(&other_copy).unwrap();
        let mut other_trust = LocalTrust::enroll(
            &other_keys,
            workspace.clone(),
            "a".repeat(64),
            Checkpoint {
                sequence: 0,
                deletion: 0,
                parent: None,
            },
        )
        .unwrap();
        assert_eq!(
            other_trust.configuration().revision,
            trust.configuration().revision
        );
        let mut mismatched = crate::restore_stage::Restore::begin(
            &temp.path().join("other-trust"),
            &forbidden,
            CommandId::new(),
            &other_trust,
            finalized.sha256.clone(),
            finalized.bytes,
        )
        .unwrap();
        mismatched.acquire(&cipher_path, &|| false).unwrap();
        assert!(matches!(
            mismatched
                .authenticate(&trust, &copy, Limits::default(), &|| false)
                .await,
            Err(Error::Conflict("restore trust or stage changed"))
        ));
        assert_eq!(
            mismatched.status().stage,
            crate::restore_stage::Stage::Acquired
        );
        let validated = restore
            .authenticate(&trust, &copy, Limits::default(), &|| false)
            .await
            .unwrap();
        let other_before = canonical_bytes(other_trust.configuration()).unwrap();
        assert!(validated.advance_trust(&mut other_trust, 0).is_err());
        assert_eq!(
            canonical_bytes(other_trust.configuration()).unwrap(),
            other_before
        );
        assert_eq!(validated.archive_state().await.unwrap(), expected);
        let destination = temp.path().join("targets").join(operation.as_str());
        let target_backend = match backend {
            BackendKind::Files => BackendKind::Sqlite,
            BackendKind::Sqlite => BackendKind::Files,
        };
        let imported = restore
            .import(
                &validated,
                &trust,
                target_backend,
                &destination,
                vcp_domain::ActorId::parse("restorer").unwrap(),
                vcp_domain::Timestamp::new(1000),
                &forbidden,
                &|| false,
            )
            .await
            .unwrap();
        let mut reopened = imported.reopen_verified().await.unwrap();
        assert_eq!(
            reopened.prefix_digest(expected.watermark).await.unwrap(),
            job.state_digest
        );
        let sanitized_watermark = reopened.current().watermark;
        assert_eq!(sanitized_watermark, expected.watermark.next().unwrap());
        assert_eq!(
            reopened.transact_original(&original_initial).await.unwrap(),
            initial.receipt
        );
        assert!(reopened
            .transact_original(&canonical_bytes(&initial).unwrap())
            .await
            .is_err());
        let changed: vcp_domain::workspace::Workspace = reopened
            .current()
            .record(Collection::Workspace, workspace.as_str(), &workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(
            changed.authority,
            common::workspace().authority.next().unwrap()
        );
        reopened.close().await.unwrap();
        // Retry after the durable import receipt preserves the same sanitizer.
        let repeated = restore
            .import(
                &validated,
                &trust,
                target_backend,
                &destination,
                vcp_domain::ActorId::parse("ignored-on-retry").unwrap(),
                vcp_domain::Timestamp::new(2000),
                &forbidden,
                &|| false,
            )
            .await
            .unwrap();
        let reopened = repeated.reopen_verified().await.unwrap();
        assert_eq!(reopened.current().watermark, sanitized_watermark);
        reopened.close().await.unwrap();
        // Stored descriptors cannot change the finalized signed root or payload.
        let mut forged = job.clone();
        forged.stream.as_mut().unwrap().payload.sha256 = "0".repeat(64);
        assert!(jobs
            .encrypt_stream(&forged, &trust, &keys, &staging, &|| false)
            .is_err());
        store.close().await.unwrap();
    }
}
