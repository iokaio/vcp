// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
//! Compiled scoped retention, physical cleanup, and reconnect without a provider.
#[path = "support/local_fixture.rs"]
mod local_fixture;
use local_fixture::*;
use serde_json::{json, Value};
use std::{collections::BTreeSet, time::Duration};
use vcp_domain::{
    artifact::{ArtifactDescriptor, ArtifactSpec, Channel},
    ids::*,
    revision::*,
    task::Task,
};
use vcp_store::{
    artifact::ArtifactWriter,
    contract::{CanonicalStore, Collection, Mutation, Record, Transaction},
    BackendKind,
};

fn initialize(client: &mut Client, profile: bool) {
    let mut methods = vec![
        "memory/forgetPreview",
        "memory/forgetPreviewRead",
        "memory/forgetRead",
        "memory/forget",
        "task/read",
        "task/cancel",
        "artifact/read",
        "controller/read",
        "controller/acquire",
        "command/read",
    ];
    if profile {
        methods.push("memory/retention/1");
    }
    let reply = client.rpc(1, "initialize", json!({"protocol_version":"1.0","client":{"name":"compiled-scoped-retention","version":"1"},"capabilities":methods,"required_capabilities":methods}));
    assert!(reply.get("error").is_none(), "{reply}");
}
fn attach(ticket: &Value, role: &str, profile: bool) -> Client {
    let mut client = Client::spawn("local-bridge");
    client.send(json!({"schema":"vcp-local-attach/1","attachment":ticket,"role":role}));
    assert_eq!(client.receive()["schema"], "vcp-local-ready/1");
    initialize(&mut client, profile);
    client
}
fn value(reply: Value, kind: &str) -> Value {
    assert!(reply.get("error").is_none(), "{reply}");
    assert_eq!(reply["result"]["kind"], kind, "{reply}");
    reply["result"]["value"].clone()
}
fn task(client: &mut Client, fixture: &Fixture) -> Value {
    value(
        client.rpc(
            2,
            "task/read",
            json!({"scope":fixture.scope(),"task":fixture.config.root_task}),
        ),
        "task",
    )
}
fn mutation(client: &mut Client, fixture: &Fixture, command: &str) -> Value {
    let task = task(client, fixture);
    json!({"command_id":command,"expected_revision":task["revision"],"steering_revision":task["steering_revision"]})
}
fn preview_request(fixture: &Fixture, action: &str) -> Value {
    json!({"scope":fixture.scope(),"task":fixture.config.root_task,"selector":{"schema_version":1,"tree":{"operator":"match","value":{"kind":"task","value":fixture.config.root_task}}},"action":action,"limit":1})
}
fn preview(client: &mut Client, fixture: &Fixture, action: &str) -> Value {
    value(
        client.rpc(3, "memory/forgetPreview", preview_request(fixture, action)),
        "retention_preview",
    )
}
fn page_request(fixture: &Fixture, preview: &Value, offset: Value) -> Value {
    json!({"scope":fixture.scope(),"task":fixture.config.root_task,"preview":preview["preview"],"offset":offset,"limit":1})
}
fn forget(client: &mut Client, fixture: &Fixture, page: &Value, command: &str) -> Value {
    json!({"scope":fixture.scope(),"task":fixture.config.root_task,"mutation":mutation(client,fixture,command),"preview":page["preview"],"preview_digest":page["digest"]})
}
fn pages(client: &mut Client, fixture: &Fixture, first: &Value) -> BTreeSet<String> {
    let mut page = first.clone();
    let mut seen = BTreeSet::new();
    let mut rows = BTreeSet::new();
    let mut protected = 0u64;
    for _ in 0..8192 {
        assert_eq!(page["digest"], first["digest"]);
        assert_eq!(page["watermark"], first["watermark"]);
        let targets = page["targets"].as_array().unwrap();
        assert!(targets.len() <= 1);
        for target in targets {
            assert!(rows.insert(target.to_string()), "duplicate page row");
            assert!(
                seen.insert(target["target"].to_string()),
                "duplicate target"
            );
            protected += u64::from(!target["protected_reason"].is_null());
        }
        if page["next_offset"].is_null() {
            let total: u64 = ["selected_count", "dependent_count"]
                .into_iter()
                .map(|key| first[key].as_str().unwrap().parse::<u64>().unwrap())
                .sum();
            assert_eq!(rows.len() as u64, total);
            assert_eq!(
                protected,
                first["protected_count"]
                    .as_str()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap()
            );
            assert!(!seen.is_empty());
            return seen;
        }
        page = value(
            client.rpc(
                4,
                "memory/forgetPreviewRead",
                page_request(fixture, first, page["next_offset"].clone()),
            ),
            "retention_preview",
        );
    }
    panic!("bounded preview did not terminate");
}
async fn seed(fixture: &Fixture) -> ArtifactDescriptor {
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
    let mut writer = store
        .spool()
        .create(ArtifactSpec {
            id: ArtifactId::new(),
            scope: task.scope.clone(),
            media_type: "text/plain".into(),
            schema: "retention-fixture/1".into(),
            source: "offline retained evidence".into(),
            channel: Channel::Evidence,
            retention: "history".into(),
            omissions: vec![],
        })
        .unwrap();
    writer
        .write_chunk(b"private-retention-source-must-disappear")
        .unwrap();
    let descriptor = writer.finalize().unwrap();
    drop(writer);
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![Mutation::Put {
                expected: None,
                record: Record::typed(
                    Collection::Artifact,
                    descriptor.spec.id.as_str(),
                    task.scope.workspace,
                    Revision::ZERO,
                    &descriptor,
                )
                .unwrap(),
            }],
            events: vec![],
            command: None,
        })
        .await
        .unwrap();
    store.close().await.unwrap();
    descriptor
}

