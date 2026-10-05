// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_domain::{CommandId, EventId, TransactionId};
use vcp_protocol::command::CommandReceipt;
use vcp_store::{contract::CanonicalStore, BackendKind, CanonicalHistory, Store};
#[path = "../../../vcp-store/tests/common/mod.rs"]
mod common;

fn normalized(mut value: Value) -> Value {
    value["collection"]["elapsed_micros"] = json!(0);
    value
}

#[tokio::test]
async fn native_snapshot_bundle_keeps_cut_across_append_and_reopen() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("canonical");
        let mut store = Store::open(&root, backend, &[]).await.unwrap();
        store.transact(common::initial()).await.unwrap();
        let access = Access {
            workspace: common::workspace().id,
            authority: common::workspace().authority,
            read: true,
            tasks: None,
        };
        let task = common::task().scope.task;
        let archive = store.archive_state().await.unwrap();
        let expected = normalized(collect(&archive, &access, &task).unwrap());
        let pinned = store.snapshot().unwrap();
        let initial = common::initial();
        let command = initial.command.unwrap();
        let receipt: CommandReceipt = CanonicalHistory::command_receipt(
            &pinned,
            &access.workspace,
            &command.command,
            &command.digest,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(receipt.watermark, archive.watermark);
        assert!(CanonicalHistory::command_receipt(
            &pinned,
            &access.workspace,
            &command.command,
            &"b".repeat(64)
        )
        .await
        .is_err());
        assert!(CanonicalHistory::command_receipt(
            &pinned,
            &access.workspace,
            &CommandId::new(),
            &command.digest
        )
        .await
        .unwrap()
        .is_none());
        let mut next = common::initial();
        next.id = TransactionId::new();
        next.expected_watermark = CanonicalStore::current(&store).watermark;
        next.mutations.clear();
        next.events[0].id = EventId::new();
        next.events[0].kind = vcp_protocol::event::EventKind::Diagnostic;
        next.events[0].data = json!({"later":"must not enter pinned bundle"});
        next.events[0].correlation = CommandId::new();
        next.command = None;
        store.transact(next).await.unwrap();
        assert_eq!(
            normalized(collect_store(&pinned, &access, &task).await.unwrap()),
            expected
        );
        let current = normalized(collect_store(&store, &access, &task).await.unwrap());
        assert_ne!(current["source_watermark"], expected["source_watermark"]);
        drop(pinned);
        store.close().await.unwrap();
        let reopened = Store::open(&root, backend, &[]).await.unwrap();
        let pinned = reopened.snapshot().unwrap();
        assert_eq!(
            normalized(collect_store(&pinned, &access, &task).await.unwrap()),
            current
        );
        drop(pinned);
        reopened.close().await.unwrap();
    }
}
