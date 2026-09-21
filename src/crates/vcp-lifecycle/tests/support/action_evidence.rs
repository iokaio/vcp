// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_domain::{
    accounting::*,
    artifact::{ArtifactDescriptor, ArtifactSpec, Channel},
    task::{Objective, Task, TaskState, TurnState},
    verification::{Check, CheckOutcome, CostCertainty, Verification},
};
use vcp_lifecycle::foundation::{
    routing,
    routing_state::{consumption, fits, observations::*, rewards},
};
use vcp_protocol::{
    command::{Command, CommandEnvelope},
    digest_bytes,
};
use vcp_store::artifact::ArtifactWriter;

struct LegacyAccounting<'a>(&'a mut Store);

async fn repeated_verifications(
    store: Store,
    access: &Access,
    task: &Task,
    output: &ArtifactId,
    from: u64,
    count: u64,
) -> Store {
    let mut engine = vcp_engine::Engine::new(store).unwrap();
    let actor = engine_access(access, task);
    for index in 0..count {
        let verification = Verification {
            redaction: None,
            id: VerificationId::new(),
            scope: task.scope.clone(),
            steering: task.steering,
            fingerprint: task.fingerprint.clone(),
            outputs: vec![output.clone()],
            checks: vec![Check {
                specification: "frozen fit check".into(),
                outcome: CheckOutcome::Failed {
                    reason: "same diagnostic".into(),
                },
                output: output.clone(),
                exit_code: Some(1),
            }],
            unresolved_effects: vec![],
            outstanding_issues: vec![],
            cost: CostCertainty::Known,
        };
        engine
            .handle(
                command(
                    &engine,
                    access,
                    task,
                    task.revision,
                    Command::RecordVerification { verification },
                ),
                &actor,
                &vcp_engine::HostFacts::inspect(Timestamp::new(from + index)),
            )
            .await
            .unwrap();
    }
    engine.into_store()
}

#[tokio::test]
async fn local_stall_frozen_fit_survives_append_and_reopen_but_denies_pruned_sources() {
    use vcp_domain::retention_selector::{Criterion, Selector, Tree};
    use vcp_lifecycle::foundation::routing_state::local_stall;
    use vcp_memory::retention::{self, Action};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(temp.path(), backend).await;
        let task = create_task(&mut store, &access, TaskState::Running).await;
        let output = capture(&mut store, &task, Channel::Stdout).await;
        let store = repeated_verifications(store, &access, &task, &output.spec.id, 10, 12).await;
        let training = HistoryWindow {
            from: None,
            until: Timestamp::new(30),
        };
        let fit = local_stall::fit(&store, &access, training, 3).unwrap();
        local_stall::validate_install(&store, &access, &fit).unwrap();
        let mut forged = fit.clone();
        forged.model = vcp_models::stall::fit(1, &[vec![0; 4]], 3).unwrap();
        forged.id.clear();
        forged.id = format!(
            "local-stall-fit-{}",
            digest_bytes(&vcp_protocol::canonical_bytes(&forged).unwrap())
        );
        assert!(local_stall::validate_install(&store, &access, &forged).is_err());
        let fit_bytes = serde_json::to_vec(&fit).unwrap();
        let store = repeated_verifications(store, &access, &task, &output.spec.id, 40, 3).await;
        let before = store.state().clone();
        let inference = HistoryWindow {
            from: Some(Timestamp::new(30)),
            until: Timestamp::new(50),
        };
        let result =
            local_stall::evaluate(&store, &access, &fit, &task.scope.task, inference.clone())
                .unwrap();
        assert!(result.abstention.is_none());
        assert!(result.signal.as_ref().unwrap().repeated_strategy_suspected);
        assert!(!result.exact_cycles.repetitions.is_empty());
        assert!(!result.serving_qualified && !fit.serving_qualified);
        assert_eq!(serde_json::to_vec(&fit).unwrap(), fit_bytes);
        assert_eq!(*store.state(), before);
        assert!(local_stall::evaluate(
            &store,
            &access,
            &fit,
            &task.scope.task,
            HistoryWindow {
                from: None,
                until: Timestamp::new(50)
            }
        )
        .is_err());
        let denied = Access {
            workspace: access.workspace.clone(),
            actor: access.actor.clone(),
            authority: access.authority,
            read: true,
            write: false,
            tasks: Some(BTreeSet::new()),
        };
        assert!(local_stall::validate(&store, &denied, &fit).is_err());
        drop(store);
        let store = Store::open(temp.path(), backend, &[]).await.unwrap();
        let restored: local_stall::Fit = serde_json::from_slice(&fit_bytes).unwrap();
        let replay =
            local_stall::evaluate(&store, &access, &restored, &task.scope.task, inference).unwrap();
        assert_eq!(
            serde_json::to_value(&replay).unwrap(),
            serde_json::to_value(&result).unwrap()
        );
        let actor = engine_access(&access, &task);
        let mut engine = vcp_engine::Engine::new(store).unwrap();
        engine
            .handle(
                command(
                    &engine,
                    &access,
                    &task,
                    task.revision,
                    Command::Transition {
                        next: TaskState::Cancelled,
                        reason: "retention fixture complete".into(),
                        verification: None,
                    },
                ),
                &actor,
                &vcp_engine::HostFacts::inspect(Timestamp::new(60)),
            )
            .await
            .unwrap();
        let mut store = engine.into_store();
        let plan = retention::preview(
            &store,
            &access,
            Selector {
                schema_version: 1,
                tree: Tree::Match(Criterion::Event("verification_recorded".into())),
            },
            Action::Purge,
            Timestamp::new(1001),
        )
        .unwrap();
        retention::apply(&mut store, &access, &plan, Timestamp::new(1001))
            .await
            .unwrap();
        assert!(local_stall::validate(&store, &access, &restored).is_err());
    }
}

