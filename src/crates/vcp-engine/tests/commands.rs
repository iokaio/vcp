// SPDX-License-Identifier: Apache-2.0
use vcp_domain::{ids::*, revision::*, task::*, verification::*, workspace::*};
use vcp_engine::*;
use vcp_protocol::command::*;
use vcp_store::{contract::*, BackendKind, Store};
fn access() -> Access {
    Access {
        actor: ActorId::parse("human").unwrap(),
        workspace: WorkspaceId::parse("workspace").unwrap(),
        session: SessionId::parse("session").unwrap(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: true,
    }
}

#[tokio::test]
async fn approval_is_bound_to_actor_operation_revision_expiry_and_current_steering() {
    let temporary = tempfile::tempdir().unwrap();
    let mut engine = setup(temporary.path(), BackendKind::Sqlite).await;
    let id = TaskId::new();
    let host = HostFacts::inspect(Timestamp::new(100));
    engine
        .handle(
            command(&engine, creation(&id), Some(id.clone()), Revision::ZERO),
            &access(),
            &host,
        )
        .await
        .unwrap();
    let effect = ToolRunId::new();
    engine
        .handle(
            command(
                &engine,
                Command::ProposeEffect {
                    id: effect.clone(),
                    operation_digest: "a".repeat(64),
                },
                Some(id.clone()),
                Revision::ZERO,
            ),
            &access(),
            &host,
        )
        .await
        .unwrap();
    let approval = Approval {
        id: ApprovalId::new(),
        scope: Scope {
            workspace: access().workspace,
            session: access().session,
            task: id.clone(),
        },
        effect,
        effect_revision: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        operation_digest: "a".repeat(64),
        actor: access().actor,
        policy: PolicyRevision::ZERO,
        expires_at: Timestamp::new(200),
        state: ApprovalState::Pending,
        revision: Revision::ZERO,
        controller: Some(engine.controller().clone()),
        owner_epoch: Some(engine.owner_epoch()),
        authority: Some(AuthorityRevision::ZERO),
        binding: Some(Revision::ZERO),
    };
    engine
        .handle(
            command(
                &engine,
                Command::Ask {
                    approval: approval.clone(),
                },
                Some(id.clone()),
                Revision::ZERO,
            ),
            &access(),
            &host,
        )
        .await
        .unwrap();
    let mut decision = command(
        &engine,
        Command::Decide {
            id: approval.id.clone(),
            operation_digest: "a".repeat(64),
            effect_revision: Revision::ZERO,
            allow: true,
        },
        Some(id.clone()),
        Revision::ZERO,
    );
    let mut foreign = access();
    foreign.actor = ActorId::new();
    let mut wrong = decision.clone();
    wrong.caller = foreign.actor.clone();
    assert!(engine.handle(wrong, &foreign, &host).await.is_err());
    let mut wrong = decision.clone();
    if let Command::Decide {
        operation_digest, ..
    } = &mut wrong.payload
    {
        *operation_digest = "b".repeat(64);
    }
    assert!(engine.handle(wrong, &access(), &host).await.is_err());
    assert!(engine
        .handle(
            decision.clone(),
            &access(),
            &HostFacts::inspect(Timestamp::new(200))
        )
        .await
        .is_err());
    let objective = Objective {
        text: "Changed operation objective".into(),
        constraints: vec![],
        acceptance: vec![],
        source: EventId::new(),
        steering: SteeringRevision::ZERO,
    };
    engine
        .handle(
            command(
                &engine,
                Command::Steer { objective },
                Some(id.clone()),
                Revision::new(1),
            ),
            &access(),
            &host,
        )
        .await
        .unwrap();
    assert!(engine
        .handle(decision.clone(), &access(), &host)
        .await
        .is_err());
    decision.steering = SteeringRevision::new(1);
    assert!(engine.handle(decision, &access(), &host).await.is_err());
    let stored: Approval = engine
        .store()
        .state()
        .record(
            Collection::Approval,
            approval.id.as_str(),
            &access().workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(stored.state, ApprovalState::Pending);
}
#[tokio::test]
async fn policy_questions_commit_waiting_answers_and_grants_once_on_both_backends() {
    use std::collections::BTreeSet;
    use vcp_domain::policy::*;
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for allow in [false, true] {
            let temporary = tempfile::tempdir().unwrap();
            let mut engine = setup(temporary.path(), backend).await;
            let mut access = access();
            let host = HostFacts::inspect(Timestamp::new(100));
            let trust = command(
                &engine,
                Command::SetWorkspaceTrust {
                    trust: Trust::Trusted,
                },
                None,
                Revision::ZERO,
            );
            engine.handle(trust, &access, &host).await.unwrap();
            access.authority = AuthorityRevision::new(1);
            let root = RootId::new();
            let policy = Policy {
                workspace: access.workspace.clone(),
                revision: PolicyRevision::ZERO,
                mode: Autonomy::Ask,
                denials: vec![],
                workspace_roots: BTreeSet::from([root]),
                automatic_effects: BTreeSet::new(),
                timeout_ceiling_ms: Units::new(1000),
                output_ceiling_bytes: ByteCount::new(4096),
            };
            let set = command(
                &engine,
                Command::SetPolicy {
                    policy: policy.clone(),
                },
                None,
                Revision::ZERO,
            );
            engine.handle(set, &access, &host).await.unwrap();
            access.authority = AuthorityRevision::new(2);
            let id = TaskId::new();
            engine
                .handle(
                    command(&engine, creation(&id), Some(id.clone()), Revision::ZERO),
                    &access,
                    &host,
                )
                .await
                .unwrap();
            let effect = ToolRunId::new();
            engine
                .handle(
                    command(
                        &engine,
                        Command::ProposeEffect {
                            id: effect.clone(),
                            operation_digest: "a".repeat(64),
                        },
                        Some(id.clone()),
                        Revision::ZERO,
                    ),
                    &access,
                    &host,
                )
                .await
                .unwrap();
            let approval = Approval {
                id: ApprovalId::new(),
                scope: Scope {
                    workspace: access.workspace.clone(),
                    session: access.session.clone(),
                    task: id.clone(),
                },
                effect,
                effect_revision: Revision::ZERO,
                steering: SteeringRevision::ZERO,
                operation_digest: "a".repeat(64),
                actor: access.actor.clone(),
                policy: PolicyRevision::ZERO,
                expires_at: Timestamp::new(200),
                state: ApprovalState::Pending,
                revision: Revision::ZERO,
                controller: Some(engine.controller().clone()),
                owner_epoch: Some(engine.owner_epoch()),
                authority: Some(access.authority),
                binding: Some(Revision::ZERO),
            };
            let ask = command(
                &engine,
                Command::Ask {
                    approval: approval.clone(),
                },
                Some(id.clone()),
                Revision::ZERO,
            );
            // Headless JSONL returns after the same durable transaction. It does
            // not wait for terminal input or treat a highlighted answer as consent.
            let response = engine
                .jsonl(&serde_json::to_vec(&ask).unwrap(), &access, &host)
                .await
                .unwrap();
            assert_eq!(
                serde_json::from_slice::<CommandReceipt>(&response)
                    .unwrap()
                    .result,
                CommandResult::Accepted {
                    revision: Revision::ZERO
                }
            );
            let task: Task = engine
                .store()
                .state()
                .record(Collection::Task, id.as_str(), &access.workspace)
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(task.state, TaskState::WaitingForInput);
            if allow {
                engine
                    .handle(
                        command(
                            &engine,
                            Command::Transition {
                                next: TaskState::Paused,
                                reason: "owner paused while approval pending".into(),
                                verification: None,
                            },
                            Some(id.clone()),
                            task.revision,
                        ),
                        &access,
                        &host,
                    )
                    .await
                    .unwrap();
            }
            let decide = Command::Decide {
                id: approval.id.clone(),
                operation_digest: approval.operation_digest.clone(),
                effect_revision: Revision::ZERO,
                allow,
            };
            engine
                .handle(
                    command(&engine, decide.clone(), Some(id.clone()), Revision::ZERO),
                    &access,
                    &host,
                )
                .await
                .unwrap();
            let count = engine
                .store()
                .state()
                .events
                .iter()
                .filter(|e| e.event.kind == vcp_protocol::event::EventKind::ApprovalResolved)
                .count();
            // A different command ID carrying the same answer returns the saved
            // result without a second decision or grant. Opposite answers reject.
            engine
                .handle(
                    command(&engine, decide, Some(id.clone()), Revision::ZERO),
                    &access,
                    &host,
                )
                .await
                .unwrap();
            assert_eq!(
                engine
                    .store()
                    .state()
                    .events
                    .iter()
                    .filter(|e| e.event.kind == vcp_protocol::event::EventKind::ApprovalResolved)
                    .count(),
                count
            );
            let opposite = Command::Decide {
                id: approval.id.clone(),
                operation_digest: approval.operation_digest.clone(),
                effect_revision: Revision::ZERO,
                allow: !allow,
            };
            assert!(engine
                .handle(
                    command(&engine, opposite, Some(id.clone()), Revision::ZERO),
                    &access,
                    &host
                )
                .await
                .is_err());
            let grants =
                vcp_engine::policy::grants(engine.store().state(), &access.workspace).unwrap();
            assert_eq!(grants.len(), usize::from(allow));
            if allow {
                assert_eq!(grants[0].approval.as_ref(), Some(&approval.id));
            }
            // Accepting an answer never resumes a waiting/paused task itself.
            let task: Task = engine
                .store()
                .state()
                .record(Collection::Task, id.as_str(), &access.workspace)
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(
                task.state,
                if allow {
                    TaskState::Paused
                } else {
                    TaskState::WaitingForInput
                }
            );
            drop(engine);
            let reopened =
                Engine::new(Store::open(temporary.path(), backend, &[]).await.unwrap()).unwrap();
            assert_eq!(
                vcp_engine::policy::current(reopened.store().state(), &access.workspace).unwrap(),
                policy
            );
            assert_eq!(
                vcp_engine::policy::grants(reopened.store().state(), &access.workspace).unwrap(),
                grants
            );
            let saved: Approval = reopened
                .store()
                .state()
                .record(
                    Collection::Approval,
                    approval.id.as_str(),
                    &access.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(
                saved.state,
                if allow {
                    ApprovalState::Allowed
                } else {
                    ApprovalState::Denied
                }
            );
        }
    }
}

#[tokio::test]
async fn pending_question_survives_reopen_but_old_owner_cannot_supply_new_authority() {
    use std::collections::BTreeSet;
    use vcp_domain::policy::*;
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temporary = tempfile::tempdir().unwrap();
        let mut engine = setup(temporary.path(), backend).await;
        let mut access = access();
        let host = HostFacts::inspect(Timestamp::new(100));
        engine
            .handle(
                command(
                    &engine,
                    Command::SetWorkspaceTrust {
                        trust: Trust::Trusted,
                    },
                    None,
                    Revision::ZERO,
                ),
                &access,
                &host,
            )
            .await
            .unwrap();
        access.authority = AuthorityRevision::new(1);
        let policy = Policy {
            workspace: access.workspace.clone(),
            revision: PolicyRevision::ZERO,
            mode: Autonomy::Ask,
            denials: vec![],
            workspace_roots: BTreeSet::new(),
            automatic_effects: BTreeSet::new(),
            timeout_ceiling_ms: Units::new(1000),
            output_ceiling_bytes: ByteCount::new(4096),
        };
        engine
            .handle(
                command(&engine, Command::SetPolicy { policy }, None, Revision::ZERO),
                &access,
                &host,
            )
            .await
            .unwrap();
        access.authority = AuthorityRevision::new(2);
        let id = TaskId::new();
        engine
            .handle(
                command(&engine, creation(&id), Some(id.clone()), Revision::ZERO),
                &access,
                &host,
            )
            .await
            .unwrap();
        let effect = ToolRunId::new();
        engine
            .handle(
                command(
                    &engine,
                    Command::ProposeEffect {
                        id: effect.clone(),
                        operation_digest: "a".repeat(64),
                    },
                    Some(id.clone()),
                    Revision::ZERO,
                ),
                &access,
                &host,
            )
            .await
            .unwrap();
        let approval = Approval {
            id: ApprovalId::new(),
            scope: Scope {
                workspace: access.workspace.clone(),
                session: access.session.clone(),
                task: id.clone(),
            },
            effect,
            effect_revision: Revision::ZERO,
            steering: SteeringRevision::ZERO,
            operation_digest: "a".repeat(64),
            actor: access.actor.clone(),
            policy: PolicyRevision::ZERO,
            expires_at: Timestamp::new(200),
            state: ApprovalState::Pending,
            revision: Revision::ZERO,
            controller: Some(engine.controller().clone()),
            owner_epoch: Some(engine.owner_epoch()),
            authority: Some(access.authority),
            binding: Some(Revision::ZERO),
        };
        engine
            .handle(
                command(
                    &engine,
                    Command::Ask {
                        approval: approval.clone(),
                    },
                    Some(id.clone()),
                    Revision::ZERO,
                ),
                &access,
                &host,
            )
            .await
            .unwrap();
        drop(engine);
        let mut reopened =
            Engine::new(Store::open(temporary.path(), backend, &[]).await.unwrap()).unwrap();
        let decide = command(
            &reopened,
            Command::Decide {
                id: approval.id.clone(),
                operation_digest: approval.operation_digest.clone(),
                effect_revision: Revision::ZERO,
                allow: true,
            },
            Some(id),
            Revision::ZERO,
        );
        assert!(reopened.handle(decide, &access, &host).await.is_err());
        let saved: Approval = reopened
            .store()
            .state()
            .record(
                Collection::Approval,
                approval.id.as_str(),
                &access.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(saved.state, ApprovalState::Pending);
        assert!(
            vcp_engine::policy::grants(reopened.store().state(), &access.workspace)
                .unwrap()
                .is_empty()
        );
    }
}

fn command(
    engine: &Engine<Store>,
    payload: Command,
    task: Option<TaskId>,
    expected: Revision,
) -> CommandEnvelope {
    let access = access();
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
fn binding() -> Binding {
    Binding {
        host: HostId::new(),
        root: "C:/synthetic".into(),
        repository: "fixture".into(),
        worktree: "main".into(),
        revision: Revision::ZERO,
    }
}
fn creation(id: &TaskId) -> Command {
    Command::CreateTask {
        root: id.clone(),
        parent: None,
        fork_origin: None,
        objective: Objective {
            text: "Explain the fixture".into(),
            constraints: vec![],
            acceptance: vec!["cite evidence".into()],
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
async fn setup(root: &std::path::Path, kind: BackendKind) -> Engine<Store> {
    let mut engine = Engine::new(Store::open(root, kind, &[]).await.unwrap()).unwrap();
    let init = command(
        &engine,
        Command::Initialize { binding: binding() },
        None,
        Revision::ZERO,
    );
    engine
        .handle(init, &access(), &HostFacts::inspect(Timestamp::new(100)))
        .await
        .unwrap();
    engine
}
#[tokio::test]
async fn interactive_and_jsonl_share_receipts_after_state_advance_and_restart() {
    for kind in [BackendKind::Sqlite, BackendKind::Files] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("canonical");
        let mut engine = setup(&root, kind).await;
        let id = TaskId::new();
        let create = command(&engine, creation(&id), Some(id.clone()), Revision::ZERO);
        let host = HostFacts::inspect(Timestamp::new(101));
        let receipt = engine
            .handle(create.clone(), &access(), &host)
            .await
            .unwrap();
        let encoded = engine
            .jsonl(&serde_json::to_vec(&create).unwrap(), &access(), &host)
            .await
            .unwrap();
        assert_eq!(encoded, receipt.jsonl().unwrap());
        let count = engine.store().state().events.len();
        let mut different = create.clone();
        different.expected = Revision::new(9);
        assert!(engine.handle(different, &access(), &host).await.is_err());
        assert_eq!(engine.store().state().events.len(), count);
        drop(engine);
        let mut reopened = Engine::new(Store::open(&root, kind, &[]).await.unwrap()).unwrap();
        assert_ne!(reopened.controller(), &create.controller);
        assert_eq!(
            reopened
                .handle(create.clone(), &access(), &host)
                .await
                .unwrap(),
            receipt
        );
        let mut denied = access();
        denied.read = false;
        assert!(reopened.handle(create, &denied, &host).await.is_err());
        assert_eq!(reopened.store().state().events.len(), count);
    }
}
#[tokio::test]
async fn question_preserves_objective_pause_preserves_children_and_fork_is_independent() {
    let temporary = tempfile::tempdir().unwrap();
    let mut engine = setup(temporary.path(), BackendKind::Sqlite).await;
    let root = TaskId::new();
    let create = command(&engine, creation(&root), Some(root.clone()), Revision::ZERO);
    let mut host = HostFacts::inspect(Timestamp::new(100));
    host.may_execute = true;
    engine.handle(create, &access(), &host).await.unwrap();
    let child = TaskId::new();
    let mut child_payload = creation(&child);
    if let Command::CreateTask {
        root: task_root,
        parent,
        ..
    } = &mut child_payload
    {
        *task_root = root.clone();
        *parent = Some(root.clone());
    }
    engine
        .handle(
            command(&engine, child_payload, Some(child.clone()), Revision::ZERO),
            &access(),
            &host,
        )
        .await
        .unwrap();
    engine
        .handle(
            command(
                &engine,
                Command::Transition {
                    next: TaskState::Running,
                    reason: "start".into(),
                    verification: None,
                },
                Some(root.clone()),
                Revision::ZERO,
            ),
            &access(),
            &host,
        )
        .await
        .unwrap();
    engine
        .handle(
            command(
                &engine,
                Command::Transition {
                    next: TaskState::Paused,
                    reason: "owner pause".into(),
                    verification: None,
                },
                Some(root.clone()),
                Revision::new(1),
            ),
            &access(),
            &host,
        )
        .await
        .unwrap();
    let before = engine
        .store()
        .state()
        .record(Collection::Task, root.as_str(), &access().workspace)
        .unwrap()
        .clone();
    let inspected = engine
        .handle(
            command(
                &engine,
                Command::Inspect,
                Some(root.clone()),
                Revision::ZERO,
            ),
            &access(),
            &host,
        )
        .await
        .unwrap();
    assert!(matches!(
        inspected.result,
        CommandResult::Inspection { task: Some(_) }
    ));
    assert_eq!(
        engine
            .store()
            .state()
            .record(Collection::Task, root.as_str(), &access().workspace)
            .unwrap(),
        &before
    );
    assert!(engine
        .store()
        .state()
        .record(Collection::Task, child.as_str(), &access().workspace)
        .is_ok());
    let resume = command(
        &engine,
        Command::Transition {
            next: TaskState::Running,
            reason: "resume".into(),
            verification: None,
        },
        Some(root.clone()),
        Revision::new(2),
    );
    assert!(engine
        .handle(resume.clone(), &access(), &host)
        .await
        .is_err());
    host.resume = Some(ResumeEvidence {
        workspace_current: true,
        policy_current: true,
        budget_current: true,
        effects_reconciled: true,
        owner_current: true,
    });
    engine.handle(resume, &access(), &host).await.unwrap();
    let fork = TaskId::new();
    let mut payload = creation(&fork);
    if let Command::CreateTask { fork_origin, .. } = &mut payload {
        *fork_origin = Some(root.clone());
    }
    engine
        .handle(
            command(&engine, payload, Some(fork.clone()), Revision::ZERO),
            &access(),
            &host,
        )
        .await
        .unwrap();
    let original = engine
        .store()
        .state()
        .record(Collection::Task, root.as_str(), &access().workspace)
        .unwrap()
        .clone();
    engine
        .handle(
            command(
                &engine,
                Command::Transition {
                    next: TaskState::Cancelled,
                    reason: "cancel fork".into(),
                    verification: None,
                },
                Some(fork),
                Revision::ZERO,
            ),
            &access(),
            &host,
        )
        .await
        .unwrap();
    assert_eq!(
        engine
            .store()
            .state()
            .record(Collection::Task, root.as_str(), &access().workspace)
            .unwrap(),
        &original
    );
}
#[tokio::test]
async fn revoked_authority_blocks_old_receipt_and_stale_owner_blocks_new_work() {
    let temporary = tempfile::tempdir().unwrap();
    let mut engine = setup(temporary.path(), BackendKind::Files).await;
    let host = HostFacts::inspect(Timestamp::new(100));
    let inspect = command(&engine, Command::Inspect, None, Revision::ZERO);
    engine
        .handle(inspect.clone(), &access(), &host)
        .await
        .unwrap();
    let mut stale = command(&engine, Command::Inspect, None, Revision::ZERO);
    stale.controller = ControllerId::new();
    assert!(engine.handle(stale, &access(), &host).await.is_err());
    let rebind = command(
        &engine,
        Command::Rebind {
            binding: Binding {
                root: "D:/moved".into(),
                ..binding()
            },
        },
        None,
        Revision::ZERO,
    );
    engine.handle(rebind, &access(), &host).await.unwrap();
    assert!(engine
        .handle(inspect.clone(), &access(), &host)
        .await
        .is_err());
    let mut renewed = access();
    renewed.authority = AuthorityRevision::new(1);
    assert!(engine.handle(inspect, &renewed, &host).await.is_ok());
}

#[tokio::test]
async fn bounded_pull_subscription_reconnects_without_duplicates_or_mixed_snapshots() {
    use vcp_protocol::subscription::*;
    let temporary = tempfile::tempdir().unwrap();
    let mut engine = setup(temporary.path(), BackendKind::Files).await;
    let host = HostFacts::inspect(Timestamp::new(100));
    for _ in 0..6 {
        engine
            .handle(
                command(&engine, Command::Inspect, None, Revision::ZERO),
                &access(),
                &host,
            )
            .await
            .unwrap();
    }
    let first = engine
        .subscribe(&access(), SessionSeq::ZERO, 2, host.now)
        .unwrap();
    let mut cursor = first.clone();
    let mut ids = std::collections::BTreeSet::new();
    let expected = engine.store().state().events.len();
    engine
        .handle(
            command(&engine, Command::Inspect, None, Revision::ZERO),
            &access(),
            &host,
        )
        .await
        .unwrap();
    loop {
        match engine.events(&access(), &cursor, host.now).unwrap() {
            EventPage::Events {
                events,
                next_cursor,
                at_end,
                snapshot_watermark,
            } => {
                assert_eq!(snapshot_watermark, first.watermark);
                assert!(events.len() <= 2);
                for event in events {
                    assert!(ids.insert(event.event.id));
                }
                cursor = next_cursor;
                if at_end {
                    break;
                }
            }
            _ => panic!("unexpected gap"),
        }
    }
    assert_eq!(ids.len(), expected);
    assert!(matches!(
        engine
            .events(&access(), &first, Timestamp::new(60_100))
            .unwrap(),
        EventPage::Gap {
            reason: GapReason::SnapshotExpired,
            ..
        }
    ));
    let mut changed = first.clone();
    changed.limit = 128;
    assert!(matches!(
        engine.events(&access(), &changed, host.now).unwrap(),
        EventPage::Gap {
            reason: GapReason::CursorChanged,
            ..
        }
    ));
    for _ in 1..16 {
        engine
            .subscribe(&access(), SessionSeq::ZERO, 1, host.now)
            .unwrap();
    }
    assert!(engine
        .subscribe(&access(), SessionSeq::ZERO, 1, host.now)
        .is_err());
    engine.unsubscribe(&first.snapshot);
    assert!(engine
        .subscribe(&access(), SessionSeq::ZERO, 1, host.now)
        .is_ok());
}
