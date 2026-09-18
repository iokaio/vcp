// SPDX-License-Identifier: Apache-2.0
mod common;
use common::*;
use vcp_domain::{artifact::*, ids::*, revision::*};
use vcp_store::{artifact::ArtifactWriter, contract::*, *};

#[tokio::test]
async fn backends_share_atomicity_receipts_scope_and_revision_contract() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("canonical");
        let mut store = Store::open(&root, backend, &[]).await.unwrap();
        assert!(Store::open(&root, backend, &[]).await.is_err());
        let mut transaction = initial();
        // Version-1 access documents predate typed authority. A generic `data`
        // field must not reinterpret their bytes as a new policy or grant.
        let legacy = serde_json::json!({"schema_version":1,"data":{"legacy":true}});
        transaction.mutations.push(Mutation::Put {
            expected: None,
            record: Record::typed(
                Collection::Access,
                "legacy-access",
                workspace().id,
                Revision::ZERO,
                &legacy,
            )
            .unwrap(),
        });
        let receipt = store.transact(transaction.clone()).await.unwrap();
        assert_eq!(store.transact(transaction.clone()).await.unwrap(), receipt);
        let snapshot = store.snapshot().unwrap();
        let mut conflict = transaction.clone();
        conflict.events[0].data = serde_json::json!({"different":true});
        assert!(store.transact(conflict).await.is_err());
        assert_eq!(store.state(), snapshot.state());
        let mut stale = transaction.clone();
        stale.id = TransactionId::new();
        assert!(store.transact(stale).await.is_err());
        let foreign = WorkspaceId::new();
        assert!(store
            .state()
            .record(Collection::Task, "task", &foreign)
            .is_err());
        assert!(store
            .state()
            .command(
                &workspace().id,
                &CommandId::parse("create").unwrap(),
                &"b".repeat(64)
            )
            .is_err());
        let mut missing = initial();
        missing.id = TransactionId::new();
        missing.expected_watermark = store.state().watermark;
        missing.command = None;
        missing.events.clear();
        missing.mutations = vec![Mutation::Put {
            expected: None,
            record: Record::typed(
                Collection::Session,
                "orphan",
                foreign.clone(),
                Revision::ZERO,
                &vcp_domain::workspace::Session {
                    id: SessionId::parse("orphan").unwrap(),
                    workspace: foreign,
                    revision: Revision::ZERO,
                    configuration: Revision::ZERO,
                    fork_origin: None,
                },
            )
            .unwrap(),
        }];
        assert!(store.transact(missing).await.is_err());
        assert_eq!(store.state(), snapshot.state());
        store.checkpoint().unwrap();
        drop(store);
        let mut reopened = Store::open(&root, backend, &[]).await.unwrap();
        assert_eq!(reopened.state(), snapshot.state());
        assert_eq!(
            reopened
                .state()
                .record(Collection::Access, "legacy-access", &workspace().id)
                .unwrap()
                .value,
            legacy
        );
        assert_eq!(reopened.transact(transaction).await.unwrap(), receipt);
        assert!(reopened.configuration().await.is_ok());
        drop(reopened);
        assert!(Store::open(
            &root,
            if backend == BackendKind::Sqlite {
                BackendKind::Files
            } else {
                BackendKind::Sqlite
            },
            &[]
        )
        .await
        .is_err());
    }
}

#[tokio::test]
async fn durable_artifacts_precede_references_and_snapshots_pin_orphans() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temporary = tempfile::tempdir().unwrap();
        let mut store = Store::open(&temporary.path().join("store"), backend, &[])
            .await
            .unwrap();
        store.transact(initial()).await.unwrap();
        let spec = spec();
        let mut writer = store.spool().create(spec.clone()).unwrap();
        writer.write_chunk(b"binary\0\xff").unwrap();
        let pending = store.spool().inspect(&spec.id).unwrap();
        store
            .transact(attach(store.state(), pending.clone(), None))
            .await
            .unwrap();
        let snapshot = store.snapshot().unwrap();
        writer.write_chunk(b"later").unwrap();
        let complete = writer.finalize().unwrap();
        drop(writer);
        store
            .transact(attach(store.state(), complete, Some(Revision::ZERO)))
            .await
            .unwrap();
        let old: ArtifactDescriptor = snapshot
            .state()
            .record(
                Collection::Artifact,
                spec.id.as_str(),
                &spec.scope.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let mut oldbytes = Vec::new();
        store.spool().read(&old, &mut oldbytes).unwrap();
        assert_eq!(oldbytes, b"binary\0\xff");
        assert!(!store.collect_orphan(&spec.id).unwrap());
        let orphan = common::spec();
        let mut writer = store.spool().create(orphan.clone()).unwrap();
        writer.write_chunk(b"safe orphan").unwrap();
        assert!(!store.collect_orphan(&orphan.id).unwrap());
        writer.finalize().unwrap();
        drop(writer);
        let pin = store.spool().pin(&orphan.id).unwrap();
        assert!(!store.collect_orphan(&orphan.id).unwrap());
        drop(pin);
        assert!(store.collect_orphan(&orphan.id).unwrap());
        assert!(!store.spool().root().join(orphan.id.as_str()).exists());
        let mut forged = pending;
        forged.spec.id = ArtifactId::new();
        assert!(store
            .transact(attach(store.state(), forged, None))
            .await
            .is_err());
    }
}

