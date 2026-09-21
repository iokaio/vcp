// SPDX-License-Identifier: Apache-2.0
use super::action_evidence::{
    budget_actor, capture, command, create_named_task, engine_access, money, reserve_role, settle,
};
use super::*;
use vcp_domain::{
    accounting::*,
    artifact::Channel,
    task::{Task, TaskState},
    verification::{Check, CheckOutcome, CostCertainty, Verification},
};
use vcp_lifecycle::foundation::routing_state::forecasts::{self, ForecastStatus, State};
use vcp_models::routing as r;
use vcp_protocol::command::Command;

fn window() -> HistoryWindow {
    HistoryWindow {
        from: None,
        until: Timestamp::new(1000),
    }
}

fn copy_access(access: &Access) -> Access {
    Access {
        workspace: access.workspace.clone(),
        actor: access.actor.clone(),
        authority: access.authority,
        read: access.read,
        write: access.write,
        tasks: access.tasks.clone(),
    }
}

async fn transition(
    engine: &mut vcp_engine::Engine<Store>,
    access: &Access,
    task: &Task,
    next: TaskState,
    verification: Option<VerificationId>,
    now: u64,
) -> Task {
    let mut host = vcp_engine::HostFacts::inspect(Timestamp::new(now));
    host.may_execute = true;
    engine
        .handle(
            command(
                engine,
                access,
                task,
                task.revision,
                Command::Transition {
                    next,
                    reason: "forecast fixture transition".into(),
                    verification,
                },
            ),
            &engine_access(access, task),
            &host,
        )
        .await
        .unwrap();
    engine
        .store()
        .state()
        .record(
            Collection::Task,
            task.scope.task.as_str(),
            &access.workspace,
        )
        .unwrap()
        .decode()
        .unwrap()
}

async fn route(store: &mut Store, access: &Access, task: &Task, attempt: &Attempt, now: u64) {
    let selected = r::ModelEndpoint {
        model: attempt.quote.price.model.clone(),
        endpoint: attempt.quote.price.provider.clone(),
    };
    let mut decision = r::RoutingDecision {
        schema_version: 1,
        id: String::new(),
        input: r::RoutingInput {
            workspace: access.workspace.clone(),
            root: task.root.clone(),
            task: task.scope.task.clone(),
            input_revision: task.revision,
            steering: task.steering,
            input_digest: "c".repeat(64),
            catalog: "d".repeat(64),
            policy: "e".repeat(64),
            role: attempt.role,
            task_class: "synthetic".into(),
            now: Timestamp::new(now),
            required_capabilities: BTreeSet::new(),
            excluded: BTreeSet::new(),
            retry_pin: None,
            input_tokens: Units::new(1),
            output_tokens: Units::new(1),
            available: money(1000),
            protected_verification: Micros::ZERO,
            estimates: vec![],
        },
        profile: Profile::Low,
        ordering: policy().ordering,
        candidates: vec![r::CandidateDecision {
            identity: selected.clone(),
            exclusions: vec![],
            source_reasons: vec![],
            group: Some(Group::Low),
            quality_bps: Some(9000),
            samples: Some(20),
            latency_p95_ms: Some(10),
            total_estimate: None,
            evidence_refs: vec![],
            broader_cohort_used: None,
            assumptions: vec!["canonical persistence fixture; not serving qualification".into()],
        }],
        selected: Some(selected),
        fallback_from: None,
        immediate_reservation: None,
    };
    decision.id = decision.digest().unwrap();
    record_decision(
        store,
        access,
        &task.scope,
        &decision,
        &attempt.request_digest,
        attempt.id.clone(),
        Timestamp::new(now),
    )
    .await
    .unwrap();
}

