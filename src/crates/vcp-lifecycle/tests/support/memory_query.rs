// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::sync::atomic::AtomicBool;
use vcp_lifecycle::foundation::{memory_publication, memory_query, memory_vectors};
use vcp_memory::{publication::Publisher, retrieval, search_record::ChunkerSpec};

fn seal_memory(
    host: &CanonicalHost,
    thread: codex_protocol::ThreadId,
    snapshot: &vcp_models::catalog::Snapshot,
    selection: &memory_query::Selection,
) -> vcp_context::manifest::Sealed {
    use vcp_context::{
        manifest::{Kind, Part, Trust},
        selection::{assemble, Utf8ByteCeiling},
    };
    let text = "Use sourced evidence with retained dispute and inference labels.";
    let artifact = host
        .capture(thread, Channel::Evidence, text.as_bytes().to_vec())
        .unwrap();
    let operating = Part::captured_text(
        "operating".into(),
        Kind::Operating,
        Trust::Operating,
        &artifact,
        text.as_bytes(),
        true,
        0,
        "fixture operating constraint".into(),
    )
    .unwrap();
    let mut parts = vec![operating];
    for (id, kind, trust, text) in [
        (
            "objective",
            Kind::Objective,
            Trust::User,
            "Explain the retained source evidence.",
        ),
        (
            "task",
            Kind::TaskState,
            Trust::Observed,
            "The root task is running and this fixture authorizes no tools.",
        ),
    ] {
        let artifact = host
            .capture(thread, Channel::Evidence, text.as_bytes().to_vec())
            .unwrap();
        parts.push(
            Part::captured_text(
                id.into(),
                kind,
                trust,
                &artifact,
                text.as_bytes(),
                true,
                0,
                "current canonical task fixture".into(),
            )
            .unwrap(),
        );
    }
    parts.push(selection.part().unwrap().clone());
    assemble(
        parts,
        host.context_revisions(thread).unwrap(),
        vcp_models::request::envelope(
            snapshot,
            Units::new(1024),
            Units::new(512),
            Timestamp::new(1),
        )
        .unwrap(),
        serde_json::json!([]),
        vec![],
        &Utf8ByteCeiling,
        |parts, envelope, schemas| {
            vcp_models::request::encode(parts, envelope, schemas, snapshot)
                .map_err(|_| vcp_context::manifest::Error::Incompatible("memory fixture codec"))
        },
    )
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn opaque_memory_context_blocks_filesystem_edits_revocation_and_unfenced_reuse_before_send() {
    use codex_extension_api::{HostModelPurpose, HostWorkAdmission};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for race in [
            "filesystem",
            "authority",
            "retention",
            "unfenced",
            "routing_policy",
            "retrieval_limit",
            "unchanged",
        ] {
            let (temp, config, host, owner, _test, thread) =
                super::memory_publication::vector_fixture(backend).await;
            let source = super::memory_publication::retained_source(&host, &config, thread);
            let private = temp.path().join("private");
            std::fs::create_dir(&private).unwrap();
            let publisher = Arc::new(
                Publisher::new(&config.canonical_root.join("search-generations")).unwrap(),
            );
            let manager = host.publication_manager(thread, publisher.clone()).unwrap();
            let published = host
                .publish_memory(
                    thread,
                    manager,
                    memory_publication::Request {
                        vectors: memory_vectors::Request {
                            assets: temp.path().join("missing-assets"),
                            private_root: private,
                            sources: vec![source.clone()],
                            chunker: ChunkerSpec::default(),
                            cancelled: Arc::new(AtomicBool::new(false)),
                        },
                        allow_lexical_only: true,
                    },
                )
                .await
                .unwrap();
            assert!(matches!(
                published.status,
                memory_publication::Status::LexicalOnly(_)
            ));
            let (recovery, measured) = host
                .open_memory_view(thread, publisher, Arc::new(AtomicBool::new(false)))
                .await
                .unwrap();
            assert!(measured.observation_retained);
            let (snapshot, raw) = provider_snapshot();
            host.configure_provider(snapshot.clone(), raw).unwrap();
            if race == "retrieval_limit" {
                let mut routing =
                    super::routing::routing_configuration(vcp_models::routing::Profile::Low, false);
                routing.policy.retrieval_limits = Some(vcp_models::routing::RetrievalLimits {
                    results: 1,
                    tokens: Units::new(if race == "retrieval_limit" { 2 } else { 4096 }),
                    bytes: ByteCount::new(4096),
                });
                routing.policy = routing.policy.seal().unwrap();
                host.configure_routing(routing).unwrap();
            }
            let selection = host
                .query_memory_context(
                    thread,
                    Arc::new(recovery.view.unwrap()),
                    memory_query::Query {
                        request: retrieval::Request {
                            workspace: config.workspace.clone(),
                            tasks: None,
                            roots: None,
                            paths: Some(vec!["source.rs".into()]),
                            symbols: None,
                            text: "retained".into(),
                            historical: None,
                            minimum_sequence: None,
                            timeout_ms: 30_000,
                            results: 8,
                            tokens: 4096,
                            bytes: 4096,
                        },
                        sources: vec![source],
                        chunker: ChunkerSpec::default(),
                        embedding: None,
                        cancelled: Arc::new(AtomicBool::new(false)),
                    },
                )
                .await
                .unwrap();
            if race == "retrieval_limit" {
                assert!(selection.response().passages.is_empty());
                assert!(selection.response().token_upper_bound <= 2);
                assert!(selection.part().is_none());
                assert!(!host
                    .snapshot()
                    .unwrap()
                    .records
                    .values()
                    .any(|row| row.collection == Collection::Attempt));
                owner.close().await.unwrap();
                continue;
            }
            assert!(!selection.response().passages.is_empty());
            assert!(selection.resources.as_ref().unwrap().observation_retained);
            let sealed = seal_memory(&host, thread, &snapshot, &selection);
            if race == "unfenced" {
                assert!(host
                    .prepare_context(thread, sealed, serde_json::json!([]), vec![])
                    .is_err());
            } else {
                host.prepare_memory_context(
                    thread,
                    sealed,
                    serde_json::json!([]),
                    vec![],
                    selection,
                )
                .unwrap();
                if race == "routing_policy" {
                    // The explicit memory adapter prepares a fixed-provider
                    // request. Adding routed policy after preparation must
                    // invalidate its previously unconfigured source fence.
                    host.configure_routing(super::routing::routing_configuration(
                        vcp_models::routing::Profile::Low,
                        false,
                    ))
                    .unwrap();
                    use vcp_lifecycle::foundation::{
                        routing::Request,
                        routing_state::{Edit, Preview},
                    };
                    let now = Timestamp::new(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_millis() as u64,
                    );
                    let report = host
                        .routing_control(Request::Report {
                            from: None,
                            until: now,
                        })
                        .unwrap();
                    let preview: Preview = serde_json::from_value(
                        host.routing_control(Request::Preview {
                            report: report["report"]["id"].as_str().unwrap().into(),
                            selected: vec![Edit::RetrievalLimits(Some(
                                vcp_models::routing::RetrievalLimits {
                                    results: 1,
                                    tokens: Units::new(2),
                                    bytes: ByteCount::new(2),
                                },
                            ))],
                        })
                        .unwrap(),
                    )
                    .unwrap();
                    host.routing_control(Request::Apply {
                        command: CommandId::new(),
                        preview: Box::new(preview),
                    })
                    .unwrap();
                } else if race == "filesystem" {
                    // No canonical ObserveFingerprint: the send fence must
                    // detect the independent editor's actual filesystem change.
                    std::fs::write(
                        std::path::Path::new(&config.binding.root).join("source.rs"),
                        b"pub fn retained_answer() -> u32 { 666 }\n",
                    )
                    .unwrap();
                } else if race == "retention" {
                    use vcp_domain::retention_selector::{Criterion, Selector, Tree};
                    use vcp_lifecycle::foundation::history_retention::Request;
                    let preview = host
                        .history_retention(Request::Preview {
                            selector: Selector {
                                schema_version: 1,
                                tree: Tree::Match(Criterion::Workspace(config.workspace.clone())),
                            },
                            action: vcp_memory::retention::Action::Exclude,
                        })
                        .unwrap();
                    host.history_retention(Request::Apply {
                        preview: preview["id"].as_str().unwrap().into(),
                    })
                    .unwrap();
                } else if race == "authority" {
                    let state = host.snapshot().unwrap();
                    let workspace: Workspace = state
                        .record(
                            Collection::Workspace,
                            config.workspace.as_str(),
                            &config.workspace,
                        )
                        .unwrap()
                        .decode()
                        .unwrap();
                    host.command(
                        Command::SetWorkspaceTrust {
                            trust: Trust::Untrusted,
                        },
                        None,
                        workspace.revision,
                    )
                    .unwrap();
                }
                let admission = host.admit_model(
                    thread,
                    &mut serde_json::json!({"model":"gpt-5.1"}),
                    HostModelPurpose::Turn,
                );
                assert_eq!(admission.is_ok(), race == "unchanged", "{backend:?}/{race}");
                // A granted but deliberately untransported permit remains an
                // honest unknown attempt; this fixture never starts HTTP.
                drop(admission);
            }
            let state = host.snapshot().unwrap();
            assert_eq!(
                state
                    .records
                    .values()
                    .filter(|r| r.collection == Collection::Attempt)
                    .count(),
                usize::from(race == "unchanged"),
                "{backend:?}/{race}"
            );
            owner.close().await.unwrap();
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn query_embedding_cancellation_and_missing_assets_never_create_remote_attempts() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let (temp, _config, host, owner, _test, thread) =
            super::memory_publication::vector_fixture(backend).await;
        assert!(host
            .embed_memory_query(
                thread,
                temp.path().join("missing"),
                "short local query".into(),
                Arc::new(AtomicBool::new(true))
            )
            .await
            .is_err());
        let missing = host
            .embed_memory_query(
                thread,
                temp.path().join("missing"),
                "short local query".into(),
                Arc::new(AtomicBool::new(false)),
            )
            .await
            .unwrap();
        assert!(missing.embedding.is_none());
        assert!(missing.failure.is_some());
        assert!(missing.resources.unwrap().observation_retained);
        assert!(!host
            .snapshot()
            .unwrap()
            .records
            .values()
            .any(|r| r.collection == Collection::Attempt));
        owner.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn paused_inspection_preserves_canonical_state_and_does_not_create_missing_index() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let (_temp, config, host, owner, _test, _thread) =
            super::memory_publication::vector_fixture(backend).await;
        let task: Task = host
            .snapshot()
            .unwrap()
            .record(
                Collection::Task,
                config.root_task.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        host.command(
            Command::Transition {
                next: TaskState::Paused,
                reason: "inspect while paused".into(),
                verification: None,
            },
            Some(config.root_task.clone()),
            task.revision,
        )
        .unwrap();
        let before = host.snapshot().unwrap();
        let result = host
            .inspect_memory(retrieval::Request {
                workspace: config.workspace.clone(),
                tasks: None,
                roots: None,
                paths: None,
                symbols: None,
                text: "output".into(),
                historical: None,
                minimum_sequence: None,
                timeout_ms: 30_000,
                results: 8,
                tokens: 4096,
                bytes: 4096,
            })
            .await
            .unwrap();
        assert!(result.rebuild_required);
        assert!(result.generation.is_none());
        assert!(result.passages.is_empty());
        assert_eq!(host.snapshot().unwrap(), before);
        assert!(!config.canonical_root.join("search-generations").exists());
        owner.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "explicit native query inference gate: VCP_MINILM_ASSETS required"]
async fn actual_local_query_embedding_retains_measured_resources_without_provider_calls() {
    let assets = std::env::var_os("VCP_MINILM_ASSETS").expect("qualified local assets required");
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let (_temp, _config, host, owner, _test, thread) =
            super::memory_publication::vector_fixture(backend).await;
        let result = host
            .embed_memory_query(
                thread,
                assets.clone().into(),
                "Where is the retained answer?".into(),
                Arc::new(AtomicBool::new(false)),
            )
            .await
            .unwrap();
        assert!(result.embedding.is_some(), "{:?}", result.failure);
        assert!(result.failure.is_none());
        let resources = result.resources.unwrap();
        assert!(resources.observation_retained);
        assert!(resources.samples >= 4);
        assert!(resources.sampled_private_commit_peak_bytes > 0);
        eprintln!(
            "local_query_embedding_resources={}",
            serde_json::to_string(&resources).unwrap()
        );
        assert!(!host
            .snapshot()
            .unwrap()
            .records
            .values()
            .any(|r| r.collection == Collection::Attempt));
        owner.close().await.unwrap();
    }
}
