// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
//! Public read routing through the compiled authenticated observer process.
#[path = "support/local_fixture.rs"]
mod local_fixture;
use local_fixture::*;
use serde_json::json;
use vcp_domain::{
    accounting::Ledger,
    artifact::{ArtifactSpec, Channel},
    ids::*,
    revision::*,
    task::Task,
};
use vcp_store::{
    artifact::ArtifactWriter,
    contract::{CanonicalStore, Collection, Mutation, Record, Transaction},
    BackendKind,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_observer_reads_exact_accounting_and_binary_ranges_with_scope_denial() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let fixture = Fixture::new(backend).await;
        let mut store = fixture.reopen().await;
        let task: Task = store
            .state()
            .record(
                Collection::Task,
                fixture.config.root_task.as_str(),
                &fixture.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let mut other = task.clone();
        other.scope.task = TaskId::new();
        other.root = other.scope.task.clone();
        other.revision = Revision::ZERO;
        let ledger_expected = store
            .state()
            .record(
                Collection::Ledger,
                task.scope.task.as_str(),
                &task.scope.workspace,
            )
            .ok()
            .map(|row| row.revision);
        let revision = ledger_expected
            .map(|rev| rev.next().unwrap())
            .unwrap_or(Revision::ZERO);
        let ledger = Ledger {
            schema_version: 1,
            scope: task.scope.clone(),
            revision,
            policy: PolicyRevision::ZERO,
            currency: "USD".to_owned().try_into().unwrap(),
            cap: Micros::new(u64::MAX),
            protected: Micros::ZERO,
            // No provider/reservation evidence exists in this offline fixture.
            settled: Micros::ZERO,
            active: Micros::ZERO,
            unresolved: Micros::ZERO,
            allocations: Default::default(),
            daily: None,
            overrun: false,
        };
        let artifact = ArtifactId::new();
        let mut writer = store
            .spool()
            .create(ArtifactSpec {
                id: artifact.clone(),
                scope: task.scope.clone(),
                media_type: "application/octet-stream".into(),
                schema: "compiled-read-fixture/1".into(),
                source: "offline fixture".into(),
                channel: Channel::Response,
                retention: "history".into(),
                omissions: vec![],
            })
            .unwrap();
        writer.write_chunk(&[0, 255, 128, 65, 66, 67]).unwrap();
        let descriptor = writer.finalize().unwrap();
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![
                    Mutation::Put {
                        expected: None,
                        record: Record::typed(
                            Collection::Task,
                            other.scope.task.as_str(),
                            other.scope.workspace.clone(),
                            other.revision,
                            &other,
                        )
                        .unwrap(),
                    },
                    Mutation::Put {
                        expected: ledger_expected,
                        record: Record::typed(
                            Collection::Ledger,
                            task.scope.task.as_str(),
                            task.scope.workspace.clone(),
                            revision,
                            &ledger,
                        )
                        .unwrap(),
                    },
                    Mutation::Put {
                        expected: None,
                        record: Record::typed(
                            Collection::Artifact,
                            artifact.as_str(),
                            task.scope.workspace.clone(),
                            Revision::ZERO,
                            &descriptor,
                        )
                        .unwrap(),
                    },
                ],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        store.close().await.unwrap();

        let mut client = Client::connect(&fixture, "observer");
        let methods = ["usage/read", "artifact/read"];
        let initialized = client.rpc(
            1,
            "initialize",
            json!({"protocol_version":"1.0",
            "client":{"name":"compiled-reads","version":"1"},
            "capabilities":methods,"required_capabilities":methods}),
        );
        assert!(initialized.get("error").is_none(), "{initialized}");
        let usage_request = json!({"scope":fixture.scope(),"task":task.scope.task,
            "target":null,"cursor":null,"limit":1});
        let usage = client.rpc(2, "usage/read", usage_request.clone());
        assert!(usage.get("error").is_none(), "{usage}");
        assert_eq!(usage["result"]["kind"], "usage");
        let totals = &usage["result"]["value"];
        assert_eq!(totals["currency"], "USD");
        assert_eq!(totals["cap_micros"], "18446744073709551615");
        assert_eq!(totals["settled_micros"], "0");
        assert_eq!(totals["reserved_micros"], "0");
        assert_eq!(totals["unresolved_micros"], "0");

        let range_request = json!({"scope":fixture.scope(),"task":task.scope.task,
            "artifact":artifact,"offset":"1","length":3});
        let range = client.rpc(3, "artifact/read", range_request.clone());
        assert!(range.get("error").is_none(), "{range}");
        assert_eq!(range["result"]["kind"], "artifact");
        let value = &range["result"]["value"];
        assert_eq!(value["encoding"], "base64");
        assert_eq!(value["content"], "/4BB");
        assert_eq!(value["offset"], "1");
        assert_eq!(value["total_bytes"], "6");
        assert_eq!(value["complete"], false);
        assert_eq!(value["sha256"], descriptor.sha256);
        let mut final_range = range_request.clone();
        final_range["offset"] = json!("4");
        let tail = client.rpc(4, "artifact/read", final_range);
        assert!(tail.get("error").is_none(), "{tail}");
        assert_eq!(tail["result"]["value"]["content"], "QkM=");
        assert_eq!(tail["result"]["value"]["complete"], true);

        let mut foreign_scope = usage_request.clone();
        foreign_scope["scope"]["session"] = json!(SessionId::new());
        assert!(client
            .rpc(5, "usage/read", foreign_scope)
            .get("error")
            .is_some());
        let mut foreign_scope = range_request.clone();
        foreign_scope["scope"]["workspace"] = json!(WorkspaceId::new());
        assert!(client
            .rpc(6, "artifact/read", foreign_scope)
            .get("error")
            .is_some());
        let mut wrong_task = range_request;
        wrong_task["task"] = json!(other.scope.task);
        assert!(client
            .rpc(7, "artifact/read", wrong_task)
            .get("error")
            .is_some());
        let mut wrong_root = usage_request;
        wrong_root["target"] = json!(other.scope.task);
        assert!(client
            .rpc(8, "usage/read", wrong_root)
            .get("error")
            .is_some());
        assert!(client.finish().await.0.success());
        fixture.reopen().await.close().await.unwrap();
    }
}
