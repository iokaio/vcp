// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_domain::task::{Objective, Task, TaskState};
use vcp_lifecycle::foundation::{routing, routing_state::transitions::*};

#[tokio::test]
async fn engine_transitions_and_idempotent_pause_produce_the_supported_facts() {
    use vcp_engine::{Engine, HostFacts};
    use vcp_protocol::command::{Command, CommandEnvelope};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(temp.path(), backend).await;
        let task = append(
            &mut store,
            &access,
            "engine-task",
            TaskState::Pending,
            2,
            true,
        )
        .await;
        let actor = vcp_engine::Access {
            actor: access.actor.clone(),
            workspace: access.workspace.clone(),
            session: task.scope.session.clone(),
            authority: access.authority,
            read: true,
            write: true,
            bootstrap: false,
        };
        let mut engine = Engine::new(store).unwrap();
        let mut host = HostFacts::inspect(Timestamp::new(10));
        host.may_execute = true;
        for (expected, next) in [
            (0, TaskState::Running),
            (1, TaskState::Paused),
            (2, TaskState::Paused),
        ] {
            let command = CommandEnvelope {
                version: 1,
                id: CommandId::new(),
                workspace: access.workspace.clone(),
                session: task.scope.session.clone(),
                task: Some(task.scope.task.clone()),
                caller: access.actor.clone(),
                controller: engine.controller().clone(),
                owner_epoch: engine.owner_epoch(),
                expected: Revision::new(expected),
                steering: SteeringRevision::ZERO,
                payload: Command::Transition {
                    next,
                    reason: "observed test transition".into(),
                    verification: None,
                },
            };
            engine.handle(command.clone(), &actor, &host).await.unwrap();
            engine.handle(command, &actor, &host).await.unwrap(); // command replay adds no event
        }
        let evidence = observe(engine.store(), &access, window()).unwrap();
        assert_eq!(evidence.traces[0].observations.len(), 3);
        assert!(evidence.traces[0].gaps.is_empty());
        assert!(evidence.traces[0].right_censored);
        assert_eq!(evidence.transitions.iter().map(|e| e.count).sum::<u64>(), 2);
    }
}

fn window() -> HistoryWindow {
    HistoryWindow {
        from: None,
        until: Timestamp::new(100),
    }
}

async fn event_only(store: &mut Store, access: &Access, task: &Task, data: serde_json::Value) {
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![],
            events: vec![EventInput {
                id: EventId::new(),
                workspace: access.workspace.clone(),
                session: task.scope.session.clone(),
                task: Some(task.scope.task.clone()),
                actor: access.actor.clone(),
                correlation: CommandId::new(),
                causation: None,
                timestamp: Timestamp::new(20),
                kind: EventKind::TaskTransition,
                artifacts: vec![],
                data,
                metadata: None,
            }],
            command: None,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn invalid_sources_unknown_versions_and_bounds_abstain_without_mutation() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(temp.path(), backend).await;
        let task = append(&mut store, &access, "a", TaskState::Pending, 2, true).await;
        let fact = serde_json::json!({"collection":"task","id":task.scope.task,"revision":task.revision,"value":task});
        event_only(
            &mut store,
            &access,
            &task,
            serde_json::json!({"schema_version":1,"facts":[]}),
        )
        .await;
        let unchanged = observe(&store, &access, window()).unwrap();
        assert_eq!(unchanged.traces[0].observations.len(), 1);
        assert!(unchanged.traces[0].gaps.is_empty());
        // A repeated snapshot cannot claim a different causing event.
        event_only(
            &mut store,
            &access,
            &task,
            serde_json::json!({"schema_version":1,"facts":[fact.clone()]}),
        )
        .await;
        event_only(
            &mut store,
            &access,
            &task,
            serde_json::json!({"schema_version":2,"facts":[fact.clone()]}),
        )
        .await;
        let evidence = observe(&store, &access, window()).unwrap();
        assert!(evidence.transitions.is_empty());
        assert_eq!(
            evidence.traces[0]
                .gaps
                .iter()
                .map(|g| g.reason.clone())
                .collect::<Vec<_>>(),
            vec![GapReason::InvalidFact, GapReason::MissingFacts]
        );
        event_only(
            &mut store,
            &access,
            &task,
            serde_json::json!({"schema_version":1,"facts":vec![fact; 4097]}),
        )
        .await;
        let before = store.state().clone();
        assert!(observe(&store, &access, window())
            .unwrap_err()
            .contains("4096 observations"));
        assert_eq!(store.state(), &before);
    }
}