async fn foreign_rows(fixture: &Fixture, source: &ArtifactDescriptor, copied: bool) -> Vec<Record> {
    use vcp_engine::{Access, Engine, HostFacts};
    use vcp_protocol::command::{Command, CommandEnvelope};
    let mut engine = Engine::new(fixture.reopen().await).unwrap();
    let mut access = Access {
        actor: fixture.config.actor.clone(),
        workspace: fixture.config.workspace.clone(),
        session: fixture.config.session.clone(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: false,
    };
    let session = SessionId::new();
    let task = TaskId::new();
    let root: Task = engine
        .store()
        .state()
        .record(
            Collection::Task,
            fixture.config.root_task.as_str(),
            &access.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    for (scope_task, payload) in [
        (
            None,
            Command::CreateSession {
                id: session.clone(),
                fork_through: None,
            },
        ),
        (
            Some(task.clone()),
            Command::CreateTask {
                root: task.clone(),
                parent: None,
                fork_origin: None,
                objective: vcp_domain::task::Objective {
                    text: "foreign-session-immutable-marker".into(),
                    constraints: vec![],
                    acceptance: vec![],
                    source: EventId::new(),
                    steering: SteeringRevision::ZERO,
                },
                fingerprint: root.fingerprint.clone(),
                editing: false,
                required_checks: vec![],
            },
        ),
        (
            Some(task.clone()),
            Command::Transition {
                next: vcp_domain::task::TaskState::Cancelled,
                reason: "offline foreign fixture".into(),
                verification: None,
            },
        ),
    ] {
        let command = CommandEnvelope {
            version: 1,
            id: CommandId::new(),
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            task: scope_task,
            caller: access.actor.clone(),
            controller: engine.controller().clone(),
            owner_epoch: engine.owner_epoch(),
            expected: Revision::ZERO,
            steering: SteeringRevision::ZERO,
            payload,
        };
        engine
            .handle(command, &access, &HostFacts::inspect(Timestamp::new(1)))
            .await
            .unwrap();
        access.session = session.clone();
    }
    if copied {
        let scope = vcp_domain::workspace::Scope {
            workspace: access.workspace.clone(),
            session: session.clone(),
            task: task.clone(),
        };
        let mut writer = engine
            .store()
            .spool()
            .create(ArtifactSpec {
                id: ArtifactId::new(),
                scope,
                media_type: "application/json".into(),
                schema: "memory-context/1".into(),
                source: "foreign copied-context marker".into(),
                channel: Channel::Evidence,
                retention: "history".into(),
                omissions: vec![],
            })
            .unwrap();
        writer.write_chunk(&serde_json::to_vec(&json!([{"source":{"kind":"artifact","id":source.spec.id},"evidence":[source.spec.id]}])).unwrap()).unwrap();
        let descriptor = writer.finalize().unwrap();
        drop(writer);
        let command = CommandEnvelope {
            version: 1,
            id: CommandId::new(),
            workspace: access.workspace.clone(),
            session: session.clone(),
            task: Some(task.clone()),
            caller: access.actor.clone(),
            controller: engine.controller().clone(),
            owner_epoch: engine.owner_epoch(),
            expected: Revision::ZERO,
            steering: SteeringRevision::ZERO,
            payload: Command::AttachArtifact { descriptor },
        };
        engine
            .handle(command, &access, &HostFacts::inspect(Timestamp::new(2)))
            .await
            .unwrap();
    }
    let store = engine.into_store();
    let rows = store
        .state()
        .records
        .values()
        .filter(|row| {
            row.id == session.as_str()
                || row.id == task.as_str()
                || row.value["spec"]["scope"]["session"] == session.as_str()
        })
        .cloned()
        .collect();
    store.close().await.unwrap();
    rows
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_scoped_retention_pages_purges_and_replays_after_reconnect() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let fixture = Fixture::new(backend).await;
        let artifact = seed(&fixture).await;
        let foreign = foreign_rows(&fixture, &artifact, false).await;
        let mut owner = Client::spawn("local-bridge");
        let mut bootstrap = fixture.bootstrap("controller");
        bootstrap["transport"] = json!("windows_pipe");
        owner.send(bootstrap);
        let ready = owner.receive();
        assert_eq!(ready["schema"], "vcp-local-ready/1");
        let ticket = ready["attachment"].clone();
        initialize(&mut owner, true);
        let mut observer = attach(&ready["observer_attachment"], "observer", true);
        let mut legacy = attach(&ready["observer_attachment"], "observer", false);
        let denied = legacy.rpc(
            5,
            "memory/forgetPreview",
            preview_request(&fixture, "purge"),
        );
        assert_eq!(
            denied["error"]["data"]["details"]["code"],
            "CAPABILITY_UNAVAILABLE"
        );
        assert!(legacy.finish().await.0.success());
        let first = preview(&mut observer, &fixture, "purge");
        assert!(first["expires_in_ms"].as_u64().unwrap() <= 60000);
        let targets = pages(&mut observer, &fixture, &first);
        assert!(targets
            .iter()
            .any(|target| target.contains(artifact.spec.id.as_str())));
        assert!(!first.to_string().contains("private-retention-source"));
        let denied = forget(&mut observer, &fixture, &first, "observer-forget");
        assert!(observer
            .rpc(6, "memory/forget", denied)
            .get("error")
            .is_some());
        let before_lease = preview(&mut owner, &fixture, "exclude");
        let denied = forget(&mut owner, &fixture, &before_lease, "unleased-forget");
        assert!(owner.rpc(7, "memory/forget", denied).get("error").is_some());
        value(owner.rpc(8,"controller/acquire",json!({"scope":fixture.scope(),"command_id":"retention-owner","expected_revision":null})),"acceptance");
        let stale = preview(&mut owner, &fixture, "exclude");
        let cancel = json!({"scope":fixture.scope(),"task":fixture.config.root_task,"mutation":mutation(&mut owner,&fixture,"settle-retention-root"),"reason":"explicitly finish offline fixture before purge"});
        value(owner.rpc(9, "task/cancel", cancel), "acceptance");
        assert_eq!(task(&mut owner, &fixture)["state"], "cancelled");
        let stale_request = forget(&mut owner, &fixture, &stale, "stale-forget");
        assert!(owner
            .rpc(10, "memory/forget", stale_request)
            .get("error")
            .is_some());
        // Two frozen previews are retained; a third evicts the oldest.
        let oldest = preview(&mut owner, &fixture, "exclude");
        preview(&mut owner, &fixture, "compact");
        let current = preview(&mut owner, &fixture, "purge");
        let evicted = owner.rpc(
            11,
            "memory/forgetPreviewRead",
            page_request(&fixture, &oldest, json!(0)),
        );
        assert_eq!(evicted["error"]["data"]["details"]["code"], "CURSOR_GAP");
        let selected = pages(&mut owner, &fixture, &current);
        assert!(selected
            .iter()
            .any(|target| target.contains(artifact.spec.id.as_str())));
        let mut excessive = page_request(&fixture, &current, json!(0));
        excessive["limit"] = json!(129);
        assert!(owner
            .rpc(12, "memory/forgetPreviewRead", excessive)
            .get("error")
            .is_some());
        let original = forget(&mut owner, &fixture, &current, "purge-once");
        let forgotten = value(
            owner.rpc(13, "memory/forget", original.clone()),
            "forgotten",
        );
        assert_eq!(forgotten["job"]["logical_unavailable"], true);
        let job_request = json!({"scope":fixture.scope(),"task":fixture.config.root_task,"job":forgotten["job"]["job"]});
        let job = value(
            observer.rpc(14, "memory/forgetRead", job_request.clone()),
            "retention",
        );
        assert_eq!(job["job"], forgotten["job"]["job"]);
        assert_eq!(job["rewrite_complete"], true);
        assert_eq!(job["local_cleanup_complete"], true);
        assert!(observer.rpc(15,"artifact/read",json!({"scope":fixture.scope(),"task":fixture.config.root_task,"artifact":artifact.spec.id,"offset":"0","length":128})).get("error").is_some());
        assert!(owner.finish().await.0.success());
        let mut released = None;
        for _ in 0..100 {
            let controller = value(
                observer.rpc(16, "controller/read", json!({"scope":fixture.scope()})),
                "controller",
            );
            if controller["ownership"] == "released" {
                released = Some(controller["revision"].clone());
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let mut reconnect = attach(&ticket, "controller", true);
        value(reconnect.rpc(17,"controller/acquire",json!({"scope":fixture.scope(),"command_id":"retention-reconnect","expected_revision":released.expect("controller release")})),"acceptance");
        let replay = value(
            reconnect.rpc(18, "memory/forget", original.clone()),
            "forgotten",
        );
        assert_eq!(replay["acceptance"], forgotten["acceptance"]);
        assert_eq!(replay["job"]["job"], forgotten["job"]["job"]);
        assert_eq!(replay["job"]["deletion"], forgotten["job"]["deletion"]);
        let mut changed = original;
        changed["preview_digest"] = json!("f".repeat(64));
        let conflict = reconnect.rpc(19, "memory/forget", changed);
        assert_eq!(
            conflict["error"]["data"]["details"]["code"],
            "COMMAND_CONFLICT"
        );
        assert_eq!(
            value(
                observer.rpc(20, "memory/forgetRead", job_request),
                "retention"
            ),
            job
        );
        assert!(reconnect.finish().await.0.success());
        assert!(observer.finish().await.0.success());
        let store = fixture.reopen_within(Duration::from_secs(45)).await;
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
        assert!(task.redaction.is_some());
        assert!(store.spool().read(&artifact, &mut Vec::new()).is_err());
        for row in &foreign {
            assert_eq!(
                store
                    .state()
                    .record(row.collection, &row.id, &row.workspace)
                    .unwrap(),
                row
            );
        }
        for command in ["observer-forget", "unleased-forget", "stale-forget"] {
            assert!(!store
                .state()
                .commands
                .contains_key(&vcp_store::contract::command_key(
                    &fixture.config.workspace,
                    &CommandId::parse(command).unwrap()
                )));
        }
        assert!(store
            .state()
            .commands
            .contains_key(&vcp_store::contract::command_key(
                &fixture.config.workspace,
                &CommandId::parse("purge-once").unwrap()
            )));
        assert!(!store
            .state()
            .records
            .values()
            .any(|record| matches!(record.collection, Collection::Attempt | Collection::Effect)));
        store.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_foreign_session_copy_denies_entire_preview_without_disclosure() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let fixture = Fixture::new(backend).await;
        let source = seed(&fixture).await;
        let foreign = foreign_rows(&fixture, &source, true).await;
        let before = {
            let store = fixture.reopen().await;
            let before = store.state().clone();
            store.close().await.unwrap();
            before
        };
        let mut client = Client::connect(&fixture, "observer");
        initialize(&mut client, true);
        let denied = client.rpc(
            30,
            "memory/forgetPreview",
            preview_request(&fixture, "purge"),
        );
        assert!(denied.get("error").is_some(), "{denied}");
        assert!(!denied
            .to_string()
            .contains("foreign-session-immutable-marker"));
        for row in &foreign {
            assert!(!denied.to_string().contains(&row.id));
        }
        assert!(client.finish().await.0.success());
        let store = fixture.reopen().await;
        assert_eq!(
            store.state(),
            &before,
            "denied observer preview must not write"
        );
        store.close().await.unwrap();
    }
}
