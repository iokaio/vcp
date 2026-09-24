// SPDX-License-Identifier: Apache-2.0
use vcp_audit::{history::*, projection, Error};
use vcp_domain::{
    accounting::*, artifact::*, effect::*, ids::*, revision::*, task::*, verification::*,
    workspace::*,
};
use vcp_engine::{Engine, HostFacts};
use vcp_protocol::{command::*, event::*};
use vcp_store::{artifact::ArtifactWriter, contract::*, BackendKind, Store};
fn engine_access() -> vcp_engine::Access {
    vcp_engine::Access {
        actor: ActorId::parse("owner").unwrap(),
        workspace: WorkspaceId::parse("workspace").unwrap(),
        session: SessionId::parse("session").unwrap(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: true,
    }
}

#[tokio::test]
async fn browser_pages_survive_append_but_recheck_current_access_and_retention() {
    use vcp_audit::history_query::{self, Query};
    use vcp_domain::retention_selector::{Criterion, Selector, Tree};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temporary = tempfile::tempdir().unwrap();
        let mut fixture = fixture(temporary.path(), backend).await;
        let initial: std::collections::BTreeSet<_> = fixture
            .engine
            .store()
            .state()
            .events
            .iter()
            .map(|e| e.event.id.clone())
            .collect();
        let mut query = Query {
            selector: Selector {
                schema_version: 1,
                tree: Tree::Match(Criterion::Workspace(access().workspace)),
            },
            text: None,
            limit: 2,
            cursor: None,
            artifact: None,
            expand_compacted: false,
        };
        let first =
            history_query::query(fixture.engine.store().state(), &access(), &query).unwrap();
        query.cursor = first.next_cursor.clone();
        let mut selected: std::collections::BTreeSet<_> = first
            .rows
            .iter()
            .map(|r| r.event.event.id.clone())
            .collect();
        let id = TaskId::new();
        issue(&mut fixture.engine, create(&id, None, None), Some(id), 0).await;
        while query.cursor.is_some() {
            let page =
                history_query::query(fixture.engine.store().state(), &access(), &query).unwrap();
            assert!(page.newer_events > 0);
            for row in page.rows {
                assert!(selected.insert(row.event.event.id));
            }
            query.cursor = page.next_cursor;
        }
        assert_eq!(selected, initial);
        query.cursor = first.next_cursor;
        let mut restricted = access();
        restricted.tasks = Some([fixture.root.clone()].into_iter().collect());
        assert!(matches!(
            history_query::query(fixture.engine.store().state(), &restricted, &query),
            Err(Error::Restart(_))
        ));
        query.cursor = None;
        query.artifact = Some(fixture.request.spec.id.clone());
        let linked =
            history_query::query(fixture.engine.store().state(), &access(), &query).unwrap();
        assert!(!linked.rows.is_empty());
        assert!(linked
            .rows
            .iter()
            .all(|r| r.artifacts.contains(&fixture.request.spec.id)));
        let cursor = history_query::query(
            fixture.engine.store().state(),
            &access(),
            &Query {
                artifact: None,
                limit: 1,
                ..query.clone()
            },
        )
        .unwrap()
        .next_cursor
        .unwrap();
        let state = fixture.engine.store().state();
        let mut workspace: Workspace = state
            .record(
                Collection::Workspace,
                access().workspace.as_str(),
                &access().workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let prior = workspace.revision;
        workspace.revision = prior.next().unwrap();
        workspace.deletion = workspace.deletion.next().unwrap();
        let tx = Transaction {
            id: TransactionId::new(),
            expected_watermark: state.watermark,
            mutations: vec![Mutation::Put {
                expected: Some(prior),
                record: Record::typed(
                    Collection::Workspace,
                    workspace.id.to_string(),
                    workspace.id.clone(),
                    workspace.revision,
                    &workspace,
                )
                .unwrap(),
            }],
            events: vec![],
            command: None,
        };
        fixture.engine.store_mut().transact(tx).await.unwrap();
        query.artifact = None;
        query.limit = 1;
        query.cursor = Some(cursor);
        assert!(matches!(
            history_query::query(fixture.engine.store().state(), &access(), &query),
            Err(Error::Restart(_))
        ));
    }
}

#[tokio::test]
async fn browser_compaction_is_presentation_and_exclusion_keeps_raw_history() {
    use vcp_audit::history_query::{self, Query};
    use vcp_domain::retention_selector::{Criterion, Selector, Tree};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temporary = tempfile::tempdir().unwrap();
        let fixture = fixture(temporary.path(), backend).await;
        let mut state = fixture.engine.store().state().clone();
        let event = state
            .events
            .iter()
            .find(|e| {
                e.event.kind == EventKind::TaskCreated
                    && e.event.task.as_ref() == Some(&fixture.root)
            })
            .unwrap()
            .clone();
        let target = serde_json::json!({"kind":"event","id":event.event.id});
        let id = format!(
            "recall-{}",
            vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&target).unwrap())
        );
        let record=Record::typed(Collection::Projection,id.clone(),access().workspace,Revision::ZERO,&serde_json::json!({"schema_version":1,"workspace":access().workspace,"revision":Revision::ZERO,"action":"compact","deletion":DeletionEpoch::ZERO,"document_type":"vcp_retention_decision_v1","target":target,"recall_excluded":true,"compacted":true,"purged":false})).unwrap();
        let key = key(Collection::Projection, &id);
        state.records.insert(key.clone(), record);
        let mut request = Query {
            selector: Selector {
                schema_version: 1,
                tree: Tree::All(vec![
                    Tree::Match(Criterion::Task(fixture.root.clone())),
                    Tree::Match(Criterion::Event("task_created".into())),
                ]),
            },
            text: None,
            limit: 4,
            cursor: None,
            artifact: None,
            expand_compacted: false,
        };
        let page = history_query::query(&state, &access(), &request).unwrap();
        assert_eq!(page.rows.len(), 1);
        assert!(
            page.rows[0].compacted
                && page.rows[0].recall_excluded
                && page.rows[0].content_truncated
        );
        request.expand_compacted = true;
        let page = history_query::query(&state, &access(), &request).unwrap();
        assert_eq!(page.rows[0].event.event.data, event.event.data);
        assert!(!page.rows[0].content_truncated);
        request.text = Some("Preserve observed synthetic work".into());
        assert_eq!(
            history_query::query(&state, &access(), &request)
                .unwrap()
                .rows
                .len(),
            1
        );
        state.records.get_mut(&key).unwrap().value["purged"] = serde_json::json!(true);
        assert!(history_query::query(&state, &access(), &request)
            .unwrap()
            .rows
            .is_empty());
    }
}
fn access() -> Access {
    Access {
        workspace: engine_access().workspace,
        authority: AuthorityRevision::ZERO,
        read: true,
        tasks: None,
    }
}

