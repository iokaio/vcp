// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_domain::task::TaskState;
use vcp_domain::{accounting::RequestRole, artifact::*};
use vcp_lifecycle::foundation::routing_state::advisory::{self, Disposition};
use vcp_models::decision::{
    self, Answer, Binding as DecisionBinding, Mode, Operation, Outcome, Purpose,
    QualifiedEvaluator, Question, Request, Usage,
};
use vcp_protocol::{canonical_bytes, digest_bytes};
use vcp_store::artifact::ArtifactWriter;

fn prepare(binding: DecisionBinding, marker: &str) -> decision::Prepared {
    let state = serde_json::json!({"marker":marker,"observations":["bounded"]});
    let request = Request {
        version: decision::VERSION,
        binding: DecisionBinding {
            input: digest_bytes(&canonical_bytes(&state).unwrap()),
            ..binding
        },
        purpose: Purpose::Escalation,
        question_revision: digest_bytes(b"advisory-record-test-v1"),
        state,
        questions: BTreeMap::from([(
            "next_action".into(),
            Question::Choice {
                instructions: "Choose a bounded action from recorded evidence.".into(),
                options: BTreeMap::from([
                    ("retry".into(), "Retry under the same limits".into()),
                    ("stop".into(), "Stop the bounded loop".into()),
                ]),
            },
        )]),
        deadline: Timestamp::new(100),
    };
    let evaluator = QualifiedEvaluator {
        model: "fixture/advisory".into(),
        provider: "fixture".into(),
        served_model: "fixture/advisory-v1".into(),
        served_provider: "Fixture".into(),
        operation: Operation::JevDecisions,
        purpose: Purpose::Escalation,
        mode: Mode::Advisory,
        evidence_digest: "a".repeat(64),
        configuration_digest: "b".repeat(64),
        valid_until: Timestamp::new(200),
        require_distributions: true,
        require_confidence: true,
        deny_data_collection: true,
        require_zdr: true,
        prompt_price_per_million: "0.01".into(),
        output_price_per_million: "0.01".into(),
        request_price: "0".into(),
    };
    decision::prepare(
        &request,
        &decision::Policy {
            mode: Mode::Advisory,
            evaluator: Some(evaluator),
            attempt_limit: 1,
            attempts_used: 0,
        },
        Timestamp::new(10),
    )
    .unwrap()
    .unwrap()
}

fn advice(prepared: &decision::Prepared, action: &str) -> Outcome {
    Outcome::Advice {
        binding: prepared.request().binding.clone(),
        purpose: Purpose::Escalation,
        question_revision: prepared.request().question_revision.clone(),
        request_digest: prepared.digest().into(),
        evaluator: prepared.evaluator().clone(),
        mode: Mode::Advisory,
        answers: BTreeMap::from([(
            "next_action".into(),
            Answer::Choice {
                choice: action.into(),
                probabilities: Some(BTreeMap::from([
                    ("retry".into(), if action == "retry" { 0.9 } else { 0.1 }),
                    ("stop".into(), if action == "stop" { 0.9 } else { 0.1 }),
                ])),
                confidence: Some(0.9),
            },
        )]),
        usage: Usage {
            input_tokens: Some(10),
            output_tokens: Some(2),
            observed_cost: Some(Micros::new(1)),
            unknown_liability: false,
        },
    }
}

fn binding(task: &vcp_domain::task::Task, access: &Access) -> DecisionBinding {
    DecisionBinding {
        scope: task.scope.clone(),
        root: task.root.clone(),
        step: task.revision,
        steering: task.steering,
        authority: access.authority,
        deletion: DeletionEpoch::ZERO,
        policy: "c".repeat(64),
        catalog: "d".repeat(64),
        input: String::new(),
        evidence: BTreeMap::from([("observation".into(), "e".repeat(64))]),
    }
}