async fn episode(
    store: Store,
    access: &Access,
    index: usize,
    unknown: bool,
    terminal: Option<TaskState>,
) -> Store {
    let mut store = store;
    let task = create_named_task(
        &mut store,
        access,
        TaskState::Pending,
        &format!("forecast-{index}"),
    )
    .await;
    let mut engine = vcp_engine::Engine::new(store).unwrap();
    let task = transition(&mut engine, access, &task, TaskState::Running, None, 4).await;
    vcp_budget::initialize(
        engine.store_mut(),
        task.scope.clone(),
        money(1000),
        Micros::ZERO,
        None,
        &budget_actor(access, 5),
    )
    .await
    .unwrap();
    let mut previous = None;
    for (role, now) in [
        (RequestRole::Main, 10),
        (RequestRole::Main, 20),
        (RequestRole::Helper, 30),
    ] {
        let attempt = reserve_role(
            engine.store_mut(),
            access,
            &task,
            if role == RequestRole::Main {
                previous.clone()
            } else {
                None
            },
            role,
            now,
        )
        .await;
        route(engine.store_mut(), access, &task, &attempt, now).await;
        if unknown && role == RequestRole::Helper {
            vcp_budget::submit(
                engine.store_mut(),
                &attempt.id,
                &task.scope,
                attempt.revision,
                &budget_actor(access, now + 1),
            )
            .await
            .unwrap();
            vcp_budget::hold_uncertain(
                engine.store_mut(),
                &attempt.id,
                &task.scope,
                &budget_actor(access, now + 2),
                "observed unknown provider charge",
            )
            .await
            .unwrap();
        } else {
            settle(engine.store_mut(), access, &task, &attempt, now + 1).await;
        }
        previous = Some(attempt.id);
    }
    let output = capture(engine.store_mut(), &task, Channel::Stdout).await;
    let verification = Verification {
        redaction: None,
        id: VerificationId::new(),
        scope: task.scope.clone(),
        steering: task.steering,
        fingerprint: task.fingerprint.clone(),
        outputs: vec![output.spec.id.clone()],
        checks: vec![Check {
            specification: task.required_checks[0].clone(),
            outcome: CheckOutcome::Passed,
            output: output.spec.id,
            exit_code: Some(0),
        }],
        unresolved_effects: vec![],
        outstanding_issues: vec![],
        cost: if unknown {
            CostCertainty::Uncertain {
                attempts: vec![previous.unwrap()],
                reason: "provider charge pending reconciliation".into(),
            }
        } else {
            CostCertainty::Known
        },
    };
    let verification_id = verification.id.clone();
    engine
        .handle(
            command(
                &engine,
                access,
                &task,
                task.revision,
                Command::RecordVerification { verification },
            ),
            &engine_access(access, &task),
            &vcp_engine::HostFacts::inspect(Timestamp::new(40)),
        )
        .await
        .unwrap();
    if let Some(terminal) = terminal {
        transition(
            &mut engine,
            access,
            &task,
            terminal,
            Some(verification_id),
            50,
        )
        .await;
    }
    engine.into_store()
}

async fn cohort(root: &std::path::Path, backend: BackendKind, unknown: bool) -> (Store, Access) {
    let (mut store, access) = setup(root, backend).await;
    for index in 0..20 {
        let terminal = if index < 10 {
            TaskState::Completed
        } else if index < 15 {
            TaskState::Failed
        } else {
            TaskState::Cancelled
        };
        store = episode(
            store,
            &access,
            index,
            unknown && index == 19,
            Some(terminal),
        )
        .await;
    }
    (store, access)
}

