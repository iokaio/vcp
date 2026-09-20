// SPDX-License-Identifier: Apache-2.0
use super::*;
use super::{
    mcp::Fixture,
    mcp_http,
    mcp_http_peer::{Peer, Scenario},
};
use tokio::sync::Notify;
use vcp_domain::effect::{Effect, EffectState};
use vcp_lifecycle::foundation::mcp::Request;

fn effect(host: &CanonicalHost, config: &Config, id: &ToolRunId) -> Effect {
    host.snapshot()
        .unwrap()
        .record(Collection::Effect, id.as_str(), &config.workspace)
        .unwrap()
        .decode()
        .unwrap()
}

fn receipts(host: &CanonicalHost, id: &ToolRunId) -> Vec<ArtifactId> {
    let mut result = vec![];
    for row in host
        .snapshot()
        .unwrap()
        .records
        .values()
        .filter(|row| row.collection == Collection::Artifact)
    {
        let artifact: ArtifactDescriptor = row.decode().unwrap();
        if !matches!(
            artifact.spec.schema.as_str(),
            "vcp-mcp-call-result-v1" | "vcp-mcp-http-result-v1"
        ) {
            continue;
        }
        let bytes = host.read_artifact(artifact.spec.id.clone()).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        if value["effect"] == serde_json::json!(id) {
            result.push(artifact.spec.id);
        }
    }
    result
}

async fn reopen(
    f: Fixture,
    id: &ToolRunId,
    expected: EffectState,
    count: usize,
) -> tempfile::TempDir {
    f.host.disconnect_mcp().await.unwrap();
    let Fixture {
        _temp,
        host,
        owner,
        test,
        config,
        ..
    } = f;
    owner.close().await.unwrap();
    test.codex.shutdown_and_wait().await.unwrap();
    drop(test);
    drop(host);
    // Two reopen cycles: neither recovery nor repeated recovery is a remote probe.
    for _ in 0..2 {
        let (restored, owner) = CanonicalHost::open(config.clone()).unwrap();
        let state = effect(&restored, &config, id);
        assert_eq!(state.state, expected);
        let artifacts = receipts(&restored, id);
        assert_eq!(artifacts.len(), count);
        for artifact in artifacts {
            assert!(state.observed_changes.contains(&artifact));
        }
        owner.close().await.unwrap();
        drop(restored);
    }
    _temp
}