async fn set_task_state(
    store: &mut Store,
    access: &Access,
    task: &vcp_domain::task::Task,
    state: TaskState,
) -> vcp_domain::task::Task {
    let mut changed = task.clone();
    changed.revision = task.revision.next().unwrap();
    changed.state = state;
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![Mutation::Put {
                record: Record::typed(
                    Collection::Task,
                    changed.scope.task.as_str(),
                    access.workspace.clone(),
                    changed.revision,
                    &changed,
                )
                .unwrap(),
                expected: Some(task.revision),
            }],
            events: vec![],
            command: None,
        })
        .await
        .unwrap();
    changed
}

async fn reserve_advisory(
    store: &mut Store,
    access: &Access,
    task: &vcp_domain::task::Task,
    request: &advisory::RequestRecord,
    now: u64,
) -> vcp_domain::accounting::Attempt {
    reserve_advisory_with_price(store, access, task, request, advisory_price(request), now).await
}

fn advisory_price(request: &advisory::RequestRecord) -> vcp_domain::accounting::PriceSnapshot {
    let mut price = super::action_evidence::price(&request.evaluator.model);
    price.provider = request.evaluator.provider.clone();
    price.capability = request.evaluator.configuration_digest.clone();
    price
}

async fn reserve_advisory_with_price(
    store: &mut Store,
    access: &Access,
    task: &vcp_domain::task::Task,
    request: &advisory::RequestRecord,
    price: vcp_domain::accounting::PriceSnapshot,
    now: u64,
) -> vcp_domain::accounting::Attempt {
    let spec = ArtifactSpec {
        id: ArtifactId::new(),
        scope: task.scope.clone(),
        media_type: "application/json".into(),
        schema: "vcp-escalation-advisory-request-v1".into(),
        source: "vcp-lifecycle/advisory".into(),
        channel: Channel::RequestBody,
        retention: "history".into(),
        omissions: vec![],
    };
    let mut writer = store.spool().create(spec).unwrap();
    writer
        .write_chunk(&canonical_bytes(request).unwrap())
        .unwrap();
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
                    access.workspace.clone(),
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
    let ledger = vcp_budget::ledger(store.state(), &task.scope).unwrap();
    vcp_budget::reserve(
        store,
        vcp_budget::Admission {
            transaction: TransactionId::new(),
            attempt: AttemptId::new(),
            reservation: ReservationId::new(),
            scope: task.scope.clone(),
            agent: AgentId::new(),
            role: RequestRole::Helper,
            request: artifact.spec.id,
            request_digest: artifact.sha256,
            quote: vcp_budget::arithmetic::quote(
                price,
                vcp_domain::accounting::Usage {
                    requests: Units::new(1),
                    ..Default::default()
                },
                Timestamp::new(now),
            )
            .unwrap(),
            previous: None,
            expected_ledger: ledger.revision,
            policy: ledger.policy,
            steering: task.steering,
            draw_protected: false,
            now: Timestamp::new(now),
        },
        &super::action_evidence::budget_actor(access, now),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn canonical_advisory_records_deduplicate_revalidate_and_reopen() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(temp.path(), backend).await;
        let task =
            super::action_evidence::create_task(&mut store, &access, TaskState::Running).await;
        let base = binding(&task, &access);

        let prepared = prepare(base.clone(), "current");
        let current = prepared.request().binding.clone();
        let request = advisory::record_request(
            &mut store,
            &access,
            CommandId::new(),
            &prepared,
            &current,
            Timestamp::new(10),
        )
        .await
        .unwrap();
        assert_eq!(request.request_digest.len(), 64);
        assert_eq!(request.transport_commitment, prepared.digest());
        let watermark = store.state().watermark;
        assert_eq!(
            advisory::record_request(
                &mut store,
                &access,
                CommandId::new(),
                &prepared,
                &current,
                Timestamp::new(11),
            )
            .await
            .unwrap()
            .id,
            request.id
        );
        assert_eq!(store.state().watermark, watermark);

        let outcome = advice(&prepared, "retry");
        let result = advisory::record_result(
            &mut store,
            &access,
            CommandId::new(),
            &request.id,
            &outcome,
            &current,
            Timestamp::new(20),
        )
        .await
        .unwrap();
        assert_eq!(result.disposition, Disposition::AcceptedCurrent);
        let result_watermark = store.state().watermark;
        assert_eq!(
            advisory::record_result(
                &mut store,
                &access,
                CommandId::new(),
                &request.id,
                &outcome,
                &current,
                Timestamp::new(21),
            )
            .await
            .unwrap(),
            result
        );
        assert_eq!(store.state().watermark, result_watermark);
        assert!(advisory::record_result(
            &mut store,
            &access,
            CommandId::new(),
            &request.id,
            &Outcome::Baseline { reason: "conflict" },
            &current,
            Timestamp::new(22),
        )
        .await
        .unwrap_err()
        .contains("reused"));

        let late_prepared = prepare(base, "late");
        let late_current = late_prepared.request().binding.clone();
        let late_request = advisory::record_request(
            &mut store,
            &access,
            CommandId::new(),
            &late_prepared,
            &late_current,
            Timestamp::new(30),
        )
        .await
        .unwrap();
        let mut changed = late_current.clone();
        changed.policy = "f".repeat(64);
        let late_result = advisory::record_result(
            &mut store,
            &access,
            CommandId::new(),
            &late_request.id,
            &advice(&late_prepared, "stop"),
            &changed,
            Timestamp::new(40),
        )
        .await
        .unwrap();
        assert_eq!(late_result.disposition, Disposition::HistoricalStale);
        assert!(advisory::record_request(
            &mut store,
            &access,
            CommandId::new(),
            &late_prepared,
            &changed,
            Timestamp::new(41),
        )
        .await
        .unwrap_err()
        .contains("no longer current"));

        let denied = Access {
            workspace: access.workspace.clone(),
            actor: access.actor.clone(),
            authority: access.authority,
            read: true,
            write: false,
            tasks: Some(BTreeSet::new()),
        };
        assert!(advisory::load_request(&store, &denied, &request.id).is_err());
        assert!(advisory::load_result(&store, &denied, &request.id).is_err());
        store.close().await.unwrap();

        let reopened = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(
            advisory::load_request(&reopened, &access, &request.id)
                .unwrap()
                .transport_commitment,
            request.transport_commitment
        );
        assert_eq!(
            advisory::load_result(&reopened, &access, &request.id).unwrap(),
            result
        );
        assert_eq!(
            advisory::load_result(&reopened, &access, &late_request.id)
                .unwrap()
                .disposition,
            Disposition::HistoricalStale
        );
    }
}