#[tokio::test]
async fn action_forecasts_match_hand_calculated_serial_costs_and_outcomes_without_writes() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (store, access) = cohort(temp.path(), backend, false).await;
        let before = store.state().clone();
        let mut read_only = copy_access(&access);
        read_only.write = false;
        let report = forecasts::observe(&store, &read_only, window()).unwrap();
        assert_eq!(store.state(), &before);
        assert!(!report.serving_qualified);
        assert!(report.excluded.is_empty(), "{:?}", report.excluded);
        assert_eq!(report.episodes.len(), 20);
        assert_eq!(report.cohorts.len(), 1);
        let cohort = &report.cohorts[0];
        assert_eq!(cohort.known_cost_micros, 600);
        assert_eq!(cohort.unknown_attempts, 0);
        let ForecastStatus::Forecast {
            transient_states,
            absorbing_states,
            expected_visits,
            outcome_probabilities,
            expected_cost_micros,
            ..
        } = &cohort.status
        else {
            panic!("{:?}", cohort.status);
        };
        let main = transient_states
            .iter()
            .position(|state| *state == State::Main)
            .unwrap();
        for state in [
            State::Main,
            State::Retry,
            State::Support,
            State::Verification,
        ] {
            let index = transient_states
                .iter()
                .position(|found| *found == state)
                .unwrap();
            assert!((expected_visits[main][index] - 1.0).abs() < 1e-9);
        }
        assert!((expected_cost_micros[main].unwrap() - 30.0).abs() < 1e-9);
        for (state, probability) in [
            (State::Completed, 0.5),
            (State::Failed, 0.25),
            (State::Cancelled, 0.25),
        ] {
            let index = absorbing_states
                .iter()
                .position(|found| *found == state)
                .unwrap();
            assert!((outcome_probabilities[main][index] - probability).abs() < 1e-9);
        }
        assert_eq!(cohort.identity.task_class, "synthetic");
        assert_eq!(cohort.identity.catalog, "d".repeat(64));
        assert_eq!(cohort.identity.policy, "e".repeat(64));
        assert_eq!(cohort.identity.currency, "USD");
        assert_eq!(cohort.identity.endpoints.len(), 2);
        for endpoint in &cohort.identity.endpoints {
            assert_eq!(endpoint.model, "exact-model");
            assert_eq!(endpoint.endpoint, "exact/provider-endpoint");
        }
        drop(store);
        let mut reopened = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(
            serde_json::to_value(forecasts::observe(&reopened, &access, window()).unwrap())
                .unwrap(),
            serde_json::to_value(report).unwrap()
        );
        // Late provider accounting changes cost, never the already completed action interval.
        let task: Task = reopened
            .state()
            .record(Collection::Task, "forecast-0", &access.workspace)
            .unwrap()
            .decode()
            .unwrap();
        let attempt: Attempt = reopened
            .state()
            .records
            .values()
            .filter(|record| record.collection == Collection::Attempt)
            .map(|record| record.decode::<Attempt>().unwrap())
            .find(|attempt| {
                attempt.scope.task == task.scope.task
                    && attempt.role == RequestRole::Main
                    && attempt.previous.is_none()
            })
            .unwrap();
        let raw = capture(&mut reopened, &task, Channel::Response).await;
        vcp_budget::observe(
            &mut reopened,
            UsageObservation {
                id: ObservationId::new(),
                scope: task.scope.clone(),
                attempt: attempt.id.clone(),
                provider_request: format!("provider-{}", attempt.id),
                mode: UsageMode::Cumulative {
                    version: Units::new(2),
                },
                amount: money(15),
                final_usage: true,
                raw: raw.spec.id,
                correction: None,
            },
            &budget_actor(&access, 60),
        )
        .await
        .unwrap();
        let corrected = forecasts::observe(&reopened, &access, window()).unwrap();
        assert_eq!(corrected.episodes.len(), 20);
        assert!(corrected.excluded.is_empty(), "{:?}", corrected.excluded);
        assert_eq!(corrected.cohorts[0].known_cost_micros, 605);
        let ForecastStatus::Forecast {
            transient_states,
            expected_cost_micros,
            ..
        } = &corrected.cohorts[0].status
        else {
            panic!("{:?}", corrected.cohorts[0].status);
        };
        let main = transient_states
            .iter()
            .position(|state| *state == State::Main)
            .unwrap();
        assert!((expected_cost_micros[main].unwrap() - 30.25).abs() < 1e-9);
        let mut denied = copy_access(&access);
        denied.read = false;
        assert!(forecasts::observe(&reopened, &denied, window()).is_err());
        let mut restricted = copy_access(&access);
        restricted.tasks = Some(BTreeSet::from([TaskId::parse("forecast-0").unwrap()]));
        let scoped = forecasts::observe(&reopened, &restricted, window()).unwrap();
        assert_eq!(scoped.episodes.len(), 1);
        assert!(matches!(
            scoped.cohorts[0].status,
            ForecastStatus::Abstained { .. }
        ));
        let partial = forecasts::observe(
            &reopened,
            &access,
            HistoryWindow {
                from: Some(Timestamp::new(15)),
                until: Timestamp::new(1000),
            },
        )
        .unwrap();
        assert!(partial.episodes.is_empty());
        assert!(partial
            .cohorts
            .iter()
            .all(|cohort| matches!(cohort.status, ForecastStatus::Abstained { .. })));
        let censored = forecasts::observe(
            &reopened,
            &access,
            HistoryWindow {
                from: None,
                until: Timestamp::new(45),
            },
        )
        .unwrap();
        assert!(censored.episodes.is_empty());
        assert!(!censored.excluded.is_empty());
        use vcp_domain::retention_selector::{Criterion, Selector, Tree};
        use vcp_memory::retention::{self, Action};
        let mut reopened = reopened;
        let plan = retention::preview(
            &reopened,
            &access,
            Selector {
                schema_version: 1,
                tree: Tree::Match(Criterion::Event("task_transition".into())),
            },
            Action::Purge,
            Timestamp::new(1001),
        )
        .unwrap();
        assert!(plan.protected.is_empty());
        retention::apply(&mut reopened, &access, &plan, Timestamp::new(1001))
            .await
            .unwrap();
        let before = reopened.state().clone();
        let purged = forecasts::observe(&reopened, &access, window()).unwrap();
        assert!(purged.episodes.is_empty());
        assert_eq!(reopened.state(), &before);
    }
}