#[tokio::test]
async fn exact_cycles_rebuild_read_only_and_disappear_after_source_purge() {
    use vcp_domain::retention_selector::{Criterion, Selector, Tree};
    use vcp_lifecycle::foundation::routing_state::cycles;
    use vcp_memory::retention::{self, Action};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(temp.path(), backend).await;
        let task = create_task(&mut store, &access, TaskState::Running).await;
        let output = capture(&mut store, &task, Channel::Stdout).await;
        let mut engine = vcp_engine::Engine::new(store).unwrap();
        let actor = engine_access(&access, &task);
        for index in 0..3 {
            let verification = Verification {
                redaction: None,
                id: VerificationId::new(),
                scope: task.scope.clone(),
                steering: task.steering,
                fingerprint: task.fingerprint.clone(),
                outputs: vec![output.spec.id.clone()],
                checks: vec![Check {
                    specification: "private check command".into(),
                    outcome: CheckOutcome::Failed {
                        reason: "private repeated diagnostic".into(),
                    },
                    output: output.spec.id.clone(),
                    exit_code: Some(1),
                }],
                unresolved_effects: vec![],
                outstanding_issues: vec![],
                cost: CostCertainty::Known,
            };
            engine
                .handle(
                    command(
                        &engine,
                        &access,
                        &task,
                        Revision::ZERO,
                        Command::RecordVerification { verification },
                    ),
                    &actor,
                    &vcp_engine::HostFacts::inspect(Timestamp::new(10 + index)),
                )
                .await
                .unwrap();
        }
        let mut store = engine.into_store();
        let before = store.state().clone();
        let read = Access {
            workspace: access.workspace.clone(),
            actor: access.actor.clone(),
            authority: access.authority,
            read: true,
            write: false,
            tasks: None,
        };
        let evidence = cycles::observe(&store, &read, window()).unwrap();
        assert!(!evidence.repetitions.is_empty());
        assert!(!evidence.serving_qualified);
        let encoded = serde_json::to_string(&evidence).unwrap();
        assert!(!encoded.contains("private check") && !encoded.contains("private repeated"));
        let value = routing::execute(
            &mut store,
            &read,
            routing::Request::Cycles {
                from: None,
                until: window().until,
            },
            None,
            window().until,
        )
        .await
        .unwrap();
        assert_eq!(value, serde_json::to_value(&evidence).unwrap());
        assert_eq!(*store.state(), before);
        let denied = Access {
            workspace: access.workspace.clone(),
            actor: access.actor.clone(),
            authority: access.authority,
            read: true,
            write: false,
            tasks: Some(BTreeSet::new()),
        };
        assert!(cycles::observe(&store, &denied, window())
            .unwrap()
            .repetitions
            .is_empty());
        let no_read = Access {
            read: false,
            ..read
        };
        assert!(cycles::observe(&store, &no_read, window()).is_err());
        drop(store);
        let store = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(
            cycles::observe(&store, &access, window()).unwrap(),
            evidence
        );
        let mut engine = vcp_engine::Engine::new(store).unwrap();
        engine
            .handle(
                command(
                    &engine,
                    &access,
                    &task,
                    Revision::ZERO,
                    Command::Transition {
                        next: TaskState::Cancelled,
                        reason: "retention fixture complete".into(),
                        verification: None,
                    },
                ),
                &actor,
                &vcp_engine::HostFacts::inspect(Timestamp::new(20)),
            )
            .await
            .unwrap();
        let mut store = engine.into_store();
        let plan = retention::preview(
            &store,
            &access,
            Selector {
                schema_version: 1,
                tree: Tree::Match(Criterion::Event("verification_recorded".into())),
            },
            Action::Purge,
            Timestamp::new(1001),
        )
        .unwrap();
        assert!(!plan.selected.is_empty());
        retention::apply(&mut store, &access, &plan, Timestamp::new(1001))
            .await
            .unwrap();
        let purged = cycles::observe(&store, &access, window()).unwrap();
        assert!(purged.repetitions.is_empty());
        drop(store);
        let reopened = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(
            cycles::observe(&reopened, &access, window()).unwrap(),
            purged
        );
    }
}

impl CanonicalStore for LegacyAccounting<'_> {
    fn state(&self) -> &State {
        self.0.state()
    }

    async fn transact(&mut self, mut transaction: Transaction) -> vcp_store::Result<Receipt> {
        for event in &mut transaction.events {
            if event.kind == EventKind::UsageReconciled {
                if let Some(data) = event.data.as_object_mut() {
                    data.remove("settlement");
                }
            }
        }
        self.0.transact(transaction).await
    }
}

fn window() -> HistoryWindow {
    HistoryWindow {
        from: None,
        until: Timestamp::new(1000),
    }
}

pub(super) async fn create_task(store: &mut Store, access: &Access, state: TaskState) -> Task {
    create_named_task(store, access, state, "action-task").await
}

async fn create_named_task(
    store: &mut Store,
    access: &Access,
    state: TaskState,
    name: &str,
) -> Task {
    let session: Session = store
        .state()
        .records
        .values()
        .find(|record| record.collection == Collection::Session)
        .unwrap()
        .decode()
        .unwrap();
    let id = TaskId::parse(name).unwrap();
    let cause = EventId::new();
    let task = Task {
        scope: Scope {
            workspace: access.workspace.clone(),
            session: session.id.clone(),
            task: id.clone(),
        },
        root: id.clone(),
        parent: None,
        fork_origin: None,
        revision: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        objectives: vec![Objective {
            text: "private action objective".into(),
            constraints: vec![],
            acceptance: vec![],
            source: cause.clone(),
            steering: SteeringRevision::ZERO,
        }],
        state,
        fingerprint: Fingerprint {
            repository: "a".repeat(64),
            buffers: "b".repeat(64),
            environment: "c".repeat(64),
        },
        editing: true,
        required_checks: vec!["cargo test secret argument".into()],
        cause: cause.clone(),
        reason: "private task reason".into(),
        redaction: None,
    };
    store.transact(Transaction {
        id: TransactionId::new(), expected_watermark: store.state().watermark,
        mutations: vec![Mutation::Put { record: Record::typed(Collection::Task, id.as_str(), access.workspace.clone(),
            Revision::ZERO, &task).unwrap(), expected: None }],
        events: vec![EventInput { id: cause, workspace: access.workspace.clone(), session: session.id,
            task: Some(id), actor: access.actor.clone(), correlation: CommandId::new(), causation: None,
            timestamp: Timestamp::new(2), kind: EventKind::TaskCreated, artifacts: vec![],
            data: serde_json::json!({"schema_version":1,"facts":[{"collection":"task","id":task.scope.task,
                "revision":task.revision,"value":task}]}), metadata: None }], command: None,
    }).await.unwrap();
    task
}

fn engine_access(access: &Access, task: &Task) -> vcp_engine::Access {
    vcp_engine::Access {
        actor: access.actor.clone(),
        workspace: access.workspace.clone(),
        session: task.scope.session.clone(),
        authority: access.authority,
        read: true,
        write: true,
        bootstrap: false,
    }
}

fn command(
    engine: &vcp_engine::Engine<Store>,
    access: &Access,
    task: &Task,
    expected: Revision,
    payload: Command,
) -> CommandEnvelope {
    CommandEnvelope {
        version: 1,
        id: CommandId::new(),
        workspace: access.workspace.clone(),
        session: task.scope.session.clone(),
        task: Some(task.scope.task.clone()),
        caller: access.actor.clone(),
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected,
        steering: SteeringRevision::ZERO,
        payload,
    }
}

