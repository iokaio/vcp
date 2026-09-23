// SPDX-License-Identifier: Apache-2.0
//! Public usage views over genuine budget service transitions on both stores.
use vcp_domain::{
    accounting::*,
    artifact::{ArtifactDescriptor, ArtifactSpec, Channel},
    ids::*,
    revision::*,
    task::{Objective, TaskState},
    verification::Fingerprint,
    workspace::{Binding, Scope},
};
use vcp_engine::{query::QueryError, Access, Engine, HostFacts};
use vcp_protocol::{
    command::{Command, CommandEnvelope},
    methods,
};
use vcp_store::{artifact::ArtifactWriter, BackendKind, Store};
fn id(value: &str) -> Result<methods::Id, String> {
    value.to_owned().try_into().map_err(|e| format!("{e}"))
}

fn money(amount: u64) -> Money {
    Money {
        currency: "USD".to_owned().try_into().unwrap(),
        micros: Micros::new(amount),
    }
}

async fn reserve(engine: &mut Engine<Store>, name: &str, amount: u64) -> Attempt {
    let captured = artifact(engine, name, b"synthetic request", false).await;
    let scope = captured.spec.scope.clone();
    let ledger = vcp_budget::ledger(engine.store().state(), &scope).unwrap();
    let price = PriceSnapshot {
        id: "a".repeat(64),
        provider: "synthetic".into(),
        model: "synthetic".into(),
        currency: money(0).currency,
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
                        amount
                    } else {
                        0
                    }),
                    per_units: Units::new(1),
                },
            )
        })
        .collect(),
    };
    let input = vcp_budget::Admission {
        transaction: TransactionId::new(),
        attempt: AttemptId::new(),
        reservation: ReservationId::new(),
        scope,
        agent: AgentId::new(),
        role: RequestRole::Main,
        request: captured.spec.id,
        request_digest: captured.sha256,
        quote: vcp_budget::arithmetic::quote(
            price,
            Usage {
                requests: Units::new(1),
                ..Default::default()
            },
            Timestamp::new(1),
        )
        .unwrap(),
        previous: None,
        expected_ledger: ledger.revision,
        policy: ledger.policy,
        steering: SteeringRevision::ZERO,
        draw_protected: false,
        now: Timestamp::new(1),
    };
    vcp_budget::reserve(engine.store_mut(), input, &actor())
        .await
        .unwrap()
}
fn actor() -> vcp_budget::Actor {
    vcp_budget::Actor {
        id: access().actor,
        now: Timestamp::new(1),
    }
}

