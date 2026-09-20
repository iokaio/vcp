// SPDX-License-Identifier: Apache-2.0
use super::mcp::Fixture;
use super::mcp_http::{approved, call, configure, list};
use super::mcp_http_peer::{Peer, Scenario};
use super::*;
use std::collections::BTreeSet;
use vcp_domain::effect::{Effect, EffectState};
use vcp_lifecycle::foundation::mcp::remote_authority::{RemoteProfile, RemoteProfileConfig};
use vcp_lifecycle::foundation::mcp::{RemoteRegistration, Request};

fn assert_effect(f: &Fixture, value: &serde_json::Value, expected: EffectState) {
    let effect: Effect = f
        .host
        .snapshot()
        .unwrap()
        .record(
            Collection::Effect,
            value["effect"].as_str().unwrap(),
            &f.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(effect.state, expected);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn http_untrusted_tls_never_delivers_an_http_request() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend, "normal").await;
        let peer = Peer::start(Scenario::Normal).await;
        let unrelated = Peer::start(Scenario::Normal).await;
        let profile = RemoteProfile::new(RemoteProfileConfig {
            workspace: f.config.workspace.clone(),
            server: "remote".into(),
            revision: Revision::ZERO,
            endpoint: peer.endpoint(),
            credential_ref: None,
        })
        .unwrap();
        f.host
            .configure_mcp_remote(
                RemoteRegistration::fixture_roots(
                    profile,
                    BTreeSet::from(["echo".into()]),
                    vcp_extensions::mcp::registration::Limits {
                        frame_bytes: 65536,
                        total_discovery_bytes: 1024 * 1024,
                        tools: 16,
                        pages: 8,
                        timeout_ms: 10_000,
                        stderr_bytes: 0,
                    },
                    vec![unrelated.root_certificate()],
                )
                .unwrap(),
            )
            .unwrap();
        let response = approved(
            &f,
            Request::List {
                server: "remote".into(),
            },
        )
        .await;
        assert_ne!(response["connected"], true);
        assert_eq!(response["outcome"], "unknown");
        assert!(peer.observations().is_empty());
        assert!(unrelated.observations().is_empty());
        assert_effect(&f, &response, EffectState::OutcomeUnknown);
        f.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn http_redirect_and_changed_session_do_not_follow_or_replay() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for scenario in [Scenario::Redirect, Scenario::SessionChanged] {
            let f = Fixture::new(backend, "normal").await;
            let peer = Peer::start(scenario).await;
            configure(&f, &peer, false);
            let response = approved(
                &f,
                Request::List {
                    server: "remote".into(),
                },
            )
            .await;
            assert_ne!(response["connected"], true);
            assert_eq!(response["outcome"], "unknown", "{response}");
            let requests = peer.observations();
            let expected = if matches!(scenario, Scenario::Redirect) {
                vec!["initialize"]
            } else {
                vec!["initialize", "notifications/initialized", "tools/list"]
            };
            assert_eq!(
                requests
                    .iter()
                    .map(|request| request.method.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
            assert_eq!(peer.effect_count(), 0);
            assert!(!peer.marker().exists());
            assert_effect(&f, &response, EffectState::OutcomeUnknown);
            f.close().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn http_valid_reply_survives_incomplete_and_invalid_json_suffixes() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for scenario in [
            Scenario::ReplyThenBadSuffix,
            Scenario::ReplyThenInvalidJson,
            Scenario::ReplyThenControl,
        ] {
            let f = Fixture::new(backend, "normal").await;
            let peer = Peer::start(scenario).await;
            configure(&f, &peer, false);
            let catalog = list(&f).await;
            let request = call(&catalog, "write_marker", serde_json::json!({}));
            let response = approved(&f, request.clone()).await;
            assert_eq!(response["outcome"], "succeeded", "{response}");
            assert_effect(&f, &response, EffectState::Succeeded);
            assert_eq!(std::fs::read(peer.marker()).unwrap(), b"effect\n");
            assert_eq!(peer.effect_count(), 1);
            assert_eq!(peer.callback_count(), 0);
            assert!(f.host.mcp_control(f.thread, request).await.is_err());
            assert_eq!(
                peer.observations()
                    .iter()
                    .filter(|request| request.method == "tools/call")
                    .count(),
                1
            );
            let refreshed = list(&f).await;
            assert_ne!(
                catalog["catalog"]["tools"][0]["identity_digest"],
                refreshed["catalog"]["tools"][0]["identity_digest"]
            );
            assert_eq!(peer.effect_count(), 1);
            f.close().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn http_operation_output_limit_is_shared_across_separate_post_responses() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let f = Fixture::new(backend, "normal").await;
        let peer = Peer::start(Scenario::CumulativeBody).await;
        let profile = RemoteProfile::new(RemoteProfileConfig {
            workspace: f.config.workspace.clone(),
            server: "remote".into(),
            revision: Revision::ZERO,
            endpoint: peer.endpoint(),
            credential_ref: None,
        })
        .unwrap();
        const LIMIT: usize = 1400;
        f.host
            .configure_mcp_remote(
                RemoteRegistration::fixture_roots(
                    profile,
                    BTreeSet::from(["echo".into()]),
                    vcp_extensions::mcp::registration::Limits {
                        frame_bytes: LIMIT as u64,
                        total_discovery_bytes: LIMIT as u64,
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
        let response = approved(
            &f,
            Request::List {
                server: "remote".into(),
            },
        )
        .await;
        assert_eq!(response["outcome"], "unknown", "{response}");
        let sizes = peer.response_sizes();
        assert_eq!(sizes.len(), 2);
        assert!(sizes.iter().all(|size| *size < LIMIT));
        assert!(sizes.iter().sum::<usize>() > LIMIT);
        assert_eq!(
            peer.observations()
                .iter()
                .map(|request| request.method.as_str())
                .collect::<Vec<_>>(),
            vec!["initialize", "notifications/initialized", "tools/list"]
        );
        let state = f.host.snapshot().unwrap();
        let mut observed_bytes = 0;
        let mut observed_frames = 0;
        for row in state
            .records
            .values()
            .filter(|row| row.collection == Collection::Artifact)
        {
            let artifact: ArtifactDescriptor = row.decode().unwrap();
            let bytes = f.host.read_artifact(artifact.spec.id).unwrap();
            if serde_json::from_slice::<serde_json::Value>(&bytes)
                .is_ok_and(|value| value["jsonrpc"] == "2.0")
            {
                observed_bytes += bytes.len();
                observed_frames += 1;
            }
        }
        assert_eq!(observed_frames, 1);
        assert!(observed_bytes <= LIMIT);
        assert_eq!(peer.effect_count(), 0);
        assert!(!peer.marker().exists());
        f.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn http_authorization_and_session_echoes_are_redacted_in_actual_artifact_bodies() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for scenario in [
            Scenario::SecretEcho,
            Scenario::SecretEchoEscaped,
            Scenario::CallbackSecret,
            Scenario::CallbackSecretEscaped,
        ] {
            let f = Fixture::new(backend, "normal").await;
            let peer =
                Peer::with_authorization(scenario, Some("Bearer synthetic-http-bearer".into()))
                    .await;
            configure(&f, &peer, true);
            let catalog = list(&f).await;
            let response = approved(
                &f,
                call(&catalog, "echo", serde_json::json!({"text":"public input"})),
            )
            .await;
            assert_eq!(response["outcome"], "succeeded", "{response}");
            let secrets = ["synthetic-http-bearer", "fixture-session"];
            let returned = serde_json::to_string(&response).unwrap();
            for secret in secrets {
                assert!(!returned.contains(secret));
            }
            let state = f.host.snapshot().unwrap();
            let mut inspected = 0;
            let mut callback_intents = 0;
            let mut wire_digests = BTreeSet::new();
            for row in state
                .records
                .values()
                .filter(|row| row.collection == Collection::Artifact)
            {
                let artifact: ArtifactDescriptor = row.decode().unwrap();
                let bytes = f.host.read_artifact(artifact.spec.id).unwrap();
                let raw = String::from_utf8_lossy(&bytes);
                if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                    if let Some(digest) = value["request_digest"].as_str() {
                        wire_digests.insert(digest.to_owned());
                    }
                    if value["body"]["id"].is_string() && value["body"].get("result").is_some() {
                        callback_intents += 1;
                    }
                }
                let decoded = serde_json::from_slice::<serde_json::Value>(&bytes)
                    .ok()
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                for secret in secrets {
                    assert!(!raw.contains(secret), "raw artifact contains secret");
                    assert!(
                        !decoded.contains(secret),
                        "decoded artifact contains secret"
                    );
                }
                inspected += 1;
            }
            assert!(inspected >= 6);
            let callbacks = usize::from(matches!(
                scenario,
                Scenario::CallbackSecret | Scenario::CallbackSecretEscaped
            ));
            assert_eq!(peer.callback_count(), callbacks);
            assert_eq!(callback_intents, callbacks);
            assert!(peer
                .observations()
                .iter()
                .all(|request| wire_digests.contains(&request.body_digest)));
            assert_eq!(
                peer.observations()
                    .iter()
                    .filter(|request| request.tool.as_deref() == Some("echo"))
                    .count(),
                1
            );
            assert!(peer
                .observations()
                .iter()
                .all(|request| request.authorization_matches));
            f.close().await;
        }
    }
}
