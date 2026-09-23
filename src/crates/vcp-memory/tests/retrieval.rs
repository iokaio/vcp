// SPDX-License-Identifier: Apache-2.0
//! Register only with P5-06 and its publication/vector prerequisites.
use std::{collections::BTreeSet, sync::atomic::AtomicBool};
use vcp_domain::{
    memory::{EvidenceStatus, Outcome},
    task::Objective,
    verification::Fingerprint,
    workspace::*,
    *,
};
use vcp_engine::{Engine, HostFacts};
use vcp_memory::{
    access::Access,
    lexical,
    publication::{self, Publisher},
    retrieval::*,
    search_record::{self, ChunkerSpec},
    vector,
};
use vcp_protocol::command::{Command, CommandEnvelope};
use vcp_store::{contract::*, BackendKind, Store};

#[test]
fn fusion_preserves_component_ranks_deduplicates_subchunks_and_has_stable_ties() {
    let lexical = vec![lexical::Candidate {
        id: "a".into(),
        rank: 1,
        score: 4.0,
    }];
    let vectors = vec![
        vector::Candidate {
            chunk: "b1".into(),
            source: "b".into(),
            distance: 0.1,
        },
        vector::Candidate {
            chunk: "b2".into(),
            source: "b".into(),
            distance: 0.2,
        },
    ];
    let result = fuse(&lexical, &vectors).unwrap();
    assert_eq!(
        result.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        vec!["a", "b"]
    );
    assert_eq!(result[0].fused_score, result[1].fused_score);
    assert_eq!(result[1].vector_rank, Some(1));
    let ranked = fuse(
        &[lexical::Candidate {
            id: "a".into(),
            rank: 7,
            score: 1.0,
        }],
        &[],
    )
    .unwrap();
    assert_eq!(ranked[0].lexical_rank, Some(7));
    assert_eq!(ranked[0].fused_score, 1.0 / 67.0);
    assert!(fuse(
        &[lexical::Candidate {
            id: "x".into(),
            rank: 0,
            score: f32::NAN
        }],
        &[]
    )
    .is_err());
}
fn owner(scope: &Scope) -> vcp_engine::Access {
    vcp_engine::Access {
        workspace: scope.workspace.clone(),
        session: scope.session.clone(),
        actor: ActorId::parse("owner").unwrap(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: true,
    }
}
async fn issue(engine: &mut Engine<Store>, scope: &Scope, payload: Command) {
    let command = CommandEnvelope {
        version: 1,
        id: CommandId::new(),
        workspace: scope.workspace.clone(),
        session: scope.session.clone(),
        task: if matches!(&payload, Command::Initialize { .. }) {
            None
        } else {
            Some(scope.task.clone())
        },
        caller: owner(scope).actor,
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        payload,
    };
    engine
        .handle(
            command,
            &owner(scope),
            &HostFacts::inspect(Timestamp::new(100)),
        )
        .await
        .unwrap();
}
async fn fixture(path: &std::path::Path, backend: BackendKind) -> (Store, Scope, Access) {
    let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
    let scope = Scope {
        workspace: WorkspaceId::new(),
        session: SessionId::new(),
        task: TaskId::new(),
    };
    issue(
        &mut engine,
        &scope,
        Command::Initialize {
            binding: Binding {
                host: HostId::new(),
                root: "C:/retrieval-fixture".into(),
                repository: "repo".into(),
                worktree: "main".into(),
                revision: Revision::ZERO,
            },
        },
    )
    .await;
    issue(&mut engine,&scope,Command::CreateTask{root:scope.task.clone(),parent:None,fork_origin:None,
        objective:Objective{text:serde_json::json!({"memory_preference":{"key":"style","value":"retained-preference-only ".repeat(40)}}).to_string(),constraints:vec![],acceptance:vec!["retain".into()],source:EventId::new(),steering:SteeringRevision::ZERO},
        fingerprint:Fingerprint{repository:"a".repeat(64),buffers:"b".repeat(64),environment:"c".repeat(64)},editing:false,required_checks:vec![]}).await;
    let access = Access {
        workspace: scope.workspace.clone(),
        actor: owner(&scope).actor,
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        tasks: None,
    };
    let event = engine.store().state().events.last().unwrap().clone();
    let proposal = vcp_memory::preferences::materialize(engine.store_mut(), &access, &event)
        .await
        .unwrap()
        .unwrap();
    let commit =
        vcp_memory::repository::propose(engine.store_mut(), &access, proposal, Timestamp::new(200))
            .await
            .unwrap();
    assert_eq!(commit.result.resolution.outcome, Outcome::Accepted);
    (engine.into_store(), scope, access)
}
fn request(scope: &Scope) -> Request {
    Request {
        workspace: scope.workspace.clone(),
        tasks: None,
        roots: None,
        paths: None,
        symbols: None,
        text: "style".into(),
        historical: None,
        minimum_sequence: Some(MemorySeq::new(1)),
        timeout_ms: 30_000,
        results: 8,
        tokens: 4096,
        bytes: 8192,
    }
}

#[tokio::test]
async fn recent_overlay_recalls_new_claims_with_current_scope_and_return_fences() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for bounded in [false, true] {
            let temporary = tempfile::tempdir().unwrap();
            let (mut store, scope, access) =
                fixture(&temporary.path().join("canonical"), backend).await;
            let chunker = ChunkerSpec {
                max_bytes: if bounded { 64 } else { 2048 },
                ..ChunkerSpec::default()
            };
            let inventory = search_record::inventory(
                &store,
                &access,
                &[],
                &chunker,
                search_record::Limits::default(),
            )
            .unwrap();
            let publisher = Publisher::new(&temporary.path().join("derived")).unwrap();
            let prepared = publisher
                .prepare(
                    publication::capture(&store, &access, &scope, inventory).unwrap(),
                    None,
                    &AtomicBool::new(false),
                    &|_| {},
                )
                .unwrap();
            publisher
                .publish(&mut store, &access, &prepared, Timestamp::new(300), &|_| {})
                .await
                .unwrap();
            let view = publisher.recover(&store, &access).unwrap().view.unwrap();
            let recent_scope = Scope {
                task: TaskId::new(),
                ..scope.clone()
            };
            let mut engine = Engine::new(store).unwrap();
            let value = if bounded {
                "heliotrope ".repeat(744)
            } else {
                "heliotrope".into()
            };
            issue(&mut engine, &recent_scope, Command::CreateTask {
            root: recent_scope.task.clone(), parent: None, fork_origin: None,
            objective: Objective {
                text: serde_json::json!({"memory_preference":{"key":"recentColor","value":value}}).to_string(),
                constraints: vec![], acceptance: vec!["retain".into()], source: EventId::new(), steering: SteeringRevision::ZERO,
            },
            fingerprint: Fingerprint {repository:"a".repeat(64), buffers:"b".repeat(64), environment:"c".repeat(64)},
            editing:false, required_checks:vec![],
        }).await;
            let event = engine.store().state().events.last().unwrap().clone();
            let proposal =
                vcp_memory::preferences::materialize(engine.store_mut(), &access, &event)
                    .await
                    .unwrap()
                    .unwrap();
            vcp_memory::repository::propose(
                engine.store_mut(),
                &access,
                proposal,
                Timestamp::new(400),
            )
            .await
            .unwrap();
            let store = engine.into_store();
            let mut query_request = request(&scope);
            query_request.text = "heliotrope".into();
            query_request.minimum_sequence = Some(MemorySeq::new(2));
            let result = query(
                &store,
                &access,
                Some(&view),
                &query_request,
                &[],
                &chunker,
                None,
                &|| false,
            )
            .unwrap();
            assert!(
                !result.passages.is_empty(),
                "acknowledged recent evidence must be recalled during index lag"
            );
            assert!(result.passages.iter().all(|p| p.text.contains("heliotrope")
                && p.rank.overlay_rank.is_some()
                && p.rank.lexical_rank.is_none()
                && p.rank.vector_rank.is_none()));
            assert!(result.degraded.contains(&"recent_overlay_lexical_only"));
            assert_eq!(result.degraded.contains(&"recent_overlay_bounded"), bounded);
            assert!(result.degraded.contains(&"minimum_sequence_unsatisfied"));
            assert_eq!(result.truncated, bounded);
            let mut one = query_request.clone();
            one.text = String::new();
            one.results = 1;
            let limited = query(
                &store,
                &access,
                Some(&view),
                &one,
                &[],
                &chunker,
                None,
                &|| false,
            )
            .unwrap();
            assert_eq!(limited.passages.len(), 1);
            assert!(
                limited.truncated,
                "published plus recent claims exceed the one-result ceiling"
            );
            revalidate_fence(&store, &access, result.fence.as_ref().unwrap()).unwrap();
            let narrowed = Access {
                tasks: Some(BTreeSet::from([scope.task.clone()])),
                workspace: access.workspace.clone(),
                actor: access.actor.clone(),
                authority: access.authority,
                read: access.read,
                write: access.write,
            };
            let denied = query(
                &store,
                &narrowed,
                Some(&view),
                &query_request,
                &[],
                &chunker,
                None,
                &|| false,
            )
            .unwrap();
            assert!(denied.passages.is_empty());
            let selection = search(
                capture(&store, &access, &query_request, &[], &chunker, &|| false).unwrap(),
                Some(&view),
                None,
                &|| false,
            )
            .unwrap();
            assert!(finish(&store, &narrowed, selection, &|| false)
                .unwrap()
                .passages
                .is_empty());
            query_request.paths = Some(vec!["src/unrelated.rs".into()]);
            assert!(query(
                &store,
                &access,
                Some(&view),
                &query_request,
                &[],
                &chunker,
                None,
                &|| false
            )
            .unwrap()
            .passages
            .is_empty());
        }
    }
}