async fn append(
    store: &mut Store,
    access: &Access,
    name: &str,
    state: TaskState,
    time: u64,
    facts: bool,
) -> Task {
    let id = TaskId::parse(name).unwrap();
    let event = EventId::new();
    let existing = store
        .state()
        .records
        .get(&key(Collection::Task, name))
        .map(|r| r.decode::<Task>().unwrap());
    let expected = existing.as_ref().map(|t| t.revision);
    let mut task = existing.unwrap_or_else(|| {
        let session: Session = store
            .state()
            .records
            .values()
            .find(|r| r.collection == Collection::Session)
            .unwrap()
            .decode()
            .unwrap();
        Task {
            scope: Scope {
                workspace: access.workspace.clone(),
                session: session.id,
                task: id.clone(),
            },
            root: id.clone(),
            parent: None,
            fork_origin: None,
            revision: Revision::ZERO,
            steering: SteeringRevision::ZERO,
            objectives: vec![Objective {
                text: "private objective never returned".into(),
                constraints: vec![],
                acceptance: vec![],
                source: event.clone(),
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
            cause: event.clone(),
            reason: "synthetic observation".into(),
            redaction: None,
        }
    });
    task.state = state;
    task.revision = expected
        .map(|r| r.next().unwrap())
        .unwrap_or(Revision::ZERO);
    task.cause = event.clone();
    let data = if facts {
        serde_json::json!({"schema_version":1,"facts":[{"collection":"task","id":id,"revision":task.revision,"value":task}]})
    } else {
        serde_json::json!({"schema_version":1,"reason":"legacy restore observation"})
    };
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![Mutation::Put {
                record: Record::typed(
                    Collection::Task,
                    name,
                    access.workspace.clone(),
                    task.revision,
                    &task,
                )
                .unwrap(),
                expected,
            }],
            events: vec![EventInput {
                id: event,
                workspace: access.workspace.clone(),
                session: task.scope.session.clone(),
                task: Some(id),
                actor: access.actor.clone(),
                correlation: CommandId::new(),
                causation: None,
                timestamp: Timestamp::new(time),
                kind: if expected.is_none() {
                    EventKind::TaskCreated
                } else {
                    EventKind::TaskTransition
                },
                artifacts: vec![],
                data,
                metadata: None,
            }],
            command: None,
        })
        .await
        .unwrap();
    task
}

#[tokio::test]
async fn canonical_transition_chains_are_scoped_ordered_read_only_and_rebuild_after_reopen() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, mut access) = setup(temp.path(), backend).await;
        append(&mut store, &access, "a", TaskState::Pending, 2, true).await;
        append(&mut store, &access, "b", TaskState::Pending, 3, true).await;
        append(&mut store, &access, "a", TaskState::Running, 10, true).await;
        append(&mut store, &access, "b", TaskState::Failed, 11, true).await;
        // Canonical order, not wall clock sorting. Blocked is resumable.
        append(&mut store, &access, "a", TaskState::Blocked, 8, true).await;
        append(&mut store, &access, "a", TaskState::Running, 12, true).await;
        // A same-state revision represents a fingerprint/steering observation.
        append(&mut store, &access, "a", TaskState::Running, 13, true).await;
        append(&mut store, &access, "a", TaskState::Cancelled, 14, true).await;
        let before = store.state().clone();
        access.write = false;
        let evidence = observe(&store, &access, window()).unwrap();
        assert_eq!(evidence.traces.len(), 2);
        let a = &evidence.traces[0];
        assert!(!a.left_censored && !a.right_censored && a.gaps.is_empty());
        assert_eq!(
            a.observations.iter().map(|o| o.state).collect::<Vec<_>>(),
            vec![
                TaskState::Pending,
                TaskState::Running,
                TaskState::Blocked,
                TaskState::Running,
                TaskState::Running,
                TaskState::Cancelled
            ]
        );
        assert_eq!(evidence.transitions.iter().map(|e| e.count).sum::<u64>(), 5);
        assert!(evidence
            .transitions
            .iter()
            .any(|e| e.from == TaskState::Blocked && e.to == TaskState::Running && e.count == 1));
        assert!(!serde_json::to_string(&evidence)
            .unwrap()
            .contains("private objective"));
        let discontinuous = observe(
            &store,
            &access,
            HistoryWindow {
                from: Some(Timestamp::new(9)),
                until: Timestamp::new(14),
            },
        )
        .unwrap();
        assert!(discontinuous.traces[0].left_censored);
        assert!(discontinuous.traces[0]
            .gaps
            .iter()
            .any(|gap| gap.reason == GapReason::RevisionGap));
        assert!(discontinuous.transitions.is_empty());
        let value = routing::execute(
            &mut store,
            &access,
            routing::Request::Transitions {
                from: None,
                until: window().until,
            },
            None,
            Timestamp::new(100),
        )
        .await
        .unwrap();
        assert_eq!(serde_json::from_value::<Evidence>(value).unwrap(), evidence);
        assert_eq!(store.state(), &before);
        drop(store);
        let store = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(observe(&store, &access, window()).unwrap(), evidence);
        access.tasks = Some(BTreeSet::from([TaskId::parse("a").unwrap()]));
        let scoped = observe(&store, &access, window()).unwrap();
        assert_eq!(scoped.traces.len(), 1);
        assert_eq!(scoped.transitions.iter().map(|e| e.count).sum::<u64>(), 4);
        access.tasks = Some(
            (0..6000)
                .map(|i| TaskId::parse(format!("scope-{i:090}")).unwrap())
                .collect(),
        );
        assert!(observe(&store, &access, window())
            .unwrap_err()
            .contains("512 KiB"));
        access.tasks = None;
        access.authority = AuthorityRevision::new(1);
        assert!(observe(&store, &access, window()).is_err());
        access.authority = AuthorityRevision::ZERO;
        access.read = false;
        assert!(observe(&store, &access, window()).is_err());
    }
}

