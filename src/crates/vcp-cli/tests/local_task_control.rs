// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
//! Offline control acceptance through the compiled production bridge and server.
#[path = "support/local_fixture.rs"]
mod local_fixture;
use local_fixture::*;
use serde_json::{json, Value};
use vcp_domain::task::{Task, TaskState};
use vcp_store::{contract::Collection, BackendKind};

fn initialize(client: &mut Client) {
    let methods = [
        "task/read",
        "task/cancel",
        "controller/read",
        "controller/acquire",
        "command/read",
    ];
    let response = client.rpc(
        1,
        "initialize",
        json!({
            "protocol_version":"1.0", "client":{"name":"compiled-task-control","version":"1"},
            "capabilities":methods, "required_capabilities":methods
        }),
    );
    assert!(response.get("error").is_none(), "{response}");
}
fn read(client: &mut Client, fixture: &Fixture, id: u64) -> Value {
    let result = client.rpc(
        id,
        "task/read",
        json!({"scope":fixture.scope(),"task":fixture.config.root_task}),
    );
    assert!(result.get("error").is_none(), "{result}");
    assert_eq!(result["result"]["kind"], "task");
    result["result"]["value"].clone()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_cancel_retries_and_task_reads_keep_controller_connection_open() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let fixture = Fixture::new(backend).await;
        let mut client = Client::connect(&fixture, "controller");
        initialize(&mut client);
        let before = read(&mut client, &fixture, 2);
        assert_eq!(before["state"], "paused");
        assert_eq!(before["effects"], "known");
        accepted(&client.rpc(
            3,
            "controller/acquire",
            json!({"scope":fixture.scope(),"command_id":"task-owner","expected_revision":null}),
        ));
        let request = json!({"scope":fixture.scope(),"task":fixture.config.root_task,
            "mutation":{"command_id":"cancel-once","expected_revision":before["revision"],"steering_revision":before["steering_revision"]},
            "reason":"explicit compiled client cancellation"});
        let first = client.rpc(4, "task/cancel", request.clone());
        accepted(&first);
        let cancelled = read(&mut client, &fixture, 5);
        assert_eq!(cancelled["state"], "cancelled");
        let replay = client.rpc(6, "task/cancel", request.clone());
        assert_eq!(first["result"], replay["result"]);
        assert_eq!(read(&mut client, &fixture, 7), cancelled);
        let controller = client.rpc(8, "controller/read", json!({"scope":fixture.scope()}));
        assert_eq!(
            controller["result"]["value"]["ownership"],
            "this_connection"
        );
        let mut changed = request;
        changed["reason"] = json!("different command semantics");
        assert!(client.rpc(9, "task/cancel", changed).get("error").is_some());
        let receipt = client.rpc(
            10,
            "command/read",
            json!({"scope":fixture.scope(),"command_id":"cancel-once"}),
        );
        assert_eq!(receipt["result"], first["result"]);
        assert!(client.finish().await.0.success());
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
        assert_eq!(task.state, TaskState::Cancelled);
        assert_eq!(
            store
                .state()
                .commands
                .values()
                .filter(|receipt| receipt.command.as_str() == "cancel-once")
                .count(),
            1
        );
        store.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_observer_can_inspect_but_cannot_cancel_a_task() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let fixture = Fixture::new(backend).await;
        let mut client = Client::connect(&fixture, "observer");
        initialize(&mut client);
        let before = read(&mut client, &fixture, 2);
        let denied = client.rpc(3, "task/cancel", json!({"scope":fixture.scope(),"task":fixture.config.root_task,
            "mutation":{"command_id":"observer-cancel","expected_revision":before["revision"],"steering_revision":before["steering_revision"]},
            "reason":"denied observer cancellation"}));
        assert!(denied.get("error").is_some());
        assert_eq!(read(&mut client, &fixture, 4), before);
        assert!(client.finish().await.0.success());
        let store = fixture.reopen().await;
        assert_offline_paused(&store, &fixture.config);
        assert!(!store
            .state()
            .commands
            .values()
            .any(|receipt| receipt.command.as_str() == "observer-cancel"));
        store.close().await.unwrap();
    }
}
