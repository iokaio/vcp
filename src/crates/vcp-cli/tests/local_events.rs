// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
//! Snapshot and bounded event replay through the compiled authenticated server.
#[path = "support/local_fixture.rs"]
mod local_fixture;
use local_fixture::*;
use serde_json::{json, Value};
use vcp_store::BackendKind;

fn initialize(client: &mut Client) {
    let methods = [
        "session/snapshot",
        "events/subscribe",
        "events/next",
        "events/unsubscribe",
        "controller/acquire",
        "session/create",
        "session/read",
    ];
    let reply = client.rpc(1,"initialize",json!({"protocol_version":"1.0",
        "client":{"name":"compiled-events","version":"1"},"capabilities":methods,"required_capabilities":methods}));
    assert!(reply.get("error").is_none(), "{reply}");
}

fn next(
    client: &mut Client,
    fixture: &Fixture,
    id: u64,
    subscription: &Value,
    cursor: &Value,
) -> Value {
    let reply = client.rpc(
        id,
        "events/next",
        json!({"scope":fixture.scope(),"subscription":subscription,"cursor":cursor}),
    );
    assert!(reply.get("error").is_none(), "{reply}");
    reply
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_snapshot_boundary_covers_intervening_commits_and_next_retries_without_gaps() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let fixture = Fixture::new(backend).await;
        let mut client = Client::connect(&fixture, "controller");
        initialize(&mut client);
        let snapshot = client.rpc(
            2,
            "session/snapshot",
            json!({"scope":fixture.scope(),"limit":128,"cursor":null}),
        );
        assert!(snapshot.get("error").is_none(), "{snapshot}");
        assert_eq!(snapshot["result"]["kind"], "snapshot");
        let snapshot = &snapshot["result"]["value"];
        assert_eq!(snapshot["complete"], true);
        assert_eq!(snapshot["tasks"].as_array().unwrap().len(), 1);
        let boundary: u64 = snapshot["sequence"].as_str().unwrap().parse().unwrap();
        accepted(&client.rpc(
            3,
            "controller/acquire",
            json!({"scope":fixture.scope(),"command_id":"events-owner","expected_revision":null}),
        ));
        accepted(&client.rpc(4,"session/create",json!({"scope":fixture.scope(),"mutation":{"command_id":"between-snapshot-and-next","expected_revision":"0","steering_revision":"0"},"new_session":"events-created","configuration_revision":"0"})));
        let first = next(
            &mut client,
            &fixture,
            5,
            &snapshot["subscription"],
            &snapshot["event_cursor"],
        );
        let batch = &first["result"]["value"];
        let events = batch["events"].as_array().expect("bounded event batch");
        assert!(!events.is_empty(), "{first}");
        let mut expected = boundary + 1;
        for event in events {
            assert_eq!(
                event["sequence"].as_str().unwrap().parse::<u64>().unwrap(),
                expected
            );
            assert!(event.get("data").is_none());
            assert!(event.get("record").is_none());
            expected += 1;
        }
        assert!(events
            .iter()
            .any(|event| event["command_id"] == "between-snapshot-and-next"));
        assert_eq!(
            next(
                &mut client,
                &fixture,
                6,
                &snapshot["subscription"],
                &snapshot["event_cursor"]
            )["result"],
            first["result"]
        );
        let empty = next(
            &mut client,
            &fixture,
            7,
            &snapshot["subscription"],
            &batch["cursor"],
        );
        assert!(empty["result"]["value"]["events"]
            .as_array()
            .unwrap()
            .is_empty());
        let removed = client.rpc(
            8,
            "events/unsubscribe",
            json!({"scope":fixture.scope(),"subscription":snapshot["subscription"]}),
        );
        assert!(removed.get("error").is_none(), "{removed}");
        let gap = client.rpc(9,"events/next",json!({"scope":fixture.scope(),"subscription":snapshot["subscription"],"cursor":batch["cursor"]}));
        assert_eq!(gap["result"]["kind"], "gap");
        assert_eq!(gap["result"]["value"]["resubscribe_required"], true);
        assert!(client.finish().await.0.success());
        fixture.reopen().await.close().await.unwrap();
    }
}