#[tokio::test]
async fn caller_owned_schedule_closes_pause_interruption_and_reopen_without_replay() {
    use advisory::{Cancellation, Claim, ScheduleState};

    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(temp.path(), backend).await;
        let mut task =
            super::action_evidence::create_task(&mut store, &access, TaskState::Running).await;

        let interrupted_prepared = prepare(binding(&task, &access), "interrupted");
        let interrupted_current = interrupted_prepared.request().binding.clone();
        let interrupted_request = advisory::record_request(
            &mut store,
            &access,
            CommandId::new(),
            &interrupted_prepared,
            &interrupted_current,
            Timestamp::new(10),
        )
        .await
        .unwrap();
        let pending = advisory::schedule(
            &mut store,
            &access,
            CommandId::new(),
            &interrupted_request.id,
            &interrupted_current,
            Timestamp::new(11),
        )
        .await
        .unwrap();
        assert_eq!(pending.state, ScheduleState::Pending);
        let watermark = store.state().watermark;
        assert_eq!(
            advisory::schedule(
                &mut store,
                &access,
                CommandId::new(),
                &interrupted_request.id,
                &interrupted_current,
                Timestamp::new(12),
            )
            .await
            .unwrap(),
            pending
        );
        assert_eq!(store.state().watermark, watermark);
        let interrupted_claimant = CommandId::new();
        let Claim::Updated(claimed) = advisory::claim(
            &mut store,
            &access,
            CommandId::new(),
            &interrupted_request.id,
            interrupted_claimant.clone(),
            &interrupted_current,
            Timestamp::new(13),
        )
        .await
        .unwrap() else {
            panic!("pending schedule must admit exactly one claim")
        };
        assert!(matches!(claimed.state, ScheduleState::Claimed { .. }));
        let watermark = store.state().watermark;
        assert!(matches!(
            advisory::claim(
                &mut store,
                &access,
                CommandId::new(),
                &interrupted_request.id,
                interrupted_claimant.clone(),
                &interrupted_current,
                Timestamp::new(14),
            )
            .await
            .unwrap(),
            Claim::Existing(_)
        ));
        assert_eq!(store.state().watermark, watermark);
        store.close().await.unwrap();

        let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert!(matches!(
            advisory::load_schedule(&store, &access, &interrupted_request.id)
                .unwrap()
                .state,
            ScheduleState::Claimed { .. }
        ));
        let watermark = store.state().watermark;
        assert!(matches!(
            advisory::claim(
                &mut store,
                &access,
                CommandId::new(),
                &interrupted_request.id,
                CommandId::new(),
                &interrupted_current,
                Timestamp::new(15),
            )
            .await
            .unwrap(),
            Claim::Existing(_)
        ));
        assert_eq!(store.state().watermark, watermark);
        let interrupted = advisory::interrupt(
            &mut store,
            &access,
            CommandId::new(),
            &interrupted_request.id,
            &interrupted_claimant,
            true,
            Timestamp::new(16),
        )
        .await
        .unwrap();
        assert_eq!(
            interrupted.state,
            ScheduleState::Cancelled {
                reason: Cancellation::Interrupted,
                cancelled_at: Timestamp::new(16),
            }
        );
        let watermark = store.state().watermark;
        assert_eq!(
            advisory::interrupt(
                &mut store,
                &access,
                CommandId::new(),
                &interrupted_request.id,
                &interrupted_claimant,
                true,
                Timestamp::new(17),
            )
            .await
            .unwrap(),
            interrupted
        );
        assert_eq!(store.state().watermark, watermark);

        let paused_prepared = prepare(binding(&task, &access), "paused");
        let paused_current = paused_prepared.request().binding.clone();
        let paused_request = advisory::record_request(
            &mut store,
            &access,
            CommandId::new(),
            &paused_prepared,
            &paused_current,
            Timestamp::new(20),
        )
        .await
        .unwrap();
        advisory::schedule(
            &mut store,
            &access,
            CommandId::new(),
            &paused_request.id,
            &paused_current,
            Timestamp::new(21),
        )
        .await
        .unwrap();
        let paused_claimant = CommandId::new();
        advisory::claim(
            &mut store,
            &access,
            CommandId::new(),
            &paused_request.id,
            paused_claimant.clone(),
            &paused_current,
            Timestamp::new(22),
        )
        .await
        .unwrap();
        task = set_task_state(&mut store, &access, &task, TaskState::Paused).await;
        let mut current_pause = paused_current;
        current_pause.step = task.revision;
        let cancelled = advisory::revalidate_dispatch(
            &mut store,
            &access,
            CommandId::new(),
            &paused_request.id,
            &paused_claimant,
            &current_pause,
            Timestamp::new(23),
        )
        .await
        .unwrap();
        assert!(matches!(
            cancelled.state,
            ScheduleState::Cancelled {
                reason: Cancellation::Paused,
                ..
            }
        ));

        task = set_task_state(&mut store, &access, &task, TaskState::Running).await;
        let complete_prepared = prepare(binding(&task, &access), "complete");
        let complete_current = complete_prepared.request().binding.clone();
        let complete_request = advisory::record_request(
            &mut store,
            &access,
            CommandId::new(),
            &complete_prepared,
            &complete_current,
            Timestamp::new(30),
        )
        .await
        .unwrap();
        advisory::schedule(
            &mut store,
            &access,
            CommandId::new(),
            &complete_request.id,
            &complete_current,
            Timestamp::new(31),
        )
        .await
        .unwrap();
        let complete_claimant = CommandId::new();
        advisory::claim(
            &mut store,
            &access,
            CommandId::new(),
            &complete_request.id,
            complete_claimant.clone(),
            &complete_current,
            Timestamp::new(32),
        )
        .await
        .unwrap();
        let ready = advisory::revalidate_dispatch(
            &mut store,
            &access,
            CommandId::new(),
            &complete_request.id,
            &complete_claimant,
            &complete_current,
            Timestamp::new(33),
        )
        .await
        .unwrap();
        assert!(matches!(ready.state, ScheduleState::Claimed { .. }));
        let outcome = advice(&complete_prepared, "retry");
        advisory::record_result(
            &mut store,
            &access,
            CommandId::new(),
            &complete_request.id,
            &outcome,
            &complete_current,
            Timestamp::new(34),
        )
        .await
        .unwrap();
        let completed = advisory::complete(
            &mut store,
            &access,
            CommandId::new(),
            &complete_request.id,
            &complete_claimant,
            &complete_current,
            Timestamp::new(35),
        )
        .await
        .unwrap();
        assert!(matches!(completed.state, ScheduleState::Completed { .. }));
        assert_eq!(completed.revision, Revision::new(2));
        let watermark = store.state().watermark;
        assert_eq!(
            advisory::complete(
                &mut store,
                &access,
                CommandId::new(),
                &complete_request.id,
                &complete_claimant,
                &complete_current,
                Timestamp::new(36),
            )
            .await
            .unwrap(),
            completed
        );
        assert_eq!(store.state().watermark, watermark);
        store.close().await.unwrap();
    }
}