#[tokio::test]
async fn engine_turn_and_verification_facts_are_revision_bound_without_prose() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(temp.path(), backend).await;
        let task = create_task(&mut store, &access, TaskState::Running).await;
        let trigger = capture(&mut store, &task, Channel::Evidence).await;
        let check_output = capture(&mut store, &task, Channel::Stdout).await;
        let mut engine = vcp_engine::Engine::new(store).unwrap();
        let actor = engine_access(&access, &task);
        let mut host = vcp_engine::HostFacts::inspect(Timestamp::new(10));
        host.may_execute = true;
        let turn = TurnId::new();
        engine
            .handle(
                command(
                    &engine,
                    &access,
                    &task,
                    Revision::ZERO,
                    Command::StartTurn {
                        id: turn.clone(),
                        trigger: trigger.spec.id,
                    },
                ),
                &actor,
                &host,
            )
            .await
            .unwrap();
        let effect = ToolRunId::new();
        engine
            .handle(
                command(
                    &engine,
                    &access,
                    &task,
                    Revision::ZERO,
                    Command::ProposeEffect {
                        id: effect.clone(),
                        operation_digest: "f".repeat(64),
                    },
                ),
                &actor,
                &host,
            )
            .await
            .unwrap();
        engine
            .handle(
                command(
                    &engine,
                    &access,
                    &task,
                    Revision::ZERO,
                    Command::AdvanceEffect {
                        id: effect,
                        next: vcp_domain::effect::EffectState::Validated,
                        reason: "private effect narrative".into(),
                        execution: None,
                        exit_code: None,
                        observed_changes: vec![],
                    },
                ),
                &actor,
                &host,
            )
            .await
            .unwrap();
        engine
            .handle(
                command(
                    &engine,
                    &access,
                    &task,
                    Revision::ZERO,
                    Command::AdvanceTurn {
                        id: turn,
                        next: TurnState::AssemblingContext,
                        reason: "private turn narrative".into(),
                    },
                ),
                &actor,
                &host,
            )
            .await
            .unwrap();
        let verification = Verification {
            redaction: None,
            id: VerificationId::new(),
            scope: task.scope.clone(),
            steering: task.steering,
            fingerprint: task.fingerprint.clone(),
            outputs: vec![check_output.spec.id.clone()],
            checks: vec![Check {
                specification: " Cargo   TEST secret/path ".into(),
                outcome: CheckOutcome::Failed {
                    reason: " Line 99: PRIVATE mismatch ".into(),
                },
                output: check_output.spec.id,
                exit_code: Some(101),
            }],
            unresolved_effects: vec![],
            outstanding_issues: vec![],
            cost: CostCertainty::Known,
        };
        let envelope = command(
            &engine,
            &access,
            &task,
            Revision::ZERO,
            Command::RecordVerification { verification },
        );
        engine
            .handle(envelope.clone(), &actor, &host)
            .await
            .unwrap();
        engine.handle(envelope, &actor, &host).await.unwrap();
        let before = engine.store().state().clone();
        let read = Access {
            workspace: access.workspace.clone(),
            actor: access.actor.clone(),
            authority: access.authority,
            read: true,
            write: false,
            tasks: access.tasks.clone(),
        };
        let evidence = observe(engine.store(), &read, window()).unwrap();
        assert_eq!(evidence.turns.len(), 1);
        assert_eq!(evidence.turns[0].observations.len(), 2);
        assert!(evidence.turns[0].gaps.is_empty() && evidence.turns[0].right_censored);
        assert_eq!(evidence.effects.len(), 1);
        assert_eq!(evidence.effects[0].observations.len(), 2);
        assert_eq!(
            evidence.effects[0]
                .observations
                .iter()
                .map(|observation| observation.state)
                .collect::<Vec<_>>(),
            vec![
                vcp_domain::effect::EffectState::Proposed,
                vcp_domain::effect::EffectState::Validated
            ]
        );
        assert!(evidence.effects[0].right_censored);
        assert_eq!(evidence.verifications.len(), 1);
        assert_eq!(
            evidence.verifications[0].observed_task_revision,
            Some(Revision::ZERO)
        );
        assert!(evidence.verifications[0].checks[0]
            .failure_signature
            .is_some());
        let encoded = serde_json::to_string(&evidence).unwrap();
        for private in [
            "private action objective",
            "private turn narrative",
            "private effect narrative",
            "cargo",
            "secret/path",
            "mismatch",
        ] {
            assert!(!encoded.to_lowercase().contains(private));
        }
        assert_eq!(engine.store().state(), &before);
    }
}

pub(super) fn money(value: u64) -> Money {
    Money {
        currency: "USD".to_owned().try_into().unwrap(),
        micros: Micros::new(value),
    }
}
pub(super) fn budget_actor(access: &Access, now: u64) -> vcp_budget::Actor {
    vcp_budget::Actor {
        id: access.actor.clone(),
        now: Timestamp::new(now),
    }
}
async fn capture(store: &mut Store, task: &Task, channel: Channel) -> ArtifactDescriptor {
    let spec = ArtifactSpec {
        id: ArtifactId::new(),
        scope: task.scope.clone(),
        media_type: "application/json".into(),
        schema: "synthetic-action/1".into(),
        source: "fixture".into(),
        channel,
        retention: "history".into(),
        omissions: vec![],
    };
    let mut writer = store.spool().create(spec).unwrap();
    writer.write_chunk(b"synthetic retained input").unwrap();
    let artifact = writer.finalize().unwrap();
    drop(writer);
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![Mutation::Put {
                record: Record::typed(
                    Collection::Artifact,
                    artifact.spec.id.as_str(),
                    task.scope.workspace.clone(),
                    Revision::ZERO,
                    &artifact,
                )
                .unwrap(),
                expected: None,
            }],
            events: vec![],
            command: None,
        })
        .await
        .unwrap();
    artifact
}
pub(super) fn price(model: &str) -> PriceSnapshot {
    PriceSnapshot {
        id: "d".repeat(64),
        provider: "exact/provider-endpoint".into(),
        model: model.into(),
        currency: money(0).currency,
        capability: "e".repeat(64),
        valid_until: Timestamp::new(1000),
        rates: [
            ChargeCategory::Input,
            ChargeCategory::Output,
            ChargeCategory::CacheRead,
            ChargeCategory::CacheWrite,
            ChargeCategory::Request,
            ChargeCategory::ProviderTool,
        ]
        .into_iter()
        .map(|category| {
            (
                category,
                Rate {
                    micros: Micros::new(u64::from(category == ChargeCategory::Request) * 10),
                    per_units: Units::new(1),
                },
            )
        })
        .collect(),
    }
}
async fn reserve(
    store: &mut Store,
    access: &Access,
    task: &Task,
    previous: Option<AttemptId>,
    now: u64,
) -> Attempt {
    reserve_role(store, access, task, previous, RequestRole::Main, now).await
}

