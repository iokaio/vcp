// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
//! Compiled controlled-launch bootstrap and canonical RPC process evidence.
#[path = "support/local_fixture.rs"]
mod local_fixture;
use local_fixture::*;
use serde_json::json;
use std::{io::Write, sync::mpsc, time::Duration};
use vcp_domain::{
    controller::{Lease, Reason},
    revision::Revision,
};
use vcp_store::{contract::Collection, BackendKind, Store};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_controller_retries_preserve_identity_and_release_keeps_reads_open() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let fixture = Fixture::new(backend).await;
        let mut client = Client::connect(&fixture, "controller");
        client.initialize();
        let scope = fixture.scope();
        let acquire = json!({"scope":scope,"command_id":"acquire","expected_revision":null});
        let receipt = client.rpc(2, "controller/acquire", acquire.clone());
        accepted(&receipt);
        assert_eq!(
            client.rpc(3, "controller/acquire", acquire)["result"],
            receipt["result"]
        );
        let create = json!({"scope":scope,"mutation":{"command_id":"create-once","expected_revision":"0","steering_revision":"0"},"new_session":"created","configuration_revision":"0"});
        let created = client.rpc(4, "session/create", create.clone());
        accepted(&created);
        assert_eq!(
            client.rpc(5, "session/create", create.clone())["result"],
            created["result"]
        );
        assert_eq!(
            client.rpc(
                6,
                "command/read",
                json!({"scope":scope,"command_id":"create-once"})
            )["result"],
            created["result"]
        );
        let mut conflict = create;
        conflict["new_session"] = json!("different");
        assert!(client
            .rpc(7, "session/create", conflict)
            .get("error")
            .is_some());
        let mut other = scope.clone();
        other["session"] = json!("created");
        assert!(client
            .rpc(8, "session/read", json!({"scope":other}))
            .get("error")
            .is_some());
        let lease = client.rpc(9, "controller/read", json!({"scope":scope}));
        assert_eq!(lease["result"]["value"]["ownership"], "this_connection");
        let release = json!({"scope":scope,"command_id":"release","expected_revision":lease["result"]["value"]["revision"],"generation":lease["result"]["value"]["generation"]});
        let released = client.rpc(10, "controller/release", release.clone());
        accepted(&released);
        assert_eq!(
            client.rpc(11, "controller/release", release)["result"],
            released["result"]
        );
        assert_eq!(
            client.rpc(12, "session/read", json!({"scope":scope}))["result"]["kind"],
            "session"
        );
        assert_eq!(
            client.rpc(13, "controller/read", json!({"scope":scope}))["result"]["value"]
                ["ownership"],
            "released"
        );
        assert!(client.finish().await.0.success());
        let store = fixture.reopen().await;
        assert_offline_paused(&store, &fixture.config);
        assert_eq!(
            store
                .state()
                .commands
                .values()
                .filter(|receipt| receipt.command.as_str() == "create-once")
                .count(),
            1
        );
        store.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_observer_reads_without_controller_authority_or_provider_configuration() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let fixture = Fixture::new(backend).await;
        let mut client = Client::connect(&fixture, "observer");
        client.initialize();
        let scope = fixture.scope();
        assert_eq!(
            client.rpc(2, "session/read", json!({"scope":scope}))["result"]["kind"],
            "session"
        );
        assert!(client
            .rpc(
                3,
                "controller/acquire",
                json!({"scope":scope,"command_id":"observer-claim","expected_revision":null})
            )
            .get("error")
            .is_some());
        assert!(client.rpc(4, "session/create", json!({"scope":scope,"mutation":{"command_id":"observer-create","expected_revision":"0","steering_revision":"0"},"new_session":"denied","configuration_revision":"0"})).get("error").is_some());
        assert_eq!(
            client.rpc(5, "controller/read", json!({"scope":scope}))["result"]["value"]
                ["ownership"],
            "unclaimed"
        );
        assert!(client.finish().await.0.success());
        let store = fixture.reopen().await;
        assert_offline_paused(&store, &fixture.config);
        assert!(!store
            .state()
            .commands
            .values()
            .any(|receipt| receipt.command.as_str().starts_with("observer-")));
        store.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_attachment_refuses_competing_writer_and_eof_releases_owner_paused() {
    let fixture = Fixture::new(BackendKind::Files).await;
    let mut owner = Client::connect(&fixture, "controller");
    owner.initialize();
    accepted(&owner.rpc(
        2,
        "controller/acquire",
        json!({"scope":fixture.scope(),"command_id":"owner-acquire","expected_revision":null}),
    ));
    assert!(
        Store::open(&fixture.config.canonical_root, fixture.config.backend, &[])
            .await
            .is_err()
    );
    let mut competing = Client::spawn("local-bridge");
    competing.send(fixture.bootstrap("controller"));
    let (status, diagnostics) = competing.rejected().await;
    assert!(!status.success());
    assert!(!diagnostics.is_empty());
    assert_eq!(
        owner.rpc(3, "session/read", json!({"scope":fixture.scope()}))["result"]["kind"],
        "session"
    );
    assert!(owner.finish().await.0.success());
    let store = fixture.reopen().await;
    assert_offline_paused(&store, &fixture.config);
    let lease: Lease = store
        .state()
        .records
        .values()
        .find(|row| {
            row.collection == Collection::Access
                && row.value["document_type"] == "vcp_controller_lease_v1"
        })
        .unwrap()
        .decode()
        .unwrap();
    assert!(lease.holder.is_none());
    assert_eq!(lease.reason, Reason::ConnectionLost);
    store.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_bootstrap_rejects_malformed_oversized_and_unproved_server_launch_without_stdout()
{
    for (mode, bytes) in [
        ("local-bridge", b"{invalid}\n".to_vec()),
        ("local-bridge", { let mut frame = vec![b' '; 16 * 1024 + 1]; frame.push(b'\n'); frame }),
        ("local-bridge", b"{\"schema\":\"vcp-local-bootstrap/1\",\"workspace\":\"relative\",\"data\":null,\"role\":\"controller\"}\n".to_vec()),
        ("local-server", b"{\"schema\":\"vcp-local-bootstrap/1\",\"parent_handle\":0}\n".to_vec()),
    ] {
        let mut client = Client::spawn(mode); client.raw(&bytes);
        let (status, diagnostics) = client.rejected().await;
        assert!(!status.success()); assert!(!diagnostics.is_empty());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_oversized_protocol_frame_closes_live_controller_and_releases_writer() {
    let fixture = Fixture::new(BackendKind::Sqlite).await;
    let mut client = Client::connect(&fixture, "controller");
    client.initialize();
    accepted(&client.rpc(
        2,
        "controller/acquire",
        json!({"scope":fixture.scope(),"command_id":"oversize-owner","expected_revision":null}),
    ));
    assert!(
        Store::open(&fixture.config.canonical_root, fixture.config.backend, &[])
            .await
            .is_err()
    );

    let mut input = client.input.take().unwrap();
    let (written, result) = mpsc::sync_channel(1);
    let (release, hold) = mpsc::sync_channel(1);
    let writer = std::thread::spawn(move || {
        // This exceeds the negotiated 1 MiB lexical frame ceiling before LF.
        // Keep the input handle open afterward: the helper must close on its
        // bounded decoder failure, independently of EOF or a valid request.
        let outcome = input.write_all(&vec![b' '; 1024 * 1024 + 2]);
        let _ = written.send(outcome.map_err(|error| error.kind()));
        let _ = hold.recv_timeout(Duration::from_secs(30));
        drop(input);
    });
    let _ = client.rejected().await;
    // A failed write is expected when the peer rejects before consuming all bytes.
    let _ = result
        .recv_timeout(Duration::from_secs(3))
        .expect("bounded writer must unblock after peer rejection");
    let _ = release.send(());
    writer.join().unwrap();

    let store = fixture.reopen().await;
    assert_offline_paused(&store, &fixture.config);
    let lease: Lease = store
        .state()
        .records
        .values()
        .find(|row| {
            row.collection == Collection::Access
                && row.value["document_type"] == "vcp_controller_lease_v1"
        })
        .unwrap()
        .decode()
        .unwrap();
    assert!(lease.holder.is_none());
    assert_eq!(lease.reason, Reason::ConnectionLost);
    assert_eq!(lease.generation, Revision::new(1));
    assert_eq!(
        store
            .state()
            .commands
            .values()
            .filter(|receipt| receipt.command.as_str() == "oversize-owner")
            .count(),
        1
    );
    assert_eq!(
        store
            .state()
            .records
            .values()
            .filter(|row| row.collection == Collection::Session)
            .count(),
        1
    );
    store.close().await.unwrap();
}
