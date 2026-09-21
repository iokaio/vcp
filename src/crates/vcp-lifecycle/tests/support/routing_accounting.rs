// SPDX-License-Identifier: Apache-2.0
use super::*;
#[path = "../../../vcp-store/tests/common/mod.rs"]
mod common;
use vcp_domain::{
    accounting::*,
    artifact::*,
    task::{Task, TaskState},
};
use vcp_store::artifact::ArtifactWriter;
#[path = "owner_declarations.rs"]
mod owner_declarations;
fn money(value: u64) -> Money {
    Money {
        currency: "USD".to_owned().try_into().unwrap(),
        micros: Micros::new(value),
    }
}
fn actor() -> vcp_budget::Actor {
    vcp_budget::Actor {
        id: ActorId::parse("owner").unwrap(),
        now: Timestamp::new(1000),
    }
}
async fn capture(store: &mut Store, scope: &Scope) -> ArtifactDescriptor {
    let mut spec = common::spec();
    spec.scope = scope.clone();
    spec.channel = Channel::RequestBody;
    let mut writer = store.spool().create(spec).unwrap();
    writer.write_chunk(b"synthetic evidence").unwrap();
    let artifact = writer.finalize().unwrap();
    drop(writer);
    store
        .transact(common::attach(store.state(), artifact.clone(), None))
        .await
        .unwrap();
    artifact
}
async fn reserve_attempt(
    store: &mut Store,
    scope: &Scope,
    role: RequestRole,
    cost: u64,
    model: &str,
) -> Attempt {
    let request = capture(store, scope).await;
    let price = PriceSnapshot {
        id: "a".repeat(64),
        provider: "scripted-endpoint".into(),
        model: model.into(),
        currency: money(0).currency,
        capability: "b".repeat(64),
        valid_until: Timestamp::new(9000),
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
                    micros: Micros::new(if category == ChargeCategory::Request {
                        cost
                    } else {
                        0
                    }),
                    per_units: Units::new(1),
                },
            )
        })
        .collect(),
    };
    let ledger = vcp_budget::ledger(store.state(), scope).unwrap();
    let attempt = vcp_budget::reserve(
        store,
        vcp_budget::Admission {
            transaction: TransactionId::new(),
            attempt: AttemptId::new(),
            reservation: ReservationId::new(),
            scope: scope.clone(),
            agent: AgentId::new(),
            role,
            request: request.spec.id,
            request_digest: request.sha256,
            quote: vcp_budget::arithmetic::quote(
                price,
                Usage {
                    requests: Units::new(1),
                    ..Default::default()
                },
                actor().now,
            )
            .unwrap(),
            previous: None,
            expected_ledger: ledger.revision,
            policy: ledger.policy,
            steering: SteeringRevision::ZERO,
            draw_protected: false,
            now: actor().now,
        },
        &actor(),
    )
    .await
    .unwrap();
    attempt
}
async fn attempt(store: &mut Store, scope: &Scope, role: RequestRole, cost: u64, settle: bool) {
    let attempt = reserve_attempt(store, scope, role, cost, "synthetic-model").await;
    vcp_budget::submit(store, &attempt.id, scope, attempt.revision, &actor())
        .await
        .unwrap();
    if settle {
        let raw = capture(store, scope).await;
        vcp_budget::observe(
            store,
            UsageObservation {
                id: ObservationId::new(),
                scope: scope.clone(),
                attempt: attempt.id.clone(),
                provider_request: format!("provider-{}", attempt.id),
                mode: UsageMode::Cumulative {
                    version: Units::new(1),
                },
                amount: money(cost),
                final_usage: true,
                raw: raw.spec.id,
                correction: None,
            },
            &actor(),
        )
        .await
        .unwrap();
    } else {
        vcp_budget::hold_uncertain(
            store,
            &attempt.id,
            scope,
            &actor(),
            "synthetic missing final usage",
        )
        .await
        .unwrap();
    }
}
#[tokio::test]
async fn mixed_failed_child_and_unresolved_support_costs_stay_in_denominator() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
        let mut initial = common::initial();
        for mutation in &mut initial.mutations {
            if let Mutation::Put { record, .. } = mutation {
                if record.collection == Collection::Task {
                    let mut task: Task = record.decode().unwrap();
                    task.state = TaskState::Running;
                    record.value = serde_json::to_value(task).unwrap();
                }
            }
        }
        store.transact(initial).await.unwrap();
        let scope = common::task().scope;
        vcp_budget::initialize(
            &mut store,
            scope.clone(),
            money(1000),
            Micros::ZERO,
            None,
            &actor(),
        )
        .await
        .unwrap();
        let mut child = common::task();
        child.scope.task = TaskId::new();
        child.parent = Some(scope.task.clone());
        child.state = TaskState::Running;
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![Mutation::Put {
                    record: Record::typed(
                        Collection::Task,
                        child.scope.task.as_str(),
                        scope.workspace.clone(),
                        Revision::ZERO,
                        &child,
                    )
                    .unwrap(),
                    expected: None,
                }],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        let ledger = vcp_budget::ledger(store.state(), &scope).unwrap();
        vcp_budget::configure(
            &mut store,
            &scope,
            ledger.revision,
            money(1000),
            Micros::ZERO,
            BTreeMap::from([(child.scope.task.clone(), Micros::new(200))]),
            &actor(),
            "synthetic child ceiling",
        )
        .await
        .unwrap();
        attempt(&mut store, &scope, RequestRole::Main, 25, true).await;
        attempt(&mut store, &scope, RequestRole::Optimizer, 50, false).await;
        attempt(&mut store, &child.scope, RequestRole::Child, 30, true).await;
        let check_output = capture(&mut store, &scope).await;
        let verification = vcp_domain::verification::Verification {
            redaction: None,
            id: VerificationId::new(),
            scope: scope.clone(),
            steering: SteeringRevision::ZERO,
            fingerprint: common::task().fingerprint,
            outputs: vec![check_output.spec.id.clone()],
            checks: vec![
                vcp_domain::verification::Check {
                    specification: "passed fixture".into(),
                    outcome: vcp_domain::verification::CheckOutcome::Passed,
                    output: check_output.spec.id.clone(),
                    exit_code: Some(0),
                },
                vcp_domain::verification::Check {
                    specification: "failed fixture".into(),
                    outcome: vcp_domain::verification::CheckOutcome::Failed {
                        reason: "observed fixture failure".into(),
                    },
                    output: check_output.spec.id.clone(),
                    exit_code: Some(1),
                },
                vcp_domain::verification::Check {
                    specification: "unavailable fixture".into(),
                    outcome: vcp_domain::verification::CheckOutcome::NotRun {
                        reason: "explicit missing execution".into(),
                    },
                    output: check_output.spec.id,
                    exit_code: None,
                },
            ],
            unresolved_effects: vec![],
            outstanding_issues: vec![],
            cost: vcp_domain::verification::CostCertainty::Known,
        };
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![Mutation::Put {
                    record: Record::typed(
                        Collection::Verification,
                        verification.id.as_str(),
                        scope.workspace.clone(),
                        Revision::ZERO,
                        &verification,
                    )
                    .unwrap(),
                    expected: None,
                }],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        let access = Access {
            workspace: scope.workspace.clone(),
            actor: actor().id,
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            tasks: None,
        };
        let baseline = save_report(
            &mut store,
            &access,
            HistoryWindow {
                from: None,
                until: Timestamp::new(2000),
            },
            Timestamp::new(1500),
        )
        .await
        .unwrap();
        let mut root: Task = store
            .state()
            .record(Collection::Task, scope.task.as_str(), &scope.workspace)
            .unwrap()
            .decode()
            .unwrap();
        root.state = TaskState::Failed;
        root.revision = Revision::new(1);
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![Mutation::Put {
                    record: Record::typed(
                        Collection::Task,
                        scope.task.as_str(),
                        scope.workspace.clone(),
                        root.revision,
                        &root,
                    )
                    .unwrap(),
                    expected: Some(Revision::ZERO),
                }],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        let access = Access {
            workspace: scope.workspace,
            actor: actor().id,
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            tasks: None,
        };
        let report = save_report(
            &mut store,
            &access,
            HistoryWindow {
                from: None,
                until: Timestamp::new(2000),
            },
            Timestamp::new(1501),
        )
        .await
        .unwrap();
        assert_eq!(report.counts.tasks, 2);
        assert_eq!(report.counts.failed, 1);
        assert_eq!(report.counts.unfinished, 1);
        assert_eq!(report.counts.child_tasks, 1);
        assert_eq!(report.counts.attempts, 3);
        assert_eq!(report.counts.supporting_attempts, 2);
        assert_eq!(report.counts.known_spend_micros.get("USD"), Some(&55));
        assert_eq!(report.counts.uncertain_attempts, 1);
        assert_eq!(report.observed.submission_to_final_usage_ms, vec![0, 0]);
        assert_eq!(report.observed.attempts_without_complete_latency, 1);
        assert_eq!(report.observed.verification_checks_passed, 1);
        assert_eq!(report.observed.verification_checks_failed, 1);
        assert_eq!(report.observed.verification_checks_not_run, 1);
        assert_eq!(
            report.counts.reserved_liability_micros.get("USD"),
            Some(&50)
        );
        assert_eq!(report.cohorts.values().sum::<u64>(), 3);
        let cutoff = store.state().watermark;
        let comparison = compare_reports(&store, &access, &baseline.id, &report.id).unwrap();
        assert!(comparison.comparable);
        assert_eq!(
            comparison
                .metrics
                .iter()
                .find(|m| m.metric == "failed_tasks_per_selected_task")
                .unwrap()
                .direction,
            Direction::Increased
        );
        assert!(comparison
            .metrics
            .iter()
            .all(|m| !m.metric.contains("micros")));
        assert!(comparison
            .caveats
            .iter()
            .any(|c| c.contains("Unknown charges")));
        assert!(!comparison.automatic_action);
        assert_eq!(store.state().watermark, cutoff);
        assert!(
            !compare_reports(&store, &access, &report.id, &baseline.id)
                .unwrap()
                .comparable
        );
        assert!(report.uncertainty.iter().any(|s| s.contains("Unresolved")));
        let mut interview = Interview {
            version: 1,
            revision: Revision::ZERO,
            answers: BTreeMap::new(),
        };
        assert_eq!(
            next_question_for_report(&interview, &report),
            Some(Question::Priority)
        );
        interview.answers.insert(Question::Priority, "spend".into());
        assert_eq!(
            next_question_for_report(&interview, &report),
            Some(Question::ModelRestrictions)
        );
        interview
            .answers
            .insert(Question::ModelRestrictions, "keep provider".into());
        assert_eq!(
            next_question_for_report(&interview, &report),
            Some(Question::ReviewPreference)
        );
        store.close().await.unwrap();
    }
}

