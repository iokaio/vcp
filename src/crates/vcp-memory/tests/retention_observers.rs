// SPDX-License-Identifier: Apache-2.0
#[path = "../../vcp-store/tests/common/mod.rs"]
mod common;
use vcp_domain::{
    retention_selector::{Criterion, Selector, Tree},
    task::TaskState,
    *,
};
use vcp_engine::observers::{
    budget::Limits,
    subscription::{Input, State as Observer},
};
use vcp_memory::{
    access::Access,
    retention::{self, Action, Target},
};
use vcp_store::{contract::*, BackendKind, Store};

fn observer_record() -> Record {
    let task = common::task();
    let mut observer = Observer::new(task.root.clone(), Limits::default()).unwrap();
    observer.set_enabled(true).unwrap();
    observer
        .enqueue(
            Input {
                root: task.root.clone(),
                task: task.root.clone(),
                steering: SteeringRevision::ZERO,
                task_revision: Revision::ZERO,
                authority: AuthorityRevision::ZERO,
                deletion: DeletionEpoch::ZERO,
                input_digest: vcp_protocol::digest_bytes(b"private-input-marker"),
                pattern_digest: vcp_protocol::digest_bytes(b"private-pattern-marker"),
                watermark: Watermark::new(1),
                deadline: Timestamp::new(10000),
            },
            true,
        )
        .unwrap();
    observer.begin(Timestamp::new(200), true).unwrap().unwrap();
    let id = format!(
        "observer-{}",
        vcp_protocol::digest_bytes(task.root.as_str().as_bytes())
    );
    Record::typed(Collection::Projection,id,task.scope.workspace.clone(),Revision::ZERO,&serde_json::json!({"schema_version":1,"document_type":redaction::OBSERVER_SOURCE,"scope":task.scope,"revision":Revision::ZERO,"owner":"a".repeat(64),"state":observer,"notice":"private-observer-copy","elapsed_millis":1})).unwrap()
}
#[tokio::test]
async fn observer_retention_closure_scrubs_payload_and_keeps_absorbing_identity() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
        let mut initial = common::initial();
        let mut task = common::task();
        task.state = TaskState::Cancelled;
        initial.mutations[2] = Mutation::Put {
            expected: None,
            record: Record::typed(
                Collection::Task,
                task.scope.task.to_string(),
                task.scope.workspace.clone(),
                task.revision,
                &task,
            )
            .unwrap(),
        };
        store.transact(initial).await.unwrap();
        let original = observer_record();
        let target = Target::Record(original.key());
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![Mutation::Put {
                    record: original.clone(),
                    expected: None,
                }],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        let access = Access {
            workspace: task.scope.workspace.clone(),
            actor: ActorId::parse("owner").unwrap(),
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            tasks: None,
        };
        let preview = retention::preview(
            &store,
            &access,
            Selector {
                schema_version: 1,
                tree: Tree::Match(Criterion::Task(task.scope.task.clone())),
            },
            Action::Purge,
            Timestamp::new(300),
        )
        .unwrap();
        assert!(preview.selected.contains(&target) || preview.dependent.contains(&target));
        let receipt = retention::apply(&mut store, &access, &preview, Timestamp::new(301))
            .await
            .unwrap();
        assert!(!retention::recall_allowed(store.state(), &access.workspace, &target).unwrap());
        retention::cleanup(&mut store, &access, &receipt.id, Timestamp::new(302))
            .await
            .unwrap();
        let row = &store.state().records[&original.key()];
        let tombstone: redaction::RedactedObserver = row.decode().unwrap();
        tombstone.validate().unwrap();
        assert_eq!(tombstone.id, original.id);
        assert!(row.value.get("state").is_none());
        let retained =
            String::from_utf8(vcp_protocol::canonical_bytes(store.state()).unwrap()).unwrap();
        for marker in [
            "private-observer-copy",
            &vcp_protocol::digest_bytes(b"private-input-marker"),
            &vcp_protocol::digest_bytes(b"private-pattern-marker"),
        ] {
            assert!(!retained.contains(marker));
        }
        let mut replacement = original.clone();
        replacement.revision = Revision::new(1);
        replacement.value["revision"] = serde_json::json!(Revision::new(1));
        assert!(store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![Mutation::Put {
                    record: replacement,
                    expected: Some(Revision::ZERO)
                }],
                events: vec![],
                command: None
            })
            .await
            .is_err());
        store.close().await.unwrap();
        let reopened = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert!(retention::purged(reopened.state(), &access.workspace, &target).unwrap());
        assert_eq!(
            reopened.state().records[&original.key()].value["document_type"],
            redaction::OBSERVER
        );
    }
}
#[test]
fn observer_projection_unknown_fields_or_identity_are_rejected() {
    let original = observer_record();
    original.validate_shape().unwrap();
    for mutation in ["unknown", "state_unknown", "scope"] {
        let mut row = original.clone();
        match mutation {
            "unknown" => row.value["unexpected"] = true.into(),
            "state_unknown" => row.value["state"]["unexpected"] = true.into(),
            _ => row.value["state"]["root"] = "other-root".into(),
        };
        assert!(row.validate_shape().is_err());
    }
}
