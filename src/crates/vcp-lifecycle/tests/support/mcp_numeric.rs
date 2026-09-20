// SPDX-License-Identifier: Apache-2.0
use super::*;
use super::{
    mcp::Fixture,
    mcp_http_peer::{numeric, Peer, Scenario},
};
use serde_json::Value;
use vcp_domain::effect::{Effect, EffectState};
use vcp_lifecycle::foundation::mcp::Request;

struct Harness {
    f: Fixture,
    peer: Option<Peer>,
    server: &'static str,
}
impl Harness {
    async fn scalar_secret(
        backend: BackendKind,
        session: bool,
        mode: numeric::Mode,
        secret: &str,
    ) -> Self {
        use std::collections::BTreeSet;
        use vcp_lifecycle::foundation::mcp::{
            remote_authority::{CredentialMaterial, RemoteProfile, RemoteProfileConfig},
            RemoteRegistration,
        };
        let f = Fixture::new(backend, "normal").await;
        let peer = if session {
            Peer::with_session(Scenario::Numeric(mode), secret).await
        } else {
            Peer::with_authorization(Scenario::Numeric(mode), Some(format!("Bearer {secret}")))
                .await
        };
        f.host
            .configure_mcp_remote(
                RemoteRegistration::fixture_roots(
                    RemoteProfile::new(RemoteProfileConfig {
                        workspace: f.config.workspace.clone(),
                        server: "remote".into(),
                        revision: Revision::ZERO,
                        endpoint: peer.endpoint(),
                        credential_ref: (!session).then(|| "numeric-auth".into()),
                    })
                    .unwrap(),
                    BTreeSet::from(["echo".into()]),
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
        if !session {
            f.host
                .install_mcp_credential(
                    "remote",
                    None,
                    CredentialMaterial::bearer(secret.into()).unwrap(),
                    Timestamp::new(u64::MAX - 1),
                )
                .unwrap();
        }
        Self {
            f,
            peer: Some(peer),
            server: "remote",
        }
    }
    async fn new(backend: BackendKind, remote: bool, mode: numeric::Mode) -> Self {
        // Fail early if the actual fixture feature graph rounds numeric tokens.
        assert_eq!(
            serde_json::from_str::<Value>(numeric::STRUCTURED).unwrap()["exact"].to_string(),
            "9007199254740993.00000000001"
        );
        let f = Fixture::new(backend, if remote { "normal" } else { mode.scenario() }).await;
        let peer = if remote {
            let peer = Peer::with_authorization(
                Scenario::Numeric(mode),
                (mode == numeric::Mode::Secret).then(|| "Bearer synthetic-http-bearer".into()),
            )
            .await;
            super::mcp_http::configure(&f, &peer, mode == numeric::Mode::Secret);
            Some(peer)
        } else {
            None
        };
        Self {
            f,
            peer,
            server: if remote { "remote" } else { "fixture" },
        }
    }
    async fn execute(&self, request: Request) -> Result<Value, String> {
        for _ in 0..4 {
            let value = self
                .f
                .host
                .mcp_control(self.f.thread, request.clone())
                .await?;
            if value["decision"]["kind"] == "question" {
                super::mcp_http::approve(&self.f, &value);
            } else {
                return Ok(value);
            }
        }
        panic!("bounded numeric approval loop")
    }
    async fn list(&self) -> Value {
        let value = self
            .execute(Request::List {
                server: self.server.into(),
            })
            .await
            .unwrap();
        assert_eq!(value["connected"], true, "{value}");
        assert_eq!(
            value["catalog"]["tools"].as_array().unwrap().len(),
            1,
            "{value}"
        );
        value
    }
    fn call(&self, catalog: &Value, args: &str) -> Request {
        Request::Call {
            server: self.server.into(),
            tool: "echo".into(),
            identity_digest: catalog["catalog"]["tools"][0]["identity_digest"]
                .as_str()
                .unwrap()
                .into(),
            arguments_json: args.into(),
        }
    }
    fn wire(&self) -> Vec<Vec<u8>> {
        if let Some(peer) = &self.peer {
            peer.numeric_wire()
        } else {
            std::fs::read(self.f.workspace.join("numeric-wire.jsonl"))
                .unwrap_or_default()
                .split(|v| *v == b'\n')
                .filter(|v| !v.is_empty())
                .map(|v| v.to_vec())
                .collect()
        }
    }
    fn effects(&self) -> usize {
        if let Some(peer) = &self.peer {
            peer.effect_count()
        } else {
            std::fs::read_to_string(self.f.workspace.join("numeric-effects.jsonl"))
                .unwrap_or_default()
                .lines()
                .count()
        }
    }
    async fn close(self) {
        self.f.close().await;
    }
}
pub(super) fn structured(value: &Value) -> Option<&Value> {
    match value {
        Value::Object(map) => map
            .get("structured_content")
            .filter(|v| v.is_object())
            .or_else(|| map.values().find_map(structured)),
        Value::Array(values) => values.iter().find_map(structured),
        _ => None,
    }
}
pub(super) fn assert_exact(value: &Value, secret: bool) {
    let found = structured(value).unwrap_or_else(|| panic!("no structured result: {value}"));
    for (key, expected) in [
        ("amount", "3e-1"),
        ("huge", "18446744073709551616"),
        ("exact", "900719925474099300000000001e-11"),
        ("exponent", "1e+128"),
    ] {
        assert_eq!(found[key].to_string(), expected, "{key}");
    }
    if !secret {
        assert_eq!(
            serde_json::to_string(found).unwrap(),
            numeric::NORMALIZED_STRUCTURED
        );
        assert!(value.to_string().contains(
            &serde_json::to_string(numeric::OPAQUE_TEXT).unwrap()
                [1..serde_json::to_string(numeric::OPAQUE_TEXT).unwrap().len() - 1]
        ));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exact_numeric_wire_receipt_and_schema_rejections_on_both_transports() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for remote in [false, true] {
            let h = Harness::new(backend, remote, numeric::Mode::Normal).await;
            let catalog = h.list().await;
            assert!(catalog.to_string().contains("mcp-schema/2"));
            let before = h.wire().len();
            for bad in [
                numeric::ARGUMENTS.replace("0.30", "0.30000000000000000000001"),
                numeric::ARGUMENTS.replace("0.30", "0.8"),
                numeric::ARGUMENTS.replace("\"x\":0.10", "\"x\":0.15"),
                numeric::ARGUMENTS.replace("[0.1,0.2]", "[1,1.0]"),
                numeric::ARGUMENTS.replace("1e128", "1e129"),
                numeric::ARGUMENTS.replace("\"nullable\":null,", ""),
                numeric::ARGUMENTS.replace("\"nullable\":null", "\"nullable\":false"),
            ] {
                assert!(h.execute(h.call(&catalog, &bad)).await.is_err());
                assert_eq!(h.wire().len(), before);
                assert_eq!(h.effects(), 0);
            }
            let result = h
                .execute(h.call(&catalog, numeric::ARGUMENTS))
                .await
                .unwrap();
            assert_eq!(result["outcome"], "succeeded", "{result}");
            assert_exact(&result, false);
            assert_eq!(h.effects(), 1);
            let wires = h.wire();
            let call = wires
                .iter()
                .find(|bytes| {
                    serde_json::from_slice::<Value>(bytes).unwrap()["method"] == "tools/call"
                })
                .unwrap();
            assert!(std::str::from_utf8(call)
                .unwrap()
                .contains(numeric::NORMALIZED_ARGUMENTS));
            numeric::assert_received(
                &serde_json::from_slice::<Value>(call).unwrap()["params"]["arguments"],
            );
            let receipt =
                h.f.host
                    .read_artifact(ArtifactId::parse(result["artifact"].as_str().unwrap()).unwrap())
                    .unwrap();
            let receipt: Value = serde_json::from_slice(&receipt).unwrap();
            assert_exact(&receipt, false);
            assert!(receipt["identity"].to_string().contains("mcp-schema/2"));
            let digest = vcp_protocol::digest_bytes(call);
            let mut joined = false;
            for row in
                h.f.host
                    .snapshot()
                    .unwrap()
                    .records
                    .values()
                    .filter(|r| r.collection == Collection::Artifact)
            {
                let descriptor: ArtifactDescriptor = row.decode().unwrap();
                if descriptor.spec.schema.contains("mcp") {
                    let bytes = h.f.host.read_artifact(descriptor.spec.id).unwrap();
                    if String::from_utf8_lossy(&bytes).contains(&digest) {
                        joined = true;
                    }
                }
            }
            assert!(
                joined,
                "actual call body digest absent from canonical MCP evidence"
            );
            if let Some(peer) = &h.peer {
                peer.change_content_descriptors();
            } else {
                std::fs::write(h.f.workspace.join("numeric-changed"), b"changed").unwrap();
            }
            let changed = h.list().await;
            assert_ne!(
                catalog["catalog"]["tools"][0]["identity_digest"],
                changed["catalog"]["tools"][0]["identity_digest"]
            );
            let count = h.wire().len();
            assert!(h
                .execute(h.call(&catalog, numeric::ARGUMENTS))
                .await
                .is_err());
            assert_eq!(h.wire().len(), count);
            assert_eq!(h.effects(), 1);
            h.close().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn numeric_invalid_output_and_lost_reply_do_not_replay_effects() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for remote in [false, true] {
            for mode in [numeric::Mode::InvalidOutput, numeric::Mode::LostResponse] {
                let h = Harness::new(backend, remote, mode).await;
                let catalog = h.list().await;
                let result = h.execute(h.call(&catalog, numeric::ARGUMENTS)).await;
                assert!(
                    result
                        .as_ref()
                        .map_or(true, |v| v["outcome"] != "succeeded"),
                    "{result:?}"
                );
                assert_eq!(h.effects(), 1);
                assert_eq!(
                    h.wire()
                        .iter()
                        .filter(|b| serde_json::from_slice::<Value>(b).unwrap()["method"]
                            == "tools/call")
                        .count(),
                    1
                );
                let state = h.f.host.snapshot().unwrap();
                assert!(state
                    .records
                    .values()
                    .filter(|r| r.collection == Collection::Effect)
                    .any(|r| r.decode::<Effect>().unwrap().state == EffectState::OutcomeUnknown));
                h.close().await;
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn numeric_callback_ids_are_mathematical_integers_without_fraction_coercion() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for remote in [false, true] {
            for mode in [
                numeric::Mode::IntegerCallback,
                numeric::Mode::FractionalCallback,
            ] {
                let h = Harness::new(backend, remote, mode).await;
                let catalog = h.list().await;
                let result = h.execute(h.call(&catalog, numeric::ARGUMENTS)).await;
                if mode == numeric::Mode::IntegerCallback {
                    let value = result.unwrap();
                    assert_eq!(value["outcome"], "succeeded");
                    assert_exact(&value, false);
                    let reply = h
                        .wire()
                        .into_iter()
                        .find(|b| {
                            let v: Value = serde_json::from_slice(b).unwrap();
                            v.get("method").is_none()
                                && v["id"].as_number().is_some_and(|id| id.as_str() == "1")
                        })
                        .expect("exact integer callback reply");
                    assert!(!std::str::from_utf8(&reply).unwrap().contains("1.0"));
                } else {
                    assert!(result
                        .as_ref()
                        .map_or(true, |v| v["outcome"] != "succeeded"));
                    assert!(!h.wire().iter().any(|b| serde_json::from_slice::<Value>(b)
                        .unwrap()
                        .get("method")
                        .is_none()));
                }
                h.close().await;
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn http_secret_sanitization_preserves_exact_numeric_siblings() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let h = Harness::new(backend, true, numeric::Mode::Secret).await;
        let catalog = h.list().await;
        let result = h
            .execute(h.call(&catalog, numeric::ARGUMENTS))
            .await
            .unwrap();
        assert_eq!(result["outcome"], "succeeded");
        assert_exact(&result, true);
        assert_eq!(h.effects(), 1);
        for secret in ["synthetic-http-bearer", "fixture-session"] {
            assert!(!result.to_string().contains(secret));
        }
        let artifact = ArtifactId::parse(result["artifact"].as_str().unwrap()).unwrap();
        let receipt: Value =
            serde_json::from_slice(&h.f.host.read_artifact(artifact).unwrap()).unwrap();
        assert_exact(&receipt, true);
        for row in
            h.f.host
                .snapshot()
                .unwrap()
                .records
                .values()
                .filter(|r| r.collection == Collection::Artifact)
        {
            let descriptor: ArtifactDescriptor = row.decode().unwrap();
            let bytes = h.f.host.read_artifact(descriptor.spec.id).unwrap();
            let mut text = String::from_utf8_lossy(&bytes).into_owned();
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                text.push_str(&value.to_string());
            }
            for secret in ["synthetic-http-bearer", "fixture-session"] {
                assert!(!text.contains(secret), "secret in canonical artifact");
            }
        }
        h.close().await;
    }
}

fn has_exact_text(value: &Value, text: &str) -> bool {
    match value {
        Value::String(v) => v == text,
        Value::Array(v) => v.iter().any(|v| has_exact_text(v, text)),
        Value::Object(v) => v.values().any(|v| has_exact_text(v, text)),
        _ => false,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn numeric_annotation_resource_prompt_and_cache_keep_opaque_text_bytes() {
    use super::mcp_http_peer::{ContentMode, CONTENT_URIS};
    use std::collections::BTreeSet;
    use vcp_lifecycle::foundation::mcp::{
        remote_authority::{RemoteProfile, RemoteProfileConfig},
        RemoteRegistration,
    };
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for remote in [false, true] {
            let f = Fixture::new(backend, "content-numeric-opaque").await;
            let peer = if remote {
                let peer = Peer::start(Scenario::Content(ContentMode::NumericOpaque)).await;
                f.host
                    .configure_mcp_remote(
                        RemoteRegistration::fixture_roots(
                            RemoteProfile::new(RemoteProfileConfig {
                                workspace: f.config.workspace.clone(),
                                server: "remote".into(),
                                revision: Revision::ZERO,
                                endpoint: peer.endpoint(),
                                credential_ref: None,
                            })
                            .unwrap(),
                            BTreeSet::new(),
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
                        .unwrap()
                        .with_content(
                            BTreeSet::from([CONTENT_URIS[0].into()]),
                            BTreeSet::from(["review".into()]),
                        )
                        .unwrap(),
                    )
                    .unwrap();
                Some(peer)
            } else {
                None
            };
            let h = Harness {
                f,
                peer,
                server: if remote { "remote" } else { "fixture" },
            };
            let catalog = h
                .execute(Request::Resources {
                    server: h.server.into(),
                })
                .await
                .unwrap();
            let entry = catalog["catalog"]["resources"]
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["uri"] == CONTENT_URIS[0])
                .unwrap();
            let result = h
                .execute(Request::ReadResource {
                    server: h.server.into(),
                    uri: CONTENT_URIS[0].into(),
                    identity_digest: entry["identity_digest"].as_str().unwrap().into(),
                })
                .await
                .unwrap();
            assert_eq!(result["outcome"], "succeeded");
            assert!(has_exact_text(&result, numeric::OPAQUE_TEXT));
            let id = ArtifactId::parse(result["artifact"].as_str().unwrap()).unwrap();
            let count = || {
                if let Some(peer) = &h.peer {
                    peer.observations().len()
                } else {
                    std::fs::read_to_string(h.f.workspace.join("requests.jsonl"))
                        .unwrap()
                        .lines()
                        .count()
                }
            };
            let before = count();
            let cached = h
                .execute(Request::ReadCached {
                    server: h.server.into(),
                    artifact: id.clone(),
                })
                .await
                .unwrap();
            assert_eq!(cached["prior_observation"], true);
            assert!(has_exact_text(&cached, numeric::OPAQUE_TEXT));
            assert_eq!(count(), before);
            let bytes = h.f.host.read_artifact(id).unwrap();
            assert!(has_exact_text(
                &serde_json::from_slice::<Value>(&bytes).unwrap(),
                numeric::OPAQUE_TEXT
            ));
            let prompts = h
                .execute(Request::Prompts {
                    server: h.server.into(),
                })
                .await
                .unwrap();
            let entry = prompts["catalog"]["prompts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["name"] == "review")
                .unwrap();
            let prompt = h
                .execute(Request::GetPrompt {
                    server: h.server.into(),
                    prompt: "review".into(),
                    identity_digest: entry["identity_digest"].as_str().unwrap().into(),
                    arguments_json: r#"{"topic":"opaque text"}"#.into(),
                })
                .await
                .unwrap();
            assert_eq!(prompt["outcome"], "succeeded");
            assert_eq!(prompt["roles_are_external_data"], true);
            assert!(has_exact_text(&prompt, numeric::OPAQUE_TEXT));
            assert!(!h.f.workspace.join("value.txt").exists());
            h.close().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn normalized_numeric_credential_and_session_aliases_never_become_authority_or_evidence() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for session in [false, true] {
            let h = Harness::scalar_secret(
                backend,
                session,
                numeric::Mode::Normal,
                "0.1234567890123456700",
            )
            .await;
            let catalog = h.list().await;
            let state = h.f.host.snapshot().unwrap();
            let sensitive = |state: &vcp_store::contract::State| {
                state
                    .records
                    .values()
                    .filter(|r| {
                        matches!(
                            r.collection,
                            Collection::Artifact | Collection::Effect | Collection::Approval
                        )
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            };
            let before = sensitive(&state);
            let requests = h.wire().len();
            for value in ["0.12345678901234567", "12345678901234567e-17"] {
                let arguments = numeric::ARGUMENTS
                    .replace("\"nullable\":null", &format!("\"nullable\":{value}"));
                assert!(
                    vcp_extensions::mcp::schema::Schema::compile(
                        numeric::INPUT_SCHEMA.as_bytes(),
                        vcp_extensions::mcp::schema::Limits::default(),
                    )
                    .unwrap()
                    .arguments(arguments.as_bytes())
                    .is_ok(),
                    "secret input must independently satisfy the schema"
                );
                let error = h.execute(h.call(&catalog, &arguments)).await.unwrap_err();
                assert!(!error.contains("0.12345678901234567"));
                assert!(!error.contains("12345678901234567e-17"));
                assert_eq!(sensitive(&h.f.host.snapshot().unwrap()), before);
                assert_eq!(h.wire().len(), requests);
                assert_eq!(h.effects(), 0);
            }
            h.close().await;

            let h = Harness::scalar_secret(
                backend,
                session,
                numeric::Mode::NumericSecret,
                "0.1234567890123456700",
            )
            .await;
            let catalog = h.list().await;
            let result = h
                .execute(h.call(&catalog, numeric::ARGUMENTS))
                .await
                .unwrap();
            assert_eq!(result["outcome"], "succeeded", "{result}");
            // Secret rejection/redaction may omit structured content altogether. If
            // numeric siblings remain available, their exact values must be preserved.
            if structured(&result).is_some() {
                assert_exact(&result, true);
            }
            for token in [
                "0.1234567890123456700",
                "0.12345678901234567",
                "12345678901234567e-17",
            ] {
                assert!(!result.to_string().contains(token));
            }
            for row in
                h.f.host
                    .snapshot()
                    .unwrap()
                    .records
                    .values()
                    .filter(|r| r.collection == Collection::Artifact)
            {
                let descriptor: ArtifactDescriptor = row.decode().unwrap();
                let bytes = h.f.host.read_artifact(descriptor.spec.id).unwrap();
                let text = String::from_utf8_lossy(&bytes);
                for token in [
                    "0.1234567890123456700",
                    "0.12345678901234567",
                    "12345678901234567e-17",
                ] {
                    assert!(!text.contains(token), "secret alias in canonical artifact");
                }
            }
            assert_eq!(h.effects(), 1);
            h.close().await;

            let h = Harness::scalar_secret(
                backend,
                session,
                numeric::Mode::Normal,
                "9007199254740993.0000000000100",
            )
            .await;
            let result = h
                .execute(Request::List {
                    server: h.server.into(),
                })
                .await;
            if let Ok(value) = result {
                assert!(
                    value["catalog"]["tools"]
                        .as_array()
                        .is_none_or(|tools| tools.is_empty()),
                    "secret-bearing numeric schema became callable"
                );
            }
            assert_eq!(h.effects(), 0);
            let methods = h
                .peer
                .as_ref()
                .unwrap()
                .observations()
                .into_iter()
                .map(|observation| observation.method)
                .collect::<Vec<_>>();
            assert!(methods.iter().any(|method| method == "initialize"));
            assert!(
                methods.iter().any(|method| method == "tools/list"),
                "secret-bearing schema must actually reach discovery: {methods:?}"
            );
            assert!(!h
                .wire()
                .iter()
                .any(|b| serde_json::from_slice::<Value>(b).unwrap()["method"] == "tools/call"));
            h.close().await;
        }
    }
}