async fn at_barrier(arrived: &Notify, f: &Fixture, id: &ToolRunId) {
    tokio::time::timeout(Duration::from_secs(5), arrived.notified())
        .await
        .unwrap();
    assert!(matches!(
        effect(&f.host, &f.config, id).state,
        EffectState::Running | EffectState::DispatchRecorded
    ));
    assert!(
        receipts(&f.host, id).is_empty(),
        "barrier must precede durable result capture"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stdio_valid_reply_before_receipt_interruption_reopens_unknown_without_replay() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for interrupt in [true, false] {
            let f = Fixture::new(backend, "normal").await;
            let catalog = f
                .host
                .mcp_control(
                    f.thread,
                    Request::List {
                        server: "fixture".into(),
                    },
                )
                .await
                .unwrap();
            let entry = catalog["catalog"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["tool"] == "write_marker")
                .unwrap();
            let ticket = f
                .host
                .prepare_mcp_call(
                    f.thread,
                    Request::Call {
                        server: "fixture".into(),
                        tool: "write_marker".into(),
                        identity_digest: entry["identity_digest"].as_str().unwrap().into(),
                        arguments_json: r#"{"value":"receipt-boundary-once"}"#.into(),
                    },
                )
                .await
                .unwrap();
            let id = ticket.effect().clone();
            let arrived = Arc::new(Notify::new());
            let release = Arc::new(Notify::new());
            let ticket =
                ticket.qualification_block_before_receipt(arrived.clone(), release.clone());
            let host = f.host.clone();
            let waiter = tokio::spawn(async move { host.dispatch_mcp_call(ticket).await });
            at_barrier(&arrived, &f, &id).await;
            let marker = f.workspace.join("writes.jsonl");
            let written = std::fs::read(&marker).unwrap();
            assert_eq!(
                String::from_utf8(written.clone()).unwrap().lines().count(),
                1
            );
            let expected = if interrupt {
                waiter.abort();
                assert!(waiter.await.unwrap_err().is_cancelled());
                EffectState::OutcomeUnknown
            } else {
                release.notify_one();
                let reply = waiter.await.unwrap().unwrap();
                assert_eq!(reply["outcome"], "succeeded");
                EffectState::Succeeded
            };
            let retained = reopen(f, &id, expected, usize::from(!interrupt)).await;
            assert_eq!(std::fs::read(marker).unwrap(), written);
            drop(retained);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn http_valid_reply_before_receipt_interruption_reopens_unknown_without_replay() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for interrupt in [true, false] {
            let f = Fixture::new(backend, "normal").await;
            let peer = Peer::start(Scenario::Normal).await;
            mcp_http::configure(&f, &peer, false);
            let catalog = mcp_http::list(&f).await;
            let ticket = f
                .host
                .prepare_remote_mcp_call(
                    f.thread,
                    mcp_http::call(&catalog, "write_marker", serde_json::json!({})),
                )
                .await
                .unwrap();
            mcp_http::approve_ticket(&f, &ticket);
            let id = ticket.effect().clone();
            let arrived = Arc::new(Notify::new());
            let release = Arc::new(Notify::new());
            let ticket =
                ticket.qualification_block_before_receipt(arrived.clone(), release.clone());
            let host = f.host.clone();
            let waiter = tokio::spawn(async move { host.dispatch_remote_mcp_call(ticket).await });
            at_barrier(&arrived, &f, &id).await;
            assert_eq!(peer.effect_count(), 1);
            assert_eq!(std::fs::read(peer.marker()).unwrap(), b"effect\n");
            let requests = peer.observations().len();
            assert_eq!(
                peer.observations()
                    .iter()
                    .filter(|row| row.method == "tools/call")
                    .count(),
                1
            );
            let expected = if interrupt {
                waiter.abort();
                assert!(waiter.await.unwrap_err().is_cancelled());
                EffectState::OutcomeUnknown
            } else {
                release.notify_one();
                let reply = waiter.await.unwrap().unwrap();
                assert_eq!(reply["outcome"], "succeeded");
                EffectState::Succeeded
            };
            let retained = reopen(f, &id, expected, usize::from(!interrupt)).await;
            assert_eq!(peer.effect_count(), 1);
            assert_eq!(std::fs::read(peer.marker()).unwrap(), b"effect\n");
            assert_eq!(peer.observations().len(), requests);
            drop(retained);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn http_actual_unauthorized_reply_is_unknown_without_catalog_or_replay() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend, "normal").await;
        let peer = Peer::with_authorization(
            Scenario::Normal,
            Some("Bearer different-synthetic-server-canary".into()),
        )
        .await;
        mcp_http::configure(&f, &peer, true);
        let reply = mcp_http::approved(
            &f,
            Request::List {
                server: "remote".into(),
            },
        )
        .await;
        assert_eq!(reply["outcome"], "unknown", "{reply}");
        assert_eq!(reply["replay"], false);
        assert!(reply.get("catalog").is_none());
        assert_ne!(reply["connected"], true);
        let id = ToolRunId::parse(reply["effect"].as_str().unwrap()).unwrap();
        let observations = peer.observations();
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].method, "initialize");
        assert!(!observations[0].authorization_matches);
        tokio::time::timeout(Duration::from_secs(5), async {
            while peer.unauthorized_responses() == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(peer.unauthorized_responses(), 1);
        assert_eq!(peer.effect_count(), 0);
        assert!(!peer.marker().exists());
        let canaries = ["synthetic-http-bearer", "different-synthetic-server-canary"];
        for canary in canaries {
            assert!(!reply.to_string().contains(canary));
        }
        let mut inspected = 0;
        for row in f
            .host
            .snapshot()
            .unwrap()
            .records
            .values()
            .filter(|row| row.collection == Collection::Artifact)
        {
            let artifact: ArtifactDescriptor = row.decode().unwrap();
            let bytes = f.host.read_artifact(artifact.spec.id).unwrap();
            let text = String::from_utf8_lossy(&bytes);
            for canary in canaries {
                assert!(!text.contains(canary), "secret in artifact");
            }
            inspected += 1;
        }
        assert!(inspected > 0);
        let retained = reopen(f, &id, EffectState::OutcomeUnknown, 0).await;
        assert_eq!(peer.observations().len(), 1);
        assert_eq!(peer.effect_count(), 0);
        assert!(!peer.marker().exists());
        drop(retained);
    }
}