#[tokio::test]
async fn session_browser_bounds_large_artifact_lists_before_projection_and_scopes_every_window() {
    use vcp_audit::history_query::{self, Query};
    use vcp_domain::retention_selector::{Criterion, Selector, Tree};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let directory = tempfile::tempdir().unwrap();
        let mut fixture = fixture(directory.path(), backend).await;
        let source = engine_access();
        let event = EventInput {
            id: EventId::new(),
            workspace: source.workspace.clone(),
            session: source.session.clone(),
            task: None,
            actor: source.actor.clone(),
            correlation: CommandId::new(),
            causation: None,
            timestamp: Timestamp::new(100),
            kind: EventKind::Diagnostic,
            artifacts: vec![fixture.request.spec.id.clone(); 256],
            data: serde_json::json!({"large_event_marker":"retained"}),
            metadata: None,
        };
        let event_id = event.id.clone();
        let before = fixture.engine.store().state().watermark;
        fixture
            .engine
            .store_mut()
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: before,
                mutations: vec![],
                events: vec![event],
                command: None,
            })
            .await
            .unwrap();
        let mut request = Query {
            selector: Selector {
                schema_version: 1,
                tree: Tree::Match(Criterion::Workspace(source.workspace.clone())),
            },
            text: Some("large_event_marker".into()),
            limit: 1,
            cursor: None,
            artifact: None,
            expand_compacted: false,
        };
        let page = history_query::query_session(
            fixture.engine.store().state(),
            &access(),
            &request,
            &source.session,
        )
        .unwrap();
        assert_eq!(page.rows.len(), 1);
        assert_eq!(page.rows[0].event.event.id, event_id);
        assert!(page.rows[0].event.event.task.is_none());
        assert!(page.rows[0].content_truncated);
        assert_eq!(page.rows[0].artifact_links.len(), 128);
        let cli =
            history_query::query(fixture.engine.store().state(), &access(), &request).unwrap();
        assert_eq!(
            cli.rows[0].artifact_links.len(),
            256,
            "existing CLI query is unchanged"
        );
        request.text = None;
        let first = history_query::query_session(
            fixture.engine.store().state(),
            &access(),
            &request,
            &source.session,
        )
        .unwrap();
        request.cursor = first.next_cursor;
        let foreign = SessionId::new();
        assert!(matches!(
            history_query::query_session(
                fixture.engine.store().state(),
                &access(),
                &request,
                &foreign
            ),
            Err(Error::Restart(_))
        ));
        let mut state = fixture.engine.store().state().clone();
        let mut foreign_event = state.events.last().unwrap().clone();
        foreign_event.event.id = EventId::new();
        foreign_event.event.session = foreign.clone();
        foreign_event.event.data = serde_json::json!({"foreign_secret":"never match"});
        state.events.push(foreign_event);
        let page =
            history_query::query_session(&state, &access(), &request, &source.session).unwrap();
        assert_eq!(page.newer_events, 0);
        request.cursor = None;
        request.text = Some("foreign_secret".into());
        let page =
            history_query::query_session(&state, &access(), &request, &source.session).unwrap();
        assert!(page.rows.is_empty());
        assert!(page.gaps.is_empty());
        // A taskless event never lends its session authority to a foreign
        // artifact descriptor, even when that descriptor shares a workspace.
        let mut foreign_artifact = fixture.engine.store().state().clone();
        foreign_artifact
            .records
            .get_mut(&key(Collection::Artifact, fixture.request.spec.id.as_str()))
            .unwrap()
            .value["spec"]["scope"]["session"] = serde_json::json!(foreign);
        request.text = Some("large_event_marker".into());
        let page =
            history_query::query_session(&foreign_artifact, &access(), &request, &source.session)
                .unwrap();
        assert_eq!(page.rows.len(), 1);
        assert!(page.rows[0].artifact_links.is_empty());
        request.text = None;
        request.selector.tree = Tree::Match(Criterion::Task(fixture.root.clone()));
        let first =
            history_query::query_session(&state, &access(), &request, &source.session).unwrap();
        assert!(!first.rows.is_empty());
        state
            .records
            .get_mut(&key(Collection::Task, fixture.root.as_str()))
            .unwrap()
            .value["redaction"] =
            serde_json::json!({"deletion":"1","original_digest":"a".repeat(64)});
        let hidden =
            history_query::query_session(&state, &access(), &request, &source.session).unwrap();
        assert!(hidden.rows.is_empty());
        assert!(hidden.gaps.is_empty());
        request.cursor = first.next_cursor;
        assert!(request.cursor.is_some());
        assert!(matches!(
            history_query::query_session(&state, &access(), &request, &source.session),
            Err(Error::Restart(_))
        ));
    }
}
fn scope(id: &TaskId) -> Scope {
    Scope {
        workspace: engine_access().workspace,
        session: engine_access().session,
        task: id.clone(),
    }
}
fn host() -> HostFacts {
    HostFacts {
        now: Timestamp::new(1000),
        policy: PolicyRevision::ZERO,
        resume: None,
        may_execute: true,
    }
}
fn command(
    engine: &Engine<Store>,
    payload: Command,
    task: Option<TaskId>,
    expected: Revision,
) -> CommandEnvelope {
    let access = engine_access();
    CommandEnvelope {
        version: 1,
        id: CommandId::new(),
        workspace: access.workspace,
        session: access.session,
        task,
        caller: access.actor,
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected,
        steering: SteeringRevision::ZERO,
        payload,
    }
}
fn create(id: &TaskId, parent: Option<TaskId>, fork: Option<TaskId>) -> Command {
    Command::CreateTask {
        root: parent.clone().unwrap_or(id.clone()),
        parent,
        fork_origin: fork,
        objective: Objective {
            text: "Preserve observed synthetic work".into(),
            constraints: vec![],
            acceptance: vec!["inspect history".into()],
            source: EventId::new(),
            steering: SteeringRevision::ZERO,
        },
        fingerprint: Fingerprint {
            repository: "a".repeat(64),
            buffers: "b".repeat(64),
            environment: "c".repeat(64),
        },
        editing: false,
        required_checks: vec![],
    }
}
async fn issue(engine: &mut Engine<Store>, payload: Command, id: Option<TaskId>, revision: u64) {
    let command = command(engine, payload, id, Revision::new(revision));
    engine
        .handle(command, &engine_access(), &host())
        .await
        .unwrap();
}
async fn capture(engine: &mut Engine<Store>, id: &TaskId, bytes: &[u8]) -> ArtifactDescriptor {
    let spec = ArtifactSpec {
        id: ArtifactId::new(),
        scope: scope(id),
        media_type: "application/json".into(),
        schema: "fixture/1".into(),
        source: "src/fixture.rs".into(),
        channel: Channel::Evidence,
        retention: "history".into(),
        omissions: vec![],
    };
    let mut writer = engine.store().spool().create(spec).unwrap();
    for chunk in bytes.chunks(vcp_store::artifact::CHUNK_BYTES) {
        writer.write_chunk(chunk).unwrap();
    }
    let captured = writer.finalize().unwrap();
    drop(writer);
    issue(
        engine,
        Command::AttachArtifact {
            descriptor: captured.clone(),
        },
        Some(id.clone()),
        0,
    )
    .await;
    captured
}
struct Fixture {
    engine: Engine<Store>,
    root: TaskId,
    child: TaskId,
    fork: TaskId,
    effect: ToolRunId,
    attempt: Attempt,
    request: ArtifactDescriptor,
}
async fn fixture(directory: &std::path::Path, kind: BackendKind) -> Fixture {
    let mut engine = Engine::new(Store::open(directory, kind, &[]).await.unwrap()).unwrap();
    issue(
        &mut engine,
        Command::Initialize {
            binding: Binding {
                host: HostId::new(),
                root: "C:/synthetic".into(),
                repository: "fixture".into(),
                worktree: "main".into(),
                revision: Revision::ZERO,
            },
        },
        None,
        0,
    )
    .await;
    let root = TaskId::new();
    issue(
        &mut engine,
        create(&root, None, None),
        Some(root.clone()),
        0,
    )
    .await;
    issue(
        &mut engine,
        Command::Transition {
            next: TaskState::Running,
            reason: "start fixture".into(),
            verification: None,
        },
        Some(root.clone()),
        0,
    )
    .await;
    let child = TaskId::new();
    issue(
        &mut engine,
        create(&child, Some(root.clone()), None),
        Some(child.clone()),
        0,
    )
    .await;
    issue(
        &mut engine,
        Command::Transition {
            next: TaskState::Running,
            reason: "start child".into(),
            verification: None,
        },
        Some(child.clone()),
        0,
    )
    .await;
    issue(
        &mut engine,
        Command::Transition {
            next: TaskState::Paused,
            reason: "child held".into(),
            verification: None,
        },
        Some(child.clone()),
        1,
    )
    .await;
    let fork = TaskId::new();
    issue(
        &mut engine,
        create(&fork, None, Some(root.clone())),
        Some(fork.clone()),
        0,
    )
    .await;
    let effect = ToolRunId::new();
    issue(
        &mut engine,
        Command::ProposeEffect {
            id: effect.clone(),
            operation_digest: "d".repeat(64),
        },
        Some(root.clone()),
        1,
    )
    .await;
    let execution = ExecutionId::new();
    for (revision, next) in [
        EffectState::Validated,
        EffectState::Authorized,
        EffectState::DispatchRecorded,
        EffectState::Running,
        EffectState::OutcomeUnknown,
    ]
    .into_iter()
    .enumerate()
    {
        issue(
            &mut engine,
            Command::AdvanceEffect {
                id: effect.clone(),
                next,
                reason: "synthetic process observation".into(),
                execution: if revision >= 2 {
                    Some(execution.clone())
                } else {
                    None
                },
                exit_code: None,
                observed_changes: vec![],
            },
            Some(root.clone()),
            revision as u64,
        )
        .await;
    }
    let currency: Currency = "USD".to_owned().try_into().unwrap();
    let actor = vcp_budget::Actor {
        id: engine_access().actor,
        now: host().now,
    };
    vcp_budget::initialize(
        engine.store_mut(),
        scope(&root),
        Money {
            currency: currency.clone(),
            micros: Micros::new(1000),
        },
        Micros::new(10),
        None,
        &actor,
    )
    .await
    .unwrap();
    let request = capture(&mut engine, &root, b"{\"request\":\"synthetic\"}").await;
    let price = PriceSnapshot {
        id: "a".repeat(64),
        provider: "offline-provider".into(),
        model: "offline-model".into(),
        currency,
        capability: "b".repeat(64),
        valid_until: Timestamp::new(u64::MAX),
        rates: [
            ChargeCategory::Input,
            ChargeCategory::Output,
            ChargeCategory::CacheRead,
            ChargeCategory::CacheWrite,
            ChargeCategory::Request,
            ChargeCategory::ProviderTool,
        ]
        .into_iter()
        .map(|kind| {
            (
                kind,
                Rate {
                    micros: Micros::new(if kind == ChargeCategory::Request {
                        20
                    } else {
                        0
                    }),
                    per_units: Units::new(1),
                },
            )
        })
        .collect(),
    };
    let quote = vcp_budget::arithmetic::quote(
        price,
        Usage {
            requests: Units::new(1),
            ..Default::default()
        },
        host().now,
    )
    .unwrap();
    let ledger = vcp_budget::ledger(engine.store().state(), &scope(&root)).unwrap();
    let admission = vcp_budget::Admission {
        transaction: TransactionId::new(),
        attempt: AttemptId::new(),
        reservation: ReservationId::new(),
        scope: scope(&root),
        agent: AgentId::new(),
        role: RequestRole::Main,
        request: request.spec.id.clone(),
        request_digest: request.sha256.clone(),
        quote,
        previous: None,
        expected_ledger: ledger.revision,
        policy: ledger.policy,
        steering: SteeringRevision::ZERO,
        draw_protected: false,
        now: host().now,
    };
    let attempt = vcp_budget::reserve(engine.store_mut(), admission, &actor)
        .await
        .unwrap();
    vcp_budget::submit(
        engine.store_mut(),
        &attempt.id,
        &attempt.scope,
        Revision::ZERO,
        &actor,
    )
    .await
    .unwrap();
    vcp_budget::hold_uncertain(
        engine.store_mut(),
        &attempt.id,
        &attempt.scope,
        &actor,
        "provider stream lost",
    )
    .await
    .unwrap();
    issue(
        &mut engine,
        Command::Transition {
            next: TaskState::Paused,
            reason: "owning CLI closed".into(),
            verification: None,
        },
        Some(root.clone()),
        1,
    )
    .await;
    Fixture {
        engine,
        root,
        child,
        fork,
        effect,
        attempt,
        request,
    }
}
async fn late_charge(fixture: &mut Fixture) {
    let raw = capture(
        &mut fixture.engine,
        &fixture.root,
        b"{\"charged_micros\":30}",
    )
    .await;
    let observation = UsageObservation {
        id: ObservationId::new(),
        scope: scope(&fixture.root),
        attempt: fixture.attempt.id.clone(),
        provider_request: "synthetic-provider-request".into(),
        mode: UsageMode::Cumulative {
            version: Units::new(1),
        },
        amount: Money {
            currency: "USD".to_owned().try_into().unwrap(),
            micros: Micros::new(30),
        },
        final_usage: true,
        raw: raw.spec.id,
        correction: None,
    };
    vcp_budget::observe(
        fixture.engine.store_mut(),
        observation,
        &vcp_budget::Actor {
            id: engine_access().actor,
            now: Timestamp::new(2000),
        },
    )
    .await
    .unwrap();
}
#[tokio::test]
async fn live_fold_duplicate_delivery_and_version_activation_preserve_unknowns() {
    for kind in [BackendKind::Sqlite, BackendKind::Files] {
        let temporary = tempfile::tempdir().unwrap();
        let mut fixture = fixture(temporary.path(), kind).await;
        let workspace = access().workspace;
        let initial = projection::publish(fixture.engine.store_mut(), &workspace, 1)
            .await
            .unwrap();
        assert_eq!(initial.ledgers[&fixture.root].unresolved.get(), 20);
        assert_eq!(
            initial.effects[&fixture.effect].state,
            EffectState::OutcomeUnknown
        );
        assert_eq!(initial.tasks[&fixture.child].state, TaskState::Paused);
        let mut duplicate = initial.clone();
        assert!(!duplicate
            .apply(fixture.engine.store().state().events.last().unwrap())
            .unwrap());
        assert_eq!(duplicate, initial);
        late_charge(&mut fixture).await;
        let live = projection::publish(fixture.engine.store_mut(), &workspace, 1)
            .await
            .unwrap();
        assert_eq!(live.ledgers[&fixture.root].settled.get(), 30);
        assert_eq!(live.ledgers[&fixture.root].unresolved.get(), 0);
        assert_eq!(
            live.tasks[&fixture.fork].fork_origin,
            Some(fixture.root.clone())
        );
        let rebuilt = projection::rebuild(
            fixture.engine.store().state(),
            &workspace,
            1,
            live.watermark,
        )
        .unwrap();
        assert_eq!(live, rebuilt);
        let new = projection::publish(fixture.engine.store_mut(), &workspace, 2)
            .await
            .unwrap();
        let old = projection::rebuild(fixture.engine.store().state(), &workspace, 1, new.watermark)
            .unwrap();
        assert_eq!(
            new.semantic_digest().unwrap(),
            old.semantic_digest().unwrap()
        );
        assert!(!new.event_kinds.is_empty());
        let state = fixture.engine.store().state().clone();
        assert!(
            projection::publish(fixture.engine.store_mut(), &workspace, 3)
                .await
                .is_err()
        );
        assert_eq!(&state, fixture.engine.store().state());
    }
}
#[tokio::test]
async fn filtered_history_uses_one_snapshot_and_current_artifact_authority() {
    let temporary = tempfile::tempdir().unwrap();
    let mut fixture = fixture(temporary.path(), BackendKind::Sqlite).await;
    late_charge(&mut fixture).await;
    let mut history = History::default();
    let filter = Filter {
        task: Some(fixture.root.clone()),
        agent: Some(fixture.attempt.agent.clone()),
        provider: Some("offline-provider".into()),
        model: Some("offline-model".into()),
        kind: Some(EventKind::UsageReconciled),
        from_inclusive: Some(Timestamp::new(1500)),
        to_exclusive: Some(Timestamp::new(2500)),
        ..Default::default()
    };
    let cursor = history
        .start(
            fixture.engine.store(),
            &access(),
            filter.clone(),
            1,
            Timestamp::new(2000),
        )
        .unwrap();
    let page = history
        .page(
            fixture.engine.store(),
            &access(),
            &filter,
            &cursor,
            Timestamp::new(2000),
        )
        .unwrap();
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].event.kind, EventKind::UsageReconciled);
    let wrong = Filter {
        model: Some("another-model".into()),
        ..filter.clone()
    };
    assert!(matches!(
        history.page(
            fixture.engine.store(),
            &access(),
            &wrong,
            &cursor,
            Timestamp::new(2000)
        ),
        Err(Error::Restart(_))
    ));
    assert!(matches!(
        history.page(
            fixture.engine.store(),
            &access(),
            &filter,
            &cursor,
            Timestamp::new(62_000)
        ),
        Err(Error::Restart(_))
    ));
    let mut bytes = Vec::new();
    History::read_artifact(
        fixture.engine.store(),
        &access(),
        &fixture.request.spec.id,
        &mut bytes,
    )
    .unwrap();
    assert_eq!(bytes, b"{\"request\":\"synthetic\"}");
    let denied = Access {
        read: false,
        ..access()
    };
    assert!(matches!(
        History::read_artifact(
            fixture.engine.store(),
            &denied,
            &fixture.request.spec.id,
            Vec::new()
        ),
        Err(Error::Access)
    ));
    issue(
        &mut fixture.engine,
        Command::Rebind {
            binding: Binding {
                host: HostId::new(),
                root: "D:/renamed".into(),
                repository: "fixture".into(),
                worktree: "main".into(),
                revision: Revision::ZERO,
            },
        },
        None,
        0,
    )
    .await;
    assert!(History::read_artifact(
        fixture.engine.store(),
        &access(),
        &fixture.request.spec.id,
        Vec::new()
    )
    .is_err());
    let renewed = Access {
        authority: AuthorityRevision::new(1),
        ..access()
    };
    assert!(History::read_artifact(
        fixture.engine.store(),
        &renewed,
        &fixture.request.spec.id,
        Vec::new()
    )
    .is_ok());
    assert!(matches!(
        history.page(
            fixture.engine.store(),
            &renewed,
            &filter,
            &cursor,
            Timestamp::new(2000)
        ),
        Err(Error::Restart(_))
    ));
}
#[tokio::test]
async fn retention_masks_invalidate_old_cursors_and_prevent_historical_artifact_dereference() {
    let temporary = tempfile::tempdir().unwrap();
    let mut fixture = fixture(temporary.path(), BackendKind::Files).await;
    let mut history = History::default();
    let filter = Filter::default();
    let cursor = history
        .start(
            fixture.engine.store(),
            &access(),
            filter.clone(),
            128,
            Timestamp::new(1000),
        )
        .unwrap();
    let state = fixture.engine.store().state();
    let mut workspace: Workspace = state
        .record(
            Collection::Workspace,
            access().workspace.as_str(),
            &access().workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    let prior = workspace.revision;
    workspace.revision = workspace.revision.next().unwrap();
    workspace.deletion = workspace.deletion.next().unwrap();
    let masked = state
        .events
        .iter()
        .find(|e| e.event.artifacts.contains(&fixture.request.spec.id))
        .unwrap();
    let mask = RetentionMask {
        schema_version: 1,
        workspace: workspace.id.clone(),
        session: masked.event.session.clone(),
        first: masked.sequence,
        last: masked.sequence,
        artifacts: vec![fixture.request.spec.id.clone()],
        deletion: workspace.deletion,
        reason: "explicit fixture restriction".into(),
    };
    let transaction = Transaction {
        id: TransactionId::new(),
        expected_watermark: state.watermark,
        mutations: vec![
            Mutation::Put {
                expected: Some(prior),
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
                    "mask",
                    workspace.id.clone(),
                    Revision::ZERO,
                    &mask,
                )
                .unwrap(),
            },
        ],
        events: vec![],
        command: None,
    };
    fixture
        .engine
        .store_mut()
        .transact(transaction)
        .await
        .unwrap();
    assert!(matches!(
        history.page(
            fixture.engine.store(),
            &access(),
            &filter,
            &cursor,
            Timestamp::new(1000)
        ),
        Err(Error::Restart(_))
    ));
    assert!(matches!(
        History::read_artifact(
            fixture.engine.store(),
            &access(),
            &fixture.request.spec.id,
            Vec::new()
        ),
        Err(Error::Removed)
    ));
    let query = vcp_audit::inspection::InspectionQuery {
        id: fixture.request.spec.id.to_string(),
        view: vcp_audit::inspection::View::Prompts,
        limit: 2,
        cursor: None,
        range: Some(vcp_audit::inspection::RangeRequest {
            offset: 0,
            length: 12,
        }),
    };
    let removed =
        vcp_audit::inspection::inspect(fixture.engine.store(), &access(), &query).unwrap();
    assert!(removed.items.is_empty());
    assert_eq!(removed.gaps[0]["visibility"], "pruned");
    assert_eq!(removed.gaps[0]["source"], fixture.request.spec.source);
    let fresh = history
        .start(
            fixture.engine.store(),
            &access(),
            filter.clone(),
            128,
            Timestamp::new(1000),
        )
        .unwrap();
    let page = history
        .page(
            fixture.engine.store(),
            &access(),
            &filter,
            &fresh,
            Timestamp::new(1000),
        )
        .unwrap();
    assert_eq!(page.gaps.len(), 1);
    assert!(!page.events.iter().any(|e| e.sequence == mask.first));
}