#[tokio::test]
async fn advisory_completion_rechecks_inputs_after_result_and_after_completion() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for completed_first in [false, true] {
            for change in [
                "pause",
                "stale_step",
                "policy",
                "catalog",
                "evidence",
                "deadline",
            ] {
                let temp = tempfile::tempdir().unwrap();
                let (mut store, access) = setup(temp.path(), backend).await;
                let task =
                    super::action_evidence::create_task(&mut store, &access, TaskState::Running)
                        .await;
                let prepared = prepare(binding(&task, &access), "completion-recheck");
                let mut current = prepared.request().binding.clone();
                let request = advisory::record_request(
                    &mut store,
                    &access,
                    CommandId::new(),
                    &prepared,
                    &current,
                    Timestamp::new(10),
                )
                .await
                .unwrap();
                advisory::schedule(
                    &mut store,
                    &access,
                    CommandId::new(),
                    &request.id,
                    &current,
                    Timestamp::new(11),
                )
                .await
                .unwrap();
                let claimant = CommandId::new();
                advisory::claim(
                    &mut store,
                    &access,
                    CommandId::new(),
                    &request.id,
                    claimant.clone(),
                    &current,
                    Timestamp::new(12),
                )
                .await
                .unwrap();
                advisory::record_result(
                    &mut store,
                    &access,
                    CommandId::new(),
                    &request.id,
                    &advice(&prepared, "retry"),
                    &current,
                    Timestamp::new(13),
                )
                .await
                .unwrap();
                if completed_first {
                    advisory::complete(
                        &mut store,
                        &access,
                        CommandId::new(),
                        &request.id,
                        &claimant,
                        &current,
                        Timestamp::new(14),
                    )
                    .await
                    .unwrap();
                }
                let now = match change {
                    "pause" | "stale_step" => {
                        let paused =
                            set_task_state(&mut store, &access, &task, TaskState::Paused).await;
                        if change == "pause" {
                            current.step = paused.revision;
                        }
                        Timestamp::new(15)
                    }
                    "policy" => {
                        current.policy = "f".repeat(64);
                        Timestamp::new(15)
                    }
                    "catalog" => {
                        current.catalog = "f".repeat(64);
                        Timestamp::new(15)
                    }
                    "evidence" => {
                        current
                            .evidence
                            .insert("observation".into(), "f".repeat(64));
                        Timestamp::new(15)
                    }
                    "deadline" => Timestamp::new(100),
                    _ => unreachable!(),
                };
                let before = advisory::load_schedule(&store, &access, &request.id).unwrap();
                let watermark = store.state().watermark;
                assert!(
                    advisory::complete(
                        &mut store,
                        &access,
                        CommandId::new(),
                        &request.id,
                        &claimant,
                        &current,
                        now,
                    )
                    .await
                    .is_err(),
                    "{backend:?} {change} completed_first={completed_first}"
                );
                assert_eq!(store.state().watermark, watermark);
                assert_eq!(
                    advisory::load_schedule(&store, &access, &request.id).unwrap(),
                    before
                );
                // Rejection never rewrites historical evidence as if it were newly received.
                assert_eq!(
                    advisory::load_result(&store, &access, &request.id)
                        .unwrap()
                        .disposition,
                    Disposition::AcceptedCurrent
                );
                store.close().await.unwrap();
            }
        }
    }
}

