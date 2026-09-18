// SPDX-License-Identifier: Apache-2.0
use codex_core::StartThreadOptions;
use codex_extension_api::{
    ExtensionDataInit, ExtensionRegistryBuilder, HostWorkAdmission, HostWorkKind, SessionIsolation,
    ToolName,
};
use core_test_support::{responses::*, test_codex::test_codex};
use serde_json::json;
use std::{path::Path, sync::Arc, time::Duration};
use vcp_lifecycle::{
    integration::{configure_fixture_provider, WorkspaceTools},
    Lifecycle,
};

const PATCH: &str = "*** Begin Patch\n*** Update File: fixture.txt\n@@\n-answer = 41\n+answer = 42\n*** End Patch\n";

fn tools(host: &Lifecycle, root: &Path) -> WorkspaceTools {
    WorkspaceTools::new(
        host.clone(),
        root,
        Path::new(env!("CARGO_BIN_EXE_vcp-process-fixture")),
    )
    .unwrap()
}
#[path = "../examples/support/coding_trace.rs"]
mod coding_trace;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retained_loop_read_prepare_apply_native_verify_memory_and_summary() {
    let directory = tempfile::tempdir().unwrap();
    assert_eq!(
        coding_trace::run(
            directory.path(),
            Path::new(env!("CARGO_BIN_EXE_vcp-process-fixture"))
        )
        .await["status"],
        "pass"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_budget_unknown_liability_isolation_and_prepared_user_edits() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().canonicalize().unwrap();
    std::fs::write(workspace.join("fixture.txt"), "answer = 41\n").unwrap();
    let (host, owner) = Lifecycle::open(
        &workspace.join("journal"),
        "fixture-workspace",
        Duration::from_secs(5),
    )
    .unwrap();
    host.configure_integration(100, 100).unwrap();
    let mut registry = ExtensionRegistryBuilder::new();
    registry.turn_start_admission(Arc::new(host.clone()));
    registry.work_admission(Arc::new(host.clone()));
    let registry = Arc::new(registry.build());
    let server = start_mock_server().await;
    let observer = mount_sse_once(&server, sse_completed("unexpected")).await;
    let starter = host.clone();
    let test = test_codex()
        .with_extensions(registry.clone())
        .with_config(move |config| {
            configure_fixture_provider(config);
            starter
                .authorize_startup(config.cwd.as_path(), None)
                .unwrap();
        })
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let root = host.attach_root(test.codex.clone()).unwrap();
    let mut init = ExtensionDataInit::default();
    init.insert(SessionIsolation::Isolated);
    assert!(test
        .thread_manager
        .start_thread(StartThreadOptions {
            thread_extension_init: init,
            ..StartThreadOptions::new(test.config.clone())
        })
        .await
        .is_err());
    assert!(registry
        .isolated_host_controls()
        .work_admission()
        .unwrap()
        .admit_startup(test.config.cwd.as_path(), None)
        .is_err());
    assert!(host
        .admit_tool(root, "bypass", &ToolName::plain("exec_command"))
        .is_err());
    let adapter = tools(&host, &workspace);
    assert!(adapter.execute(&json!({"op":"apply","patch":PATCH,"expected":vcp_lifecycle::integration::digest(b"answer = 41\n")}).to_string()).await.is_err());
    let prepared = adapter
        .execute(&json!({"op":"prepare","patch":PATCH}).to_string())
        .await
        .unwrap();
    assert!(adapter.execute(&json!({"op":"apply","patch":PATCH.replace("+answer = 42", "+answer = 43"),"expected":prepared["expected"]}).to_string()).await.is_err());
    std::fs::write(workspace.join("fixture.txt"), "user edit\n").unwrap();
    assert!(adapter
        .execute(&json!({"op":"apply","patch":PATCH,"expected":prepared["expected"]}).to_string())
        .await
        .is_err());
    assert_eq!(
        std::fs::read(workspace.join("fixture.txt")).unwrap(),
        b"user edit\n"
    );
    assert!(adapter
        .execute(r#"{"op":"recall","workspace":"foreign"}"#)
        .await
        .is_err());
    adapter
        .execute(r#"{"op":"remember","workspace":"fixture-workspace","value":"one"}"#)
        .await
        .unwrap();
    let disputed = adapter
        .execute(r#"{"op":"remember","workspace":"fixture-workspace","value":"two"}"#)
        .await
        .unwrap();
    assert_eq!(disputed["claim"]["status"], "disputed");
    let child_starter = host.clone();
    child_starter
        .authorize_startup(test.config.cwd.as_path(), None)
        .unwrap();
    let child = test
        .thread_manager
        .start_thread(StartThreadOptions::new(test.config.clone()))
        .await
        .unwrap()
        .thread;
    let child_id = host
        .attach_child(root, &host.inspect(root).unwrap().revision, child.clone())
        .unwrap();
    let a = host.clone();
    let b = host.clone();
    let one = std::thread::spawn(move || a.admit(root, HostWorkKind::Model, "root-race").is_ok());
    let two = std::thread::spawn(move || {
        b.admit(child_id, HostWorkKind::Model, "helper-race")
            .is_ok()
    });
    assert_ne!(one.join().unwrap(), two.join().unwrap());
    let ledger = host.integration_ledger().unwrap();
    assert_eq!(ledger.allocated().unwrap(), 100);
    assert!(!ledger.requests[0].settled);
    assert!(host.admit(root, HostWorkKind::Model, "retry").is_err());
    assert_eq!(observer.requests().len(), 0);
    owner.close().await.unwrap();
    test.codex.shutdown_and_wait().await.unwrap();
    child.shutdown_and_wait().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interrupted_provider_stream_keeps_liability_without_hidden_retry() {
    use codex_core::TurnInputRequest;
    use codex_protocol::{protocol::EventMsg, user_input::UserInput};
    use core_test_support::wait_for_event;
    let directory = tempfile::tempdir().unwrap();
    let journal = directory.path().join("journal");
    let (host, owner) =
        Lifecycle::open(&journal, "fixture-workspace", Duration::from_secs(5)).unwrap();
    host.configure_integration(300, 100).unwrap();
    let mut registry = ExtensionRegistryBuilder::new();
    registry.turn_start_admission(Arc::new(host.clone()));
    registry.work_admission(Arc::new(host.clone()));
    let server = start_mock_server().await;
    let observer = mount_sse_once(
        &server,
        sse(vec![ev_response_created("missing-completion")]),
    )
    .await;
    let starter = host.clone();
    let test = test_codex()
        .with_extensions(Arc::new(registry.build()))
        .with_config(move |config| {
            configure_fixture_provider(config);
            starter
                .authorize_startup(config.cwd.as_path(), None)
                .unwrap();
        })
        .build_with_auto_env(&server)
        .await
        .unwrap();
    host.attach_root(test.codex.clone()).unwrap();
    test.codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: "fixture stream failure".into(),
            text_elements: vec![],
        }]))
        .await
        .unwrap();
    tokio::time::timeout(
        Duration::from_secs(15),
        wait_for_event(&test.codex, |e| matches!(e, EventMsg::Error(_))),
    )
    .await
    .unwrap();
    owner.close().await.unwrap();
    test.codex.shutdown_and_wait().await.unwrap();
    assert_eq!(observer.requests().len(), 1);
    assert_eq!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path().ends_with("/responses"))
            .count(),
        1
    );
    let ledger = host.integration_ledger().unwrap();
    assert_eq!(ledger.allocated().unwrap(), 100);
    assert_eq!(ledger.requests.len(), 1);
    assert!(!ledger.requests[0].settled);
    drop(test);
    drop(host);
    let (restored, _owner) =
        Lifecycle::open(&journal, "fixture-workspace", Duration::from_secs(5)).unwrap();
    assert_eq!(restored.integration_ledger().unwrap(), ledger);
}