#[tokio::test]
async fn missing_and_windowed_revisions_never_invent_edges_or_current_terminal_state() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(temp.path(), backend).await;
        append(&mut store, &access, "a", TaskState::Pending, 2, true).await;
        append(&mut store, &access, "a", TaskState::Running, 3, true).await;
        append(&mut store, &access, "a", TaskState::Paused, 4, false).await;
        append(&mut store, &access, "a", TaskState::Running, 5, true).await;
        append(&mut store, &access, "a", TaskState::Failed, 9, true).await;
        let evidence = observe(
            &store,
            &access,
            HistoryWindow {
                from: None,
                until: Timestamp::new(6),
            },
        )
        .unwrap();
        assert_eq!(evidence.transitions.len(), 1);
        assert_eq!(evidence.transitions[0].from, TaskState::Pending);
        assert_eq!(evidence.transitions[0].to, TaskState::Running);
        assert_eq!(evidence.traces[0].gaps[0].reason, GapReason::MissingFacts);
        assert!(evidence.traces[0].right_censored); // current row is Failed, historical view isn't
        let partial = observe(
            &store,
            &access,
            HistoryWindow {
                from: Some(Timestamp::new(5)),
                until: Timestamp::new(9),
            },
        )
        .unwrap();
        assert!(partial.traces[0].left_censored && partial.traces[0].right_censored);
        assert!(partial.transitions.is_empty());
        assert!(observe(
            &store,
            &access,
            HistoryWindow {
                from: Some(Timestamp::new(9)),
                until: Timestamp::new(9)
            }
        )
        .is_err());
    }
}

#[tokio::test]
async fn pruning_source_history_removes_transition_evidence_on_both_backends() {
    use vcp_domain::retention_selector::{
        Bound, Criterion, InstantSpec, Selector, TimeWindow, Tree,
    };
    use vcp_memory::retention::{self, Action};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(temp.path(), backend).await;
        append(&mut store, &access, "a", TaskState::Pending, 2, true).await;
        append(&mut store, &access, "a", TaskState::Running, 3, true).await;
        append(&mut store, &access, "a", TaskState::Cancelled, 4, true).await;
        assert_eq!(
            observe(&store, &access, window())
                .unwrap()
                .transitions
                .len(),
            2
        );
        let instant = InstantSpec::parse("1970-01-01T00:00:00.003Z", None).unwrap();
        let event_plan = retention::preview(
            &store,
            &access,
            Selector {
                schema_version: 1,
                tree: Tree::All(vec![
                    Tree::Match(Criterion::Task(TaskId::parse("a").unwrap())),
                    Tree::Match(Criterion::Date(TimeWindow {
                        lower: Some(Bound {
                            instant: instant.clone(),
                            inclusive: true,
                        }),
                        upper: Some(Bound {
                            instant,
                            inclusive: true,
                        }),
                    })),
                ]),
            },
            Action::Purge,
            Timestamp::new(101),
        )
        .unwrap();
        // Retention follows snapshot dependencies: selecting one task fact also
        // removes its task record and remaining history, not just one edge.
        assert!(event_plan
            .dependent
            .contains(&retention::Target::Record(key(Collection::Task, "a"))));
        retention::apply(&mut store, &access, &event_plan, Timestamp::new(101))
            .await
            .unwrap();
        // Logical purge must take effect before any physical cleanup.
        let masked = observe(&store, &access, window()).unwrap();
        assert!(masked.transitions.is_empty());
        assert!(masked.traces.is_empty());
        assert_eq!(masked.excluded_pruned_tasks, 1);
        let plan = retention::preview(
            &store,
            &access,
            Selector {
                schema_version: 1,
                tree: Tree::Match(Criterion::Task(TaskId::parse("a").unwrap())),
            },
            Action::Purge,
            Timestamp::new(101),
        )
        .unwrap();
        retention::apply(&mut store, &access, &plan, Timestamp::new(101))
            .await
            .unwrap();
        let evidence = observe(&store, &access, window()).unwrap();
        assert!(
            evidence.traces.is_empty() && evidence.transitions.is_empty(),
            "{evidence:#?}"
        );
        assert_eq!(evidence.excluded_pruned_tasks, 1);
        drop(store);
        let store = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(observe(&store, &access, window()).unwrap(), evidence);
    }
}