#[cfg(feature = "qualification")]
#[tokio::test]
async fn fresh_process_rebuild_after_dropping_only_projections_agrees_at_same_watermark() {
    use std::process::Command as ProcessCommand;
    for kind in [BackendKind::Sqlite, BackendKind::Files] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("canonical");
        let mut fixture = fixture(&root, kind).await;
        projection::publish(fixture.engine.store_mut(), &access().workspace, 1)
            .await
            .unwrap();
        late_charge(&mut fixture).await;
        let live = projection::publish(fixture.engine.store_mut(), &access().workspace, 1)
            .await
            .unwrap();
        let state = fixture.engine.store().state();
        let mutations = state
            .records
            .values()
            .filter(|r| r.collection == Collection::Projection)
            .map(|r| Mutation::DropProjection {
                id: r.id.clone(),
                expected: r.revision,
            })
            .collect();
        let tx = Transaction {
            id: TransactionId::new(),
            expected_watermark: state.watermark,
            mutations,
            events: vec![],
            command: None,
        };
        fixture.engine.store_mut().transact(tx).await.unwrap();
        let canary = temporary.path().join("external-effect.txt");
        std::fs::write(&canary, b"must not execute again").unwrap();
        drop(fixture);
        let output = temporary.path().join("view.json");
        let status = ProcessCommand::new(env!("CARGO_BIN_EXE_vcp-history-fixture"))
            .arg(&root)
            .arg(if kind == BackendKind::Sqlite {
                "sqlite"
            } else {
                "files"
            })
            .arg(access().workspace.as_str())
            .arg(live.watermark.get().to_string())
            .arg(&output)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        let rebuilt: projection::View =
            serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
        assert_eq!(rebuilt, live);
        assert_eq!(std::fs::read(canary).unwrap(), b"must not execute again");
    }
}