async fn reserve_role(
    store: &mut Store,
    access: &Access,
    task: &Task,
    previous: Option<AttemptId>,
    role: RequestRole,
    now: u64,
) -> Attempt {
    let request = capture(store, task, Channel::RequestBody).await;
    let ledger = vcp_budget::ledger(store.state(), &task.scope).unwrap();
    vcp_budget::reserve(
        store,
        vcp_budget::Admission {
            transaction: TransactionId::new(),
            attempt: AttemptId::new(),
            reservation: ReservationId::new(),
            scope: task.scope.clone(),
            agent: AgentId::new(),
            role,
            request: request.spec.id,
            request_digest: request.sha256,
            quote: vcp_budget::arithmetic::quote(
                price("exact-model"),
                Usage {
                    requests: Units::new(1),
                    ..Default::default()
                },
                Timestamp::new(now),
            )
            .unwrap(),
            previous,
            expected_ledger: ledger.revision,
            policy: ledger.policy,
            steering: task.steering,
            draw_protected: false,
            now: Timestamp::new(now),
        },
        &budget_actor(access, now),
    )
    .await
    .unwrap()
}
async fn settle(store: &mut Store, access: &Access, task: &Task, attempt: &Attempt, now: u64) {
    vcp_budget::submit(
        store,
        &attempt.id,
        &task.scope,
        attempt.revision,
        &budget_actor(access, now),
    )
    .await
    .unwrap();
    let raw = capture(store, task, Channel::Response).await;
    vcp_budget::observe(
        store,
        UsageObservation {
            id: ObservationId::new(),
            scope: task.scope.clone(),
            attempt: attempt.id.clone(),
            provider_request: format!("provider-{}", attempt.id),
            mode: UsageMode::Cumulative {
                version: Units::new(1),
            },
            amount: money(10),
            final_usage: true,
            raw: raw.spec.id,
            correction: None,
        },
        &budget_actor(access, now + 1),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn accounting_attempts_keep_exact_cohorts_and_retry_lineage_on_both_stores() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(temp.path(), backend).await;
        let task = create_task(&mut store, &access, TaskState::Running).await;
        vcp_budget::initialize(
            &mut store,
            task.scope.clone(),
            money(1000),
            Micros::ZERO,
            None,
            &budget_actor(&access, 3),
        )
        .await
        .unwrap();
        let first = reserve(&mut store, &access, &task, None, 10).await;
        settle(&mut store, &access, &task, &first, 11).await;
        let late_raw = capture(&mut store, &task, Channel::Response).await;
        let late = UsageObservation {
            id: ObservationId::new(),
            scope: task.scope.clone(),
            attempt: first.id.clone(),
            provider_request: format!("provider-{}", first.id),
            mode: UsageMode::Cumulative {
                version: Units::new(2),
            },
            amount: money(15),
            final_usage: false,
            raw: late_raw.spec.id,
            correction: None,
        };
        vcp_budget::observe(&mut store, late.clone(), &budget_actor(&access, 13))
            .await
            .unwrap();
        // Replaying the immutable provider receipt returns the same settlement
        // without appending a second charge event.
        vcp_budget::observe(&mut store, late, &budget_actor(&access, 13))
            .await
            .unwrap();
        let stale_raw = capture(&mut store, &task, Channel::Response).await;
        vcp_budget::observe(
            &mut store,
            UsageObservation {
                id: ObservationId::new(),
                scope: task.scope.clone(),
                attempt: first.id.clone(),
                provider_request: format!("provider-{}", first.id),
                mode: UsageMode::Cumulative {
                    version: Units::new(2),
                },
                amount: money(15),
                final_usage: false,
                raw: stale_raw.spec.id,
                correction: None,
            },
            &budget_actor(&access, 13),
        )
        .await
        .unwrap();
        let correction_raw = capture(&mut store, &task, Channel::Response).await;
        let policy = vcp_budget::ledger(store.state(), &task.scope)
            .unwrap()
            .policy;
        vcp_budget::observe(
            &mut store,
            UsageObservation {
                id: ObservationId::new(),
                scope: task.scope.clone(),
                attempt: first.id.clone(),
                provider_request: format!("provider-{}", first.id),
                mode: UsageMode::Cumulative {
                    version: Units::new(3),
                },
                amount: money(12),
                final_usage: true,
                raw: correction_raw.spec.id,
                correction: Some(Resolution {
                    actor: access.actor.clone(),
                    policy,
                    reason: "provider corrected cumulative usage".into(),
                    remaining_uncertainty: String::new(),
                }),
            },
            &budget_actor(&access, 14),
        )
        .await
        .unwrap();
        let second = reserve(&mut store, &access, &task, Some(first.id.clone()), 20).await;
        settle(&mut store, &access, &task, &second, 21).await;
        let decision_id = format!(
            "routing-decision-{}",
            digest_bytes(second.id.as_str().as_bytes())
        );
        let value = serde_json::json!({"routing_encoding":"object_v1","schema_version":1,
            "document_type":"vcp_routing_decision_v1","scope":task.scope,"task":task.scope.task,"root":task.root,
            "decision":{"input":{"workspace":access.workspace,"task":task.scope.task,"root":task.root,"task_class":"coding"},
                "selected":{"model":"exact-model","endpoint":"exact/provider-endpoint"}},
            "request_digest":second.request_digest,"attempt":second.id});
        let mut record = Record::typed(
            Collection::Projection,
            decision_id,
            access.workspace.clone(),
            Revision::ZERO,
            &value,
        )
        .unwrap();
        record.references.extend([
            key(Collection::Attempt, second.id.as_str()),
            key(Collection::Task, task.scope.task.as_str()),
        ]);
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![Mutation::Put {
                    record,
                    expected: None,
                }],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        let usage_events = store
            .state()
            .events
            .iter()
            .filter(|event| event.event.kind == EventKind::UsageReconciled)
            .collect::<Vec<_>>();
        assert_eq!(usage_events.len(), 5);
        for event in &usage_events {
            let reference = event.event.data["settlement"].as_object().unwrap();
            assert_eq!(reference.len(), 2);
            assert_eq!(reference["schema_version"], 1);
            assert!(reference["id"].as_str().is_some());
        }
        assert!(!serde_json::to_string(&usage_events)
            .unwrap()
            .contains("provider corrected cumulative usage"));
        let evidence = observe(&store, &access, window()).unwrap();
        assert_eq!(evidence.attempts.len(), 2);
        let initial = evidence
            .attempts
            .iter()
            .find(|trace| trace.attempt == first.id)
            .unwrap();
        assert_eq!(initial.cohort.retry_depth, Some(0));
        assert_eq!(initial.cohort.prior_attempts, Some(0));
        assert!(initial.charge.complete && !initial.charge.unknown_remainder);
        assert_eq!(initial.charge.currency, "USD");
        assert_eq!(initial.charge.charged_micros, Some(12));
        assert_eq!(initial.charge.liability_micros, Some(0));
        assert_eq!(initial.charge.final_charge_micros, Some(12));
        assert_eq!(initial.charge.settlements.len(), 4);
        assert_eq!(
            initial
                .charge
                .settlements
                .iter()
                .map(|charge| (
                    charge.direction,
                    charge.adjustment_micros,
                    charge.total_micros
                ))
                .collect::<Vec<_>>(),
            vec![
                (AdjustmentDirection::Debit, 10, 10),
                (AdjustmentDirection::Debit, 5, 15),
                (AdjustmentDirection::None, 0, 15),
                (AdjustmentDirection::Credit, 3, 12),
            ]
        );
        assert_eq!(
            initial
                .observations
                .iter()
                .map(|o| o.phase)
                .collect::<Vec<_>>(),
            vec![
                ReservationState::Created,
                ReservationState::Submitted,
                ReservationState::Settled,
                ReservationState::ReconciliationPending,
                ReservationState::ReconciliationPending,
                ReservationState::Settled
            ]
        );
        let retry = evidence
            .attempts
            .iter()
            .find(|trace| trace.attempt == second.id)
            .unwrap();
        assert_eq!(retry.cohort.retry_depth, Some(1));
        assert_eq!(retry.cohort.prior_attempts, Some(1));
        assert_eq!(retry.cohort.decomposition_depth, Some(0));
        assert_eq!(retry.cohort.task_class.as_deref(), Some("coding"));
        assert_eq!(retry.cohort.endpoint, "exact/provider-endpoint");
        assert_eq!(retry.cohort.model, "exact-model");
        assert!(retry.gaps.is_empty() && !retry.right_censored);
        assert_eq!(retry.charge.final_charge_micros, Some(10));
        let partial = observe(
            &store,
            &access,
            HistoryWindow {
                from: Some(Timestamp::new(11)),
                until: Timestamp::new(1000),
            },
        )
        .unwrap();
        let partial_first = partial
            .attempts
            .iter()
            .find(|trace| trace.attempt == first.id)
            .unwrap();
        assert!(!partial_first.charge.complete);
        assert!(partial_first.charge.unknown_remainder);
        assert_eq!(partial_first.charge.charged_micros, None);
        assert_eq!(partial_first.charge.final_charge_micros, None);
        assert!(partial
            .attempts
            .iter()
            .all(|trace| trace.cohort.prior_attempts.is_none()
                && trace.cohort.prior_failed_checks.is_none()
                && trace.cohort.retry_depth.is_none()));
        let scoped = Access {
            workspace: access.workspace.clone(),
            actor: access.actor.clone(),
            authority: access.authority,
            read: true,
            write: false,
            tasks: Some(BTreeSet::from([task.scope.task.clone()])),
        };
        assert_eq!(
            observe(&store, &scoped, window()).unwrap().attempts.len(),
            2
        );
        let denied = Access {
            workspace: access.workspace.clone(),
            actor: access.actor.clone(),
            authority: access.authority,
            read: false,
            write: false,
            tasks: None,
        };
        assert!(observe(&store, &denied, window()).is_err());
        let mut oversized_tasks = (0..6000)
            .map(|index| TaskId::parse(format!("scope-{index:090}")).unwrap())
            .collect::<BTreeSet<_>>();
        oversized_tasks.insert(task.scope.task.clone());
        let oversized = Access {
            workspace: access.workspace.clone(),
            actor: access.actor.clone(),
            authority: access.authority,
            read: true,
            write: false,
            tasks: Some(oversized_tasks),
        };
        assert!(observe(&store, &oversized, window())
            .unwrap_err()
            .contains("512 KiB"));
        drop(store);
        let reopened = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(observe(&reopened, &access, window()).unwrap(), evidence);
    }
}