#[tokio::test]
async fn action_forecasts_preserve_unknown_charge_liability_without_free_cost_prediction() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (store, access) = cohort(temp.path(), backend, true).await;
        let before = store.state().clone();
        let report = forecasts::observe(&store, &access, window()).unwrap();
        assert_eq!(store.state(), &before);
        assert_eq!(report.episodes.len(), 20);
        let cohort = &report.cohorts[0];
        assert_eq!(cohort.known_cost_micros, 590);
        assert_eq!(cohort.unknown_attempts, 1);
        assert_eq!(cohort.liability_micros, 10);
        let ForecastStatus::Forecast {
            transient_states,
            expected_cost_micros,
            ..
        } = &cohort.status
        else {
            panic!("{:?}", cohort.status);
        };
        let main = transient_states
            .iter()
            .position(|state| *state == State::Main)
            .unwrap();
        assert!(expected_cost_micros[main].is_none());
    }
}

#[tokio::test]
async fn action_forecasts_exclude_old_schema_and_revision_gaps_instead_of_joining_across_them() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (store, access) = setup(temp.path(), backend).await;
        let mut store = episode(store, &access, 0, false, Some(TaskState::Failed)).await;
        store = episode(store, &access, 1, false, Some(TaskState::Cancelled)).await;
        let task: Task = store
            .state()
            .record(Collection::Task, "forecast-0", &access.workspace)
            .unwrap()
            .decode()
            .unwrap();
        let event = EventInput {
            id: EventId::new(),
            workspace: access.workspace.clone(),
            session: task.scope.session.clone(),
            task: Some(task.scope.task.clone()),
            actor: access.actor.clone(),
            correlation: CommandId::new(),
            causation: None,
            timestamp: Timestamp::new(55),
            kind: EventKind::TaskTransition,
            artifacts: vec![],
            data: serde_json::json!({"schema_version":0,"facts":[]}),
            metadata: None,
        };
        // Import-style malformed retained evidence must never bridge a missing revision.
        let mut gap_task: Task = store
            .state()
            .record(Collection::Task, "forecast-1", &access.workspace)
            .unwrap()
            .decode()
            .unwrap();
        let gap_id = EventId::new();
        gap_task.revision = gap_task.revision.next().unwrap().next().unwrap();
        gap_task.cause = gap_id.clone();
        let gap_event = EventInput {
            id: gap_id,
            task: Some(gap_task.scope.task.clone()),
            data: serde_json::json!({"schema_version":1,"facts":[{"collection":"task","id":gap_task.scope.task,"revision":gap_task.revision,"value":gap_task}]}),
            ..event.clone()
        };
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![],
                events: vec![event, gap_event],
                command: None,
            })
            .await
            .unwrap();
        let before = store.state().clone();
        let report = forecasts::observe(&store, &access, window()).unwrap();
        assert!(report.episodes.is_empty());
        assert_eq!(report.excluded.len(), 2);
        assert_eq!(store.state(), &before);
    }
}

