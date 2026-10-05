// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    keys::{LocalKeys, RecoveryDirectory},
    restore_stage::Restore,
    vault_publish::Checkpoint,
    BackendKind,
};
#[path = "../tests/common/mod.rs"]
mod common;

#[tokio::test]
async fn public_job_and_native_restore_preserve_history_beyond_legacy_capacity() {
    const PAYLOAD: usize = 6 * 1024 * 1024;
    const COUNT: usize = 11;
    assert!(PAYLOAD * COUNT > crate::contract::MAX_STATE_BYTES);
    let temp = tempfile::tempdir().unwrap();
    for name in ["jobs", "stage", "vault", "recovery", "restore", "targets"] {
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
    let mut store = Store::open(&root, BackendKind::Files, &[]).await.unwrap();
    store.transact(common::initial()).await.unwrap();
    for index in 0..COUNT {
        let mut transaction = common::initial();
        transaction.id = TransactionId::parse(format!("large-snapshot-{index}")).unwrap();
        transaction.expected_watermark = store.current().watermark;
        transaction.mutations.clear();
        transaction.events[0].id =
            vcp_domain::EventId::parse(format!("large-event-{index}")).unwrap();
        transaction.events[0].correlation =
            CommandId::parse(format!("large-command-{index}")).unwrap();
        transaction.command.as_mut().unwrap().command = transaction.events[0].correlation.clone();
        transaction.command.as_mut().unwrap().digest = format!("{index:064x}");
        // Distinct chunks prevent content-addressed deduplication from turning
        // the transport qualification into a tiny repeated-payload archive.
        let mut payload = String::with_capacity(PAYLOAD);
        for chunk in 0..PAYLOAD / 65536 {
            let prefix = format!("{index:08x}:{chunk:08x}:");
            payload.push_str(&prefix);
            payload.extend(std::iter::repeat_n('x', 65536 - prefix.len()));
        }
        transaction.events[0].data = serde_json::json!({"payload": payload, "index": index});
        store.transact(transaction).await.unwrap();
        eprintln!(
            "large public archive: retained payload commit {}/{}",
            index + 1,
            COUNT
        );
    }
    assert!(canonical_bytes(&store.current()).unwrap().len() < 16 * 1024);
    assert!(matches!(store.archive_state().await, Err(Error::Limit(_))));
    let expected = store.current_state();
    let expected_digest = store.snapshot().unwrap().logical_digest().await.unwrap();
    let id = CommandId::new();
    // This is the public default path, with no private format selector.
    let capture = jobs
        .begin(&mut store, id.clone(), &workspace, &trust)
        .await
        .unwrap();
    assert_eq!(capture.job().archive_format, ArchiveFormat::Stream);
    let prepared = jobs.prepare_detached(capture, &|| false).await.unwrap();
    eprintln!(
        "large public archive: prepared {} wire bytes",
        prepared.stream.as_ref().unwrap().payload.bytes
    );
    assert!(prepared.stream.as_ref().unwrap().payload.bytes > 64 * 1024 * 1024);
    let job = jobs
        .accept_prepared(&mut store, &workspace, prepared)
        .await
        .unwrap();
    let encrypted = jobs
        .encrypt(&job, &trust, &keys, &staging, &|| false)
        .unwrap();
    eprintln!("large public archive: encrypted");
    let job = jobs
        .accept_encrypted(&mut store, &workspace, encrypted)
        .await
        .unwrap();
    let object = jobs.reopen_ciphertext(&job).unwrap();
    assert!(object.bytes() > 64 * 1024 * 1024);
    drop(object);
    store.close().await.unwrap();
    let operation = CommandId::new();
    let finalized = job.finalization.as_ref().unwrap();
    let mut restore = Restore::begin(
        &temp.path().join("restore"),
        &forbidden,
        operation.clone(),
        &trust,
        finalized.sha256.clone(),
        finalized.bytes,
    )
    .unwrap();
    restore
        .acquire(
            &jobs.path(&id, "encrypted").unwrap().join("object.age"),
            &|| false,
        )
        .unwrap();
    let validated = restore
        .authenticate(&trust, &copy, Limits::default(), &|| false)
        .await
        .unwrap();
    eprintln!("large public archive: authenticated and semantically replayed");
    assert_eq!(
        canonical_bytes(&validated.current()).unwrap(),
        canonical_bytes(&expected.as_ref()).unwrap()
    );
    assert!(matches!(
        validated.archive_state().await,
        Err(Error::Limit(_))
    ));
    let imported = restore
        .import(
            &validated,
            &trust,
            BackendKind::Sqlite,
            &temp.path().join("targets").join(operation.as_str()),
            vcp_domain::ActorId::parse("large-restorer").unwrap(),
            vcp_domain::Timestamp::new(1000),
            &forbidden,
            &|| false,
        )
        .await
        .unwrap();
    let reopened = imported.reopen_verified().await.unwrap();
    eprintln!("large public archive: native imported and independently reopened");
    assert_eq!(
        reopened.prefix_digest(expected.watermark).await.unwrap(),
        expected_digest
    );
    assert_eq!(
        reopened.current().watermark,
        expected.watermark.next().unwrap()
    );
    // Initial task event plus all payload events, then workspace rebind,
    // pending-task pause and access revocation from the existing sanitizer.
    assert_eq!(
        reopened.history_event_count().await.unwrap(),
        COUNT as u64 + 4
    );
    for index in 0..COUNT {
        let event = reopened
            .history_event(&vcp_domain::EventId::parse(format!("large-event-{index}")).unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.event.data["index"], index);
        assert_eq!(event.event.data["payload"].as_str().unwrap().len(), PAYLOAD);
    }
    assert!(matches!(
        reopened.archive_state().await,
        Err(Error::Limit(_))
    ));
    reopened.close().await.unwrap();
}

#[tokio::test]
async fn public_stream_restore_preserves_exact_retained_base_and_suffix_bytes() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        for name in ["jobs", "stage", "vault", "recovery", "restore", "targets"] {
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
        let mut store = Store::open(&temp.path().join("canonical"), backend, &forbidden)
            .await
            .unwrap();
        store.transact(common::initial()).await.unwrap();
        let baseline = store.archive_state().await.unwrap();
        store
            .rewrite_base(baseline.clone(), &forbidden)
            .await
            .unwrap();
        let base_bytes = fs::read(store.root().join("replay-base.json")).unwrap();
        let mut transaction = common::initial();
        transaction.id = TransactionId::new();
        transaction.expected_watermark = store.current().watermark;
        transaction.mutations.clear();
        transaction.events[0].id = vcp_domain::EventId::new();
        transaction.events[0].correlation = CommandId::new();
        transaction.command.as_mut().unwrap().command = transaction.events[0].correlation.clone();
        let (_, commit) = baseline.prepare(&transaction).unwrap();
        let exact_suffix = serde_json::to_vec_pretty(&commit).unwrap();
        store.transact_original(&exact_suffix).await.unwrap();
        let digest = store.snapshot().unwrap().logical_digest().await.unwrap();
        let cut = store.current().watermark;
        let id = CommandId::new();
        let capture = jobs
            .begin(&mut store, id.clone(), &workspace, &trust)
            .await
            .unwrap();
        assert_eq!(capture.job().archive_format, ArchiveFormat::Stream);
        let prepared = jobs.prepare_detached(capture, &|| false).await.unwrap();
        let job = jobs
            .accept_prepared(&mut store, &workspace, prepared)
            .await
            .unwrap();
        let encrypted = jobs
            .encrypt(&job, &trust, &keys, &staging, &|| false)
            .unwrap();
        let job = jobs
            .accept_encrypted(&mut store, &workspace, encrypted)
            .await
            .unwrap();
        store.close().await.unwrap();
        let operation = CommandId::new();
        let finalized = job.finalization.as_ref().unwrap();
        let mut restore = Restore::begin(
            &temp.path().join("restore"),
            &forbidden,
            operation.clone(),
            &trust,
            finalized.sha256.clone(),
            finalized.bytes,
        )
        .unwrap();
        restore
            .acquire(
                &jobs.path(&id, "encrypted").unwrap().join("object.age"),
                &|| false,
            )
            .unwrap();
        let validated = restore
            .authenticate(&trust, &copy, Limits::default(), &|| false)
            .await
            .unwrap();
        let target_backend = if backend == BackendKind::Files {
            BackendKind::Sqlite
        } else {
            BackendKind::Files
        };
        let imported = restore
            .import(
                &validated,
                &trust,
                target_backend,
                &temp.path().join("targets").join(operation.as_str()),
                vcp_domain::ActorId::parse("restorer").unwrap(),
                vcp_domain::Timestamp::new(1000),
                &forbidden,
                &|| false,
            )
            .await
            .unwrap();
        let mut reopened = imported.reopen_verified().await.unwrap();
        assert_eq!(
            fs::read(reopened.root().join("replay-base.json")).unwrap(),
            base_bytes
        );
        assert_eq!(reopened.prefix_digest(cut).await.unwrap(), digest);
        assert_eq!(
            reopened.transact_original(&exact_suffix).await.unwrap(),
            commit.receipt
        );
        assert!(reopened
            .transact_original(&canonical_bytes(&commit).unwrap())
            .await
            .is_err());
        reopened.close().await.unwrap();
    }
}