#[cfg(feature = "qualification")]
#[tokio::test]
async fn killed_projection_activation_keeps_view_and_watermark_atomic() {
    use std::{
        process::{Command as ProcessCommand, Stdio},
        time::{Duration, Instant},
    };
    for kind in [BackendKind::Sqlite, BackendKind::Files] {
        for barrier in ["before_commit", "after_commit"] {
            let temporary = tempfile::tempdir().unwrap();
            let root = temporary.path().join("canonical");
            let mut fixture = fixture(&root, kind).await;
            let original = projection::publish(fixture.engine.store_mut(), &access().workspace, 1)
                .await
                .unwrap();
            late_charge(&mut fixture).await;
            let expected = projection::rebuild(
                fixture.engine.store().state(),
                &access().workspace,
                2,
                fixture.engine.store().state().watermark,
            )
            .unwrap();
            drop(fixture);
            let marker = temporary.path().join("activation-barrier");
            let mut child = ProcessCommand::new(env!("CARGO_BIN_EXE_vcp-history-fixture"))
                .arg(&root)
                .arg(if kind == BackendKind::Sqlite {
                    "sqlite"
                } else {
                    "files"
                })
                .arg(access().workspace.as_str())
                .arg("activate")
                .arg(&marker)
                .arg(barrier)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let start = Instant::now();
            while !marker.exists() {
                if let Some(status) = child.try_wait().unwrap() {
                    panic!("activation exited before barrier: {status}");
                }
                if start.elapsed() > Duration::from_secs(20) {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("activation barrier timeout");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            child.kill().unwrap();
            assert!(!child.wait().unwrap().success());
            let mut store = Store::open(&root, kind, &[]).await.unwrap();
            let recovered = projection::active(store.state(), &access().workspace)
                .unwrap()
                .unwrap();
            assert_eq!(
                recovered,
                if barrier == "after_commit" {
                    expected.clone()
                } else {
                    original
                }
            );
            let activated = projection::publish(&mut store, &access().workspace, 2)
                .await
                .unwrap();
            assert_eq!(activated, expected);
            assert_eq!(
                activated,
                projection::rebuild(store.state(), &access().workspace, 2, activated.watermark)
                    .unwrap()
            );
        }
    }
}

#[tokio::test]
async fn inspection_pages_navigate_canonical_evidence_and_survive_projection_rebuild() {
    use vcp_audit::inspection::{self, InspectionQuery, View};
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let directory = tempfile::tempdir().unwrap();
        let mut fixture = fixture(directory.path(), kind).await;
        late_charge(&mut fixture).await;
        projection::publish(fixture.engine.store_mut(), &access().workspace, 1)
            .await
            .unwrap();
        let mut query = InspectionQuery {
            id: fixture.effect.to_string(),
            view: View::Chain,
            limit: 2,
            cursor: None,
            range: None,
        };
        let first = inspection::inspect(fixture.engine.store(), &access(), &query).unwrap();
        assert_eq!(first.scope.task, fixture.root);
        assert!(first.next_cursor.is_some());
        let mut items = Vec::new();
        loop {
            let page = inspection::inspect(fixture.engine.store(), &access(), &query).unwrap();
            assert_eq!(page.source_watermark, first.source_watermark);
            assert!(page.items.len() <= 2);
            items.extend(page.items);
            query.cursor = page.next_cursor;
            if query.cursor.is_none() {
                break;
            }
        }
        for collection in [
            "attempt",
            "reservation",
            "settlement",
            "effect",
            "artifact",
            "workspace",
        ] {
            assert!(
                items.iter().any(|i| i["collection"] == collection),
                "{collection}"
            );
        }
        let attempt = items.iter().find(|i| i["collection"] == "attempt").unwrap();
        assert!(attempt["references"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!(format!(
                "artifact:{}",
                fixture.request.spec.id
            ))));
        assert!(items.iter().any(
            |i| i.pointer("/event/event/kind") == Some(&serde_json::json!("effect_transition"))
        ));
        query.cursor = first.next_cursor.clone();
        let mut wrong = query.clone();
        wrong.view = View::Costs;
        assert!(matches!(
            inspection::inspect(fixture.engine.store(), &access(), &wrong),
            Err(Error::Restart(_))
        ));
        let restricted = Access {
            tasks: Some([fixture.child.clone()].into()),
            ..access()
        };
        assert!(matches!(
            inspection::inspect(fixture.engine.store(), &restricted, &query),
            Err(Error::Access)
        ));
        let mut state = fixture.engine.store().state().clone();
        let _ = projection::rebuild(&state, &access().workspace, 1, state.watermark).unwrap();
        state
            .records
            .retain(|_, r| r.collection != Collection::Projection);
        assert_eq!(
            inspection::records(&state, &access(), &query).unwrap(),
            inspection::inspect(fixture.engine.store(), &access(), &query).unwrap()
        );
        // A canonical change cannot silently mix pages.
        capture(&mut fixture.engine, &fixture.root, b"new evidence").await;
        assert!(matches!(
            inspection::inspect(fixture.engine.store(), &access(), &query),
            Err(Error::Restart(_))
        ));
    }
}

#[tokio::test]
async fn generic_history_and_inspection_do_not_expose_derived_memory_payloads() {
    use vcp_audit::inspection::{self, InspectionQuery, View};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temporary = tempfile::tempdir().unwrap();
        let mut fixture = fixture(temporary.path(), backend).await;
        let marker = "private-derived-memory-marker";
        let id = ClaimId::new();
        let proposal = ProposalId::new();
        let scope = Scope {
            workspace: access().workspace,
            session: engine_access().session,
            task: fixture.root.clone(),
        };
        // An older generic claim and raw-facts memory event are both valid
        // canonical inputs; neither can rely on the newer producer's summary.
        let claim = Record::typed(
            Collection::Claim,
            id.as_str(),
            scope.workspace.clone(),
            Revision::ZERO,
            &serde_json::json!({"schema_version":1,"scope":scope,"statement":marker}),
        )
        .unwrap();
        let event = EventInput {
            id: EventId::new(),
            workspace: scope.workspace.clone(),
            session: scope.session.clone(),
            task: Some(scope.task.clone()),
            actor: engine_access().actor,
            correlation: CommandId::new(),
            causation: None,
            timestamp: Timestamp::new(3000),
            kind: EventKind::MemoryResolved,
            artifacts: vec![],
            data: serde_json::json!({"schema_version":1,"proposal":proposal,"resolution":"accepted","facts":[claim]}),
            metadata: None,
        };
        let transaction = Transaction {
            id: TransactionId::new(),
            expected_watermark: fixture.engine.store().state().watermark,
            mutations: vec![Mutation::Put {
                expected: None,
                record: claim,
            }],
            events: vec![event],
            command: None,
        };
        fixture
            .engine
            .store_mut()
            .transact(transaction)
            .await
            .unwrap();
        let restricted = Access {
            tasks: Some([fixture.root.clone()].into()),
            ..access()
        };
        let query = InspectionQuery {
            id: fixture.root.to_string(),
            view: View::Memory,
            limit: 128,
            cursor: None,
            range: None,
        };
        let memory = inspection::inspect(fixture.engine.store(), &restricted, &query).unwrap();
        assert!(!serde_json::to_string(&memory).unwrap().contains(marker));
        assert!(memory.items.iter().any(
            |item| item["id"] == id.as_str() && item["visibility"] == "governed_query_required"
        ));
        let mut chain = InspectionQuery {
            view: View::Chain,
            ..query
        };
        let mut found = false;
        loop {
            let page = inspection::inspect(fixture.engine.store(), &restricted, &chain).unwrap();
            assert!(!serde_json::to_string(&page).unwrap().contains(marker));
            found |= page.items.iter().any(|item| {
                item.pointer("/event/event/data/proposal") == Some(&serde_json::json!(proposal))
            });
            chain.cursor = page.next_cursor;
            if chain.cursor.is_none() {
                break;
            }
        }
        assert!(found);
        let mut history = History::default();
        let filter = Filter {
            kind: Some(EventKind::MemoryResolved),
            ..Default::default()
        };
        let cursor = history
            .start(
                fixture.engine.store(),
                &restricted,
                filter.clone(),
                128,
                Timestamp::new(3000),
            )
            .unwrap();
        let page = history
            .page(
                fixture.engine.store(),
                &restricted,
                &filter,
                &cursor,
                Timestamp::new(3000),
            )
            .unwrap();
        assert_eq!(page.events.len(), 1);
        assert_eq!(page.events[0].event.data["proposal"], proposal.as_str());
        assert!(!serde_json::to_string(&page).unwrap().contains(marker));
        assert!(fixture
            .engine
            .store()
            .state()
            .events
            .iter()
            .any(|event| event.event.data.to_string().contains(marker)));
    }
}

