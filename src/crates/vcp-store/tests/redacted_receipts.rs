// SPDX-License-Identifier: Apache-2.0
mod common;
use common::*;
use vcp_domain::{task::TaskState, *};
use vcp_protocol::command::CommandResult;
use vcp_store::{contract::*, BackendKind, Store};

#[tokio::test]
async fn inspection_receipt_copies_are_atomically_redacted_without_changing_commit_digest() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
        let mut tx = initial();
        let mut task = task();
        task.state = TaskState::Cancelled;
        task.objectives[0].text = "inspection-private-payload".into();
        for mutation in &mut tx.mutations {
            if let Mutation::Put { record, .. } = mutation {
                if record.collection == Collection::Task {
                    record.value = serde_json::to_value(&task).unwrap();
                }
                if record.collection == Collection::Workspace {
                    let mut w = workspace();
                    w.deletion = DeletionEpoch::new(1);
                    record.value = serde_json::to_value(w).unwrap();
                }
            }
        }
        tx.command.as_mut().unwrap().result = CommandResult::Inspection { task: Some(task) };
        let acknowledged = store.transact(tx.clone()).await.unwrap();
        let mut rewritten = store.state().clone();
        let command_id = command_key(&workspace().id, &tx.command.as_ref().unwrap().command);
        let old = rewritten.commands[&command_id].clone();
        rewritten.commands.get_mut(&command_id).unwrap().result =
            vcp_protocol::redaction::inspection(&old.result, DeletionEpoch::new(1)).unwrap();
        // One-copy mutation must never install a replay base.
        assert!(store.rewrite_base(rewritten.clone(), &[]).await.is_err());
        rewritten
            .transactions
            .get_mut(&old.transaction)
            .unwrap()
            .command = Some(rewritten.commands[&command_id].clone());
        store.rewrite_base(rewritten, &[]).await.unwrap();
        let retry = store.transact(tx).await.unwrap();
        assert_eq!(retry.digest, acknowledged.digest);
        assert_eq!(retry.watermark, acknowledged.watermark);
        assert!(matches!(
            retry.command.as_ref().unwrap().result,
            CommandResult::InspectionRedacted { .. }
        ));
        assert_eq!(
            retry.command.as_ref().unwrap(),
            &store.state().commands[&command_id]
        );
        store.close().await.unwrap();
        let reopened = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert!(matches!(
            reopened.state().commands[&command_id].result,
            CommandResult::InspectionRedacted { .. }
        ));
        reopened.close().await.unwrap();
    }
}
