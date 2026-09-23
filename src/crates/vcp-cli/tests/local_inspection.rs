// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
//! Retained inspection references through the compiled authenticated observer.
#[path = "support/local_fixture.rs"]
mod local_fixture;
use local_fixture::*;
use serde_json::{json, Value};
use vcp_domain::{
    artifact::{ArtifactSpec, Channel},
    ids::*,
    revision::*,
    task::Task,
    workspace::Scope,
};
use vcp_store::{
    artifact::ArtifactWriter,
    contract::{CanonicalStore, Collection, Mutation, Record, Transaction},
    BackendKind, Store,
};

fn artifact(store: &Store, scope: &Scope, name: &str, schema: &str, channel: Channel) -> Record {
    let mut writer = store
        .spool()
        .create(ArtifactSpec {
            id: ArtifactId::parse(name).unwrap(),
            scope: scope.clone(),
            media_type: "application/json".into(),
            schema: schema.into(),
            source: "offline inspection fixture".into(),
            channel,
            retention: "history".into(),
            omissions: vec![],
        })
        .unwrap();
    writer.write_chunk(b"{\"private\":\"evidence\"}").unwrap();
    let descriptor = writer.finalize().unwrap();
    Record::typed(
        Collection::Artifact,
        name,
        scope.workspace.clone(),
        Revision::ZERO,
        &descriptor,
    )
    .unwrap()
}

fn initialize(client: &mut Client, fixture: &Fixture) {
    let methods = [
        "context/inspect",
        "routing/explain",
        "artifact/read",
        "workspace/open",
    ];
    let reply = client.rpc(
        1,
        "initialize",
        json!({"protocol_version":"1.0",
        "client":{"name":"compiled-inspection","version":"1"},
        "capabilities":methods,"required_capabilities":methods}),
    );
    assert!(reply.get("error").is_none(), "{reply}");
    assert_eq!(
        reply["result"]["execution_host"]["id"],
        fixture.config.binding.host.as_str()
    );
}