#[tokio::test]
async fn inspection_ranges_preserve_binary_bytes_and_enforce_current_scope() {
    use vcp_audit::inspection::{self, InspectionQuery, RangeRequest, View, MAX_RANGE};
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = fixture(directory.path(), BackendKind::Files).await;
    let bytes: Vec<u8> = (0..190_000).map(|n| (n % 256) as u8).collect();
    let artifact = capture(&mut fixture.engine, &fixture.root, &bytes).await;
    let mut query = InspectionQuery {
        id: artifact.spec.id.to_string(),
        view: View::Outputs,
        limit: 1,
        cursor: None,
        range: Some(RangeRequest {
            offset: 0,
            length: MAX_RANGE,
        }),
    };
    let mut recovered = Vec::new();
    loop {
        let page = inspection::inspect(fixture.engine.store(), &access(), &query).unwrap();
        let item = &page.items[0];
        let chunk: Vec<u8> = serde_json::from_value(item["bytes"].clone()).unwrap();
        assert!(chunk.len() <= MAX_RANGE as usize);
        recovered.extend(chunk);
        let Some(next) = item["next_offset"].as_u64() else {
            break;
        };
        query.range.as_mut().unwrap().offset = next;
    }
    assert_eq!(recovered, bytes);
    let denied = Access {
        tasks: Some([fixture.child.clone()].into()),
        ..access()
    };
    assert!(matches!(
        inspection::inspect(fixture.engine.store(), &denied, &query),
        Err(Error::Access)
    ));
    let other = Access {
        workspace: WorkspaceId::new(),
        ..access()
    };
    assert!(inspection::inspect(fixture.engine.store(), &other, &query).is_err());
    let stale = Access {
        authority: AuthorityRevision::new(99),
        ..access()
    };
    assert!(matches!(
        inspection::inspect(fixture.engine.store(), &stale, &query),
        Err(Error::Access)
    ));
    query.range.as_mut().unwrap().length = MAX_RANGE + 1;
    assert!(matches!(
        inspection::inspect(fixture.engine.store(), &access(), &query),
        Err(Error::Limit)
    ));
    query.range = Some(RangeRequest {
        offset: 0,
        length: 16,
    });
    let spool_file = fixture
        .engine
        .store()
        .spool()
        .root()
        .join(artifact.spec.id.as_str())
        .join("seal.json");
    // Removing metadata is not an empty output; corruption stays a hard error.
    std::fs::rename(&spool_file, spool_file.with_extension("missing")).unwrap();
    assert!(inspection::inspect(fixture.engine.store(), &access(), &query).is_err());
}

