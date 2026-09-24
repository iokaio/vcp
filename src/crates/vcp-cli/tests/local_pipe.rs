// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
//! Authenticated named-pipe attachment through independent compiled helpers.
#[path = "support/local_fixture.rs"]
mod local_fixture;
use local_fixture::*;
use serde_json::{json, Value};
use std::time::Duration;
use vcp_domain::{
    controller::{Lease, Reason},
    revision::Revision,
};
use vcp_store::{contract::Collection, BackendKind, Store};

fn launch(fixture: &Fixture, role: &str) -> (Client, Value) {
    let mut client = Client::spawn("local-bridge");
    let mut request = fixture.bootstrap(role);
    request["transport"] = json!("windows_pipe");
    request["observer_reconnect"] = json!(true);
    client.send(request);
    let ready = client.receive();
    assert_eq!(ready["schema"], "vcp-local-ready/1");
    assert_eq!(ready["scope"], fixture.scope());
    assert_eq!(ready["role"], role);
    assert!(ready["attachment"].is_object());
    assert!(ready["attachment"]["ticket"]
        .as_str()
        .is_some_and(|ticket| ticket.len() == 64));
    (client, ready)
}
fn attach(attachment: &Value, role: &str) -> Client {
    let mut client = Client::spawn("local-bridge");
    client.send(json!({"schema":"vcp-local-attach/1","attachment":attachment,"role":role,"observer_reconnect":true}));
    let ready = client.receive();
    assert_eq!(ready["schema"], "vcp-local-ready/1");
    assert_eq!(ready["role"], role);
    // Process identity may be shown in assertion diagnostics; credentials may not.
    assert_eq!(ready["server"], attachment["server"]);
    if role == "observer" {
        for field in ["attachment", "observer_attachment"] {
            if let Some(ticket) = ready[field]["ticket"].as_str() {
                assert!(
                    Some(ticket) == attachment["ticket"].as_str(),
                    "observer readiness must never disclose a stronger attachment credential"
                );
            }
        }
    }
    client
}