#[tokio::test]
async fn pinned_stale_view_cannot_return_denied_or_pruned_text_and_fence_rechecks_sources() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temporary = tempfile::tempdir().unwrap();
        let (mut store, scope, access) =
            fixture(&temporary.path().join("canonical"), backend).await;
        let chunker = ChunkerSpec::default();
        let inventory = search_record::inventory(
            &store,
            &access,
            &[],
            &chunker,
            search_record::Limits::default(),
        )
        .unwrap();
        let publisher = Publisher::new(&temporary.path().join("derived")).unwrap();
        let prepared = publisher
            .prepare(
                publication::capture(&store, &access, &scope, inventory).unwrap(),
                None,
                &AtomicBool::new(false),
                &|_| {},
            )
            .unwrap();
        publisher
            .publish(&mut store, &access, &prepared, Timestamp::new(300), &|_| {})
            .await
            .unwrap();
        let recovery = publisher.recover(&store, &access).unwrap();
        let view = recovery.view.as_ref().unwrap();
        let query_request = request(&scope);
        {
            let narrowed = Access {
                workspace: access.workspace.clone(),
                actor: access.actor.clone(),
                authority: access.authority,
                read: true,
                write: false,
                tasks: Some(BTreeSet::from([scope.task.clone()])),
            };
            let snapshot = store.snapshot().unwrap();
            assert!(
                publisher.recover_snapshot(&snapshot, &narrowed).is_err(),
                "scoped query does not gain a broad component View"
            );
            let broad = capture(&store, &access, &query_request, &[], &chunker, &|| false).unwrap();
            assert!(
                publisher
                    .search_captured_with_check(&narrowed, broad, &|| Ok(()))
                    .is_err(),
                "broad capture cannot cross a narrow reader"
            );
            let scoped =
                capture(&store, &narrowed, &query_request, &[], &chunker, &|| false).unwrap();
            let selected = publisher
                .search_captured_with_check(&narrowed, scoped, &|| Ok(()))
                .unwrap();
            let page = finish(&store, &narrowed, selected, &|| false).unwrap();
            assert_eq!(page.passages.len(), 1);
            assert_eq!(page.passages[0].scope.task, scope.task);
            let mut foreign_actor = narrowed;
            foreign_actor.actor = ActorId::parse("foreign-actor").unwrap();
            let scoped =
                capture(&store, &access, &query_request, &[], &chunker, &|| false).unwrap();
            assert!(publisher
                .search_captured_with_check(&foreign_actor, scoped, &|| Ok(()))
                .is_err());
        }
        let checkpoints = std::cell::Cell::new(0usize);
        let interrupt = || {
            checkpoints.set(checkpoints.get() + 1);
            checkpoints.get() >= 6
        };
        assert!(matches!(
            capture(&store, &access, &query_request, &[], &chunker, &interrupt),
            Err(vcp_memory::Error::Conflict("retrieval cancelled"))
        ));
        assert_eq!(checkpoints.get(), 6);
        let captured = capture(&store, &access, &query_request, &[], &chunker, &|| false).unwrap();
        let selection = search(captured, Some(view), None, &|| false).unwrap();
        checkpoints.set(0);
        assert!(
            matches!(
                finish(&store, &access, selection, &interrupt),
                Err(vcp_memory::Error::Conflict("retrieval cancelled"))
            ),
            "interrupted rematerialization must not return a successful fence"
        );
        assert_eq!(checkpoints.get(), 6);
        let result = query(
            &store,
            &access,
            Some(view),
            &query_request,
            &[],
            &chunker,
            None,
            &|| false,
        )
        .unwrap();
        assert_eq!(result.passages.len(), 1);
        assert!(
            !result.truncated,
            "lexical-only or generation diagnostics do not invent budget truncation"
        );
        let mut exact_limit = query_request.clone();
        exact_limit.results = 1;
        let exact = query(
            &store,
            &access,
            Some(view),
            &exact_limit,
            &[],
            &chunker,
            None,
            &|| false,
        )
        .unwrap();
        assert_eq!(exact.passages.len(), 1);
        assert!(
            !exact.truncated,
            "exactly filling a result limit is not itself truncation"
        );
        let mut no_room = query_request.clone();
        no_room.tokens = 2;
        let omitted = query(
            &store,
            &access,
            Some(view),
            &no_room,
            &[],
            &chunker,
            None,
            &|| false,
        )
        .unwrap();
        assert!(omitted.passages.is_empty() && omitted.truncated);
        assert!(result.passages[0].text.contains("retained-preference-only"));
        assert_eq!(result.passages[0].evidence_status, EvidenceStatus::Observed);
        assert!(result.degraded.contains(&"minimum_sequence_unsatisfied"));
        let fence = result.fence.as_ref().unwrap();
        revalidate_fence(&store, &access, fence).unwrap();
        let captured = capture(&store, &access, &query_request, &[], &chunker, &|| false).unwrap();
        assert!(store.try_snapshot_cleanup_guard().unwrap().is_none());
        let off_owner = std::thread::scope(|threads| {
            threads
                .spawn(|| search(captured, Some(view), None, &|| false))
                .join()
                .unwrap()
                .unwrap()
        });
        assert!(store.try_snapshot_cleanup_guard().unwrap().is_none());
        let finished = finish(&store, &access, off_owner, &|| false).unwrap();
        assert_eq!(finished.passages[0].record_id, result.passages[0].record_id);
        assert!(store.try_snapshot_cleanup_guard().unwrap().is_some());
        let captured_broad =
            capture(&store, &access, &query_request, &[], &chunker, &|| false).unwrap();
        let ranked_broad = search(captured_broad, Some(view), None, &|| false).unwrap();
        let mut narrow = access;
        narrow.tasks = Some(BTreeSet::new());
        let narrowed_after_search = finish(&store, &narrow, ranked_broad, &|| false).unwrap();
        assert!(narrowed_after_search.passages.is_empty());
        assert!(!serde_json::to_string(&narrowed_after_search)
            .unwrap()
            .contains("retained-preference-only"));
        let captured_narrow =
            capture(&store, &narrow, &query_request, &[], &chunker, &|| false).unwrap();
        let ranked_narrow = search(captured_narrow, Some(view), None, &|| false).unwrap();
        let denied = query(
            &store,
            &narrow,
            Some(view),
            &query_request,
            &[],
            &chunker,
            None,
            &|| false,
        )
        .unwrap();
        assert!(denied.passages.is_empty());
        assert!(!serde_json::to_string(&denied)
            .unwrap()
            .contains("retained-preference-only"));
        assert!(revalidate_fence(&store, &narrow, fence).is_err());
        narrow.tasks = None;
        assert!(
            finish(&store, &narrow, ranked_narrow, &|| false)
                .unwrap()
                .passages
                .is_empty(),
            "finish cannot expand captured authorization"
        );
        let mut trimmed_request = query_request.clone();
        trimmed_request.tokens = serde_json::to_vec(&result.passages[0]).unwrap().len() - 100;
        let trimmed = query(
            &store,
            &narrow,
            Some(view),
            &trimmed_request,
            &[],
            &chunker,
            None,
            &|| false,
        )
        .unwrap();
        assert_eq!(trimmed.passages.len(), 1);
        assert!(trimmed.passages[0].trimmed);
        assert!(trimmed.truncated);
        assert_eq!(trimmed.passages[0].status, result.passages[0].status);
        assert_eq!(trimmed.passages[0].source, result.passages[0].source);
        assert!(trimmed.token_upper_bound <= trimmed_request.tokens);
        assert_eq!(
            trimmed.token_upper_bound,
            serde_json::to_vec(&trimmed.passages).unwrap().len()
        );
        assert_eq!(
            trimmed.passages[0].span.end.get() - trimmed.passages[0].span.start.get(),
            trimmed.passages[0].text.len() as u64
        );
        let mut historical = query_request.clone();
        historical.historical = Some(MemorySeq::new(1));
        let history = query(
            &store,
            &narrow,
            Some(view),
            &historical,
            &[],
            &chunker,
            None,
            &|| false,
        )
        .unwrap();
        assert!(history.rebuild_required && history.passages.is_empty());
        let artifact = result.passages[0].evidence[0].clone();
        let attached = store
            .state()
            .events
            .iter()
            .find(|event| event.event.artifacts.contains(&artifact))
            .unwrap()
            .clone();
        let mut workspace: Workspace = store
            .state()
            .record(
                Collection::Workspace,
                scope.workspace.as_str(),
                &scope.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let previous = workspace.revision;
        workspace.revision = workspace.revision.next().unwrap();
        workspace.deletion = workspace.deletion.next().unwrap();
        let mask = vcp_audit::history::RetentionMask {
            schema_version: 1,
            workspace: scope.workspace.clone(),
            session: scope.session.clone(),
            first: attached.sequence,
            last: attached.sequence,
            artifacts: vec![artifact],
            deletion: workspace.deletion,
            reason: "fixture prune".into(),
        };
        let before_prune =
            capture(&store, &narrow, &query_request, &[], &chunker, &|| false).unwrap();
        let before_prune = search(before_prune, Some(view), None, &|| false).unwrap();
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![
                    Mutation::Put {
                        expected: Some(previous),
                        record: Record::typed(
                            Collection::Workspace,
                            workspace.id.to_string(),
                            workspace.id.clone(),
                            workspace.revision,
                            &workspace,
                        )
                        .unwrap(),
                    },
                    Mutation::Put {
                        expected: None,
                        record: Record::typed(
                            Collection::Tombstone,
                            "retrieval-mask",
                            workspace.id.clone(),
                            Revision::ZERO,
                            &mask,
                        )
                        .unwrap(),
                    },
                ],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        assert!(
            matches!(
                finish(&store, &narrow, before_prune, &|| false),
                Err(vcp_memory::Error::Access)
            ),
            "epoch change while native search runs must reject old selection"
        );
        assert!(revalidate_fence(&store, &narrow, fence).is_err());
        let mut epoch_refreshed = fence.clone();
        epoch_refreshed.deletion = workspace.deletion;
        assert!(
            revalidate_fence(&store, &narrow, &epoch_refreshed).is_err(),
            "current epochs cannot restore a pruned source identity"
        );
        let pruned = query(
            &store,
            &narrow,
            Some(view),
            &query_request,
            &[],
            &chunker,
            None,
            &|| false,
        )
        .unwrap();
        assert!(pruned.passages.is_empty());
        assert!(!serde_json::to_string(&pruned)
            .unwrap()
            .contains("retained-preference-only"));
    }
}