#[tokio::test]
async fn inspection_marks_missing_and_incomplete_content_without_reconstruction() {
    use vcp_audit::inspection::{self, InspectionQuery, RangeRequest, View};
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = fixture(directory.path(), BackendKind::Files).await;
    let mut spec = fixture.request.spec.clone();
    spec.id = ArtifactId::new();
    spec.channel = Channel::RequestBody;
    spec.omissions = vec![Omission::AuthenticationHeaders];
    let mut writer = fixture.engine.store().spool().create(spec).unwrap();
    writer.write_chunk(b"partial\x1b[31m").unwrap();
    let artifact = writer.abort().unwrap();
    drop(writer);
    issue(
        &mut fixture.engine,
        Command::AttachArtifact {
            descriptor: artifact.clone(),
        },
        Some(fixture.root.clone()),
        0,
    )
    .await;
    let query = InspectionQuery {
        id: artifact.spec.id.to_string(),
        view: View::Prompts,
        limit: 1,
        cursor: None,
        range: Some(RangeRequest {
            offset: 0,
            length: 100,
        }),
    };
    let page = inspection::inspect(fixture.engine.store(), &access(), &query).unwrap();
    assert_eq!(page.items[0]["text"], "partial\x1b[31m");
    assert_eq!(page.items[0]["representation"], "captured_bytes");
    assert_eq!(page.gaps[0]["capture_state"], "aborted");
    assert!(page.gaps[0]["omissions"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("authentication_headers")));
    let serialized = serde_json::to_string(&page).unwrap();
    assert!(!serialized.contains('\x1b'));
    let spec_file = fixture
        .engine
        .store()
        .spool()
        .root()
        .join(artifact.spec.id.as_str())
        .join("spec.json");
    std::fs::rename(&spec_file, spec_file.with_extension("missing")).unwrap();
    let missing = inspection::inspect(fixture.engine.store(), &access(), &query).unwrap();
    assert!(missing.items.is_empty());
    assert_eq!(missing.gaps[0]["visibility"], "missing");
    assert_eq!(missing.gaps[0]["source"], artifact.spec.source);
}

