// SPDX-License-Identifier: Apache-2.0
use super::*;
use super::{
    mcp::Fixture,
    mcp_http_peer::{Peer, Scenario},
};
use std::collections::BTreeSet;
use vcp_domain::effect::{Effect, EffectState};
use vcp_lifecycle::foundation::mcp::remote_authority::{
    CredentialMaterial, RemoteProfile, RemoteProfileConfig,
};
use vcp_lifecycle::foundation::mcp::{RemoteRegistration, Request};
pub(super) fn configure(f: &Fixture, peer: &Peer, credential: bool) {
    configure_endpoint(f, peer, credential, peer.endpoint());
}
fn configure_endpoint(f: &Fixture, peer: &Peer, credential: bool, endpoint: String) {
    let profile = RemoteProfile::new(RemoteProfileConfig {
        workspace: f.config.workspace.clone(),
        server: "remote".into(),
        revision: Revision::ZERO,
        endpoint,
        credential_ref: credential.then(|| "fixture-auth".into()),
    })
    .unwrap();
    f.host
        .configure_mcp_remote(
            RemoteRegistration::fixture_roots(
                profile,
                BTreeSet::from(["echo".into(), "write_marker".into(), "read_marker".into()]),
                vcp_extensions::mcp::registration::Limits {
                    frame_bytes: 64 * 1024,
                    total_discovery_bytes: 1024 * 1024,
                    tools: 16,
                    pages: 8,
                    timeout_ms: 10_000,
                    stderr_bytes: 0,
                },
                vec![peer.root_certificate()],
            )
            .unwrap(),
        )
        .unwrap();
    if credential {
        f.host
            .install_mcp_credential(
                "remote",
                None,
                CredentialMaterial::bearer("synthetic-http-bearer".into()).unwrap(),
                Timestamp::new(u64::MAX - 1),
            )
            .unwrap();
    }
}
pub(super) fn call(catalog: &serde_json::Value, tool: &str, args: serde_json::Value) -> Request {
    let entry = catalog["catalog"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["tool"] == tool)
        .unwrap();
    Request::Call {
        server: "remote".into(),
        tool: tool.into(),
        identity_digest: entry["identity_digest"].as_str().unwrap().into(),
        arguments_json: args.to_string(),
    }
}
pub(super) fn approve(f: &Fixture, value: &serde_json::Value) {
    assert_eq!(value["decision"]["kind"], "question", "{value}");
    let state = f.host.snapshot().unwrap();
    let approval: vcp_protocol::command::Approval = state
        .record(
            Collection::Approval,
            value["question"].as_str().unwrap(),
            &f.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    f.host
        .command(
            Command::Decide {
                id: approval.id,
                operation_digest: approval.operation_digest,
                effect_revision: approval.effect_revision,
                allow: true,
            },
            Some(f.config.root_task.clone()),
            approval.revision,
        )
        .unwrap();
    f.resume_approved();
}
pub(super) async fn approved(f: &Fixture, request: Request) -> serde_json::Value {
    let question = f.host.mcp_control(f.thread, request.clone()).await.unwrap();
    approve(f, &question);
    f.host.mcp_control(f.thread, request).await.unwrap()
}
pub(super) fn approve_ticket(
    f: &Fixture,
    ticket: &vcp_lifecycle::foundation::mcp::remote::RemoteProposal,
) {
    let digest = match &ticket.decision {
        vcp_policy::Decision::Question { digest, .. } => digest,
        _ => panic!("expected scoped approval"),
    };
    approve(
        f,
        &serde_json::json!({"question":ticket.question,"decision":{"kind":"question","digest":digest}}),
    );
}
pub(super) async fn list(f: &Fixture) -> serde_json::Value {
    let value = approved(
        f,
        Request::List {
            server: "remote".into(),
        },
    )
    .await;
    assert_eq!(value["connected"], true, "{value}");
    value
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_http_tls_discovery_and_governed_call_are_accounted() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend, "normal").await;
        let peer = Peer::with_authorization(
            Scenario::Normal,
            Some("Bearer synthetic-http-bearer".into()),
        )
        .await;
        configure(&f, &peer, true);
        let catalog = list(&f).await;
        let response = approved(&f, call(&catalog, "write_marker", serde_json::json!({}))).await;
        assert_eq!(response["outcome"], "succeeded", "{response}");
        assert_eq!(peer.effect_count(), 1);
        assert!(peer
            .observations()
            .iter()
            .all(|request| request.authorization_matches));
        let state = f.host.snapshot().unwrap();
        assert!(state
            .records
            .values()
            .filter(|row| row.collection == Collection::Effect)
            .all(|row| matches!(
                row.decode::<Effect>().unwrap().state,
                EffectState::Succeeded
            )));
        assert!(!state
            .records
            .values()
            .any(|row| row.collection == Collection::Attempt));
        let serialized = serde_json::to_string(&state).unwrap();
        assert!(!serialized.contains("synthetic-http-bearer"));
        let mut flushed = Vec::new();
        for artifact in state
            .records
            .values()
            .filter(|row| row.collection == Collection::Artifact)
        {
            let descriptor: vcp_domain::artifact::ArtifactDescriptor = artifact.decode().unwrap();
            let bytes = f.host.read_artifact(descriptor.spec.id).unwrap();
            if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                if value["observation"]["kind"] == "request_fully_flushed" {
                    assert_eq!(value["remote_effect_receipt"], false);
                    flushed.push(value["observation"]["digest"].as_str().unwrap().to_owned());
                }
            }
        }
        for request in peer.observations() {
            assert!(flushed.contains(&request.body_digest));
        }
        f.close().await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_http_callbacks_reuse_claim_and_record_each_send() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend, "normal").await;
        let peer = Peer::start(Scenario::Callbacks).await;
        configure(&f, &peer, false);
        let catalog = list(&f).await;
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(12),
            approved(&f, call(&catalog, "write_marker", serde_json::json!({}))),
        )
        .await
        .unwrap();
        assert_eq!(response["outcome"], "succeeded", "{response}");
        assert_eq!(peer.effect_count(), 1);
        assert_eq!(peer.callback_count(), 2);
        assert_eq!(
            peer.observations()
                .iter()
                .filter(|row| row.method == "tools/call")
                .count(),
            1
        );
        assert!(
            response["receipt"]["wire_artifacts"]
                .as_array()
                .unwrap()
                .len()
                >= 4
        );
        f.close().await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_http_revoked_prepared_credential_never_sends() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend, "normal").await;
        let peer = Peer::with_authorization(
            Scenario::Normal,
            Some("Bearer synthetic-http-bearer".into()),
        )
        .await;
        configure(&f, &peer, true);
        let catalog = list(&f).await;
        let prepared = f
            .host
            .prepare_remote_mcp_call(
                f.thread,
                call(&catalog, "write_marker", serde_json::json!({})),
            )
            .await
            .unwrap();
        let effect = prepared.effect().clone();
        approve_ticket(&f, &prepared);
        f.host
            .revoke_mcp_credential("remote", Revision::ZERO)
            .unwrap();
        assert!(f.host.dispatch_remote_mcp_call(prepared).await.is_err());
        assert_eq!(peer.effect_count(), 0);
        assert!(!peer
            .observations()
            .iter()
            .any(|row| row.method == "tools/call"));
        let effect: Effect = f
            .host
            .snapshot()
            .unwrap()
            .record(Collection::Effect, effect.as_str(), &f.config.workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(effect.state, EffectState::Cancelled);
        f.close().await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_http_lost_response_preserves_unknown_without_replay() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend, "normal").await;
        let peer = Peer::start(Scenario::LostResponse).await;
        configure(&f, &peer, false);
        let catalog = list(&f).await;
        let response = approved(&f, call(&catalog, "write_marker", serde_json::json!({}))).await;
        assert_eq!(response["outcome"], "unknown", "{response}");
        assert_eq!(peer.effect_count(), 1);
        assert_eq!(
            peer.observations()
                .iter()
                .filter(|row| row.method == "tools/call")
                .count(),
            1
        );
        let effect: Effect = f
            .host
            .snapshot()
            .unwrap()
            .record(
                Collection::Effect,
                response["effect"].as_str().unwrap(),
                &f.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(effect.state, EffectState::OutcomeUnknown);
        assert!(f
            .host
            .mcp_control(
                f.thread,
                Request::List {
                    server: "remote".into()
                }
            )
            .await
            .map_or(true, |value| value["dispatched"] == false));
        assert_eq!(peer.effect_count(), 1);
        let id = ToolRunId::parse(response["effect"].as_str().unwrap()).unwrap();
        reopen_unknown(f, id).await;
        assert_eq!(peer.effect_count(), 1);
        assert_eq!(std::fs::read(peer.marker()).unwrap(), b"effect\n");
    }
}
async fn reopen_unknown(f: Fixture, effect: ToolRunId) {
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
    let (restored, owner) = CanonicalHost::open(config.clone()).unwrap();
    let recovered: Effect = restored
        .snapshot()
        .unwrap()
        .record(Collection::Effect, effect.as_str(), &config.workspace)
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(recovered.state, EffectState::OutcomeUnknown);
    owner.close().await.unwrap();
    drop(restored);
    drop(_temp);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_http_cancel_or_owner_shutdown_after_marker_reopens_unknown() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for shutdown in [false, true] {
            let f = Fixture::new(backend, "normal").await;
            let peer = Peer::start(Scenario::BlockAfterEffect).await;
            configure(&f, &peer, false);
            let catalog = list(&f).await;
            let prepared = f
                .host
                .prepare_remote_mcp_call(
                    f.thread,
                    call(&catalog, "write_marker", serde_json::json!({})),
                )
                .await
                .unwrap();
            approve_ticket(&f, &prepared);
            let effect = prepared.effect().clone();
            let host = f.host.clone();
            let waiter = tokio::spawn(async move { host.dispatch_remote_mcp_call(prepared).await });
            peer.wait_effect().await;
            assert_eq!(std::fs::read(peer.marker()).unwrap(), b"effect\n");
            if shutdown {
                let Fixture {
                    _temp,
                    host,
                    owner,
                    test,
                    config,
                    ..
                } = f;
                tokio::time::timeout(std::time::Duration::from_secs(5), owner.close())
                    .await
                    .unwrap()
                    .unwrap();
                // Closing the owner can finish its journal before the local permit
                // observer returns. The durable effect below remains authoritative.
                match waiter.await.unwrap() {
                    Ok(response) => assert_eq!(response["outcome"], "unknown"),
                    Err(error) => assert_eq!(error, "owner checkpoint is closed"),
                }
                test.codex.shutdown_and_wait().await.unwrap();
                drop(test);
                drop(host);
                let (restored, owner) = CanonicalHost::open(config.clone()).unwrap();
                let record: Effect = restored
                    .snapshot()
                    .unwrap()
                    .record(Collection::Effect, effect.as_str(), &config.workspace)
                    .unwrap()
                    .decode()
                    .unwrap();
                assert_eq!(record.state, EffectState::OutcomeUnknown);
                owner.close().await.unwrap();
                drop(restored);
                drop(_temp);
            } else {
                waiter.abort();
                assert!(waiter.await.unwrap_err().is_cancelled());
                reopen_unknown(f, effect).await;
            }
            assert_eq!(peer.effect_count(), 1);
            assert_eq!(std::fs::read(peer.marker()).unwrap(), b"effect\n");
            assert_eq!(
                peer.observations()
                    .iter()
                    .filter(|request| request.method == "tools/call")
                    .count(),
                1
            );
            peer.release();
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_http_selected_source_deleted_before_send_is_never_uploaded() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for after_tls in [false, true] {
            let f = Fixture::new(backend, "normal").await;
            let peer = Peer::start(Scenario::Normal).await;
            configure(&f, &peer, false);
            let catalog = list(&f).await;
            std::fs::write(f.workspace.join("selected.txt"), "selected private source").unwrap();
            let root = vcp_repository::Root::open(
                vcp_repository::RootIdentity {
                    workspace: f.config.workspace.clone(),
                    root: RootId::parse(f.config.workspace.as_str()).unwrap(),
                    repository: f.config.binding.repository.clone(),
                    worktree: f.config.binding.worktree.clone(),
                    binding: f.config.binding.revision,
                },
                &f.workspace,
            )
            .unwrap();
            let source = root
                .read(std::path::Path::new("selected.txt"), 1024)
                .unwrap();
            let (snapshot, _) = provider_snapshot();
            let sealed = sealed_provider_context(&f.host, f.thread, &snapshot, Some(&source));
            let mut prepared = f
                .host
                .qualification_prepare_remote_mcp_context_call(
                    f.thread,
                    call(
                        &catalog,
                        "echo",
                        serde_json::json!({"text":"selected private source"}),
                    ),
                    sealed,
                    vec![root],
                )
                .await
                .unwrap();
            approve_ticket(&f, &prepared);
            if after_tls {
                let (arrived, release) = prepared.qualification_block_before_http(false);
                let host = f.host.clone();
                let waiter =
                    tokio::spawn(async move { host.dispatch_remote_mcp_call(prepared).await });
                tokio::time::timeout(std::time::Duration::from_secs(5), arrived.notified())
                    .await
                    .unwrap();
                std::fs::remove_file(f.workspace.join("selected.txt")).unwrap();
                release.notify_one();
                // Closing the owner can finish its journal before the local permit
                // observer returns. The durable effect below remains authoritative.
                match waiter.await.unwrap() {
                    Ok(response) => assert_eq!(response["outcome"], "unknown"),
                    Err(error) => assert_eq!(error, "owner checkpoint is closed"),
                }
            } else {
                std::fs::remove_file(f.workspace.join("selected.txt")).unwrap();
                assert!(f.host.dispatch_remote_mcp_call(prepared).await.is_err());
            }
            assert!(!peer
                .observations()
                .iter()
                .any(|request| request.method == "tools/call"));
            assert_eq!(peer.effect_count(), 0);
            f.close().await;
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_http_retention_after_last_source_check_blocks_physical_payload() {
    use vcp_domain::retention_selector::{Criterion, Selector, Tree};
    use vcp_lifecycle::foundation::history_retention::Request as Retention;
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend, "normal").await;
        let peer = Peer::start(Scenario::Normal).await;
        configure(&f, &peer, false);
        let catalog = list(&f).await;
        let mut prepared = f
            .host
            .prepare_remote_mcp_call(
                f.thread,
                call(&catalog, "write_marker", serde_json::json!({})),
            )
            .await
            .unwrap();
        approve_ticket(&f, &prepared);
        let (arrived, release) = prepared.qualification_block_before_http(true);
        let host = f.host.clone();
        let waiter = tokio::spawn(async move { host.dispatch_remote_mcp_call(prepared).await });
        tokio::time::timeout(std::time::Duration::from_secs(5), arrived.notified())
            .await
            .unwrap();
        let preview = f
            .host
            .history_retention(Retention::Preview {
                selector: Selector {
                    schema_version: 1,
                    tree: Tree::Match(Criterion::Task(f.config.root_task.clone())),
                },
                action: vcp_memory::retention::Action::Compact,
            })
            .unwrap();
        let before: Workspace = f
            .host
            .snapshot()
            .unwrap()
            .record(
                Collection::Workspace,
                f.config.workspace.as_str(),
                &f.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let applied = f.host.history_retention(Retention::Apply {
            preview: preview["id"].as_str().unwrap().into(),
        });
        release.notify_one();
        assert!(applied.is_ok(), "{applied:?}");
        let response = waiter.await.unwrap().unwrap();
        assert_eq!(response["outcome"], "unknown", "{response}");
        let after: Workspace = f
            .host
            .snapshot()
            .unwrap()
            .record(
                Collection::Workspace,
                f.config.workspace.as_str(),
                &f.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert!(after.deletion > before.deletion);
        assert!(!peer
            .observations()
            .iter()
            .any(|request| request.method == "tools/call"));
        assert_eq!(peer.effect_count(), 0);
        f.close().await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_http_network_denial_makes_no_peer_request() {
    use vcp_domain::policy::{Denial, EffectClass, RuleOrigin};
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let mut f = Fixture::new(backend, "normal").await;
        let peer = Peer::start(Scenario::Normal).await;
        configure(&f, &peer, false);
        f.policy.revision = PolicyRevision::new(1);
        f.policy.denials.push(Denial {
            id: "deny-http".into(),
            origin: RuleOrigin::User,
            reason: "native test explicit network denial".into(),
            effects: BTreeSet::from([EffectClass::Network]),
            tool: None,
            roots: BTreeSet::new(),
            paths: vec![],
        });
        f.host
            .command(
                Command::SetPolicy {
                    policy: f.policy.clone(),
                },
                None,
                Revision::ZERO,
            )
            .unwrap();
        let denied = f
            .host
            .mcp_control(
                f.thread,
                Request::List {
                    server: "remote".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(denied["decision"]["kind"], "deny", "{denied}");
        assert_eq!(denied["dispatched"], false);
        assert!(peer.observations().is_empty());
        assert_eq!(peer.effect_count(), 0);
        f.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_http_credential_in_endpoint_is_rejected_before_canonical_capture() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend, "normal").await;
        let peer = Peer::start(Scenario::Normal).await;
        configure_endpoint(
            &f,
            &peer,
            true,
            format!("{}/synthetic-http-bearer", peer.endpoint()),
        );
        let before = f.host.snapshot().unwrap();
        let error = f
            .host
            .mcp_control(
                f.thread,
                Request::List {
                    server: "remote".into(),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(error, "sensitive remote MCP operation rejected");
        let after = f.host.snapshot().unwrap();
        for collection in [
            Collection::Artifact,
            Collection::Effect,
            Collection::Approval,
        ] {
            let prior: Vec<_> = before
                .records
                .values()
                .filter(|row| row.collection == collection)
                .collect();
            let current: Vec<_> = after
                .records
                .values()
                .filter(|row| row.collection == collection)
                .collect();
            assert_eq!(
                serde_json::to_value(current).unwrap(),
                serde_json::to_value(prior).unwrap()
            );
        }
        assert!(peer.observations().is_empty());
        assert_eq!(peer.effect_count(), 0);
        f.close().await;
    }
}
