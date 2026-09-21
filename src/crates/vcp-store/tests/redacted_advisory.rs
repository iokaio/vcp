// SPDX-License-Identifier: Apache-2.0
mod common;
use common::*;
use std::collections::BTreeSet;
use vcp_domain::{redaction, task::TaskState, *};
use vcp_store::{contract::*, BackendKind, Store};

fn advisory_rows() -> Vec<Record> {
    ["request", "result", "schedule", "accounting"]
        .into_iter()
        .map(|kind| {
            let mut row = Record::typed(
                Collection::Projection,
                format!("advisory-{kind}"),
                workspace().id,
                Revision::ZERO,
                &serde_json::json!({
                    "schema_version":1,"routing_encoding":"object_v1",
                    "document_type":format!("vcp_escalation_advisory_{kind}_v1"),
                    "document_version":1,"id":format!("advisory-{kind}"),
                    "workspace":workspace().id,"task":task().scope.task,
                    "private_evidence":"private-advisory-prose"
                }),
            )
            .unwrap();
            row.references
                .insert(key(Collection::Task, task().scope.task.as_str()));
            if kind != "request" {
                row.references
                    .insert(key(Collection::Projection, "advisory-request"));
            }
            row
        })
        .collect()
}

fn fixture(terminal: bool) -> Transaction {
    let mut tx = initial();
    for mutation in &mut tx.mutations {
        if let Mutation::Put { record, .. } = mutation {
            if record.collection == Collection::Task && terminal {
                let mut task = task();
                task.state = TaskState::Cancelled;
                record.value = serde_json::to_value(task).unwrap();
            }
            if record.collection == Collection::Workspace {
                let mut workspace = workspace();
                workspace.deletion = DeletionEpoch::new(1);
                record.value = serde_json::to_value(workspace).unwrap();
            }
        }
    }
    tx.mutations
        .extend(advisory_rows().into_iter().map(|record| Mutation::Put {
            expected: None,
            record,
        }));
    tx
}

fn put(store: &Store, record: Record, expected: Option<Revision>) -> Transaction {
    Transaction {
        id: TransactionId::new(),
        expected_watermark: store.state().watermark,
        mutations: vec![Mutation::Put { expected, record }],
        events: vec![],
        command: None,
    }
}

#[tokio::test]
async fn advisory_purge_strips_payload_preserves_identity_and_cannot_be_restored() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
        store.transact(fixture(true)).await.unwrap();
        // Mutable schedule revision must survive redaction too.
        let mut schedule = advisory_rows().remove(2);
        schedule.revision = Revision::new(1);
        schedule.value["revision"] = serde_json::json!(Revision::new(1));
        store
            .transact(put(&store, schedule, Some(Revision::ZERO)))
            .await
            .unwrap();
        let keys: BTreeSet<_> = advisory_rows().iter().map(Record::key).collect();
        let originals: Vec<_> = keys
            .iter()
            .map(|key| store.state().records[key].clone())
            .collect();
        let candidate = store
            .retention_candidate(&keys, &BTreeSet::new(), &BTreeSet::new())
            .unwrap();
        for source in &originals {
            let row = &candidate.records[&source.key()];
            let erased: redaction::RedactedAdvisory = row.decode().unwrap();
            assert_eq!(erased.document_type, redaction::ADVISORY);
            assert_eq!(erased.original_document_type, source.value["document_type"]);
            assert_eq!(erased.scope, task().scope);
            assert_eq!(row.id, source.id);
            assert_eq!(row.revision, source.revision);
            assert_eq!(row.references, source.references);
            assert_eq!(
                erased.original_digest,
                vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&source.value).unwrap())
            );
            assert!(!serde_json::to_string(row)
                .unwrap()
                .contains("private-advisory-prose"));
            let mut forged = row.clone();
            forged.id = format!("forged-{}", row.id);
            forged.revision = Revision::ZERO;
            forged.value["id"] = serde_json::json!(forged.id);
            forged.value["revision"] = serde_json::json!(Revision::ZERO);
            forged.validate_shape().unwrap();
            assert!(store.transact(put(&store, forged, None)).await.is_err());
        }
        let selected = originals[0].key();
        let mut invalid = candidate.clone();
        invalid.records.get_mut(&selected).unwrap().value["deletion"] =
            serde_json::json!(DeletionEpoch::new(2));
        assert!(invalid.validate().is_err());
        let mut invalid = candidate.clone();
        invalid.records.get_mut(&selected).unwrap().value["unexpected_payload"] =
            serde_json::json!("restored");
        assert!(invalid.validate().is_err());
        let mut invalid = candidate.clone();
        invalid
            .records
            .get_mut(&selected)
            .unwrap()
            .references
            .clear();
        assert!(store.rewrite_base(invalid, &[]).await.is_err());
        store.rewrite_base(candidate, &[]).await.unwrap();
        for mut source in originals {
            let previous = source.revision;
            source.revision = previous.next().unwrap();
            assert!(store
                .transact(put(&store, source, Some(previous)))
                .await
                .is_err());
        }
        store.close().await.unwrap();
        let reopened = Store::open(temp.path(), backend, &[]).await.unwrap();
        for key in &keys {
            let row = &reopened.state().records[key];
            row.decode::<redaction::RedactedAdvisory>()
                .unwrap()
                .validate()
                .unwrap();
            assert!(!serde_json::to_string(row)
                .unwrap()
                .contains("private-advisory-prose"));
        }
        reopened.close().await.unwrap();
    }
}

#[tokio::test]
async fn advisory_redaction_requires_terminal_recovery_and_exact_document_contract() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
        store.transact(fixture(false)).await.unwrap();
        let keys = BTreeSet::from([key(Collection::Projection, "advisory-request")]);
        assert!(store
            .retention_candidate(&keys, &BTreeSet::new(), &BTreeSet::new())
            .is_err());
        let mut unrelated = advisory_rows().remove(0);
        unrelated.id = "unrelated-projection".into();
        unrelated.value["id"] = serde_json::json!(unrelated.id);
        unrelated.value["document_type"] = serde_json::json!("vcp_routing_policy_v1");
        let keys = BTreeSet::from([unrelated.key()]);
        store.transact(put(&store, unrelated, None)).await.unwrap();
        assert!(store
            .retention_candidate(&keys, &BTreeSet::new(), &BTreeSet::new())
            .is_err());
        store.close().await.unwrap();
    }
}
