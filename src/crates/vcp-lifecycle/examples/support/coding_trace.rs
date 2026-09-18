// SPDX-License-Identifier: Apache-2.0
use codex_core::TurnInputRequest;
use codex_extension_api::{
    AllowedTools, ExtensionRegistryBuilder, ToolName,
};
use codex_protocol::{protocol::EventMsg, user_input::UserInput};
use core_test_support::{
    responses::*,
    test_codex::{test_codex, TestCodex},
    wait_for_event,
};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc, time::Duration};
use vcp_lifecycle::{
    integration::{configure_fixture_provider, digest, WorkspaceTools},
    Lifecycle,
};

const PATCH: &str = "*** Begin Patch\n*** Update File: fixture.txt\n@@\n-answer = 41\n+answer = 42\n*** End Patch\n";

async fn turn(test: &TestCodex) {
    test.codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: "Run the scripted fixture.".into(),
            text_elements: vec![],
        }]))
        .await
        .unwrap();
    tokio::time::timeout(
        Duration::from_secs(20),
        wait_for_event(&test.codex, |e| matches!(e, EventMsg::TurnComplete(_))),
    )
    .await
    .unwrap();
}
fn response(id: &str, args: Value) -> String {
    sse(vec![
        ev_response_created(id),
        ev_function_call(&format!("call-{id}"), "vcp_workspace", &args.to_string()),
        ev_completed_with_tokens(id, 7),
    ])
}

pub async fn run(directory: &Path, verifier: &Path) -> Value {
    let workspace = directory.canonicalize().unwrap();
    std::fs::write(workspace.join("fixture.txt"), "answer = 41\n").unwrap();
    let journal = workspace.join("owner.journal");
    let (host, owner) =
        Lifecycle::open(&journal, "fixture-workspace", Duration::from_secs(5)).unwrap();
    host.configure_integration(800, 100).unwrap();
    let adapter = WorkspaceTools::new(host.clone(), &workspace, verifier).unwrap();
    let mut registry = ExtensionRegistryBuilder::new();
    registry.turn_start_admission(Arc::new(host.clone()));
    registry.work_admission(Arc::new(host.clone()));
    registry.tool_contributor(Arc::new(adapter));
    let server = start_mock_server().await;
    let observed = mount_sse_sequence(&server, vec![
        response("read", json!({"op":"read"})),
        response("prepare", json!({"op":"prepare","patch":PATCH})),
        response("apply", json!({"op":"apply","patch":PATCH,"expected":digest(b"answer = 41\n")})),
        response("verify", json!({"op":"verify"})),
        response("remember", json!({"op":"remember","workspace":"fixture-workspace","value":"answer is 42"})),
        response("recall", json!({"op":"recall","workspace":"fixture-workspace"})),
        sse(vec![ev_assistant_message("summary", "Changed answer from 41 to 42; native fixture assertion passed. Memory is a model proposal."), ev_completed_with_tokens("summary", 9)]),
    ]).await;
    let starter = host.clone();
    let test = test_codex()
        .with_extensions(Arc::new(registry.build()))
        .with_auth(codex_login::CodexAuth::from_api_key(
            "public-synthetic-p0-token",
        ))
        .with_allowed_tools(AllowedTools(vec![ToolName::plain("vcp_workspace")]))
        .with_config(move |config| {
            config.cwd = workspace.try_into().unwrap();
            configure_fixture_provider(config);
            starter
                .authorize_startup(config.cwd.as_path(), None)
                .unwrap();
        })
        .build_with_auto_env(&server)
        .await
        .unwrap();
    host.attach_root(test.codex.clone()).unwrap();
    turn(&test).await;
    assert_eq!(observed.requests().len(), 7);
    assert_eq!(
        std::fs::read(directory.join("fixture.txt")).unwrap(),
        b"answer = 42\n"
    );
    let ledger = host.integration_ledger().unwrap();
    assert_eq!(ledger.allocated().unwrap(), 700);
    assert!(ledger
        .requests
        .iter()
        .all(|row| row.settled && row.usage.is_some()));
    assert_eq!(ledger.facts.len(), 1);
    assert!(ledger
        .effects
        .iter()
        .any(|row| row["op"] == "verify" && row["exit_code"] == 0));
    assert!(host.work().unwrap().iter().all(|row| row.receipt.is_some()));
    let body = observed.requests().last().unwrap().body_json();
    assert!(body.to_string().contains("fixture assertion passed"));
    assert!(body.to_string().contains("evidence only"));
    for request in observed.requests() {
        assert!(request.path().ends_with("/responses"));
        assert_eq!(
            request.header("authorization").as_deref(),
            Some("Bearer public-synthetic-p0-token")
        );
        let body = request.body_json();
        let names: Vec<_> = body["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        assert_eq!(names, vec!["vcp_workspace"]);
    }
    owner.close().await.unwrap();
    test.codex.shutdown_and_wait().await.unwrap();
    drop(test);
    drop(host);
    let (restored, _owner) =
        Lifecycle::open(&journal, "fixture-workspace", Duration::from_secs(5)).unwrap();
    assert_eq!(restored.integration_ledger().unwrap(), ledger);
    assert!(restored.configure_integration(99999, 1).is_err());
    json!({"status":"pass","requests":7,"ledger":ledger,"summary":"Changed 41 to 42; native assertion passed; local model-proposed memory restored."})
}