#[tokio::test]
async fn public_usage_preserves_actual_settlements_reservations_unknown_liabilities_and_restart() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let mut engine = fixture(temp.path(), backend).await;
        let mut request = methods::Inspect {
            scope: scope(),
            task: id("task").unwrap(),
            target: None,
            cursor: None,
            limit: 1,
        };
        assert_eq!(
            engine.public_usage(&access(), &request),
            Err(QueryError::Unavailable)
        );
        let task = TaskId::parse("task").unwrap();
        let task_scope = Scope {
            workspace: access().workspace,
            session: access().session,
            task: task.clone(),
        };
        command(
            &mut engine,
            Command::Transition {
                next: TaskState::Running,
                reason: "explicit synthetic run".into(),
                verification: None,
            },
            Some(task),
        )
        .await;
        vcp_budget::initialize(
            engine.store_mut(),
            task_scope.clone(),
            money(u64::MAX),
            Micros::ZERO,
            None,
            &actor(),
        )
        .await
        .unwrap();

        let settled = reserve(&mut engine, "settled-request", 9_007_199_254_740_993).await;
        vcp_budget::submit(
            engine.store_mut(),
            &settled.id,
            &task_scope,
            settled.revision,
            &actor(),
        )
        .await
        .unwrap();
        let usage = artifact(
            &mut engine,
            "usage-evidence",
            b"synthetic observed settled charge",
            false,
        )
        .await;
        vcp_budget::observe(
            engine.store_mut(),
            UsageObservation {
                id: ObservationId::new(),
                scope: task_scope.clone(),
                attempt: settled.id,
                provider_request: "synthetic-request".into(),
                mode: UsageMode::Cumulative {
                    version: Units::new(1),
                },
                amount: money(9_007_199_254_740_993),
                final_usage: true,
                raw: usage.spec.id,
                correction: None,
            },
            &actor(),
        )
        .await
        .unwrap();
        reserve(&mut engine, "active-request", 11).await;
        let unknown = reserve(&mut engine, "unknown-request", 13).await;
        vcp_budget::submit(
            engine.store_mut(),
            &unknown.id,
            &task_scope,
            unknown.revision,
            &actor(),
        )
        .await
        .unwrap();
        vcp_budget::hold_uncertain(
            engine.store_mut(),
            &unknown.id,
            &task_scope,
            &actor(),
            "synthetic interrupted response",
        )
        .await
        .unwrap();

        let watermark = engine.store().state().watermark;
        let view = engine.public_usage(&access(), &request).unwrap();
        assert_eq!(view.cap_micros.as_str(), "18446744073709551615");
        assert_eq!(view.settled_micros.as_str(), "9007199254740993");
        assert_eq!(view.reserved_micros.as_str(), "11");
        assert_eq!(view.unresolved_micros.as_str(), "13");
        assert!(!view.overrun);
        request.target = Some(id("other").unwrap());
        assert_eq!(
            engine.public_usage(&access(), &request),
            Err(QueryError::Unavailable)
        );
        request.target = None;
        request.cursor = Some("invented".into());
        assert_eq!(
            engine.public_usage(&access(), &request),
            Err(QueryError::StaleCursor)
        );
        request.cursor = None;
        let mut revoked = access();
        revoked.authority = AuthorityRevision::new(1);
        assert_eq!(
            engine.public_usage(&revoked, &request),
            Err(QueryError::Access)
        );
        assert_eq!(engine.store().state().watermark, watermark);
        engine.into_store().close().await.unwrap();
        let mut engine =
            Engine::new(Store::open(temp.path(), backend, &[]).await.unwrap()).unwrap();
        assert_eq!(engine.public_usage(&access(), &request).unwrap(), view);
        let ledger = vcp_budget::ledger(engine.store().state(), &task_scope).unwrap();
        vcp_budget::configure(
            engine.store_mut(),
            &task_scope,
            ledger.revision,
            money(1),
            Micros::ZERO,
            Default::default(),
            &actor(),
            "explicit lower cap preserves incurred liability",
        )
        .await
        .unwrap();
        let overrun = engine.public_usage(&access(), &request).unwrap();
        assert!(overrun.overrun);
        assert_eq!(overrun.settled_micros, view.settled_micros);
        assert_eq!(overrun.reserved_micros, view.reserved_micros);
        assert_eq!(overrun.unresolved_micros, view.unresolved_micros);
        engine.into_store().close().await.unwrap();
    }
}
fn access() -> Access {
    Access {
        actor: ActorId::parse("owner").unwrap(),
        workspace: WorkspaceId::parse("workspace").unwrap(),
        session: SessionId::parse("session").unwrap(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: true,
    }
}
fn scope() -> methods::Scope {
    methods::Scope {
        workspace: id("workspace").unwrap(),
        session: id("session").unwrap(),
    }
}
async fn command(engine: &mut Engine<Store>, payload: Command, task: Option<TaskId>) {
    let mut facts = HostFacts::inspect(Timestamp::new(1));
    facts.may_execute = true;
    engine
        .handle(
            CommandEnvelope {
                version: 1,
                id: CommandId::new(),
                workspace: access().workspace,
                session: access().session,
                task,
                caller: access().actor,
                controller: engine.controller().clone(),
                owner_epoch: engine.owner_epoch(),
                expected: Revision::ZERO,
                steering: SteeringRevision::ZERO,
                payload,
            },
            &access(),
            &facts,
        )
        .await
        .unwrap();
}
async fn fixture(path: &std::path::Path, backend: BackendKind) -> Engine<Store> {
    let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
    command(
        &mut engine,
        Command::Initialize {
            binding: Binding {
                host: HostId::new(),
                root: "C:/public-read-fixture".into(),
                repository: "fixture".into(),
                worktree: "main".into(),
                revision: Revision::ZERO,
            },
        },
        None,
    )
    .await;
    for name in ["task", "other"] {
        let task = TaskId::parse(name).unwrap();
        command(
            &mut engine,
            Command::CreateTask {
                root: task.clone(),
                parent: None,
                fork_origin: None,
                objective: Objective {
                    text: "public read fixture".into(),
                    constraints: vec![],
                    acceptance: vec![],
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
            },
            Some(task),
        )
        .await;
    }
    engine
}
async fn artifact(
    engine: &mut Engine<Store>,
    name: &str,
    bytes: &[u8],
    aborted: bool,
) -> ArtifactDescriptor {
    let task = TaskId::parse("task").unwrap();
    let mut writer = engine
        .store()
        .spool()
        .create(ArtifactSpec {
            id: ArtifactId::parse(name).unwrap(),
            scope: Scope {
                workspace: access().workspace,
                session: access().session,
                task: task.clone(),
            },
            media_type: "application/octet-stream".into(),
            schema: "fixture/1".into(),
            source: "fixture".into(),
            channel: Channel::Response,
            retention: "history".into(),
            omissions: vec![],
        })
        .unwrap();
    for chunk in bytes.chunks(65_536) {
        writer.write_chunk(chunk).unwrap();
    }
    let descriptor = if aborted {
        writer.abort().unwrap()
    } else {
        writer.finalize().unwrap()
    };
    command(
        engine,
        Command::AttachArtifact {
            descriptor: descriptor.clone(),
        },
        Some(task),
    )
    .await;
    descriptor
}