#[tokio::test]
async fn unknown_liability_and_no_send_release_never_become_free_visits() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(temp.path(), backend).await;
        let task = create_task(&mut store, &access, TaskState::Running).await;
        vcp_budget::initialize(
            &mut store,
            task.scope.clone(),
            money(1000),
            Micros::ZERO,
            None,
            &budget_actor(&access, 3),
        )
        .await
        .unwrap();
        let uncertain = reserve(&mut store, &access, &task, None, 10).await;
        vcp_budget::submit(
            &mut store,
            &uncertain.id,
            &task.scope,
            uncertain.revision,
            &budget_actor(&access, 11),
        )
        .await
        .unwrap();
        vcp_budget::hold_uncertain(
            &mut store,
            &uncertain.id,
            &task.scope,
            &budget_actor(&access, 12),
            "provider outcome is unavailable",
        )
        .await
        .unwrap();
        let released = reserve(&mut store, &access, &task, None, 20).await;
        vcp_budget::release_before_send(
            &mut store,
            &released.id,
            &task.scope,
            &budget_actor(&access, 21),
        )
        .await
        .unwrap();
        let legacy = reserve(&mut store, &access, &task, None, 30).await;
        vcp_budget::submit(
            &mut store,
            &legacy.id,
            &task.scope,
            legacy.revision,
            &budget_actor(&access, 31),
        )
        .await
        .unwrap();
        let raw = capture(&mut store, &task, Channel::Response).await;
        vcp_budget::observe(
            &mut LegacyAccounting(&mut store),
            UsageObservation {
                id: ObservationId::new(),
                scope: task.scope.clone(),
                attempt: legacy.id.clone(),
                provider_request: format!("provider-{}", legacy.id),
                mode: UsageMode::Cumulative {
                    version: Units::new(1),
                },
                amount: money(10),
                final_usage: true,
                raw: raw.spec.id,
                correction: None,
            },
            &budget_actor(&access, 32),
        )
        .await
        .unwrap();
        let supporting = reserve_role(
            &mut store,
            &access,
            &task,
            None,
            RequestRole::Verification,
            40,
        )
        .await;
        settle(&mut store, &access, &task, &supporting, 41).await;

        let evidence = observe(&store, &access, window()).unwrap();
        let uncertain = evidence
            .attempts
            .iter()
            .find(|trace| trace.attempt == uncertain.id)
            .unwrap();
        assert!(uncertain.charge.complete && uncertain.charge.unknown_remainder);
        assert_eq!(uncertain.charge.charged_micros, Some(0));
        assert_eq!(uncertain.charge.liability_micros, Some(10));
        assert_eq!(uncertain.charge.final_charge_micros, None);
        assert!(uncertain.charge.settlements.is_empty());
        let released = evidence
            .attempts
            .iter()
            .find(|trace| trace.attempt == released.id)
            .unwrap();
        assert!(released.charge.complete && !released.charge.unknown_remainder);
        assert_eq!(released.charge.charged_micros, Some(0));
        assert_eq!(released.charge.liability_micros, Some(0));
        assert_eq!(released.charge.final_charge_micros, Some(0));
        let legacy = evidence
            .attempts
            .iter()
            .find(|trace| trace.attempt == legacy.id)
            .unwrap();
        assert!(!legacy.charge.complete);
        assert_eq!(legacy.charge.charged_micros, None);
        assert_eq!(legacy.charge.final_charge_micros, None);
        assert!(legacy.charge.settlements.is_empty());
        let supporting = evidence
            .attempts
            .iter()
            .find(|trace| trace.attempt == supporting.id)
            .unwrap();
        assert_eq!(supporting.cohort.role, RequestRole::Verification);
        assert_eq!(supporting.charge.final_charge_micros, Some(10));
        let mapped = rewards::map(&store, &access, window()).unwrap();
        assert_eq!(mapped.attempts, 4);
        assert_eq!(mapped.exact_attempts, 2);
        assert_eq!(mapped.unknown_attempts, 2);
        let rewards::Status::Mapped { cells } = mapped.status else {
            panic!("attempt rewards must map");
        };
        assert!(cells.iter().any(|cell| {
            cell.cohort.role == RequestRole::Verification && cell.point_estimate_micros == Some(10)
        }));
        assert!(cells.iter().any(|cell| {
            cell.cohort.role == RequestRole::Main
                && cell.exact_attempts == 1
                && cell.exact_charge_sum_micros == 0
                && cell.point_estimate_micros == Some(0)
        }));
    }
}

