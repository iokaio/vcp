// SPDX-License-Identifier: Apache-2.0
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{verification::Fingerprint, workspace::*, *};
use vcp_lifecycle::foundation::routing_state::*;
use vcp_memory::access::Access;
use vcp_models::routing::{Group, Policy, Preference, Profile};
use vcp_protocol::event::{EventInput, EventKind};
use vcp_store::{contract::*, BackendKind, Store};
#[path = "support/action_evidence.rs"]
mod action_evidence;
#[path = "support/advisory_records.rs"]
mod advisory_records;
#[path = "support/routing_accounting.rs"]
mod routing_accounting;
#[cfg(feature = "qualification")]
#[path = "support/routing_crash.rs"]
mod routing_crash;
#[path = "support/transition_evidence.rs"]
mod transition_evidence;

fn policy() -> Policy {
    Policy {
        schema_version: 1,
        id: String::new(),
        parent: None,
        profile: Profile::Low,
        allowed_models: BTreeSet::from(["model".into()]),
        allowed_endpoints: BTreeSet::from(["endpoint".into()]),
        allowed_groups: BTreeSet::from([Group::Low]),
        quality_floor_bps: 7000,
        minimum_samples: 10,
        maximum_evidence_age_ms: 10000,
        deny_data_collection: true,
        require_zdr: true,
        ordering: vec![
            Preference::TotalCost,
            Preference::Latency,
            Preference::Quality,
            Preference::Capability,
        ],
        pin: None,
        broader_task_class: None,
    }
    .seal()
    .unwrap()
}
async fn setup(root: &std::path::Path, backend: BackendKind) -> (Store, Access) {
    let mut store = Store::open(root, backend, &[]).await.unwrap();
    let workspace = Workspace {
        id: WorkspaceId::new(),
        binding: Binding {
            host: HostId::new(),
            root: "C:/synthetic".into(),
            repository: "synthetic".into(),
            worktree: "main".into(),
            revision: Revision::ZERO,
        },
        trust: Trust::Trusted,
        revision: Revision::ZERO,
        authority: AuthorityRevision::ZERO,
        deletion: DeletionEpoch::ZERO,
    };
    let session = Session {
        id: SessionId::new(),
        workspace: workspace.id.clone(),
        revision: Revision::ZERO,
        configuration: Revision::ZERO,
        fork_origin: None,
        fork_through: None,
    };
    let access = Access {
        workspace: workspace.id.clone(),
        actor: ActorId::new(),
        authority: workspace.authority,
        read: true,
        write: true,
        tasks: None,
    };
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: Watermark::ZERO,
            mutations: vec![
                Mutation::Put {
                    record: Record::typed(
                        Collection::Workspace,
                        workspace.id.as_str(),
                        workspace.id.clone(),
                        Revision::ZERO,
                        &workspace,
                    )
                    .unwrap(),
                    expected: None,
                },
                Mutation::Put {
                    record: Record::typed(
                        Collection::Session,
                        session.id.as_str(),
                        workspace.id.clone(),
                        Revision::ZERO,
                        &session,
                    )
                    .unwrap(),
                    expected: None,
                },
            ],
            events: vec![EventInput {
                id: EventId::new(),
                workspace: workspace.id,
                session: session.id,
                task: None,
                actor: access.actor.clone(),
                correlation: CommandId::new(),
                causation: None,
                timestamp: Timestamp::new(1),
                kind: EventKind::SessionStarted,
                artifacts: vec![],
                data: serde_json::json!({}),
                metadata: None,
            }],
            command: None,
        })
        .await
        .unwrap();
    (store, access)
}
#[tokio::test]
async fn selective_apply_is_atomic_idempotent_reopenable_and_rollback_rechecks_ceilings() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(temp.path(), backend).await;
        let ceilings = policy();
        initialize_policy(&mut store, &access, ceilings.clone(), Timestamp::new(2))
            .await
            .unwrap();
        let report = save_report(
            &mut store,
            &access,
            HistoryWindow {
                from: None,
                until: Timestamp::new(3),
            },
            Timestamp::new(3),
        )
        .await
        .unwrap();
        assert_eq!(report.counts.tasks, 0);
        assert!(report
            .uncertainty
            .iter()
            .any(|s| s.contains("Small sample")));
        let declined = preview(&store, &access, &report.id, vec![], &ceilings).unwrap();
        let watermark = store.state().watermark;
        assert!(apply(
            &mut store,
            &access,
            CommandId::new(),
            &declined,
            &ceilings,
            Timestamp::new(4)
        )
        .await
        .is_err());
        assert_eq!(store.state().watermark, watermark);
        let proposal = preview(
            &store,
            &access,
            &report.id,
            vec![Edit::QualityFloorBps(8000)],
            &ceilings,
        )
        .unwrap();
        let command = CommandId::new();
        let applied = apply(
            &mut store,
            &access,
            command.clone(),
            &proposal,
            &ceilings,
            Timestamp::new(4),
        )
        .await
        .unwrap();
        assert_eq!(applied.published.revision, Revision::new(1));
        assert_eq!(applied.effective.quality_floor_bps, 8000);
        let watermark = store.state().watermark;
        assert_eq!(
            apply(
                &mut store,
                &access,
                command.clone(),
                &proposal,
                &ceilings,
                Timestamp::new(5)
            )
            .await
            .unwrap(),
            applied
        );
        assert_eq!(store.state().watermark, watermark);
        assert!(apply(
            &mut store,
            &access,
            CommandId::new(),
            &proposal,
            &ceilings,
            Timestamp::new(5)
        )
        .await
        .is_err());
        store.close().await.unwrap();
        let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(
            apply(
                &mut store,
                &access,
                command,
                &proposal,
                &ceilings,
                Timestamp::new(6)
            )
            .await
            .unwrap(),
            applied
        );
        let mut narrowed = ceilings.clone();
        narrowed.quality_floor_bps = 9000;
        narrowed = narrowed.seal().unwrap();
        let rollback_command = CommandId::new();
        let receipt = rollback(
            &mut store,
            &access,
            rollback_command.clone(),
            Revision::new(1),
            Revision::ZERO,
            &narrowed,
            Timestamp::new(7),
        )
        .await
        .unwrap();
        assert_eq!(receipt.published.revision, Revision::new(2));
        assert_eq!(receipt.published.value.quality_floor_bps, 7000);
        assert_eq!(receipt.effective.quality_floor_bps, 9000);
        assert_eq!(
            rollback(
                &mut store,
                &access,
                rollback_command,
                Revision::new(1),
                Revision::ZERO,
                &narrowed,
                Timestamp::new(8)
            )
            .await
            .unwrap(),
            receipt
        );
        assert_eq!(
            current_policy(&store, &access).unwrap().unwrap(),
            receipt.published
        );
        assert_eq!(
            store
                .state()
                .records
                .values()
                .filter(|r| r.collection == Collection::Projection
                    && r.id.starts_with("routing-policy-"))
                .count(),
            4
        );
        store.close().await.unwrap();
    }
}
#[tokio::test]
async fn interview_reuses_answers_closed_schema_rejects_authority_and_reports_recheck_access() {
    let temp = tempfile::tempdir().unwrap();
    let (mut store, mut access) = setup(temp.path(), BackendKind::Files).await;
    assert!(
        serde_json::from_value::<Edit>(serde_json::json!({"field":"budget","value":1000})).is_err()
    );
    assert!(serde_json::from_value::<Edit>(
        serde_json::json!({"field":"allowed_models","value":["new"]})
    )
    .is_err());
    let initial = interview(&store, &access).unwrap();
    assert_eq!(next_question(&initial), Some(Question::Priority));
    let updated = answer(
        &mut store,
        &access,
        None,
        Question::Priority,
        "spend".into(),
        Timestamp::new(2),
    )
    .await
    .unwrap();
    assert_eq!(next_question(&updated), Some(Question::ExpectedSize));
    assert_eq!(
        interview(&store, &access).unwrap().answers,
        BTreeMap::from([(Question::Priority, "spend".into())])
    );
    assert!(answer(
        &mut store,
        &access,
        None,
        Question::ExpectedSize,
        "small".into(),
        Timestamp::new(2)
    )
    .await
    .is_err());
    let saved = save_report(
        &mut store,
        &access,
        HistoryWindow {
            from: None,
            until: Timestamp::new(4),
        },
        Timestamp::new(4),
    )
    .await
    .unwrap();
    assert_eq!(next_question_for_report(&updated, &saved), None);
    access.authority = AuthorityRevision::new(1);
    assert!(load_report(&store, &access, &saved.id).is_err());
    access.authority = AuthorityRevision::ZERO;
    access.tasks = Some(BTreeSet::new());
    assert!(interview(&store, &access).is_err());
    assert!(
        initialize_policy(&mut store, &access, policy(), Timestamp::new(5))
            .await
            .is_err()
    );
    store.close().await.unwrap();
}

