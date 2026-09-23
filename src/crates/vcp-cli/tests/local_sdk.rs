// SPDX-License-Identifier: Apache-2.0
#![cfg(all(windows, feature = "qualification"))]
//! Built Node SDK against the actual compiled bridge and canonical stores.
#[path = "support/sdk_execution_fixture.rs"]
mod execution_fixture;
#[path = "support/local_fixture.rs"]
mod local_fixture;
use local_fixture::*;
use serde_json::{json, Value};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
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

async fn driver(input: Value) {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/sdk-ts/tests/compiled-server.mjs");
    let bytes = serde_json::to_vec(&input).unwrap();
    assert!(bytes.len() < 16 * 1024);
    let output = tokio::task::spawn_blocking(move || {
        let mut child =
            Command::new(std::env::var_os("VCP_TEST_NODE").expect("qualified native Node path"))
                .arg(script)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
        child.stdin.take().unwrap().write_all(&bytes).unwrap();
        let deadline = Instant::now() + Duration::from_secs(120);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() > deadline {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                panic!(
                    "bounded SDK driver timeout: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        child.wait_with_output().unwrap()
    })
    .await
    .unwrap();
    assert!(
        output.status.success(),
        "SDK driver failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["ok"],
        true
    );
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
            media_type: "application/octet-stream".into(),
            schema: "sdk-fixture/1".into(),
            source: "compiled SDK immutable evidence".into(),
            channel: Channel::Evidence,
            retention: "history".into(),
            omissions: vec![],
        })
        .unwrap();
    writer.write_chunk(b"\0sdk-retained-bytes\xff").unwrap();
    let artifact = writer.finalize().unwrap();
    drop(writer);
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![Mutation::Put {
                expected: None,
                record: Record::typed(
                    Collection::Artifact,
                    artifact.spec.id.as_str(),
                    task.scope.workspace,
                    Revision::ZERO,
                    &artifact,
                )
                .unwrap(),
            }],
            events: vec![],
            command: None,
        })
        .await
        .unwrap();
    let mut engine = vcp_engine::Engine::new(store).unwrap();
    let access = vcp_engine::Access {
        actor: fixture.config.actor.clone(),
        workspace: fixture.config.workspace.clone(),
        session: fixture.config.session.clone(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: false,
    };
    let command = vcp_protocol::command::CommandEnvelope {
        version: 1,
        id: CommandId::new(),
        workspace: access.workspace.clone(),
        session: access.session.clone(),
        task: Some(TaskId::parse("sdk-retained-child").unwrap()),
        caller: access.actor.clone(),
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        payload: vcp_protocol::command::Command::CreateTask {
            root: fixture.config.root_task.clone(),
            parent: Some(fixture.config.root_task.clone()),
            fork_origin: None,
            objective: vcp_domain::task::Objective {
                text: "retained child inspection fixture".into(),
                constraints: vec![],
                acceptance: vec![],
                source: EventId::new(),
                steering: SteeringRevision::ZERO,
            },
            fingerprint: task.fingerprint,
            editing: false,
            required_checks: vec![],
        },
    };
    engine
        .handle(
            command,
            &access,
            &vcp_engine::HostFacts::inspect(Timestamp::new(1)),
        )
        .await
        .unwrap();
    engine.into_store().close().await.unwrap();
    artifact
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_sdk_attachments_preserve_authority_replay_events_and_artifacts() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for transport in ["stdio", "windows_pipe"] {
            let fixture = Fixture::new(backend).await;
            let artifact = seed(&fixture).await;
            driver(json!({"scenario":"attachment","executable":env!("CARGO_BIN_EXE_vcp"),"workspace":fixture.workspace,"data":fixture.data,"scope":fixture.scope(),"task":fixture.config.root_task,"artifact":artifact.spec.id,"transport":transport})).await;
            let store = fixture.reopen_within(Duration::from_secs(45)).await;
            assert_offline_paused(&store, &fixture.config);
            assert!(store
                .state()
                .record(
                    Collection::Session,
                    "sdk-created-session",
                    &fixture.config.workspace
                )
                .is_ok());
            let receipt = store
                .state()
                .commands
                .get(&vcp_store::contract::command_key(
                    &fixture.config.workspace,
                    &CommandId::parse("sdk-create-once").unwrap(),
                ))
                .unwrap();
            assert_eq!(
                store
                    .state()
                    .events
                    .iter()
                    .filter(|event| event.event.correlation == receipt.command)
                    .count(),
                1,
                "same original command must produce one canonical event"
            );
            store.close().await.unwrap();
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_sdk_abort_after_genuine_acceptance_reconciles_without_duplicate_effect() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let fixture = Fixture::new(backend).await;
        driver(json!({"scenario":"delivery","executable":env!("CARGO_BIN_EXE_vcp"),"workspace":fixture.workspace,"data":fixture.data,"scope":fixture.scope(),"task":fixture.config.root_task})).await;
        let store = fixture.reopen().await;
        assert_offline_paused(&store, &fixture.config);
        assert!(store
            .state()
            .record(
                Collection::Session,
                "sdk-delivery-session",
                &fixture.config.workspace
            )
            .is_ok());
        assert_eq!(
            store
                .state()
                .events
                .iter()
                .filter(|event| event.event.correlation.as_str() == "sdk-delivery-once")
                .count(),
            1
        );
        assert_eq!(
            store
                .state()
                .commands
                .values()
                .filter(|receipt| receipt.command.as_str() == "sdk-delivery-once")
                .count(),
            1
        );
        store.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_sdk_pending_input_reconnect_keeps_original_command_and_never_auto_resumes() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let server = MockServer::start().await;
        let count = Arc::new(AtomicUsize::new(0));
        let calls = count.clone();
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .respond_with(move |_: &wiremock::Request| {
                let index = calls.fetch_add(1, Ordering::SeqCst);
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(execution_fixture::response(index, "question"))
            })
            .mount(&server)
            .await;
        let fixture = execution_fixture::Fixture::new(&server.uri(), "question");
        let entry = fixture.seed(backend).await;
        assert_eq!(count.load(Ordering::SeqCst), 0);
        driver(json!({"scenario":"pending","executable":env!("CARGO_BIN_EXE_vcp"),"workspace":fixture.workspace,"data":fixture.data,"scope":{"workspace":entry.config.workspace,"session":entry.config.session},"task":"sdk-public-root","turn":"sdk-public-turn","profile":fixture.profile,"credential":"synthetic-cli-qualification"})).await;
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "reconnect/answer must not request another provider turn"
        );
        assert_eq!(
            std::fs::read_to_string(fixture.workspace.join("value.txt")).unwrap(),
            "41\n"
        );
        let deadline = Instant::now() + Duration::from_secs(45);
        let store = loop {
            match vcp_store::Store::open(&entry.config.canonical_root, backend, &[]).await {
                Ok(store) => break store,
                Err(error) if Instant::now() > deadline => panic!("SDK writer cleanup: {error}"),
                Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        };
        let task: Task = store
            .state()
            .record(Collection::Task, "sdk-public-root", &entry.config.workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(task.state, vcp_domain::task::TaskState::Paused);
        for command in ["sdk-start-once", "sdk-deny-once"] {
            assert_eq!(
                store
                    .state()
                    .commands
                    .values()
                    .filter(|receipt| receipt.command.as_str() == command)
                    .count(),
                1
            );
        }
        store.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_sdk_explicit_resume_connected_pause_and_retry_do_not_repeat_real_patch() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let server = MockServer::start().await;
        let count = Arc::new(AtomicUsize::new(0));
        let calls = count.clone();
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .respond_with(move |_: &wiremock::Request| {
                let index = calls.fetch_add(1, Ordering::SeqCst);
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(execution_fixture::response(index, "complete"))
                    .set_delay(if index == 0 {
                        Duration::ZERO
                    } else {
                        Duration::from_secs(30)
                    })
            })
            .mount(&server)
            .await;
        let fixture = execution_fixture::Fixture::new(&server.uri(), "complete");
        let entry = fixture.seed(backend).await;
        assert_eq!(count.load(Ordering::SeqCst), 0);
        driver(json!({"scenario":"execution","executable":env!("CARGO_BIN_EXE_vcp"),"workspace":fixture.workspace,"data":fixture.data,"scope":{"workspace":entry.config.workspace,"session":entry.config.session},"task":entry.config.root_task,"profile":fixture.profile,"credential":"synthetic-cli-qualification"})).await;
        assert_eq!(
            std::fs::read_to_string(fixture.workspace.join("value.txt")).unwrap(),
            "42\n"
        );
        assert_eq!(count.load(Ordering::SeqCst), 2);
        let store = vcp_store::Store::open(&entry.config.canonical_root, backend, &[])
            .await
            .unwrap();
        let task: Task = store
            .state()
            .record(
                Collection::Task,
                entry.config.root_task.as_str(),
                &entry.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(task.state, vcp_domain::task::TaskState::Paused);
        for command in ["sdk-resume-once", "sdk-connected-pause"] {
            assert_eq!(
                store
                    .state()
                    .commands
                    .values()
                    .filter(|receipt| receipt.command.as_str() == command)
                    .count(),
                1
            );
        }
        let effects: Vec<vcp_domain::effect::Effect> = store
            .state()
            .records
            .values()
            .filter(|row| row.collection == Collection::Effect)
            .map(|row| row.decode().unwrap())
            .collect();
        assert_eq!(
            effects.len(),
            1,
            "one actual patch execution, no replayed tool effect"
        );
        assert_eq!(effects[0].state, vcp_domain::effect::EffectState::Succeeded);
        assert!(!store
            .state()
            .commands
            .values()
            .any(|receipt| receipt.command.as_str() == "sdk-unsent-cancel"));
        store.close().await.unwrap();
    }
}