#[tokio::test]
async fn reward_mapping_withholds_cohort_mean_when_any_attempt_cost_is_unknown() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(temp.path(), backend).await;
        let mut tasks = Vec::new();
        for name in ["reward-task-a", "reward-task-b", "reward-task-c"] {
            let task = create_named_task(&mut store, &access, TaskState::Running, name).await;
            vcp_budget::initialize(
                &mut store,
                task.scope.clone(),
                money(1000),
                Micros::ZERO,
                None,
                &budget_actor(&access, 3),
            )
            .await
            .unwrap();
            tasks.push(task);
        }
        for (index, task) in tasks[..2].iter().enumerate() {
            let attempt = reserve(&mut store, &access, task, None, 10 + index as u64 * 10).await;
            settle(&mut store, &access, task, &attempt, 11 + index as u64 * 10).await;
        }
        let complete = rewards::map(&store, &access, window()).unwrap();
        let rewards::Status::Mapped { cells } = &complete.status else {
            panic!("settled attempts must map");
        };
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].attempts, 2);
        assert_eq!(cells[0].exact_attempts, 2);
        assert_eq!(cells[0].exact_charge_sum_micros, 20);
        assert_eq!(cells[0].exact_min_charge_micros, Some(10));
        assert_eq!(cells[0].exact_max_charge_micros, Some(10));
        assert_eq!(cells[0].point_estimate_micros, Some(10));
        assert!(!cells[0].unknown_remainder);
        let selection = consumption::RewardSelection {
            cohort: cells[0].cohort.clone(),
            currency: cells[0].currency.clone(),
        };
        let consumer_decision = CommandId::new();
        let receipt = consumption::consume_reward(
            &mut store,
            &access,
            consumer_decision.clone(),
            &complete,
            selection.clone(),
            Timestamp::new(25),
        )
        .await
        .unwrap();
        assert_eq!(receipt.consumed_micros, 10);
        assert_eq!(receipt.currency, "USD");
        assert_eq!(receipt.source_attempts, 2);
        assert_eq!(receipt.source_tasks.len(), 2);
        assert_eq!(receipt.selection, selection);
        assert_eq!(receipt.input_digest.len(), 64);
        assert_eq!(receipt.source_attempts_digest.len(), 64);
        assert!(receipt.historical_replay_only && !receipt.serving_qualified);
        let record = store
            .state()
            .records
            .get(&key(Collection::Projection, &receipt.id))
            .unwrap();
        assert_eq!(record.references.len(), 6);

        let uncertain = reserve(&mut store, &access, &tasks[2], None, 30).await;
        vcp_budget::submit(
            &mut store,
            &uncertain.id,
            &tasks[2].scope,
            uncertain.revision,
            &budget_actor(&access, 31),
        )
        .await
        .unwrap();
        vcp_budget::hold_uncertain(
            &mut store,
            &uncertain.id,
            &tasks[2].scope,
            &budget_actor(&access, 32),
            "provider outcome unavailable",
        )
        .await
        .unwrap();
        let before = store.state().clone();
        let mapped = rewards::map(&store, &access, window()).unwrap();
        assert_eq!(mapped.attempts, 3);
        assert_eq!(mapped.exact_attempts, 2);
        assert_eq!(mapped.unknown_attempts, 1);
        assert_eq!(mapped.source_digest.len(), 64);
        assert_eq!(mapped.source_gaps, 0);
        assert!(mapped.policy.is_none() && mapped.catalog.is_none());
        assert!(mapped.qualification.is_none() && !mapped.serving_qualified);
        let rewards::Status::Mapped { cells } = &mapped.status else {
            panic!("attempt rewards must map");
        };
        assert_eq!(cells.len(), 1);
        let cell = &cells[0];
        assert_eq!(cell.currency, "USD");
        assert_eq!(cell.cohort.role, RequestRole::Main);
        assert_eq!(cell.cohort.model, "exact-model");
        assert_eq!(cell.cohort.endpoint, "exact/provider-endpoint");
        assert_eq!(cell.cohort.retry_depth, Some(0));
        assert_eq!(cell.cohort.prior_attempts, Some(0));
        assert_eq!(cell.attempts, 3);
        assert_eq!(cell.exact_attempts, 2);
        assert_eq!(cell.unknown_attempts, 1);
        assert_eq!(cell.exact_charge_sum_micros, 20);
        assert_eq!(cell.exact_min_charge_micros, Some(10));
        assert_eq!(cell.exact_max_charge_micros, Some(10));
        assert_eq!(cell.observed_charged_attempts, 3);
        assert_eq!(cell.observed_charged_sum_micros, 20);
        assert_eq!(cell.observed_liability_attempts, 3);
        assert_eq!(cell.observed_liability_sum_micros, 10);
        assert_eq!(cell.point_estimate_micros, None);
        assert!(cell.unknown_remainder);
        assert_eq!(
            consumption::replay_reward(&store, &access, &consumer_decision).unwrap(),
            receipt
        );
        assert_eq!(
            consumption::consume_reward(
                &mut store,
                &access,
                consumer_decision.clone(),
                &complete,
                selection.clone(),
                Timestamp::new(90),
            )
            .await
            .unwrap(),
            receipt
        );
        let mut conflicting_selection = receipt.selection.clone();
        conflicting_selection.currency = "EUR".into();
        assert!(consumption::consume_reward(
            &mut store,
            &access,
            consumer_decision.clone(),
            &complete,
            conflicting_selection,
            Timestamp::new(90),
        )
        .await
        .unwrap_err()
        .contains("reused"));
        assert!(consumption::consume_reward(
            &mut store,
            &access,
            CommandId::new(),
            &complete,
            selection.clone(),
            Timestamp::new(91),
        )
        .await
        .unwrap_err()
        .contains("changed"));
        assert!(consumption::consume_reward(
            &mut store,
            &access,
            CommandId::new(),
            &mapped,
            selection,
            Timestamp::new(92),
        )
        .await
        .unwrap_err()
        .contains("unknown remainder"));
        let denied = Access {
            workspace: access.workspace.clone(),
            actor: access.actor.clone(),
            authority: access.authority,
            read: true,
            write: false,
            tasks: Some(BTreeSet::from([tasks[0].scope.task.clone()])),
        };
        assert!(consumption::replay_reward(&store, &denied, &consumer_decision).is_err());
        assert_eq!(rewards::map(&store, &access, window()).unwrap(), mapped);
        assert_eq!(store.state(), &before);
        assert!(matches!(
            rewards::map(
                &store,
                &access,
                HistoryWindow {
                    from: Some(Timestamp::new(100)),
                    until: Timestamp::new(200),
                },
            )
            .unwrap()
            .status,
            rewards::Status::Abstained {
                reason: rewards::Abstention::NoAttempts
            }
        ));
        drop(store);
        let reopened = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(rewards::map(&reopened, &access, window()).unwrap(), mapped);
        assert_eq!(
            consumption::replay_reward(&reopened, &access, &consumer_decision).unwrap(),
            receipt
        );
    }
}