#[tokio::test]
async fn catalog_publication_keeps_raw_attribution_and_prior_revision_across_refresh_and_reopen() {
    use vcp_models::routing::CatalogRevision;
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
        store.transact(common::initial()).await.unwrap();
        let scope = common::task().scope;
        let raw = capture(&mut store, &scope).await;
        let access = Access {
            workspace: scope.workspace,
            actor: actor().id,
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            tasks: None,
        };
        let first = CatalogRevision::create(None, Timestamp::new(10), None, vec![]).unwrap();
        let original = publish_registry(
            &mut store,
            &access,
            None,
            first.clone(),
            raw.spec.id.clone(),
            Timestamp::new(10),
        )
        .await
        .unwrap();
        assert_eq!(original.value.raw_sha256, raw.sha256);
        let next =
            CatalogRevision::create(Some(first.id.clone()), Timestamp::new(20), None, vec![])
                .unwrap();
        let watermark = store.state().watermark;
        assert!(publish_registry(
            &mut store,
            &access,
            None,
            next.clone(),
            raw.spec.id.clone(),
            Timestamp::new(20)
        )
        .await
        .is_err());
        assert_eq!(store.state().watermark, watermark);
        let updated = publish_registry(
            &mut store,
            &access,
            Some(Revision::ZERO),
            next,
            raw.spec.id,
            Timestamp::new(20),
        )
        .await
        .unwrap();
        assert_eq!(updated.revision, Revision::new(1));
        assert_eq!(original.value.catalog, first);
        let retained: Vec<String> = store
            .state()
            .records
            .values()
            .filter(|r| r.collection == Collection::Projection)
            .filter_map(|r| {
                r.value["value"]["catalog"]["id"]
                    .as_str()
                    .map(str::to_owned)
            })
            .collect();
        assert!(retained.contains(&original.value.catalog.id));
        assert!(retained.contains(&updated.value.catalog.id));
        store.close().await.unwrap();
        let store = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(current_registry(&store, &access).unwrap().unwrap(), updated);
        store.close().await.unwrap();
    }
}