#[tokio::test]
async fn report_keeps_failed_cancelled_and_unfinished_denominators_and_scopes_saved_evidence() {
    use vcp_domain::task::{Objective, Task, TaskState};
    let temp = tempfile::tempdir().unwrap();
    let (mut store, mut access) = setup(temp.path(), BackendKind::Files).await;
    let session: Session = store
        .state()
        .records
        .values()
        .find(|r| r.collection == Collection::Session)
        .unwrap()
        .decode()
        .unwrap();
    let mut selected = BTreeSet::new();
    for state in [TaskState::Failed, TaskState::Cancelled, TaskState::Paused] {
        let task_id = TaskId::new();
        let event_id = EventId::new();
        selected.insert(task_id.clone());
        let task = Task {
            scope: Scope {
                workspace: access.workspace.clone(),
                session: session.id.clone(),
                task: task_id.clone(),
            },
            root: task_id.clone(),
            parent: None,
            fork_origin: None,
            revision: Revision::ZERO,
            steering: SteeringRevision::ZERO,
            objectives: vec![Objective {
                text: "synthetic objective".into(),
                constraints: vec![],
                acceptance: vec![],
                source: event_id.clone(),
                steering: SteeringRevision::ZERO,
            }],
            state,
            fingerprint: Fingerprint {
                repository: "a".repeat(64),
                buffers: "b".repeat(64),
                environment: "c".repeat(64),
            },
            editing: false,
            required_checks: vec![],
            cause: event_id.clone(),
            reason: "synthetic sample".into(),
            redaction: None,
        };
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![Mutation::Put {
                    record: Record::typed(
                        Collection::Task,
                        task_id.as_str(),
                        access.workspace.clone(),
                        Revision::ZERO,
                        &task,
                    )
                    .unwrap(),
                    expected: None,
                }],
                events: vec![EventInput {
                    id: event_id,
                    workspace: access.workspace.clone(),
                    session: session.id.clone(),
                    task: Some(task_id),
                    actor: access.actor.clone(),
                    correlation: CommandId::new(),
                    causation: None,
                    timestamp: Timestamp::new(5),
                    kind: EventKind::TaskCreated,
                    artifacts: vec![],
                    data: serde_json::json!({}),
                    metadata: None,
                }],
                command: None,
            })
            .await
            .unwrap();
    }
    let saved = save_report(
        &mut store,
        &access,
        HistoryWindow {
            from: Some(Timestamp::new(4)),
            until: Timestamp::new(6),
        },
        Timestamp::new(6),
    )
    .await
    .unwrap();
    assert_eq!(saved.counts.tasks, 3);
    assert_eq!(saved.counts.failed, 1);
    assert_eq!(saved.counts.cancelled, 1);
    assert_eq!(saved.counts.unfinished, 1);
    assert_eq!(saved.counts.completed, 0);
    assert!(saved.uncertainty.iter().any(|s| s.contains("abandonment")));
    selected.pop_first();
    access.tasks = Some(selected);
    assert!(load_report(&store, &access, &saved.id).is_err());
    let restricted = report(
        &store,
        &access,
        HistoryWindow {
            from: None,
            until: Timestamp::new(7),
        },
    )
    .unwrap();
    assert_eq!(restricted.counts.tasks, 2);
    assert_eq!(restricted.evidence.len(), 2);
    let empty = report(
        &store,
        &access,
        HistoryWindow {
            from: Some(Timestamp::new(10)),
            until: Timestamp::new(11),
        },
    )
    .unwrap();
    assert_eq!(empty.counts.tasks, 0);
    store.close().await.unwrap();
}