#[tokio::test]
async fn old_verification_and_partial_attempt_windows_abstain_from_missing_identity_and_counters() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(temp.path(), backend).await;
        let task = create_task(&mut store, &access, TaskState::Running).await;
        let check_output = capture(&mut store, &task, Channel::Stdout).await;
        let id = VerificationId::new();
        let verification = Verification {
            redaction: None,
            id: id.clone(),
            scope: task.scope.clone(),
            steering: task.steering,
            fingerprint: task.fingerprint.clone(),
            outputs: vec![],
            checks: vec![Check {
                specification: "same check".into(),
                outcome: CheckOutcome::Failed {
                    reason: "same failure".into(),
                },
                output: check_output.spec.id,
                exit_code: Some(1),
            }],
            unresolved_effects: vec![],
            outstanding_issues: vec![],
            cost: CostCertainty::Uncertain {
                attempts: vec![],
                reason: "unknown cost".into(),
            },
        };
        let event = EventId::new();
        store.transact(Transaction { id: TransactionId::new(), expected_watermark: store.state().watermark,
            mutations: vec![Mutation::Put { record: Record::typed(Collection::Verification, id.as_str(), access.workspace.clone(),
                Revision::ZERO, &verification).unwrap(), expected: None }],
            events: vec![EventInput { id: event, workspace: access.workspace.clone(), session: task.scope.session.clone(),
                task: Some(task.scope.task.clone()), actor: access.actor.clone(), correlation: CommandId::new(), causation: None,
                timestamp: Timestamp::new(30), kind: EventKind::VerificationRecorded, artifacts: vec![],
                data: serde_json::json!({"schema_version":1,"facts":[{"collection":"verification","id":id,
                    "revision":Revision::ZERO,"value":verification}]}), metadata: None }], command: None }).await.unwrap();
        let evidence = observe(&store, &access, window()).unwrap();
        assert_eq!(evidence.verifications[0].observed_task_revision, None);
        assert_eq!(evidence.verifications[0].checks[0].failure_signature, None);
        assert!(!evidence.verifications[0].cost_known);
        let value = routing::execute(
            &mut store,
            &access,
            routing::Request::Observations {
                from: Some(Timestamp::new(25)),
                until: Timestamp::new(100),
            },
            None,
            Timestamp::new(100),
        )
        .await
        .unwrap();
        let partial: Evidence = serde_json::from_value(value).unwrap();
        assert!(partial.attempts.is_empty());
        assert_eq!(partial.verifications.len(), 1);
        assert!(observe(
            &store,
            &access,
            HistoryWindow {
                from: Some(Timestamp::new(30)),
                until: Timestamp::new(30)
            }
        )
        .is_err());
    }
}

#[tokio::test]
async fn first_order_fit_records_source_parameters_and_abstention_without_persistence() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (store, access) = setup(temp.path(), backend).await;
        let mut store = store;
        let task = create_task(&mut store, &access, TaskState::Running).await;
        let mut engine = vcp_engine::Engine::new(store).unwrap();
        let actor = engine_access(&access, &task);
        engine
            .handle(
                command(
                    &engine,
                    &access,
                    &task,
                    Revision::ZERO,
                    Command::Transition {
                        next: TaskState::Failed,
                        reason: "terminal fit fixture".into(),
                        verification: None,
                    },
                ),
                &actor,
                &vcp_engine::HostFacts::inspect(Timestamp::new(10)),
            )
            .await
            .unwrap();
        let store = engine.into_store();
        let before = store.state().clone();
        let fitted = fits::fit(&store, &access, window(), 1_000, 1).unwrap();
        assert_eq!(fitted.alphabet, vec![TaskState::Running, TaskState::Failed]);
        assert_eq!(fitted.row_samples, vec![1, 0]);
        assert!(matches!(
            &fitted.status,
            fits::FitStatus::Fitted { probabilities }
                if probabilities == &vec![vec![0.0, 1.0], vec![0.0, 1.0]]
        ));
        assert_eq!(fitted.prior_basis_points, 1_000);
        assert_eq!(fitted.minimum_samples, 1);
        assert_eq!(fitted.source_digest.len(), 64);
        assert_eq!(fitted.source_gaps, 0);
        assert_eq!(fitted.left_censored_traces, 1);
        assert_eq!(fitted.right_censored_traces, 0);
        assert_eq!(fitted.features, vec!["task_state"]);
        assert_eq!(fitted.cohort, "authorized-task-state-scope/1");
        assert!(fitted.task_class.is_none() && fitted.endpoint.is_none());
        assert!(fitted.policy.is_none() && fitted.catalog.is_none());
        assert!(fitted.qualification.is_none() && !fitted.serving_qualified);
        assert_eq!(
            fits::fit(&store, &access, window(), 1_000, 1).unwrap(),
            fitted
        );
        assert_eq!(store.state(), &before);

        let sparse = fits::fit(&store, &access, window(), 1_000, 2).unwrap();
        assert!(matches!(
            sparse.status,
            fits::FitStatus::Abstained {
                reason: fits::Abstention::Sparse
            }
        ));
        let empty = fits::fit(
            &store,
            &access,
            HistoryWindow {
                from: Some(Timestamp::new(30)),
                until: Timestamp::new(40),
            },
            0,
            1,
        )
        .unwrap();
        assert!(matches!(
            empty.status,
            fits::FitStatus::Abstained {
                reason: fits::Abstention::NoTransitions
            }
        ));
        assert!(fits::fit(&store, &access, window(), 10_001, 1).is_err());
    }
}