#[tokio::test]
async fn escalation_admission_binds_actual_request_counts_and_reopens_without_duplicate_action() {
    use vcp_models::{escalation as e, routing as r};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
        let mut initial = common::initial();
        for mutation in &mut initial.mutations {
            if let Mutation::Put { record, .. } = mutation {
                if record.collection == Collection::Task {
                    let mut task: Task = record.decode().unwrap();
                    task.state = TaskState::Running;
                    record.value = serde_json::to_value(task).unwrap();
                }
            }
        }
        store.transact(initial).await.unwrap();
        let scope = common::task().scope;
        vcp_budget::initialize(
            &mut store,
            scope.clone(),
            money(1000),
            Micros::ZERO,
            None,
            &actor(),
        )
        .await
        .unwrap();
        let previous =
            reserve_attempt(&mut store, &scope, RequestRole::Main, 25, "first-model").await;
        vcp_budget::submit(
            &mut store,
            &previous.id,
            &scope,
            previous.revision,
            &actor(),
        )
        .await
        .unwrap();
        vcp_budget::hold_uncertain(
            &mut store,
            &previous.id,
            &scope,
            &actor(),
            "synthetic failed previous attempt",
        )
        .await
        .unwrap();
        let before_ledger = vcp_budget::ledger(store.state(), &scope).unwrap();
        let current =
            reserve_attempt(&mut store, &scope, RequestRole::Main, 30, "second-model").await;
        let endpoint = |model: &str| r::ModelEndpoint {
            model: model.into(),
            endpoint: "scripted-endpoint".into(),
        };
        let selected = endpoint("second-model");
        let mut decision = r::RoutingDecision {
            schema_version: 1,
            id: String::new(),
            input: r::RoutingInput {
                retry_pin: None,
                workspace: scope.workspace.clone(),
                root: scope.task.clone(),
                task: scope.task.clone(),
                input_revision: Revision::ZERO,
                steering: SteeringRevision::ZERO,
                input_digest: "c".repeat(64),
                catalog: "d".repeat(64),
                policy: "e".repeat(64),
                role: RequestRole::Main,
                task_class: "synthetic".into(),
                now: Timestamp::new(1000),
                required_capabilities: BTreeSet::new(),
                excluded: BTreeSet::new(),
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
                group: Some(r::Group::Low),
                quality_bps: Some(9000),
                samples: Some(20),
                latency_p95_ms: Some(10),
                total_estimate: None,
                evidence_refs: vec![],
                broader_cohort_used: None,
                assumptions: vec!["persistence-boundary fixture; not live qualification".into()],
            }],
            selected: Some(selected.clone()),
            fallback_from: None,
            immediate_reservation: None,
        };
        decision.id = decision.digest().unwrap();
        let access = Access {
            workspace: scope.workspace.clone(),
            actor: actor().id,
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            tasks: None,
        };
        record_decision(
            &mut store,
            &access,
            &scope,
            &decision,
            &current.request_digest,
            current.id.clone(),
            Timestamp::new(1000),
        )
        .await
        .unwrap();
        let plan = e::Plan {
            previous_attempt: previous.id.clone(),
            previous: endpoint("first-model"),
            selected,
            selected_snapshot: current.quote.price.id.clone(),
            selected_compatibility: "synthetic-compatibility".into(),
            routing_decision: decision.id.clone(),
            routing_policy: decision.input.policy.clone(),
            trigger: e::Trigger {
                kind: e::TriggerKind::DeclaredComplexity,
                evidence: vec![previous.request.clone()],
                observations: 1,
            },
            before: e::Counters {
                total_attempts: 1,
                ..Default::default()
            },
            after: e::Counters {
                total_attempts: 2,
                quality_switches: 1,
                ..Default::default()
            },
            revisions: vcp_context::manifest::Revisions {
                scope: scope.clone(),
                steering: SteeringRevision::ZERO,
                policy: PolicyRevision::ZERO,
                authority: AuthorityRevision::ZERO,
                deletion: DeletionEpoch::ZERO,
                binding: Revision::ZERO,
                instructions: Revision::ZERO,
                tools: Revision::ZERO,
                skills: Revision::ZERO,
                memory: Revision::ZERO,
                task_state: Revision::ZERO,
            },
            policy: e::Policy {
                max_transport_retries: 2,
                max_quality_switches: 2,
                max_decompositions: 1,
                max_total_attempts: 6,
                minimum_repeated_failures: 1,
                deadline: Timestamp::new(2000),
            },
            ledger_revision: before_ledger.revision,
            remaining: Micros::new(975),
            estimated_request: Micros::new(30),
            estimated_handoff: Micros::ZERO,
            unresolved: Micros::new(25),
        };
        let handoff = e::Handoff {
            packet_sha256: "a".repeat(64),
            destination_manifest_sha256: "b".repeat(64),
            destination_request_sha256: current.request_digest.clone(),
            previous_attempt: previous.id,
            routing_decision: decision.id,
            original_artifacts: vec![previous.request],
        };
        let cutoff = store.state().watermark;
        let mut forged = handoff.clone();
        forged.destination_request_sha256 = "0".repeat(64);
        assert!(record_escalation(
            &mut store,
            &access,
            &scope,
            &plan,
            &forged,
            current.id.clone(),
            Timestamp::new(1000)
        )
        .await
        .is_err());
        let mut overcount = plan.clone();
        overcount.before.total_attempts = 2;
        overcount.after.total_attempts = 3;
        assert!(record_escalation(
            &mut store,
            &access,
            &scope,
            &overcount,
            &handoff,
            current.id.clone(),
            Timestamp::new(1000)
        )
        .await
        .is_err());
        let mut stale_ledger = plan.clone();
        stale_ledger.ledger_revision = Revision::ZERO;
        assert!(record_escalation(
            &mut store,
            &access,
            &scope,
            &stale_ledger,
            &handoff,
            current.id.clone(),
            Timestamp::new(1000)
        )
        .await
        .is_err());
        assert_eq!(store.state().watermark, cutoff);
        let admitted = record_escalation(
            &mut store,
            &access,
            &scope,
            &plan,
            &handoff,
            current.id.clone(),
            Timestamp::new(1000),
        )
        .await
        .unwrap();
        let cutoff = store.state().watermark;
        assert_eq!(
            record_escalation(
                &mut store,
                &access,
                &scope,
                &plan,
                &handoff,
                current.id.clone(),
                Timestamp::new(1001)
            )
            .await
            .unwrap(),
            admitted
        );
        assert_eq!(store.state().watermark, cutoff);
        store.close().await.unwrap();
        let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(
            admitted_escalations(&store, &access, &scope).unwrap(),
            vec![admitted.clone()]
        );
        assert_eq!(
            record_escalation(
                &mut store,
                &access,
                &scope,
                &plan,
                &handoff,
                current.id.clone(),
                Timestamp::new(1002)
            )
            .await
            .unwrap(),
            admitted
        );
        assert!(record_escalation(
            &mut store,
            &access,
            &scope,
            &plan,
            &forged,
            current.id,
            Timestamp::new(1002)
        )
        .await
        .is_err());
        store.close().await.unwrap();
    }
}