#[tokio::test]
async fn browser_rejects_unavailable_facets_even_in_nested_or_and_negation() {
    use vcp_audit::history_query::{self, Query};
    use vcp_domain::{
        memory::{ClaimKind, Outcome},
        retention_selector::{Criterion, Selector, Status, Tree},
    };
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let fixture = fixture(temp.path(), backend).await;
        let state = fixture.engine.store().state();
        let mut query = Query {
            selector: Selector {
                schema_version: 1,
                tree: Tree::Match(Criterion::Task(fixture.root.clone())),
            },
            text: None,
            limit: 32,
            cursor: None,
            artifact: None,
            expand_compacted: false,
        };
        assert!(!history_query::query(state, &access(), &query)
            .unwrap()
            .rows
            .is_empty());
        let task: Task = state
            .record(Collection::Task, fixture.root.as_str(), &access().workspace)
            .unwrap()
            .decode()
            .unwrap();
        query.selector.tree = Tree::Match(Criterion::Status(Status::Task(task.state)));
        assert!(!history_query::query(state, &access(), &query)
            .unwrap()
            .rows
            .is_empty());
        for predicate in [
            Criterion::Root(RootId::parse("source-root").unwrap()),
            Criterion::Claim(ClaimKind::Architecture),
            Criterion::Status(Status::Claim(Outcome::Accepted)),
            Criterion::Superseded(true),
        ] {
            let leaf = Tree::Match(predicate);
            for tree in [
                leaf.clone(),
                Tree::Not(Box::new(Tree::All(vec![leaf.clone()]))),
                Tree::Any(vec![
                    Tree::Match(Criterion::Workspace(access().workspace)),
                    Tree::Not(Box::new(leaf.clone())),
                ]),
            ] {
                query.selector.tree = tree;
                // The shared selector remains valid for pruning. Only this
                // event-only browsing adapter lacks these metadata capabilities.
                assert!(query.selector.clone().normalized().is_ok());
                assert!(matches!(
                    history_query::query(state, &access(), &query),
                    Err(Error::UnsupportedFilter(_))
                ));
            }
        }
    }
}