#[tokio::test]
async fn heldout_order_comparison_uses_stable_task_partitions_on_both_stores() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(temp.path(), backend).await;
        let mut training_names = Vec::new();
        let mut heldout_names = Vec::new();
        for index in 0..100 {
            let name = format!("comparison-task-{index}");
            let digest = digest_bytes(name.as_bytes());
            let prefix = u64::from_str_radix(&digest[..16], 16).unwrap();
            if prefix % 2 == 0 && heldout_names.len() < 2 {
                heldout_names.push(name);
            } else if prefix % 2 == 1 && training_names.len() < 2 {
                training_names.push(name);
            }
            if training_names.len() == 2 && heldout_names.len() == 2 {
                break;
            }
        }
        assert_eq!((training_names.len(), heldout_names.len()), (2, 2));
        let mut tasks = Vec::new();
        for name in training_names.iter().chain(&heldout_names) {
            tasks.push(create_named_task(&mut store, &access, TaskState::Running, name).await);
        }
        let mut engine = vcp_engine::Engine::new(store).unwrap();
        for task in &tasks {
            let actor = engine_access(&access, task);
            engine
                .handle(
                    command(
                        &engine,
                        &access,
                        task,
                        Revision::ZERO,
                        Command::Transition {
                            next: TaskState::Blocked,
                            reason: "comparison fixture intermediate".into(),
                            verification: None,
                        },
                    ),
                    &actor,
                    &vcp_engine::HostFacts::inspect(Timestamp::new(10)),
                )
                .await
                .unwrap();
            engine
                .handle(
                    command(
                        &engine,
                        &access,
                        task,
                        Revision::new(1),
                        Command::Transition {
                            next: TaskState::Failed,
                            reason: "comparison fixture terminal".into(),
                            verification: None,
                        },
                    ),
                    &actor,
                    &vcp_engine::HostFacts::inspect(Timestamp::new(20)),
                )
                .await
                .unwrap();
        }
        let store = engine.into_store();
        let before = store.state().clone();
        let comparison = fits::compare(&store, &access, window(), 0, 1, 2, 0).unwrap();
        assert_eq!(comparison.training_tasks, 2);
        assert_eq!(comparison.heldout_tasks, 2);
        assert_eq!(comparison.training_segments, 2);
        assert_eq!(comparison.heldout_segments, 2);
        assert_eq!(comparison.partition_digest.len(), 64);
        assert_eq!(comparison.partition_method, "task-identity-digest-modulo/1");
        assert_eq!(comparison.left_censored_traces, 4);
        assert_eq!(comparison.right_censored_traces, 0);
        assert_eq!(comparison.excluded_pruned_tasks, 0);
        assert_eq!(comparison.features, vec!["task_state"]);
        assert_eq!(comparison.cohort, "authorized-task-state-scope/1");
        assert!(comparison.task_class.is_none() && comparison.endpoint.is_none());
        assert!(comparison.policy.is_none() && comparison.catalog.is_none());
        assert!(comparison.qualification.is_none() && !comparison.serving_qualified);
        assert!(matches!(
            &comparison.status,
            fits::ComparisonStatus::Compared { result }
                if result.heldout_predictions == 2
                    && result.selected_order == 1
                    && result.first_order_two_step_max_abs_error == 0.0
        ));
        assert_eq!(
            fits::compare(&store, &access, window(), 0, 1, 2, 0).unwrap(),
            comparison
        );
        assert_eq!(store.state(), &before);
        assert!(matches!(
            fits::compare(&store, &access, window(), 0, 3, 2, 0)
                .unwrap()
                .status,
            fits::ComparisonStatus::Abstained {
                reason: fits::ComparisonAbstention::Sparse
            }
        ));
        assert!(fits::compare(&store, &access, window(), 0, 1, 1, 0).is_err());
        assert!(fits::compare(&store, &access, window(), 0, 1, 2, 2).is_err());
    }
}

#[tokio::test]
async fn logical_event_purge_removes_action_observations_before_cleanup() {
    use vcp_domain::retention_selector::{Criterion, Selector, Tree};
    use vcp_memory::retention::{self, Action};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(temp.path(), backend).await;
        let task = create_task(&mut store, &access, TaskState::Running).await;
        let trigger = capture(&mut store, &task, Channel::Evidence).await;
        let mut engine = vcp_engine::Engine::new(store).unwrap();
        let actor = engine_access(&access, &task);
        let host = vcp_engine::HostFacts::inspect(Timestamp::new(10));
        let turn = TurnId::new();
        engine
            .handle(
                command(
                    &engine,
                    &access,
                    &task,
                    Revision::ZERO,
                    Command::StartTurn {
                        id: turn.clone(),
                        trigger: trigger.spec.id,
                    },
                ),
                &actor,
                &host,
            )
            .await
            .unwrap();
        engine
            .handle(
                command(
                    &engine,
                    &access,
                    &task,
                    Revision::ZERO,
                    Command::AdvanceTurn {
                        id: turn,
                        next: TurnState::Failed,
                        reason: "terminal turn fixture before retention".into(),
                    },
                ),
                &actor,
                &host,
            )
            .await
            .unwrap();
        engine
            .handle(
                command(
                    &engine,
                    &access,
                    &task,
                    Revision::ZERO,
                    Command::Transition {
                        next: TaskState::Cancelled,
                        reason: "terminal fixture before retention".into(),
                        verification: None,
                    },
                ),
                &actor,
                &host,
            )
            .await
            .unwrap();
        let mut store = engine.into_store();
        assert_eq!(observe(&store, &access, window()).unwrap().turns.len(), 1);
        let plan = retention::preview(
            &store,
            &access,
            Selector {
                schema_version: 1,
                tree: Tree::Match(Criterion::Event("turn_transition".into())),
            },
            Action::Purge,
            Timestamp::new(1001),
        )
        .unwrap();
        assert!(plan
            .selected
            .iter()
            .any(|target| matches!(target, retention::Target::Event(_))));
        retention::apply(&mut store, &access, &plan, Timestamp::new(1001))
            .await
            .unwrap();
        let masked = observe(&store, &access, window()).unwrap();
        assert!(masked.turns.is_empty());
        assert_eq!(masked.gaps.len(), 2);
        assert!(masked
            .gaps
            .iter()
            .all(|gap| gap.reason == GapReason::RedactedEvent));
        drop(store);
        let reopened = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(observe(&reopened, &access, window()).unwrap(), masked);
    }
}
