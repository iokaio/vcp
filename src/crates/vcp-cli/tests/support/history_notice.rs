// SPDX-License-Identifier: Apache-2.0
//! A history query and its notice share one owner without invalidating previews.
use super::*;
use vcp_store::contract::{CanonicalStore, Transaction};

async fn data(fixture: &Fixture, args: &[&str]) -> Value {
    let output = fixture.run(args).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    records(&output)
        .into_iter()
        .find(|row| row["type"] == "result")
        .unwrap()["data"]
        .clone()
}

async fn open(fixture: &Fixture) -> vcp_store::Store {
    let directory = fs::read_dir(fixture.data.join("workspaces"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.join("workspace.json").is_file())
        .unwrap();
    let entry: vcp_cli::settings::WorkspaceEntry =
        serde_json::from_slice(&fs::read(directory.join("workspace.json")).unwrap()).unwrap();
    vcp_store::Store::open(
        &entry.config.canonical_root,
        entry.config.backend,
        &[fixture.workspace.canonicalize().unwrap()],
    )
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_history_notice_is_acknowledged_once_without_invalidating_preview() {
    for backend in ["sqlite", "files"] {
        let server = MockServer::start().await;
        let fixture = Fixture::new(&server.uri(), "complete");
        data(&fixture, &["storage", "configure", "--backend", backend]).await;
        fixture
            .paused_before_send("Retain a paused history fixture")
            .await;
        let mut store = open(&fixture).await;
        // Append a synthetic old observation through the canonical contract;
        // never rewrite retained bytes or receipts to manufacture notice age.
        let mut old = store.state().events[0].event.clone();
        old.id = vcp_domain::EventId::new();
        old.timestamp = Timestamp::ZERO;
        store
            .transact(Transaction {
                id: vcp_domain::TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![],
                events: vec![old],
                command: None,
            })
            .await
            .unwrap();
        let before = store.state().watermark;
        store.close().await.unwrap();

        let preview = data(
            &fixture,
            &["history", "prune", "--preview", "--action", "compact"],
        )
        .await;
        assert!(preview["retention_notice"].is_null());
        let store = open(&fixture).await;
        let preview_watermark = store.state().watermark;
        assert_eq!(preview_watermark, before.next().unwrap());
        assert!(!store
            .state()
            .records
            .values()
            .any(|record| record.value["document_type"] == "vcp_retention_notice_v1"));
        store.close().await.unwrap();
        data(
            &fixture,
            &["prune", "show", preview["id"].as_str().unwrap()],
        )
        .await;
        let store = open(&fixture).await;
        assert_eq!(store.state().watermark, preview_watermark);
        store.close().await.unwrap();

        let first = data(&fixture, &["history", "list", "--limit", "1"]).await;
        assert_eq!(first["retention_notice"]["due"], true);
        let store = open(&fixture).await;
        let acknowledged = store.state().watermark;
        assert_eq!(acknowledged, preview_watermark.next().unwrap());
        assert_eq!(
            store
                .state()
                .records
                .values()
                .filter(|record| record.value["document_type"] == "vcp_retention_notice_v1")
                .count(),
            1
        );
        store.close().await.unwrap();
        let second = data(&fixture, &["history", "list", "--limit", "1"]).await;
        assert!(second["retention_notice"].is_null());
        let store = open(&fixture).await;
        assert_eq!(store.state().watermark, acknowledged);
        store.close().await.unwrap();
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}