#[tokio::test]
async fn conversion_preserves_history_receipts_artifacts_and_source_in_both_directions() {
    for (from, to) in [
        (BackendKind::Sqlite, BackendKind::Files),
        (BackendKind::Files, BackendKind::Sqlite),
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("source");
        let target = temporary.path().join("converted");
        let mut store = Store::open(&root, from, &[]).await.unwrap();
        store.transact(initial()).await.unwrap();
        let spec = spec();
        let mut writer = store.spool().create(spec.clone()).unwrap();
        writer.write_chunk(b"prefix").unwrap();
        store
            .transact(attach(
                store.state(),
                store.spool().inspect(&spec.id).unwrap(),
                None,
            ))
            .await
            .unwrap();
        writer.write_chunk(b" tail").unwrap();
        let descriptor = writer.finalize().unwrap();
        drop(writer);
        store
            .transact(attach(
                store.state(),
                descriptor.clone(),
                Some(Revision::ZERO),
            ))
            .await
            .unwrap();
        let converted = store.convert(&target, to, &[]).await.unwrap();
        assert_eq!(converted.state(), store.state());
        let expected = store.state().clone();
        drop(converted);
        drop(store);
        let source = Store::open(&root, from, &[]).await.unwrap();
        let converted = Store::open(&target, to, &[]).await.unwrap();
        assert_eq!(source.state(), &expected);
        assert_eq!(converted.state(), &expected);
        let mut bytes = Vec::new();
        converted.spool().read(&descriptor, &mut bytes).unwrap();
        assert_eq!(bytes, b"prefix tail");
    }
}

#[tokio::test]
async fn committed_corruption_fails_closed_and_only_partial_tail_is_quarantined() {
    use std::io::Write;
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("files");
    let mut store = Store::open(&root, BackendKind::Files, &[]).await.unwrap();
    store.transact(initial()).await.unwrap();
    let state = store.state().clone();
    drop(store);
    let journal = root.join("canonical.frames");
    let before = std::fs::metadata(&journal).unwrap().len();
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&journal)
        .unwrap();
    file.write_all(b"VCPJ00").unwrap();
    file.sync_all().unwrap();
    drop(file);
    let store = Store::open(&root, BackendKind::Files, &[]).await.unwrap();
    assert_eq!(store.state(), &state);
    drop(store);
    assert_eq!(std::fs::metadata(&journal).unwrap().len(), before);
    assert_eq!(
        std::fs::read_dir(&root)
            .unwrap()
            .filter(|e| e
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("torn-tail-"))
            .count(),
        1
    );
    let mut bytes = std::fs::read(&journal).unwrap();
    bytes[100] ^= 1;
    std::fs::write(&journal, bytes).unwrap();
    assert!(Store::open(&root, BackendKind::Files, &[]).await.is_err());
}

#[tokio::test]
async fn sqlite_busy_is_bounded_and_cannot_acknowledge_or_reuse_uncertain_connection() {
    use sqlx::Connection;
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("sqlite");
    let mut store = Store::open(&root, BackendKind::Sqlite, &[]).await.unwrap();
    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(root.join("canonical.sqlite"));
    let mut blocker = sqlx::SqliteConnection::connect_with(&options)
        .await
        .unwrap();
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut blocker)
        .await
        .unwrap();
    let started = std::time::Instant::now();
    assert!(matches!(
        store.transact(initial()).await,
        Err(Error::Unavailable(_))
    ));
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert!(!store.healthy());
    assert_eq!(store.state().watermark.get(), 0);
    assert!(store.transact(initial()).await.is_err());
    sqlx::query("ROLLBACK").execute(&mut blocker).await.unwrap();
    blocker.close().await.unwrap();
    drop(store);
    let mut reopened = Store::open(&root, BackendKind::Sqlite, &[]).await.unwrap();
    assert_eq!(reopened.state().watermark.get(), 0);
    assert!(reopened.transact(initial()).await.is_ok());
}

#[tokio::test]
async fn truncating_an_acknowledged_file_commit_is_corruption_not_a_recoverable_tail() {
    let temporary = tempfile::tempdir().unwrap();
    let mut store = Store::open(temporary.path(), BackendKind::Files, &[])
        .await
        .unwrap();
    store.transact(initial()).await.unwrap();
    drop(store);
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(temporary.path().join("canonical.frames"))
        .unwrap();
    let len = file.metadata().unwrap().len();
    file.set_len(len - 10).unwrap();
    file.sync_all().unwrap();
    drop(file);
    assert!(matches!(
        Store::open(temporary.path(), BackendKind::Files, &[]).await,
        Err(Error::Corruption("acknowledged journal was truncated"))
    ));
}