#[tokio::test]
async fn advisory_accounting_rejects_other_request_and_evaluator_on_bind_and_read() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for mismatch in ["request", "model", "provider", "capability"] {
            let temp = tempfile::tempdir().unwrap();
            let (mut store, access) = setup(temp.path(), backend).await;
            let task =
                super::action_evidence::create_task(&mut store, &access, TaskState::Running).await;
            vcp_budget::initialize(
                &mut store,
                task.scope.clone(),
                super::action_evidence::money(1000),
                Micros::ZERO,
                None,
                &super::action_evidence::budget_actor(&access, 3),
            )
            .await
            .unwrap();
            let prepared = prepare(binding(&task, &access), "accounting-identity");
            let current = prepared.request().binding.clone();
            let request = advisory::record_request(
                &mut store,
                &access,
                CommandId::new(),
                &prepared,
                &current,
                Timestamp::new(10),
            )
            .await
            .unwrap();
            advisory::schedule(
                &mut store,
                &access,
                CommandId::new(),
                &request.id,
                &current,
                Timestamp::new(11),
            )
            .await
            .unwrap();
            let claimant = CommandId::new();
            advisory::claim(
                &mut store,
                &access,
                CommandId::new(),
                &request.id,
                claimant.clone(),
                &current,
                Timestamp::new(12),
            )
            .await
            .unwrap();
            let mut other_request = request.clone();
            let mut other_price = advisory_price(&request);
            match mismatch {
                "request" => {
                    let other = prepare(binding(&task, &access), "another-canonical-request");
                    other_request = advisory::record_request(
                        &mut store,
                        &access,
                        CommandId::new(),
                        &other,
                        &other.request().binding,
                        Timestamp::new(12),
                    )
                    .await
                    .unwrap();
                }
                "model" => other_price.model = "other/model".into(),
                "provider" => other_price.provider = "other/provider".into(),
                "capability" => other_price.capability = "f".repeat(64),
                _ => unreachable!(),
            }
            let other_attempt = reserve_advisory_with_price(
                &mut store,
                &access,
                &task,
                &other_request,
                other_price,
                13,
            )
            .await;
            let expected_error = if mismatch == "request" {
                "attempt request is not a retained canonical advisory body"
            } else {
                "attempt is not the canonical advisory helper reservation"
            };
            let watermark = store.state().watermark;
            assert_eq!(
                advisory::bind_attempt(
                    &mut store,
                    &access,
                    CommandId::new(),
                    &request.id,
                    &claimant,
                    &other_attempt.id,
                    Timestamp::new(14),
                )
                .await
                .unwrap_err(),
                expected_error,
                "{mismatch}"
            );
            assert_eq!(store.state().watermark, watermark);

            let attempt = reserve_advisory(&mut store, &access, &task, &request, 15).await;
            let accounting = advisory::bind_attempt(
                &mut store,
                &access,
                CommandId::new(),
                &request.id,
                &claimant,
                &attempt.id,
                Timestamp::new(16),
            )
            .await
            .unwrap();
            assert_eq!(
                advisory::accounting_attempt(&store, &access, &request.id).unwrap(),
                attempt
            );

            // Simulate an older persisted binding that points at a different
            // otherwise-valid helper. Reads must enforce the same identities.
            let mut stored =
                store.state().records[&key(Collection::Projection, &accounting.id)].clone();
            let revision = stored.revision;
            stored.revision = revision.next().unwrap();
            for (field, value) in [
                ("attempt", serde_json::to_value(&other_attempt.id).unwrap()),
                (
                    "reservation",
                    serde_json::to_value(&other_attempt.reservation).unwrap(),
                ),
                (
                    "request_artifact",
                    serde_json::to_value(&other_attempt.request).unwrap(),
                ),
                (
                    "request_artifact_digest",
                    serde_json::to_value(&other_attempt.request_digest).unwrap(),
                ),
                ("quote", serde_json::to_value(&other_attempt.quote).unwrap()),
            ] {
                stored.value[field] = value;
            }
            store
                .transact(Transaction {
                    id: TransactionId::new(),
                    expected_watermark: store.state().watermark,
                    mutations: vec![Mutation::Put {
                        record: stored,
                        expected: Some(revision),
                    }],
                    events: vec![],
                    command: None,
                })
                .await
                .unwrap();
            assert_eq!(
                advisory::accounting_attempt(&store, &access, &request.id).unwrap_err(),
                expected_error,
                "{mismatch}"
            );
        }
    }
}

