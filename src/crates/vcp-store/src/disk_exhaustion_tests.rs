// SPDX-License-Identifier: Apache-2.0
//! Bounded exhaustion regressions: SQLite's real page limit and injected journal
//! short writes. These are not evidence of a physically full Windows volume.
use super::*;
use vcp_domain::workspace::{Binding, Trust, Workspace};

fn fixture(boundary: &str) -> PathBuf {
    let root = tempfile::tempdir().unwrap().keep();
    println!(
        "P8-02 disk exhaustion boundary={boundary} evidence={}",
        root.display()
    );
    root
}

fn save_evidence(root: &Path, name: &str, value: &impl serde::Serialize) {
    fs::write(root.join(name), canonical_bytes(value).unwrap()).unwrap();
}

fn transaction(id: &str, watermark: Watermark, payload_bytes: usize) -> Transaction {
    let workspace = Workspace {
        id: WorkspaceId::parse(id).unwrap(),
        binding: Binding {
            host: HostId::parse("host").unwrap(),
            root: "C:/synthetic".into(),
            repository: "r".repeat(payload_bytes),
            worktree: "main".into(),
            revision: Revision::ZERO,
        },
        trust: Trust::Trusted,
        revision: Revision::ZERO,
        authority: AuthorityRevision::ZERO,
        deletion: DeletionEpoch::ZERO,
    };
    Transaction {
        id: TransactionId::parse(id).unwrap(),
        expected_watermark: watermark,
        mutations: vec![Mutation::Put {
            expected: None,
            record: Record::typed(
                Collection::Workspace,
                workspace.id.to_string(),
                workspace.id.clone(),
                workspace.revision,
                &workspace,
            )
            .unwrap(),
        }],
        events: vec![],
        command: None,
    }
}

async fn assert_recovery(mut store: Store, acknowledged: State, attempted: Transaction) {
    assert!(!store.healthy(), "failed append must poison the writer");
    assert_eq!(store.state(), &acknowledged);
    assert!(matches!(
        store.transact(attempted.clone()).await,
        Err(Error::Unavailable(_))
    ));
    let root = store.root().to_owned();
    let kind = store.kind();
    store.close().await.unwrap();

    // Reopening clears the test-only capacity constraint, as freeing capacity
    // would. It must retain every acknowledged byte and omit the failed append.
    let mut reopened = Store::open(&root, kind, &[]).await.unwrap();
    assert_eq!(reopened.state(), &acknowledged);
    let receipt = reopened.transact(attempted.clone()).await.unwrap();
    let recovered = reopened.state().clone();
    assert_eq!(recovered.watermark, acknowledged.watermark.next().unwrap());
    assert_eq!(
        recovered.transactions.len(),
        acknowledged.transactions.len() + 1
    );
    assert_eq!(recovered.records.len(), acknowledged.records.len() + 1);
    assert_eq!(reopened.transact(attempted.clone()).await.unwrap(), receipt);
    assert_eq!(reopened.state(), &recovered);
    reopened.close().await.unwrap();

    let mut final_open = Store::open(&root, kind, &[]).await.unwrap();
    assert_eq!(final_open.state(), &recovered);
    assert_eq!(final_open.transact(attempted).await.unwrap(), receipt);
    assert_eq!(final_open.state(), &recovered);
    final_open.close().await.unwrap();
}

#[tokio::test]
async fn sqlite_full_preserves_acknowledged_state_and_retry_is_exactly_once() {
    let evidence = fixture("sqlite_page_limit");
    let root = evidence.join("canonical");
    let mut store = Store::open(&root, BackendKind::Sqlite, &[]).await.unwrap();
    store
        .transact(transaction("acknowledged", Watermark::ZERO, 32))
        .await
        .unwrap();
    let acknowledged = store.state().clone();
    save_evidence(&evidence, "acknowledged.json", &acknowledged);
    let Backend::Sqlite(connection) = &mut store.backend else {
        unreachable!()
    };
    let pages: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    // SQLite clamps a limit below the current size to the current page count.
    let limit: i64 = sqlx::query_scalar("PRAGMA max_page_count=1")
        .fetch_one(connection)
        .await
        .unwrap();
    assert_eq!(limit, pages);
    let attempted = transaction("retry", acknowledged.watermark, 128 * 1024);
    let error = store.transact(attempted.clone()).await.unwrap_err();
    match error {
        Error::Database(sqlx::Error::Database(database)) => {
            assert_eq!(
                database.code().as_deref(),
                Some("13"),
                "expected SQLITE_FULL"
            );
        }
        other => panic!("expected SQLite page exhaustion, got {other:?}"),
    }
    assert_recovery(store, acknowledged, attempted).await;
    save_evidence(
        &evidence,
        "outcome.json",
        &serde_json::json!({
            "boundary": "sqlite_page_limit", "status": "pass", "error_code": "13",
            "acknowledged_state_preserved": true, "retry_exactly_once": true
        }),
    );
}

#[tokio::test]
async fn journal_injected_storage_full_preserves_acknowledged_state_and_retry_is_exactly_once() {
    // Partial header, payload, checksum and commit marker. Only incomplete
    // unacknowledged frames may be quarantined during reopen.
    for boundary in ["header", "payload", "checksum", "marker"] {
        let evidence = fixture(boundary);
        let root = evidence.join("canonical");
        let mut store = Store::open(&root, BackendKind::Files, &[]).await.unwrap();
        store
            .transact(transaction("acknowledged", Watermark::ZERO, 32))
            .await
            .unwrap();
        let acknowledged = store.state().clone();
        save_evidence(&evidence, "acknowledged.json", &acknowledged);
        let attempted = transaction("retry", acknowledged.watermark, 4096);
        let (_, prepared) = acknowledged.prepare(&attempted).unwrap();
        let payload = canonical_bytes(&prepared).unwrap().len();
        let header = 8 + 4 + 4 + 64;
        let budget = match boundary {
            "header" => header / 2,
            "payload" => header + payload / 2,
            "checksum" => header + payload + 32,
            "marker" => header + payload + 64 + 4,
            _ => unreachable!(),
        };
        let frames = root.join("canonical.frames");
        let before = fs::metadata(&frames).unwrap().len();
        let Backend::Files(journal) = &mut store.backend else {
            unreachable!()
        };
        journal.write_budget = Some(budget);
        let error = store.transact(attempted.clone()).await.unwrap_err();
        assert!(matches!(error, Error::Io(ref io) if io.kind() == std::io::ErrorKind::StorageFull));
        assert_eq!(fs::metadata(&frames).unwrap().len(), before + budget as u64);
        assert_recovery(store, acknowledged, attempted).await;
        let tails: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("torn-tail-")
            })
            .collect();
        assert_eq!(tails.len(), 1, "missing quarantined {boundary} short write");
        assert_eq!(fs::metadata(&tails[0]).unwrap().len(), budget as u64);
        save_evidence(
            &evidence,
            "outcome.json",
            &serde_json::json!({
                "boundary": boundary, "status": "pass", "error": "injected StorageFull",
                "short_write_bytes": budget, "acknowledged_state_preserved": true,
                "retry_exactly_once": true
            }),
        );
    }
}
