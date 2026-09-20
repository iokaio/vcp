// SPDX-License-Identifier: Apache-2.0
use super::mcp::Fixture;
use super::mcp_http::approve;
use super::mcp_http_peer::{ContentMode, Peer, Scenario, CONTENT_URIS};
use super::*;
use std::collections::BTreeSet;
use vcp_lifecycle::foundation::mcp::remote_authority::{
    CredentialMaterial, RemoteProfile, RemoteProfileConfig,
};
use vcp_lifecycle::foundation::mcp::{RemoteRegistration, Request};

struct Harness {
    f: Fixture,
    peer: Option<Peer>,
    server: &'static str,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cached_resource_rejects_a_dead_stdio_job_without_disconnect_or_io() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let h = Harness::new(backend, false, ContentMode::ExitAfterRead).await;
        let catalog = h.resources().await;
        let result = h
            .execute(h.resource(&catalog, CONTENT_URIS[0]))
            .await
            .unwrap();
        assert_eq!(result["outcome"], "succeeded", "{result}");
        assert_eq!(result["cache_available"], true, "{result}");
        let cached = Request::ReadCached {
            server: h.server.into(),
            artifact: ArtifactId::parse(result["artifact"].as_str().unwrap()).unwrap(),
        };
        assert_eq!(
            h.execute(cached.clone()).await.unwrap()["prior_observation"],
            true
        );
        let requests = h.requests();
        std::fs::write(h.f.workspace.join("release"), b"exit after retained reply").unwrap();
        let error = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                match h.execute(cached.clone()).await {
                    Err(error) => break error,
                    Ok(value) => assert_eq!(value["prior_observation"], true),
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(error.contains("no longer live"), "{error}");
        assert_eq!(h.requests(), requests);
        assert_eq!(h.effects(), 1);
        h.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resource_notification_during_read_retains_reply_but_prevents_stale_cache() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for remote in [false, true] {
            for mode in [
                ContentMode::UpdatedDuringRead,
                ContentMode::ListChangedDuringRead,
            ] {
                let h = Harness::new(backend, remote, mode).await;
                let catalog = h.resources().await;
                let result = h
                    .execute(h.resource(&catalog, CONTENT_URIS[0]))
                    .await
                    .unwrap();
                assert_eq!(result["outcome"], "succeeded", "{result}");
                assert_eq!(result["cache_available"], false, "{result}");
                assert_eq!(h.effects(), 1);
                let count = h.requests();
                let cached = Request::ReadCached {
                    server: h.server.into(),
                    artifact: ArtifactId::parse(result["artifact"].as_str().unwrap()).unwrap(),
                };
                assert!(h.execute(cached).await.is_err());
                assert_eq!(h.requests(), count);
                assert_eq!(h.effects(), 1);
                h.close().await;
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn escaped_session_value_in_prompt_arguments_is_rejected_before_any_capture() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for session in ["session\"quote", "session\\backslash"] {
            let h = Harness::configured(backend, true, ContentMode::Mixed, Some(session)).await;
            let catalog = h.prompts().await;
            let good = h
                .execute(h.prompt(
                    &catalog,
                    "review",
                    serde_json::json!({"topic":"public topic"}),
                ))
                .await
                .unwrap();
            assert_eq!(good["outcome"], "succeeded", "{good}");
            let count = h.requests();
            let before = h.f.host.snapshot().unwrap();
            let error = h
                .execute(h.prompt(&catalog, "review", serde_json::json!({"topic":session})))
                .await
                .unwrap_err();
            assert!(!error.contains(session));
            let after = h.f.host.snapshot().unwrap();
            for collection in [
                Collection::Artifact,
                Collection::Effect,
                Collection::Approval,
            ] {
                let prior = before
                    .records
                    .values()
                    .filter(|row| row.collection == collection)
                    .collect::<Vec<_>>();
                let current = after
                    .records
                    .values()
                    .filter(|row| row.collection == collection)
                    .collect::<Vec<_>>();
                assert_eq!(
                    serde_json::to_value(prior).unwrap(),
                    serde_json::to_value(current).unwrap()
                );
            }
            assert_eq!(h.requests(), count);
            assert_eq!(h.effects(), 1);
            h.close().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancelled_resource_read_after_server_marker_is_unknown_and_never_replayed() {
    use vcp_domain::effect::{Effect, EffectState};
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for remote in [false, true] {
            let h = Harness::new(backend, remote, ContentMode::BlockAfterEffect).await;
            let catalog = h.resources().await;
            let request = h.resource(&catalog, CONTENT_URIS[0]);
            if remote {
                let question =
                    h.f.host
                        .mcp_control(h.f.thread, request.clone())
                        .await
                        .unwrap();
                approve(&h.f, &question);
            }
            let host = h.f.host.clone();
            let thread = h.f.thread;
            let waiter = tokio::spawn(async move { host.mcp_control(thread, request).await });
            tokio::time::timeout(Duration::from_secs(5), async {
                while h.effects() == 0 {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            assert_eq!(h.effects(), 1);
            let requests = h.requests();
            waiter.abort();
            assert!(waiter.await.unwrap_err().is_cancelled());
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let state = h.f.host.snapshot().unwrap();
                    if state
                        .records
                        .values()
                        .filter(|row| row.collection == Collection::Effect)
                        .any(|row| {
                            row.decode::<Effect>().unwrap().state == EffectState::OutcomeUnknown
                        })
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            assert_eq!(h.requests(), requests);
            assert_eq!(h.effects(), 1);
            h.close().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cached_resources_recheck_catalog_connection_read_policy_pruning_and_credentials() {
    use vcp_domain::policy::{Denial, EffectClass, RuleOrigin};
    use vcp_domain::retention_selector::{Criterion, Selector, Tree};
    use vcp_lifecycle::foundation::history_retention::Request as Retention;
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for remote in [false, true] {
            for mutation in ["catalog", "reconnect", "read-denial", "prune", "credential"] {
                if mutation == "credential" && !remote {
                    continue;
                }
                let mut h = Harness::new(
                    backend,
                    remote,
                    if mutation == "credential" {
                        ContentMode::SecretEcho
                    } else {
                        ContentMode::ResourcesOnly
                    },
                )
                .await;
                let catalog = h.resources().await;
                let result = h
                    .execute(h.resource(&catalog, CONTENT_URIS[0]))
                    .await
                    .unwrap();
                assert_eq!(result["outcome"], "succeeded", "{result}");
                let artifact = ArtifactId::parse(result["artifact"].as_str().unwrap()).unwrap();
                let cached = Request::ReadCached {
                    server: h.server.into(),
                    artifact: artifact.clone(),
                };
                assert_eq!(
                    h.execute(cached.clone()).await.unwrap()["prior_observation"],
                    true
                );
                match mutation {
                    "catalog" => {
                        if let Some(peer) = &h.peer {
                            peer.change_content_descriptors();
                        } else {
                            std::fs::write(h.f.workspace.join("content-changed"), b"change")
                                .unwrap();
                        }
                        let changed = h.resources().await;
                        assert_ne!(
                            catalog["catalog"]["resources"][0]["identity_digest"],
                            changed["catalog"]["resources"][0]["identity_digest"]
                        );
                    }
                    "reconnect" => {
                        h.execute(Request::Disconnect {
                            server: h.server.into(),
                        })
                        .await
                        .unwrap();
                        let changed = h.resources().await;
                        assert_ne!(
                            catalog["catalog"]["resources"][0]["identity_digest"],
                            changed["catalog"]["resources"][0]["identity_digest"]
                        );
                        assert!(h.f.host.read_artifact(artifact.clone()).is_ok());
                    }
                    "read-denial" => {
                        h.f.policy.revision = PolicyRevision::new(1);
                        h.f.policy.denials.push(Denial {
                            id: "deny-cached-resource-read".into(),
                            origin: RuleOrigin::User,
                            reason: "current read authority revoked".into(),
                            effects: BTreeSet::from([EffectClass::Read]),
                            tool: Some("vcp_mcp".into()),
                            roots: BTreeSet::new(),
                            paths: vec![],
                        });
                        let command = Command::SetPolicy {
                            policy: h.f.policy.clone(),
                        };
                        if remote {
                            h.f.host.command(command, None, Revision::ZERO).unwrap();
                        } else {
                            let change =
                                h.f.host
                                    .change_authority(command, None, Revision::ZERO)
                                    .unwrap();
                            h.f.host.disconnect_mcp().await.unwrap();
                            tokio::time::timeout(Duration::from_secs(10), change.wait())
                                .await
                                .unwrap()
                                .unwrap();
                        }
                        let current = vcp_engine::policy::current(
                            &h.f.host.snapshot().unwrap(),
                            &h.f.config.workspace,
                        )
                        .unwrap();
                        assert_eq!(current.revision, PolicyRevision::new(1));
                        assert!(current
                            .denials
                            .iter()
                            .any(|denial| denial.id == "deny-cached-resource-read"));
                    }
                    "prune" => {
                        let preview =
                            h.f.host
                                .history_retention(Retention::Preview {
                                    selector: Selector {
                                        schema_version: 1,
                                        tree: Tree::Match(Criterion::Task(
                                            h.f.config.root_task.clone(),
                                        )),
                                    },
                                    action: vcp_memory::retention::Action::Compact,
                                })
                                .unwrap();
                        h.f.host
                            .history_retention(Retention::Apply {
                                preview: preview["id"].as_str().unwrap().into(),
                            })
                            .unwrap();
                        // Compact invalidates live cache use, but intentionally
                        // retains the evidence bytes for historical inspection.
                        assert!(h.f.host.read_artifact(artifact.clone()).is_ok());
                    }
                    "credential" => {
                        h.f.host
                            .revoke_mcp_credential("content", Revision::ZERO)
                            .unwrap();
                    }
                    _ => unreachable!(),
                }
                let count = h.requests();
                let effects = h.effects();
                let error = h.execute(cached).await.unwrap_err();
                if mutation == "read-denial" && remote {
                    assert!(error.contains("trusted read denial"), "{error}");
                }
                assert_eq!(h.requests(), count);
                assert_eq!(h.effects(), effects);
                if mutation == "prune" {
                    // Actual deletion requires terminal recovery state. Drain
                    // the daemon, cancel the task, then purge the retained data.
                    h.f.host.disconnect_mcp().await.unwrap();
                    let task: Task =
                        h.f.host
                            .snapshot()
                            .unwrap()
                            .record(
                                Collection::Task,
                                h.f.config.root_task.as_str(),
                                &h.f.config.workspace,
                            )
                            .unwrap()
                            .decode()
                            .unwrap();
                    h.f.host
                        .command(
                            Command::Transition {
                                next: TaskState::Cancelled,
                                reason: "terminal resource retention fixture".into(),
                                verification: None,
                            },
                            Some(task.scope.task),
                            task.revision,
                        )
                        .unwrap();
                    let preview = h
                        .f
                        .host
                        .history_retention(Retention::Preview {
                            selector: Selector {
                                schema_version: 1,
                                tree: Tree::Match(Criterion::Task(h.f.config.root_task.clone())),
                            },
                            action: vcp_memory::retention::Action::Purge,
                        })
                        .unwrap();
                    assert_eq!(preview["protected_count"], 0, "{preview}");
                    assert!(preview["selected_count"].as_u64().unwrap() > 0);
                    let receipt =
                        h.f.host
                            .history_retention(Retention::Apply {
                                preview: preview["id"].as_str().unwrap().into(),
                            })
                            .unwrap();
                    assert_eq!(receipt["logical_unavailable"], true, "{receipt}");
                    assert!(h.f.host.read_artifact(artifact.clone()).is_err());
                    assert_eq!(h.requests(), count);
                    assert_eq!(h.effects(), effects);
                }
                h.close().await;
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn duplicate_and_changed_content_descriptors_cannot_reuse_old_identity() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for remote in [false, true] {
            let h = Harness::new(backend, remote, ContentMode::Duplicate).await;
            let result = h
                .execute(Request::Resources {
                    server: h.server.into(),
                })
                .await;
            assert!(
                result
                    .as_ref()
                    .map_or(true, |value| value["connected"] != true),
                "{result:?}"
            );
            assert_eq!(h.effects(), 0);
            h.close().await;
            let h = Harness::new(backend, remote, ContentMode::Mixed).await;
            let original = h.resources().await;
            let stale = h.resource(&original, CONTENT_URIS[0]);
            if let Some(peer) = &h.peer {
                peer.change_content_descriptors();
            } else {
                std::fs::write(h.f.workspace.join("content-changed"), b"change").unwrap();
            }
            let changed = h.resources().await;
            assert_ne!(
                original["catalog"]["resources"][0]["identity_digest"],
                changed["catalog"]["resources"][0]["identity_digest"]
            );
            let count = h.requests();
            assert!(h.execute(stale).await.is_err());
            assert_eq!(h.requests(), count);
            assert_eq!(h.effects(), 0);
            h.close().await;
        }
    }
}
impl Harness {
    async fn new(backend: BackendKind, remote: bool, mode: ContentMode) -> Self {
        Self::configured(backend, remote, mode, None).await
    }
    async fn configured(
        backend: BackendKind,
        remote: bool,
        mode: ContentMode,
        session: Option<&str>,
    ) -> Self {
        let scenario = match mode {
            ContentMode::Mixed => "content-mixed",
            ContentMode::ResourcesOnly => "content-resources-only",
            ContentMode::PromptsOnly => "content-prompts-only",
            ContentMode::Pagination => "content-pagination",
            ContentMode::Duplicate => "content-duplicate",
            ContentMode::Changed => "content-changed",
            ContentMode::Binary => "content-binary",
            ContentMode::Hostile => "content-hostile",
            ContentMode::InvalidRole => "content-invalid-role",
            ContentMode::Oversized => "content-oversized",
            ContentMode::LostResponse => "content-lost",
            ContentMode::BlockAfterEffect => "content-block",
            ContentMode::SecretEcho => "content-secret",
            ContentMode::SecretEchoEscaped => "content-secret-escaped",
            ContentMode::CallbackSecret => "content-callback-secret",
            ContentMode::CallbackSecretEscaped => "content-callback-secret-escaped",
            ContentMode::UpdatedDuringRead => "content-updated-during-read",
            ContentMode::ListChangedDuringRead => "content-list-changed-during-read",
            ContentMode::ExitAfterRead => "content-exit-after-read",
            ContentMode::NumericOpaque => "content-numeric-opaque",
        };
        let f = Fixture::new(backend, if remote { "normal" } else { scenario }).await;
        let secret = matches!(
            mode,
            ContentMode::SecretEcho
                | ContentMode::SecretEchoEscaped
                | ContentMode::CallbackSecret
                | ContentMode::CallbackSecretEscaped
        );
        let peer = if remote {
            let peer = if let Some(session) = session {
                Peer::with_session(Scenario::Content(mode), session).await
            } else {
                Peer::with_authorization(
                    Scenario::Content(mode),
                    secret.then(|| "Bearer synthetic-http-bearer".into()),
                )
                .await
            };
            let profile = RemoteProfile::new(RemoteProfileConfig {
                workspace: f.config.workspace.clone(),
                server: "content".into(),
                revision: Revision::ZERO,
                endpoint: peer.endpoint(),
                credential_ref: secret.then(|| "fixture-auth".into()),
            })
            .unwrap();
            let registration = RemoteRegistration::fixture_roots(
                profile,
                if matches!(mode, ContentMode::ResourcesOnly | ContentMode::PromptsOnly) {
                    BTreeSet::new()
                } else {
                    BTreeSet::from(["echo".into()])
                },
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
                if mode == ContentMode::PromptsOnly {
                    BTreeSet::new()
                } else {
                    CONTENT_URIS.into_iter().map(str::to_owned).collect()
                },
                if mode == ContentMode::ResourcesOnly {
                    BTreeSet::new()
                } else {
                    BTreeSet::from(["review".into(), "explain".into()])
                },
            )
            .unwrap();
            f.host.configure_mcp_remote(registration).unwrap();
            if secret {
                f.host
                    .install_mcp_credential(
                        "content",
                        None,
                        CredentialMaterial::bearer("synthetic-http-bearer".into()).unwrap(),
                        Timestamp::new(u64::MAX - 1),
                    )
                    .unwrap();
            }
            Some(peer)
        } else {
            None
        };
        Self {
            f,
            peer,
            server: if remote { "content" } else { "fixture" },
        }
    }
    async fn execute(&self, request: Request) -> Result<serde_json::Value, String> {
        for _ in 0..4 {
            let value = self
                .f
                .host
                .mcp_control(self.f.thread, request.clone())
                .await?;
            if value["decision"]["kind"] == "question" {
                approve(&self.f, &value);
                continue;
            }
            return Ok(value);
        }
        Err("fixture approval count exceeded".into())
    }
    fn requests(&self) -> usize {
        self.peer.as_ref().map_or_else(
            || {
                std::fs::read_to_string(self.f.workspace.join("requests.jsonl"))
                    .unwrap_or_default()
                    .lines()
                    .count()
            },
            |peer| peer.observations().len(),
        )
    }
    fn effects(&self) -> usize {
        self.peer.as_ref().map_or_else(
            || {
                std::fs::read_to_string(self.f.workspace.join("content-effects.jsonl"))
                    .unwrap_or_default()
                    .lines()
                    .count()
            },
            Peer::effect_count,
        )
    }
    fn resource(&self, catalog: &serde_json::Value, uri: &str) -> Request {
        let item = catalog["catalog"]["resources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["uri"] == uri)
            .unwrap();
        Request::ReadResource {
            server: self.server.into(),
            uri: uri.into(),
            identity_digest: item["identity_digest"].as_str().unwrap().into(),
        }
    }
    fn prompt(&self, catalog: &serde_json::Value, name: &str, args: serde_json::Value) -> Request {
        let item = catalog["catalog"]["prompts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["name"] == name)
            .unwrap();
        Request::GetPrompt {
            server: self.server.into(),
            prompt: name.into(),
            identity_digest: item["identity_digest"].as_str().unwrap().into(),
            arguments_json: args.to_string(),
        }
    }
    async fn resources(&self) -> serde_json::Value {
        let result = self
            .execute(Request::Resources {
                server: self.server.into(),
            })
            .await
            .unwrap();
        assert_eq!(result["connected"], true, "{result}");
        result
    }
    async fn prompts(&self) -> serde_json::Value {
        let result = self
            .execute(Request::Prompts {
                server: self.server.into(),
            })
            .await
            .unwrap();
        assert_eq!(result["connected"], true, "{result}");
        result
    }
    async fn close(self) {
        self.f.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn https_resource_prompt_and_callback_secret_echoes_never_enter_artifacts() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for mode in [
            ContentMode::SecretEcho,
            ContentMode::SecretEchoEscaped,
            ContentMode::CallbackSecret,
            ContentMode::CallbackSecretEscaped,
        ] {
            let h = Harness::new(backend, true, mode).await;
            let resources = h.resources().await;
            let read = h
                .execute(h.resource(&resources, CONTENT_URIS[0]))
                .await
                .unwrap();
            assert_eq!(read["outcome"], "succeeded", "{read}");
            let prompts = h.prompts().await;
            let prompt = h
                .execute(h.prompt(&prompts, "review", serde_json::json!({"topic":"public"})))
                .await
                .unwrap();
            assert_eq!(prompt["outcome"], "succeeded", "{prompt}");
            for value in [&read, &prompt] {
                for secret in ["synthetic-http-bearer", "fixture-session"] {
                    assert!(!value.to_string().contains(secret));
                }
            }
            let state = h.f.host.snapshot().unwrap();
            let mut count = 0;
            for row in state
                .records
                .values()
                .filter(|row| row.collection == Collection::Artifact)
            {
                let descriptor: ArtifactDescriptor = row.decode().unwrap();
                let bytes = h.f.host.read_artifact(descriptor.spec.id).unwrap();
                let raw = String::from_utf8_lossy(&bytes);
                let normalized = serde_json::from_slice::<serde_json::Value>(&bytes)
                    .ok()
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                for secret in ["synthetic-http-bearer", "fixture-session"] {
                    assert!(!raw.contains(secret));
                    assert!(!normalized.contains(secret));
                }
                count += 1;
            }
            assert!(count >= 8);
            if matches!(
                mode,
                ContentMode::CallbackSecret | ContentMode::CallbackSecretEscaped
            ) {
                assert_eq!(h.peer.as_ref().unwrap().callback_count(), 2);
            }
            h.close().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn resources_and_prompts_work_without_tool_capability_and_cache_without_io() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for remote in [false, true] {
            let h = Harness::new(backend, remote, ContentMode::ResourcesOnly).await;
            let catalog = h.resources().await;
            for uri in CONTENT_URIS {
                let result = h.execute(h.resource(&catalog, uri)).await.unwrap();
                assert_eq!(result["outcome"], "succeeded", "{result}");
                assert!(result
                    .to_string()
                    .contains("Public resource bytes for opaque key"));
                assert_eq!(result["external_content"], true);
                assert_eq!(result["cache_available"], true);
                let count = h.requests();
                let effects = h.effects();
                let cached = h
                    .execute(Request::ReadCached {
                        server: h.server.into(),
                        artifact: ArtifactId::parse(result["artifact"].as_str().unwrap()).unwrap(),
                    })
                    .await
                    .unwrap();
                assert_eq!(cached["prior_observation"], true, "{cached}");
                assert_eq!(h.requests(), count);
                assert_eq!(h.effects(), effects);
            }
            assert_eq!(h.effects(), 3);
            assert!(!h.f.workspace.join("value.txt").exists());
            h.close().await;
            let h = Harness::new(backend, remote, ContentMode::PromptsOnly).await;
            let catalog = h.prompts().await;
            let count = h.requests();
            assert!(h
                .execute(h.prompt(&catalog, "review", serde_json::json!({})))
                .await
                .is_err());
            assert_eq!(h.requests(), count);
            let result = h
                .execute(h.prompt(
                    &catalog,
                    "review",
                    serde_json::json!({"topic":"public subject"}),
                ))
                .await
                .unwrap();
            assert_eq!(result["outcome"], "succeeded", "{result}");
            assert_eq!(result["roles_are_external_data"], true);
            assert!(result.to_string().contains("Embedded external bytes"));
            assert_eq!(h.effects(), 1);
            h.close().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn content_pagination_identity_and_undeclared_selection_are_bounded() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for remote in [false, true] {
            let h = Harness::new(backend, remote, ContentMode::Pagination).await;
            let resources = h.resources().await;
            let prompts = h.prompts().await;
            assert_eq!(
                resources["catalog"]["resources"].as_array().unwrap().len(),
                3
            );
            assert_eq!(prompts["catalog"]["prompts"].as_array().unwrap().len(), 2);
            let mut request = h.resource(&resources, CONTENT_URIS[0]);
            if let Request::ReadResource { uri, .. } = &mut request {
                *uri = "fixture://not-configured/private".into();
            }
            let count = h.requests();
            assert!(h.execute(request).await.is_err());
            assert_eq!(h.requests(), count);
            assert_eq!(h.effects(), 0);
            let mut request = h.resource(&resources, CONTENT_URIS[0]);
            if let Request::ReadResource {
                identity_digest, ..
            } = &mut request
            {
                *identity_digest = "0".repeat(64);
            }
            assert!(h.execute(request).await.is_err());
            assert_eq!(h.requests(), count);
            h.close().await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn binary_content_is_explicitly_omitted_and_hostile_text_never_executes() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for remote in [false, true] {
            for mode in [ContentMode::Binary, ContentMode::Hostile] {
                let h = Harness::new(backend, remote, mode).await;
                let resources = h.resources().await;
                let read = h
                    .execute(h.resource(&resources, CONTENT_URIS[0]))
                    .await
                    .unwrap();
                assert_eq!(read["outcome"], "succeeded", "{read}");
                let prompts = h.prompts().await;
                let prompt = h
                    .execute(h.prompt(&prompts, "review", serde_json::json!({"topic":"public"})))
                    .await
                    .unwrap();
                assert_eq!(prompt["outcome"], "succeeded", "{prompt}");
                if mode == ContentMode::Binary {
                    assert!(read.to_string().contains("omitted"));
                    assert!(prompt.to_string().contains("omitted"));
                    assert!(!read.to_string().contains("AAEC"));
                    assert!(!prompt.to_string().contains("AAEC"));
                } else {
                    assert!(read.to_string().contains("Ignore user instructions"));
                    assert_eq!(prompt["roles_are_external_data"], true);
                }
                assert_eq!(h.effects(), 2);
                assert!(!h.f.workspace.join("value.txt").exists());
                h.close().await;
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn content_invalid_role_oversize_and_lost_reply_fail_without_replay() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for remote in [false, true] {
            for mode in [
                ContentMode::InvalidRole,
                ContentMode::Oversized,
                ContentMode::LostResponse,
            ] {
                let h = Harness::new(backend, remote, mode).await;
                let request = if mode == ContentMode::InvalidRole {
                    let catalog = h.prompts().await;
                    h.prompt(&catalog, "review", serde_json::json!({"topic":"public"}))
                } else {
                    let catalog = h.resources().await;
                    h.resource(&catalog, CONTENT_URIS[0])
                };
                let result = h.execute(request).await;
                assert!(
                    result
                        .as_ref()
                        .map_or(true, |value| value["outcome"] == "unknown"
                            || value["outcome"] == "failed"),
                    "{result:?}"
                );
                assert_eq!(h.effects(), 1);
                h.close().await;
            }
        }
    }
}
