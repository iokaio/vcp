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
            "jobs", "stage", "vault", "recovery", "wire", "replay", "closure",
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
        let mut store = Store::open(&root, backend, &forbidden).await.unwrap();
        store.transact(common::initial()).await.unwrap();
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
        let mut store = Store::open(&root, backend, &forbidden).await.unwrap();
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
        // Stored descriptors cannot change the finalized signed root or payload.
        let mut forged = job.clone();
        forged.stream.as_mut().unwrap().payload.sha256 = "0".repeat(64);
        assert!(jobs
            .encrypt_stream(&forged, &trust, &keys, &staging, &|| false)
            .is_err());
        store.close().await.unwrap();
    }
}