fn reconnect(reference: &Value) -> Client {
    let mut client = Client::spawn("local-bridge");
    client.send(json!({"schema":"vcp-local-observer-reconnect/1","observer_reconnect":reference}));
    let ready = client.receive();
    assert_eq!(ready["schema"], "vcp-local-ready/1");
    assert_eq!(ready["role"], "observer");
    assert_eq!(ready["observer_reconnect"], *reference);
    assert!(ready["observer_attachment"].is_null());
    client
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_legacy_bootstrap_keeps_original_ready_shape_until_opted_in() {
    let fixture = Fixture::new(BackendKind::Files).await;
    let mut owner = Client::spawn("local-bridge");
    let mut request = fixture.bootstrap("controller");
    request["transport"] = json!("windows_pipe");
    owner.send(request);
    let ready = owner.receive();
    assert!(ready.get("observer_reconnect").is_none());
    assert_eq!(ready.as_object().unwrap().len(), 6);
    owner.initialize();
    for opt_in in [None, Some(false), Some(true)] {
        let mut observer = Client::spawn("local-bridge");
        let mut request = json!({"schema":"vcp-local-attach/1","attachment":ready["observer_attachment"],"role":"observer"});
        if let Some(value) = opt_in {
            request["observer_reconnect"] = json!(value);
        }
        observer.send(request);
        let attached = observer.receive();
        assert_eq!(
            attached.get("observer_reconnect").is_some(),
            opt_in == Some(true)
        );
        assert_eq!(
            attached.as_object().unwrap().len(),
            if opt_in == Some(true) { 6 } else { 5 }
        );
        observer.initialize();
        assert!(observer.finish().await.0.success());
    }
    assert!(owner.finish().await.0.success());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_observer_reload_uses_nonsecret_reference_without_disturbing_controller() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let fixture = Fixture::new(backend).await;
        let (mut controller, ready) = launch(&fixture, "controller");
        controller.initialize();
        accepted(&controller.rpc(
            2,
            "controller/acquire",
            json!({"scope":fixture.scope(),"command_id":"reload-owner","expected_revision":null}),
        ));
        let held = read_controller(&mut controller, &fixture, 3);
        let reference = ready["observer_reconnect"].clone();
        assert_eq!(reference.as_object().unwrap().len(), 3);
        assert!(reference.get("ticket").is_none());
        // Same SID/session is insufficient: this test binary is not the trusted
        // CLI executable. Direct pipe clients cannot use a discovered reference.
        {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut impostor = tokio::net::windows::named_pipe::ClientOptions::new()
                .open(reference["endpoint"].as_str().unwrap())
                .unwrap();
            let hello =
                json!({"schema":"vcp-local-pipe-observer-reconnect/1","scope":fixture.scope()});
            impostor
                .write_all(format!("{hello}\n").as_bytes())
                .await
                .unwrap();
            let mut byte = [0];
            let result = tokio::time::timeout(Duration::from_secs(10), impostor.read(&mut byte))
                .await
                .unwrap();
            assert!(
                result.is_err() || matches!(result, Ok(0)),
                "untrusted executable must receive no readiness"
            );
        }
        // Only the nonsecret reference survives each independent helper process.
        for attempt in 0..2 {
            let mut observer = reconnect(&reference);
            observer.initialize();
            assert_eq!(
                observer.rpc(2, "session/read", json!({"scope":fixture.scope()}))["result"]["kind"],
                "session"
            );
            assert!(observer.rpc(3, "controller/acquire", json!({"scope":fixture.scope(),"command_id":format!("reload-escalation-{attempt}"),"expected_revision":held["revision"]})).get("error").is_some());
            assert!(observer.finish().await.0.success());
            assert_eq!(read_controller(&mut controller, &fixture, 4), held);
        }
        for field in ["created", "scope", "file", "principal"] {
            let mut changed = reference.clone();
            match field {
                "created" => changed["server"]["created"] = json!("0"),
                "scope" => changed["scope"]["session"] = json!("wrong-session"),
                "file" => changed["server"]["file"]["index"] = json!("0"),
                _ => changed["server"]["principal"]["session"] = json!(u32::MAX),
            }
            let mut rejected = Client::spawn("local-bridge");
            rejected.send(
                json!({"schema":"vcp-local-observer-reconnect/1","observer_reconnect":changed}),
            );
            assert!(!rejected.rejected().await.0.success(), "{field}");
            assert_eq!(read_controller(&mut controller, &fixture, 5), held);
        }
        let mut elevated = Client::spawn("local-bridge");
        elevated.send(json!({"schema":"vcp-local-observer-reconnect/1","observer_reconnect":reference,"role":"controller"}));
        assert!(!elevated.rejected().await.0.success());
        assert!(controller.finish().await.0.success());
    }
}
async fn refused(attachment: &Value, role: &str, secrets: &[&str]) {
    let mut client = Client::spawn("local-bridge");
    client.send(json!({"schema":"vcp-local-attach/1","attachment":attachment,"role":role}));
    let (status, diagnostics) = client.rejected().await;
    assert!(!status.success());
    assert!(!diagnostics.is_empty());
    for secret in secrets {
        assert!(
            !diagnostics.contains(secret),
            "attachment credentials must never enter diagnostics"
        );
    }
}
fn read_controller(client: &mut Client, fixture: &Fixture, request: u64) -> Value {
    let response = client.rpc(request, "controller/read", json!({"scope":fixture.scope()}));
    assert!(
        response.get("error").is_none(),
        "controller read must remain available"
    );
    response["result"]["value"].clone()
}
async fn released(client: &mut Client, fixture: &Fixture) -> Value {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let view = read_controller(client, fixture, 100);
            if view["ownership"] == "released" {
                return view;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("owner EOF must release while observer stays connected")
}
fn lease(store: &Store) -> Lease {
    store
        .state()
        .records
        .values()
        .find(|row| {
            row.collection == Collection::Access
                && row.value["document_type"] == "vcp_controller_lease_v1"
        })
        .unwrap()
        .decode()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_pipe_competing_controllers_observers_and_reconnect_share_one_writer() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let fixture = Fixture::new(backend).await;
        let (mut first, ready) = launch(&fixture, "controller");
        let controller_attachment = ready["attachment"].clone();
        let observer_attachment = ready["observer_attachment"].clone();
        assert!(
            observer_attachment.is_object(),
            "controller launch must supply a separately restricted observer attachment"
        );
        assert!(
            controller_attachment["ticket"].as_str().unwrap()
                != observer_attachment["ticket"].as_str().unwrap(),
            "role credentials must be distinct"
        );
        first.initialize();
        accepted(&first.rpc(
            2,
            "controller/acquire",
            json!({"scope":fixture.scope(),"command_id":"first-owner","expected_revision":null}),
        ));
        let held = read_controller(&mut first, &fixture, 3);
        assert_eq!(held["ownership"], "this_connection");
        let mut second = attach(&controller_attachment, "controller");
        second.initialize();
        assert!(second.rpc(2, "controller/acquire", json!({"scope":fixture.scope(),"command_id":"competing-owner","expected_revision":held["revision"]})).get("error").is_some());
        let mut observer = attach(&observer_attachment, "observer");
        observer.initialize();
        assert_eq!(
            observer.rpc(2, "session/read", json!({"scope":fixture.scope()}))["result"]["kind"],
            "session"
        );
        assert!(observer.rpc(3, "controller/acquire", json!({"scope":fixture.scope(),"command_id":"observer-escalation","expected_revision":held["revision"]})).get("error").is_some());
        let before_observer_close = read_controller(&mut first, &fixture, 4);
        assert!(observer.finish().await.0.success());
        assert_eq!(
            read_controller(&mut first, &fixture, 5),
            before_observer_close
        );
        let mut observer = attach(&observer_attachment, "observer");
        observer.initialize();
        assert!(Store::open(&fixture.config.canonical_root, backend, &[])
            .await
            .is_err());
        assert!(first.finish().await.0.success());
        let old = released(&mut observer, &fixture).await;
        assert_eq!(
            observer.rpc(
                4,
                "command/read",
                json!({"scope":fixture.scope(),"command_id":"first-owner"})
            )["result"]["kind"],
            "acceptance"
        );
        let create = json!({"scope":fixture.scope(),"mutation":{"command_id":"reconnect-needs-acquire","expected_revision":"0","steering_revision":"0"},"new_session":"not-created","configuration_revision":"0"});
        assert!(second
            .rpc(3, "session/create", create.clone())
            .get("error")
            .is_some());
        assert!(second.finish().await.0.success());
        let mut replacement = attach(&controller_attachment, "controller");
        replacement.initialize();
        assert_eq!(
            read_controller(&mut replacement, &fixture, 2)["ownership"],
            "released"
        );
        assert!(replacement
            .rpc(3, "session/create", create)
            .get("error")
            .is_some());
        accepted(&replacement.rpc(4, "controller/acquire", json!({"scope":fixture.scope(),"command_id":"replacement-owner","expected_revision":old["revision"]})));
        let next = read_controller(&mut replacement, &fixture, 5);
        assert_eq!(next["ownership"], "this_connection");
        assert_eq!(next["generation"], "2");
        assert!(replacement.finish().await.0.success());
        released(&mut observer, &fixture).await;
        assert_eq!(
            observer.rpc(5, "session/read", json!({"scope":fixture.scope()}))["result"]["kind"],
            "session"
        );
        assert!(observer.finish().await.0.success());
        let store = fixture.reopen_within(Duration::from_secs(45)).await;
        assert_offline_paused(&store, &fixture.config);
        let state = lease(&store);
        assert!(state.holder.is_none());
        assert_eq!(state.generation, Revision::new(2));
        assert_eq!(state.reason, Reason::ConnectionLost);
        assert_eq!(
            store
                .state()
                .records
                .values()
                .filter(|row| row.collection == Collection::Session)
                .count(),
            1
        );
        assert!(!store.state().commands.values().any(|receipt| matches!(
            receipt.command.as_str(),
            "competing-owner" | "observer-escalation" | "reconnect-needs-acquire"
        )));
        store.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_pipe_rejects_wrong_pin_ticket_endpoint_and_observer_escalation() {
    let fixture = Fixture::new(BackendKind::Sqlite).await;
    let (mut owner, ready) = launch(&fixture, "controller");
    owner.initialize();
    accepted(&owner.rpc(
        2,
        "controller/acquire",
        json!({"scope":fixture.scope(),"command_id":"protected-owner","expected_revision":null}),
    ));
    let attachment = ready["attachment"].clone();
    let observer = ready["observer_attachment"].clone();
    assert!(observer.is_object());
    let secrets = [
        attachment["ticket"].as_str().unwrap(),
        observer["ticket"].as_str().unwrap(),
    ];
    let before = read_controller(&mut owner, &fixture, 3);
    let mut wrong_pin = attachment.clone();
    wrong_pin["server"]["created"] = json!(format!(
        "{}0",
        attachment["server"]["created"].as_str().unwrap()
    ));
    refused(&wrong_pin, "controller", &secrets).await;
    let mut wrong_ticket = attachment.clone();
    wrong_ticket["ticket"] = json!("00".repeat(32));
    refused(&wrong_ticket, "controller", &secrets).await;
    let mut wrong_endpoint = attachment.clone();
    wrong_endpoint["endpoint"] = json!(format!(
        "{}-absent",
        attachment["endpoint"].as_str().unwrap()
    ));
    refused(&wrong_endpoint, "controller", &secrets).await;
    refused(&observer, "controller", &secrets).await;
    assert_eq!(read_controller(&mut owner, &fixture, 4), before);
    assert!(owner.finish().await.0.success());
    let store = fixture.reopen_within(Duration::from_secs(45)).await;
    assert_offline_paused(&store, &fixture.config);
    assert!(lease(&store).holder.is_none());
    store.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn compiled_pipe_observer_launch_has_no_controller_grant_and_dead_attachment_is_rejected() {
    let fixture = Fixture::new(BackendKind::Files).await;
    let (mut first, ready) = launch(&fixture, "observer");
    first.initialize();
    let attachment = ready["attachment"].clone();
    let secret = attachment["ticket"].as_str().unwrap();
    refused(&attachment, "controller", &[secret]).await;
    assert_eq!(
        read_controller(&mut first, &fixture, 2)["ownership"],
        "unclaimed"
    );
    let mut second = attach(&attachment, "observer");
    second.initialize();
    assert!(first.finish().await.0.success());
    assert_eq!(
        second.rpc(2, "session/read", json!({"scope":fixture.scope()}))["result"]["kind"],
        "session"
    );
    assert!(
        Store::open(&fixture.config.canonical_root, BackendKind::Files, &[])
            .await
            .is_err()
    );
    assert!(second.finish().await.0.success());
    // Actual server idle expiry releases its canonical writer; the old process
    // pin and ticket cannot authenticate a replacement or dead endpoint.
    let store = fixture.reopen_within(Duration::from_secs(45)).await;
    assert_offline_paused(&store, &fixture.config);
    assert!(!store
        .state()
        .records
        .values()
        .any(|row| row.collection == Collection::Access
            && row.value["document_type"] == "vcp_controller_lease_v1"));
    refused(&attachment, "observer", &[secret]).await;
    store.close().await.unwrap();
}