#[test]
fn effective_policy_intersects_pin_fallback_and_never_weakens_trusted_privacy_or_quality() {
    use vcp_models::routing::{ModelEndpoint, Pin};
    let endpoint = |name: &str| ModelEndpoint {
        model: name.into(),
        endpoint: "endpoint".into(),
    };
    let mut ceiling = policy();
    ceiling.pin = Some(Pin {
        candidate: endpoint("model"),
        fallback_candidates: BTreeSet::new(),
    });
    ceiling = ceiling.seal().unwrap();
    let mut persisted = policy();
    persisted.pin = Some(Pin {
        candidate: endpoint("model"),
        fallback_candidates: BTreeSet::from([endpoint("extra")]),
    });
    persisted.quality_floor_bps = 1;
    persisted.deny_data_collection = false;
    persisted.require_zdr = false;
    persisted.broader_task_class = Some("unapproved".into());
    persisted = persisted.seal().unwrap();
    let effective = effective_policy(persisted, &ceiling).unwrap();
    assert_eq!(effective.pin.unwrap().fallback_candidates.len(), 0);
    assert_eq!(effective.quality_floor_bps, 7000);
    assert!(effective.deny_data_collection);
    assert!(effective.require_zdr);
    assert_eq!(effective.broader_task_class, None);
}
