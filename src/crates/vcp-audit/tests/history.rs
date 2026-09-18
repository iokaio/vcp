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
fn access() -> Access {
    Access {
        workspace: engine_access().workspace,
        authority: AuthorityRevision::ZERO,
        read: true,
        tasks: None,
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
    writer.write_chunk(bytes).unwrap();
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