fn saved_bytes(store: &Store, pin: &forecast_reports::ForecastPin) -> Vec<u8> {
    let descriptor: vcp_domain::artifact::ArtifactDescriptor = store
        .state()
        .record(
            Collection::Artifact,
            pin.artifact.as_str(),
            &store
                .state()
                .records
                .values()
                .find(|record| record.collection == Collection::Workspace)
                .unwrap()
                .workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    let mut bytes = Vec::new();
    store.spool().read(&descriptor, &mut bytes).unwrap();
    assert_eq!(vcp_protocol::digest_bytes(&bytes), pin.digest);
    bytes
}

#[tokio::test]
async fn saved_action_forecasts_pin_immutable_bytes_reopen_and_recheck_complete_source_access() {
    use vcp_domain::retention_selector::{Criterion, Selector, Tree};
    use vcp_memory::retention::{self, Action};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (store, access) = setup(temp.path(), backend).await;
        let store = episode(store, &access, 0, false, Some(TaskState::Completed)).await;
        let mut store = episode(store, &access, 1, false, Some(TaskState::Failed)).await;
        let baseline = save_report(&mut store, &access, window(), Timestamp::new(1001))
            .await
            .unwrap();
        let pin = baseline.forecast.as_ref().unwrap();
        assert_eq!(pin.source_tasks.len(), 2);
        let manifest = store
            .state()
            .record(
                Collection::Projection,
                &pin.source_manifest,
                &access.workspace,
            )
            .unwrap()
            .clone();
        let mut replacement = manifest.clone();
        replacement.revision = replacement.revision.next().unwrap();
        replacement.value["revision"] = serde_json::to_value(replacement.revision).unwrap();
        let before = store.state().clone();
        assert!(store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![Mutation::Put {
                    record: replacement,
                    expected: Some(manifest.revision)
                }],
                events: vec![],
                command: None
            })
            .await
            .is_err());
        assert_eq!(store.state(), &before);
        let bytes = saved_bytes(&store, pin);
        let frozen = forecast_reports::load(&store, &access, &baseline)
            .unwrap()
            .unwrap();
        let mut mismatched = baseline.clone();
        mismatched.forecast.as_mut().unwrap().digest = "0".repeat(64);
        assert!(forecast_reports::load(&store, &access, &mismatched).is_err());
        let mut narrowed_manifest = baseline.clone();
        narrowed_manifest
            .forecast
            .as_mut()
            .unwrap()
            .source_tasks
            .pop_last();
        assert!(forecast_reports::load(&store, &access, &narrowed_manifest).is_err());
        assert_eq!(frozen.episodes.len(), 2);
        assert!(matches!(
            frozen.cohorts[0].status,
            ForecastStatus::Abstained { .. }
        ));
        // Optional attachment preserves deserialization of reports saved before this feature.
        let mut legacy = serde_json::to_value(&baseline).unwrap();
        legacy.as_object_mut().unwrap().remove("forecast");
        let legacy: OptimizationReport = serde_json::from_value(legacy).unwrap();
        assert!(legacy.forecast.is_none());
        assert!(forecast_reports::load(&store, &access, &legacy)
            .unwrap()
            .is_none());
        let mut store = episode(store, &access, 2, false, Some(TaskState::Cancelled)).await;
        let current = save_report(&mut store, &access, window(), Timestamp::new(1002))
            .await
            .unwrap();
        assert_eq!(saved_bytes(&store, pin), bytes);
        assert_eq!(
            forecast_reports::load(&store, &access, &baseline)
                .unwrap()
                .unwrap(),
            frozen
        );
        let mut read = copy_access(&access);
        read.write = false;
        let before = store.state().clone();
        assert_eq!(load_report(&store, &read, &baseline.id).unwrap(), baseline);
        assert!(
            forecast_drift::saved(&store, &read, &baseline.id, &current.id)
                .unwrap()
                .is_some()
        );
        assert_eq!(store.state(), &before);
        drop(store);
        let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(saved_bytes(&store, pin), bytes);
        assert_eq!(
            forecast_reports::load(&store, &read, &baseline)
                .unwrap()
                .unwrap(),
            frozen
        );
        let mut narrow = copy_access(&read);
        narrow.tasks = Some(BTreeSet::from([TaskId::parse("forecast-0").unwrap()]));
        assert!(load_report(&store, &narrow, &baseline.id).is_err());
        assert!(forecast_reports::load(&store, &narrow, &baseline).is_err());
        let audit = vcp_audit::history::Access {
            workspace: access.workspace.clone(),
            authority: access.authority,
            read: true,
            tasks: narrow.tasks.clone(),
        };
        let mut sink = Vec::new();
        assert!(vcp_audit::history::History::read_artifact(
            &store,
            &audit,
            &pin.artifact,
            &mut sink
        )
        .is_err());
        assert!(sink.is_empty());
        let audit = vcp_audit::history::Access {
            tasks: None,
            ..audit
        };
        assert!(vcp_audit::history::History::read_artifact(
            &store,
            &audit,
            &pin.artifact,
            &mut sink
        )
        .is_err());
        assert!(sink.is_empty());
        let plan = retention::preview(
            &store,
            &access,
            Selector {
                schema_version: 1,
                tree: Tree::All(vec![
                    Tree::Match(Criterion::Task(TaskId::parse("forecast-1").unwrap())),
                    Tree::Match(Criterion::Event("task_transition".into())),
                ]),
            },
            Action::Purge,
            Timestamp::new(1003),
        )
        .unwrap();
        assert!(plan.protected.is_empty());
        assert!(plan.dependent.contains(&retention::Target::Record(key(
            Collection::Artifact,
            pin.artifact.as_str()
        ))));
        let receipt = retention::apply(&mut store, &access, &plan, Timestamp::new(1003))
            .await
            .unwrap();
        let before = store.state().clone();
        assert!(forecast_reports::load(&store, &read, &baseline).is_err());
        assert!(load_report(&store, &read, &baseline.id).is_err());
        assert!(forecast_drift::saved(&store, &read, &baseline.id, &current.id).is_err());
        assert_eq!(store.state(), &before);
        let cleaned = retention::cleanup(&mut store, &access, &receipt.id, Timestamp::new(1004))
            .await
            .unwrap();
        assert!(cleaned.rewrite_complete);
        let tombstone = store
            .state()
            .record(
                Collection::Projection,
                &pin.source_manifest,
                &access.workspace,
            )
            .unwrap();
        assert_eq!(
            tombstone.value["document_type"],
            vcp_domain::forecast::REDACTED
        );
        let revision = tombstone.revision;
        let artifact: vcp_domain::artifact::ArtifactDescriptor = store
            .state()
            .record(
                Collection::Artifact,
                pin.artifact.as_str(),
                &access.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(artifact.state, vcp_domain::artifact::CaptureState::Purged);
        let mut restored = manifest;
        restored.revision = revision.next().unwrap();
        restored.value["revision"] = serde_json::to_value(restored.revision).unwrap();
        let before = store.state().clone();
        assert!(store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![Mutation::Put {
                    record: restored,
                    expected: Some(revision)
                }],
                events: vec![],
                command: None
            })
            .await
            .is_err());
        assert_eq!(store.state(), &before);
    }
}