#[tokio::test]
async fn advisory_claim_binds_existing_helper_accounting_and_tracks_uncertain_charge() {
    use advisory::Claim;
    use vcp_domain::accounting::ReservationState;

    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(temp.path(), backend).await;
        let task =
            super::action_evidence::create_task(&mut store, &access, TaskState::Running).await;
        vcp_budget::initialize(
            &mut store,
            task.scope.clone(),
            super::action_evidence::money(1000),
            Micros::ZERO,
            None,
            &super::action_evidence::budget_actor(&access, 3),
        )
        .await
        .unwrap();
        let prepared = prepare(binding(&task, &access), "accounting");
        let current = prepared.request().binding.clone();
        let request = advisory::record_request(
            &mut store,
            &access,
            CommandId::new(),
            &prepared,
            &current,
            Timestamp::new(10),
        )
        .await
        .unwrap();
        advisory::schedule(
            &mut store,
            &access,
            CommandId::new(),
            &request.id,
            &current,
            Timestamp::new(11),
        )
        .await
        .unwrap();
        let claimant = CommandId::new();
        assert!(matches!(
            advisory::claim(
                &mut store,
                &access,
                CommandId::new(),
                &request.id,
                claimant.clone(),
                &current,
                Timestamp::new(12),
            )
            .await
            .unwrap(),
            Claim::Updated(_)
        ));
        let attempt = reserve_advisory(&mut store, &access, &task, &request, 13).await;
        let accounting = advisory::bind_attempt(
            &mut store,
            &access,
            CommandId::new(),
            &request.id,
            &claimant,
            &attempt.id,
            Timestamp::new(14),
        )
        .await
        .unwrap();
        assert_eq!(accounting.attempt, attempt.id);
        assert_eq!(accounting.reservation, attempt.reservation);
        assert_eq!(accounting.quote, attempt.quote);
        let watermark = store.state().watermark;
        assert_eq!(
            advisory::bind_attempt(
                &mut store,
                &access,
                CommandId::new(),
                &request.id,
                &claimant,
                &attempt.id,
                Timestamp::new(15),
            )
            .await
            .unwrap(),
            accounting
        );
        assert_eq!(store.state().watermark, watermark);

        vcp_budget::submit(
            &mut store,
            &attempt.id,
            &task.scope,
            attempt.revision,
            &super::action_evidence::budget_actor(&access, 16),
        )
        .await
        .unwrap();
        assert_eq!(
            advisory::accounting_attempt(&store, &access, &request.id)
                .unwrap()
                .phase,
            ReservationState::Submitted
        );
        vcp_budget::hold_uncertain(
            &mut store,
            &attempt.id,
            &task.scope,
            &super::action_evidence::budget_actor(&access, 17),
            "provider outcome unavailable",
        )
        .await
        .unwrap();
        let uncertain = advisory::accounting_attempt(&store, &access, &request.id).unwrap();
        assert_eq!(uncertain.phase, ReservationState::ReconciliationPending);
        assert_eq!(
            uncertain.uncertain.as_deref(),
            Some("provider outcome unavailable")
        );
        store.close().await.unwrap();
        let reopened = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(
            advisory::accounting_attempt(&reopened, &access, &request.id).unwrap(),
            uncertain
        );
    }
}