fn evidence(client: &mut Client, id: u64, method: &str, request: Value) -> Value {
    let reply = client.rpc(id, method, request);
    assert!(reply.get("error").is_none(), "{reply}");
    assert_eq!(reply["result"]["kind"], "evidence");
    let page = reply["result"]["value"].clone();
    assert!(!page.to_string().contains("private"));
    for row in page["rows"].as_array().unwrap() {
        assert!(row.get("data").is_none());
        assert!(row.get("record").is_none());
        assert_eq!(row["content"]["artifact"], row["id"]);
        assert_eq!(row["content"]["offset"], "0");
        assert_eq!(row["content"]["length"], "22");
        assert_eq!(
            row["content"]["sha256"],
            vcp_protocol::digest_bytes(b"{\"private\":\"evidence\"}")
        );
    }
    page
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_retained_inspection_pages_references_and_rejects_wrong_selection_or_changed_source(
) {
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
        let mut records = vec![Record::typed(
            Collection::Task,
            other.scope.task.as_str(),
            other.scope.workspace.clone(),
            other.revision,
            &other,
        )
        .unwrap()];
        for (name, schema, scope, channel) in [
            (
                "a-context",
                "context-manifest/1",
                &task.scope,
                Channel::Evidence,
            ),
            (
                "b-context",
                "context-manifest/1",
                &task.scope,
                Channel::Evidence,
            ),
            (
                "route",
                "routing-selection/1",
                &task.scope,
                Channel::Evidence,
            ),
            (
                "foreign-context",
                "context-manifest/1",
                &other.scope,
                Channel::Evidence,
            ),
            (
                "wrong-schema",
                "context-manifest/2",
                &task.scope,
                Channel::Evidence,
            ),
            (
                "wrong-channel",
                "context-manifest/1",
                &task.scope,
                Channel::Response,
            ),
        ] {
            records.push(artifact(&store, scope, name, schema, channel));
        }
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: records
                    .into_iter()
                    .map(|record| Mutation::Put {
                        expected: None,
                        record,
                    })
                    .collect(),
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        store.close().await.unwrap();

        let mut client = Client::connect(&fixture, "observer");
        initialize(&mut client, &fixture);
        let open = json!({"command_id":"observer-open","host":fixture.config.binding.host,
            "root":fixture.config.binding.root});
        let workspace = client.rpc(30, "workspace/open", open.clone());
        assert!(workspace.get("error").is_none(), "{workspace}");
        assert_eq!(workspace["result"]["kind"], "workspace");
        let view = &workspace["result"]["value"];
        assert_eq!(view["workspace"], fixture.config.workspace.as_str());
        assert_eq!(view["host"], fixture.config.binding.host.as_str());
        assert_eq!(view["root"], fixture.config.binding.root);
        assert!(view["trust"].is_string());
        assert!(view["revision"].is_string());
        assert!(view["authority_revision"].is_string());
        assert_eq!(
            client.rpc(31, "workspace/open", open.clone())["result"],
            workspace["result"]
        );
        let mut wrong_host = open.clone();
        wrong_host["host"] = json!(HostId::new());
        assert!(client
            .rpc(32, "workspace/open", wrong_host)
            .get("error")
            .is_some());
        let mut wrong_root = open;
        wrong_root["root"] = json!(fixture.workspace.join("other").to_string_lossy());
        assert!(client
            .rpc(33, "workspace/open", wrong_root)
            .get("error")
            .is_some());
        let request = json!({"scope":fixture.scope(),"task":task.scope.task,
            "target":null,"cursor":null,"limit":1});
        let first = evidence(&mut client, 2, "context/inspect", request.clone());
        assert_eq!(
            client.rpc(
                34,
                "workspace/open",
                json!({"command_id":"observer-open",
            "host":fixture.config.binding.host,"root":fixture.config.binding.root})
            )["result"],
            workspace["result"]
        );
        assert_eq!(first["rows"].as_array().unwrap().len(), 1);
        assert_eq!(first["rows"][0]["id"], "a-context");
        assert_eq!(first["complete"], false);
        assert!(first["next_cursor"].is_string());
        let mut continuation = request.clone();
        continuation["cursor"] = first["next_cursor"].clone();
        let second = evidence(&mut client, 3, "context/inspect", continuation.clone());
        assert_eq!(second["rows"].as_array().unwrap().len(), 1);
        assert_eq!(second["rows"][0]["id"], "b-context");
        assert_eq!(second["watermark"], first["watermark"]);
        assert_eq!(second["complete"], true);
        assert!(second["next_cursor"].is_null());
        let route = evidence(&mut client, 4, "routing/explain", request.clone());
        assert_eq!(route["rows"].as_array().unwrap().len(), 1);
        assert_eq!(route["rows"][0]["id"], "route");
        assert_eq!(route["rows"][0]["schema"], "routing-selection/1");
        assert_eq!(route["complete"], true);
        let mut selected = request.clone();
        selected["target"] = json!("b-context");
        let target = evidence(&mut client, 5, "context/inspect", selected);
        assert_eq!(target["rows"][0]["id"], "b-context");
        assert_eq!(target["complete"], true);
        let raw = client.rpc(
            6,
            "artifact/read",
            json!({"scope":fixture.scope(),
            "task":task.scope.task,"artifact":first["rows"][0]["content"]["artifact"],
            "offset":"0","length":65536}),
        );
        assert!(raw.get("error").is_none(), "{raw}");
        assert_eq!(
            raw["result"]["value"]["content"],
            "eyJwcml2YXRlIjoiZXZpZGVuY2UifQ=="
        );
        assert_eq!(raw["result"]["value"]["encoding"], "base64");
        assert_eq!(raw["result"]["value"]["complete"], true);

        for (index, target) in [
            "foreign-context",
            "wrong-schema",
            "wrong-channel",
            "route",
            task.scope.task.as_str(),
            "absent",
        ]
        .into_iter()
        .enumerate()
        {
            let mut denied = request.clone();
            denied["target"] = json!(target);
            let reply = client.rpc(10 + index as u64, "context/inspect", denied);
            assert!(reply.get("error").is_some(), "{reply}");
        }
        let mut wrong_scope = request.clone();
        wrong_scope["scope"]["session"] = json!(SessionId::new());
        assert!(client
            .rpc(20, "context/inspect", wrong_scope)
            .get("error")
            .is_some());
        assert!(client
            .rpc(21, "routing/explain", continuation.clone())
            .get("error")
            .is_some());
        let mut changed_query = continuation.clone();
        changed_query["limit"] = json!(2);
        assert!(client
            .rpc(22, "context/inspect", changed_query)
            .get("error")
            .is_some());
        assert!(client.finish().await.0.success());

        // A genuine canonical commit between attachments invalidates the saved
        // source boundary; this is not a forged cursor or a scope-only failure.
        let mut store = fixture.reopen().await;
        assert!(!store
            .state()
            .commands
            .values()
            .any(|receipt| receipt.command.as_str() == "observer-open"));
        let record = artifact(
            &store,
            &task.scope,
            "c-context",
            "context-manifest/1",
            Channel::Evidence,
        );
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![Mutation::Put {
                    expected: None,
                    record,
                }],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        store.close().await.unwrap();
        let mut client = Client::connect(&fixture, "observer");
        initialize(&mut client, &fixture);
        assert!(client
            .rpc(2, "context/inspect", continuation)
            .get("error")
            .is_some());
        let mut fresh = request;
        fresh["limit"] = json!(128);
        let page = evidence(&mut client, 3, "context/inspect", fresh);
        assert_eq!(page["rows"].as_array().unwrap().len(), 3);
        assert_eq!(page["complete"], true);
        assert!(client.finish().await.0.success());
        fixture.reopen().await.close().await.unwrap();
    }
}
