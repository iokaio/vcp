// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
//! Offline export qualification through the actual compiled local bridge.
#[path = "support/local_fixture.rs"]
mod wire;
use base64::Engine as _;
use serde_json::{json, Value};
use vcp_domain::{artifact::*, task::Task, verification::Fingerprint, workspace::Scope, *};
use vcp_engine::{Access, Engine, HostFacts};
use vcp_protocol::command::{Command, CommandEnvelope};
use vcp_store::{
    artifact::ArtifactWriter,
    contract::{Collection, State},
    BackendKind, Store,
};
const PROFILE: &str = "session/export-local/1";
const SOURCE: &[u8] = b"explicitly-authorized retained source octets\0\xff";

fn initialize(client: &mut wire::Client, profile: bool) {
    let mut methods = vec![
        "session/export",
        "artifact/read",
        "task/read",
        "command/read",
        "controller/read",
        "controller/acquire",
    ];
    if profile {
        methods.push(PROFILE);
    }
    let reply=client.rpc(1,"initialize",json!({"protocol_version":"1.0","client":{"name":"compiled-local-export","version":"1"},"capabilities":methods,"required_capabilities":methods}));
    assert!(reply.get("error").is_none(), "{reply}");
}
fn acquire(client: &mut wire::Client, fixture: &wire::Fixture, name: &str) {
    let lease = client.rpc(2, "controller/read", json!({"scope":fixture.scope()}));
    assert!(lease.get("error").is_none(), "{lease}");
    wire::accepted(&client.rpc(3,"controller/acquire",json!({"scope":fixture.scope(),"command_id":name,"expected_revision":lease["result"]["value"]["revision"]})));
}
fn request(fixture: &wire::Fixture, name: &str, capture: &str) -> Value {
    json!({"scope":fixture.scope(),"mutation":{"command_id":name,"expected_revision":"1","steering_revision":"0"},"task":fixture.config.root_task,"capture":capture})
}
#[track_caller]
fn export(client: &mut wire::Client, request: Value) -> Value {
    let result = client.rpc(10, "session/export", request);
    assert!(result.get("error").is_none(), "{result}");
    assert_eq!(result["result"]["kind"], "export");
    result["result"]["value"].clone()
}
fn read(client: &mut wire::Client, fixture: &wire::Fixture, artifact: &Value) -> Vec<u8> {
    let mut bytes = Vec::new();
    for _ in 0..128 {
        let response=client.rpc(11,"artifact/read",json!({"scope":fixture.scope(),"task":fixture.config.root_task,"artifact":artifact,"offset":bytes.len().to_string(),"length":49152}));
        assert!(response.get("error").is_none(), "{response}");
        let page = &response["result"]["value"];
        assert_eq!(page["encoding"], "base64");
        let content = base64::engine::general_purpose::STANDARD
            .decode(page["content"].as_str().unwrap())
            .unwrap();
        bytes.extend_from_slice(&content);
        if bytes.len().to_string() == page["total_bytes"].as_str().unwrap() {
            assert_eq!(
                page["complete"], true,
                "physical capture is complete even though history export declares omissions"
            );
            assert_eq!(page["sha256"], vcp_protocol::digest_bytes(&bytes));
            return bytes;
        }
        assert!(!content.is_empty());
    }
    panic!("bounded export range count exceeded")
}
async fn finish(client: &mut wire::Client) {
    let (status, diagnostics) = client.finish().await;
    assert!(status.success(), "{diagnostics}");
    assert!(!diagnostics.contains("explicitly-authorized retained source"));
}
async fn issue(
    engine: &mut Engine<Store>,
    fixture: &wire::Fixture,
    payload: Command,
    expected: u64,
) {
    let access = Access {
        actor: fixture.config.actor.clone(),
        workspace: fixture.config.workspace.clone(),
        session: fixture.config.session.clone(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: false,
    };
    let command = CommandEnvelope {
        version: 1,
        id: CommandId::new(),
        workspace: access.workspace.clone(),
        session: access.session.clone(),
        task: Some(fixture.config.root_task.clone()),
        caller: access.actor.clone(),
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected: Revision::new(expected),
        steering: SteeringRevision::ZERO,
        payload,
    };
    engine
        .handle(command, &access, &HostFacts::inspect(Timestamp::new(100)))
        .await
        .unwrap();
}
async fn seed(fixture: &wire::Fixture) -> (ArtifactDescriptor, State) {
    let mut engine = Engine::new(fixture.reopen().await).unwrap();
    let mut writer = engine
        .store()
        .spool()
        .create(ArtifactSpec {
            id: ArtifactId::new(),
            scope: Scope {
                workspace: fixture.config.workspace.clone(),
                session: fixture.config.session.clone(),
                task: fixture.config.root_task.clone(),
            },
            media_type: "application/octet-stream".into(),
            schema: "compiled-export-evidence/1".into(),
            source: "independent offline fixture".into(),
            channel: Channel::Evidence,
            retention: "history".into(),
            omissions: vec![Omission::AuthenticationHeaders, Omission::RecoveryMaterial],
        })
        .unwrap();
    writer.write_chunk(SOURCE).unwrap();
    let descriptor = writer.finalize().unwrap();
    drop(writer);
    issue(
        &mut engine,
        fixture,
        Command::AttachArtifact {
            descriptor: descriptor.clone(),
        },
        0,
    )
    .await;
    let state = engine.store().state().clone();
    engine.into_store().close().await.unwrap();
    (descriptor, state)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_local_export_enforces_profile_and_scope_and_preserves_replay_visibility() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let fixture = wire::Fixture::new(backend).await;
        let (source, initial) = seed(&fixture).await;
        let metadata_request = request(&fixture, "metadata-export", "visible_history");
        let artifacts_request =
            request(&fixture, "artifact-export", "visible_history_and_artifacts");

        let mut legacy = wire::Client::connect(&fixture, "controller");
        initialize(&mut legacy, false);
        acquire(&mut legacy, &fixture, "legacy-acquire");
        let unsupported = legacy.rpc(4, "session/export", metadata_request.clone());
        assert_eq!(
            unsupported["error"]["data"]["details"]["code"],
            "CAPABILITY_UNAVAILABLE"
        );
        finish(&mut legacy).await;

        let mut observer = wire::Client::connect(&fixture, "observer");
        initialize(&mut observer, true);
        assert!(observer
            .rpc(4, "session/export", artifacts_request.clone())
            .get("error")
            .is_some());
        assert_eq!(
            read(&mut observer, &fixture, &json!(source.spec.id)),
            SOURCE
        );
        finish(&mut observer).await;

        let mut owner = wire::Client::connect(&fixture, "controller");
        initialize(&mut owner, true);
        acquire(&mut owner, &fixture, "owner-acquire");
        let metadata = export(&mut owner, metadata_request.clone());
        let artifact_export = export(&mut owner, artifacts_request.clone());
        for view in [&metadata, &artifact_export] {
            assert_eq!(view["complete"], false);
        }
        let metadata_payload: Value =
            serde_json::from_slice(&read(&mut owner, &fixture, &metadata["artifact"])).unwrap();
        assert_eq!(metadata_payload["history_profile"], "event-metadata/1");
        assert!(metadata_payload["artifacts"].as_array().unwrap().is_empty());
        assert!(!metadata_payload
            .to_string()
            .contains("offline attachment history"));
        let captured_payload = read(&mut owner, &fixture, &artifact_export["artifact"]);
        let captured: Value = serde_json::from_slice(&captured_payload).unwrap();
        assert_eq!(captured["artifacts"].as_array().unwrap().len(), 1);
        let retained: Vec<u8> =
            serde_json::from_value(captured["artifacts"][0]["bytes"].clone()).unwrap();
        assert_eq!(retained, SOURCE);
        assert_eq!(
            captured["artifacts"][0]["descriptor"],
            serde_json::to_value(&source).unwrap()
        );
        let manifest: Value = serde_json::from_slice(&read(
            &mut owner,
            &fixture,
            &artifact_export["visibility_manifest"],
        ))
        .unwrap();
        assert_eq!(manifest["secret_sanitization"], false);
        assert_eq!(manifest["complete"], false);
        assert!(!manifest["omissions"].as_array().unwrap().is_empty());
        assert_eq!(
            export(&mut owner, artifacts_request.clone()),
            artifact_export
        );
        let mut conflict = artifacts_request.clone();
        conflict["capture"] = json!("visible_history");
        assert_eq!(
            owner.rpc(12, "session/export", conflict)["error"]["data"]["details"]["code"],
            "COMMAND_CONFLICT"
        );
        let receipt = owner.rpc(
            13,
            "command/read",
            json!({"scope":fixture.scope(),"command_id":"artifact-export"}),
        );
        wire::accepted(&receipt);
        finish(&mut owner).await;

        let mut reader = wire::Client::connect(&fixture, "observer");
        initialize(&mut reader, true);
        assert_eq!(
            read(&mut reader, &fixture, &artifact_export["artifact"]),
            captured_payload
        );
        finish(&mut reader).await;
        let mut replacement = wire::Client::connect(&fixture, "controller");
        initialize(&mut replacement, true);
        acquire(&mut replacement, &fixture, "replacement-acquire");
        assert_eq!(
            export(&mut replacement, artifacts_request.clone()),
            artifact_export
        );
        assert_eq!(
            replacement.rpc(
                13,
                "command/read",
                json!({"scope":fixture.scope(),"command_id":"artifact-export"})
            )["result"],
            receipt["result"]
        );
        finish(&mut replacement).await;

        let store = fixture.reopen().await;
        wire::assert_offline_paused(&store, &fixture.config);
        for (key, row) in &initial.records {
            // The fixture did not have a controller lease before launching.
            assert_eq!(store.state().records.get(key), Some(row));
        }
        assert_eq!(
            store
                .state()
                .records
                .values()
                .filter(|r| r.collection == Collection::Artifact)
                .count(),
            5
        );
        assert_eq!(
            store
                .state()
                .events
                .iter()
                .filter(|e| e.event.data.get("session_export").is_some())
                .count(),
            2
        );
        let mut engine = Engine::new(store).unwrap();
        issue(
            &mut engine,
            &fixture,
            Command::ObserveFingerprint {
                fingerprint: Fingerprint {
                    repository: "d".repeat(64),
                    buffers: "b".repeat(64),
                    environment: "c".repeat(64),
                },
            },
            1,
        )
        .await;
        engine.into_store().close().await.unwrap();
        let mut stale = wire::Client::connect(&fixture, "observer");
        initialize(&mut stale, true);
        for artifact in [
            &artifact_export["artifact"],
            &artifact_export["visibility_manifest"],
        ] {
            assert!(stale.rpc(14,"artifact/read",json!({"scope":fixture.scope(),"task":fixture.config.root_task,"artifact":artifact,"offset":"0","length":49152})).get("error").is_some());
        }
        finish(&mut stale).await;
        let store = fixture.reopen().await;
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
        assert_eq!(task.state, vcp_domain::task::TaskState::Paused);
        store.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_export_honors_explicit_host_denial_before_capture() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let fixture = wire::Fixture::new(backend).await;
        let entry_path = fixture
            .config
            .canonical_root
            .parent()
            .unwrap()
            .join("workspace.json");
        let mut entry: vcp_cli::settings::WorkspaceEntry =
            serde_json::from_slice(&std::fs::read(&entry_path).unwrap()).unwrap();
        entry
            .config
            .host_tool_denials
            .push(vcp_domain::policy::Denial {
                id: "deny-local-export".into(),
                origin: vcp_domain::policy::RuleOrigin::Host,
                reason: "compiled fixture forbids derived exports".into(),
                effects: Default::default(),
                tool: Some("session/export".into()),
                roots: Default::default(),
                paths: vec![],
            });
        vcp_cli::settings::save(&entry_path, &entry).unwrap();
        let mut client = wire::Client::connect(&fixture, "controller");
        initialize(&mut client, true);
        acquire(&mut client, &fixture, "denied-export-acquire");
        let denied = client.rpc(
            4,
            "session/export",
            request(&fixture, "denied-export", "visible_history"),
        );
        assert_eq!(denied["error"]["data"]["details"]["code"], "POLICY_DENIED");
        finish(&mut client).await;
        let store = fixture.reopen().await;
        assert!(!store
            .state()
            .commands
            .values()
            .any(|receipt| receipt.command.as_str() == "denied-export"));
        assert!(!store.state().events.iter().any(|event| event
            .event
            .data
            .get("session_export")
            .is_some()));
        wire::assert_offline_paused(&store, &fixture.config);
        store.close().await.unwrap();
    }
}