#[tokio::test]
async fn saved_action_forecasts_absent_for_empty_sources_and_rejected_save_publishes_nothing() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(temp.path(), backend).await;
        let empty = save_report(&mut store, &access, window(), Timestamp::new(1001))
            .await
            .unwrap();
        assert!(empty.forecast.is_none());
        assert!(forecast_reports::load(&store, &access, &empty)
            .unwrap()
            .is_none());
        assert!(forecast_drift::saved(&store, &access, &empty.id, &empty.id)
            .unwrap()
            .is_none());
        let mut store = episode(store, &access, 0, false, Some(TaskState::Failed)).await;
        let mut read = copy_access(&access);
        read.write = false;
        let before = store.state().clone();
        assert!(
            save_report(&mut store, &read, window(), Timestamp::new(1002))
                .await
                .is_err()
        );
        assert_eq!(store.state(), &before);
        #[cfg(feature = "qualification")]
        {
            let failure = forecast_reports::qualification_interrupt_after_spool(
                &mut store,
                &access,
                window(),
                Timestamp::new(1002),
            )
            .await
            .unwrap_err();
            assert!(failure.contains("qualification interruption"), "{failure}");
            // Finalized orphan bytes are unreachable until the atomic descriptor/report commit.
            assert_eq!(store.state(), &before);
        }
        assert!(!store
            .state()
            .records
            .values()
            .any(|record| record.collection == Collection::Artifact
                && record.value["spec"]["schema"] == forecast_reports::SCHEMA));
        drop(store);
        let reopened = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(reopened.state(), &before);
    }
}
