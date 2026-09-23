// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::HostFacts;
use vcp_domain::{artifact::ArtifactSpec, controller::Reason, workspace::Binding};
use vcp_protocol::{
    command::{Command, CommandEnvelope},
    methods,
};
use vcp_store::{artifact::ArtifactWriter, BackendKind, Store};

fn id(value: &str) -> methods::Id {
    value.to_owned().try_into().unwrap()
}
fn request() -> TurnStart {
    TurnStart {
        scope: methods::Scope {
            workspace: id("workspace"),
            session: id("session"),
        },
        mutation: methods::Mutation {
            command_id: id("accepted-run"),
            expected_revision: 0.into(),
            steering_revision: 0.into(),
        },
        task: id("root-task"),
        turn: id("caller-turn"),
        objective: "Inspect and report".into(),
        constraints: vec!["preserve user work".into()],
        acceptance: vec!["cite evidence".into()],
        budget: methods::Budget {
            cap_micros: u64::MAX.into(),
            currency: Currency::Usd,
            max_requests: 3,
            deadline_seconds: 30,
        },
    }
}
fn facts() -> StartFacts {
    StartFacts {
        fingerprint: Fingerprint {
            repository: "a".repeat(64),
            buffers: "b".repeat(64),
            environment: "c".repeat(64),
        },
        editing: false,
        required_checks: vec![],
        protected: Micros::new(7),
        policy: PolicyRevision::ZERO,
    }
}
async fn fixture(
    path: &std::path::Path,
    backend: BackendKind,
) -> (Engine<Store>, Access, ControllerId, ControllerToken) {
    let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
    let mut access = Access {
        actor: ActorId::parse("owner").unwrap(),
        workspace: WorkspaceId::parse("workspace").unwrap(),
        session: SessionId::parse("session").unwrap(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: true,
    };
    let command = CommandEnvelope {
        version: 1,
        id: CommandId::new(),
        workspace: access.workspace.clone(),
        session: access.session.clone(),
        task: None,
        caller: access.actor.clone(),
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        payload: Command::Initialize {
            binding: Binding {
                host: HostId::new(),
                root: "C:/public-start-fixture".into(),
                repository: "fixture".into(),
                worktree: "main".into(),
                revision: Revision::ZERO,
            },
        },
    };
    engine
        .handle(command, &access, &HostFacts::inspect(Timestamp::new(1)))
        .await
        .unwrap();
    access.bootstrap = false;
    let connection = ControllerId::new();
    engine
        .acquire_controller(
            &access,
            &connection,
            CommandId::new(),
            None,
            Timestamp::new(2),
        )
        .await
        .unwrap();
    let token = engine.controller_token(&access, &connection).unwrap();
    (engine, access, connection, token)
}
fn prepare(
    engine: &Engine<Store>,
    access: &Access,
    connection: &ControllerId,
    token: &ControllerToken,
) -> PreparedPublicStart {
    match engine
        .prepare_public_start(request(), access, connection, token)
        .unwrap()
    {
        PublicStartAdmission::Ready(prepared) => prepared,
        _ => panic!("new request must prepare"),
    }
}
fn trigger(engine: &Engine<Store>) -> ArtifactDescriptor {
    let request = request();
    let mut writer = engine
        .store()
        .spool()
        .create(ArtifactSpec {
            id: ArtifactId::new(),
            scope: scope(&request).unwrap(),
            media_type: "text/plain".into(),
            schema: "coding-turn-input/1".into(),
            source: "trusted fixture".into(),
            channel: Channel::Evidence,
            retention: "history".into(),
            omissions: vec![Omission::AuthenticationHeaders, Omission::RecoveryMaterial],
        })
        .unwrap();
    writer.write_chunk(request.objective.as_bytes()).unwrap();
    writer.finalize().unwrap()
}

#[tokio::test]
async fn atomic_acceptance_keeps_pending_run_and_caller_turn_without_constructor_or_duplicate() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let (mut engine, access, connection, token) = fixture(temp.path(), backend).await;
        let prepared = prepare(&engine, &access, &connection, &token);
        let duplicate = prepare(&engine, &access, &connection, &token);
        let trigger = trigger(&engine);
        let before = engine.store().state().clone();
        let PublicStartOutcome::Accepted(receipt) = engine
            .commit_public_start(prepared, &access, &facts(), &trigger, Timestamp::new(3))
            .await
            .unwrap()
        else {
            panic!("fresh acceptance")
        };
        let state = engine.store().state();
        assert_eq!(state.watermark, before.watermark.next().unwrap());
        assert_eq!(state.records.len(), before.records.len() + 4);
        assert_eq!(state.events.len(), before.events.len() + 4);
        assert_eq!(state.commands.len(), before.commands.len() + 1);
        let selected = scope(&request()).unwrap();
        let task: Task = state
            .record(Collection::Task, selected.task.as_str(), &access.workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(task.state, TaskState::Pending);
        assert_eq!(task.objectives[0].constraints, request().constraints);
        assert_eq!(task.objectives[0].acceptance, request().acceptance);
        let turn = crate::public::current_public_turn(state, &selected)
            .unwrap()
            .unwrap();
        assert_eq!(turn.id.as_str(), "caller-turn");
        assert_eq!(turn.state, TurnState::Queued);
        assert_eq!(turn.trigger, trigger.spec.id);
        let proof = engine
            .check_accepted_public_start(&request(), &receipt, &access, &connection, &token)
            .unwrap();
        assert_eq!(proof.task, task);
        assert_eq!(proof.turn, turn);
        assert_eq!(proof.trigger, trigger);
        let ledger: Ledger = state
            .record(
                Collection::Ledger,
                selected.task.as_str(),
                &access.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(ledger.cap.get(), u64::MAX);
        assert_eq!(ledger.protected.get(), 7);
        assert_eq!(proof.ledger, ledger);
        assert_eq!(
            (ledger.settled, ledger.active, ledger.unresolved),
            (Micros::ZERO, Micros::ZERO, Micros::ZERO)
        );
        assert!(state.records.values().all(|row| !matches!(
            row.collection,
            Collection::Attempt | Collection::Reservation | Collection::Effect
        )));
        assert_eq!(
            receipt.digest,
            Call::TurnStart(request())
                .digest(access.actor.as_str())
                .unwrap()
        );
        let accepted = state.clone();
        // No retained constructor is run: the accepted Pending/Queued pair stays
        // recoverable even if the caller disappears at this exact boundary.
        let mut unusable = trigger.clone();
        unusable.state = CaptureState::Aborted;
        let PublicStartOutcome::Replay(replayed) = engine
            .commit_public_start(duplicate, &access, &facts(), &unusable, Timestamp::new(4))
            .await
            .unwrap()
        else {
            panic!("concurrent prepared duplicate must replay")
        };
        assert_eq!(replayed, receipt);
        assert_eq!(*engine.store().state(), accepted);
        let mut occupied = request();
        occupied.mutation.command_id = id("another-command");
        assert!(matches!(
            engine.prepare_public_start(occupied, &access, &connection, &token),
            Err(PublicError::StaleState)
        ));
        let mut changed = request();
        changed.budget.max_requests += 1;
        assert!(matches!(
            engine.prepare_public_start(changed, &access, &connection, &token),
            Err(PublicError::CommandConflict)
        ));
        let mut denied = access.clone();
        denied.write = false;
        assert!(matches!(
            engine.prepare_public_start(request(), &denied, &connection, &token),
            Err(PublicError::Access)
        ));
        assert_eq!(*engine.store().state(), accepted);
        engine.into_store().close().await.unwrap();

        let mut engine =
            Engine::new(Store::open(temp.path(), backend, &[]).await.unwrap()).unwrap();
        assert!(matches!(
            engine.prepare_public_start(request(), &access, &connection, &token),
            Err(PublicError::Access)
        ));
        engine
            .recover_controller(
                &access,
                &connection,
                CommandId::new(),
                token.revision(),
                token.generation(),
                Timestamp::new(5),
            )
            .await
            .unwrap();
        engine
            .acquire_controller(
                &access,
                &connection,
                CommandId::new(),
                Some(token.revision().next().unwrap()),
                Timestamp::new(6),
            )
            .await
            .unwrap();
        let fresh = engine.controller_token(&access, &connection).unwrap();
        let before = engine.store().state().clone();
        let PublicStartAdmission::Replay(replayed) = engine
            .prepare_public_start(request(), &access, &connection, &fresh)
            .unwrap()
        else {
            panic!("restarted authorized retry must replay")
        };
        assert_eq!(replayed, receipt);
        assert_eq!(*engine.store().state(), before);
        let retained = retained_start_budget(engine.store().state(), &scope(&request()).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(retained.budget, request().budget);
        assert_eq!(retained.accepted_at, Timestamp::new(3));
        let original = crate::rpc::acceptance(&engine, &access, &receipt).unwrap();
        let methods::ResultValue::Acceptance(original) = original else {
            panic!("acceptance")
        };
        assert_eq!(
            original.turn.as_ref().map(|turn| turn.as_str()),
            Some("caller-turn")
        );
        // A later distinct turn must never replace the original command's turn
        // in command/read or a delayed acknowledgement.
        engine
            .handle(
                CommandEnvelope {
                    version: 1,
                    id: CommandId::new(),
                    workspace: access.workspace.clone(),
                    session: access.session.clone(),
                    task: Some(TaskId::parse(request().task.as_str()).unwrap()),
                    caller: access.actor.clone(),
                    controller: engine.controller().clone(),
                    owner_epoch: engine.owner_epoch(),
                    expected: Revision::ZERO,
                    steering: SteeringRevision::ZERO,
                    payload: Command::StartTurn {
                        id: TurnId::parse("later-turn").unwrap(),
                        trigger: trigger.spec.id.clone(),
                    },
                },
                &access,
                &HostFacts::inspect(Timestamp::new(7)),
            )
            .await
            .unwrap();
        let current =
            crate::public::current_public_turn(engine.store().state(), &scope(&request()).unwrap())
                .unwrap()
                .unwrap();
        assert_eq!(current.id.as_str(), "later-turn");
        assert_eq!(
            crate::rpc::acceptance(&engine, &access, &receipt).unwrap(),
            methods::ResultValue::Acceptance(original)
        );
        let original_sequence = engine
            .store()
            .state()
            .events
            .iter()
            .find(|event| {
                event.watermark == receipt.watermark
                    && event.event.kind == EventKind::TurnTransition
            })
            .unwrap()
            .sequence;
        let mask = vcp_domain::retention::RetentionMask {
            schema_version: 1,
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            first: original_sequence,
            last: original_sequence,
            artifacts: vec![],
            deletion: DeletionEpoch::ZERO,
            reason: "exclude original turn identity proof".into(),
        };
        let record = Record::typed(
            Collection::Tombstone,
            "start-proof-mask",
            access.workspace.clone(),
            Revision::ZERO,
            &mask,
        )
        .unwrap();
        engine
            .store_mut()
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: before.watermark.next().unwrap(),
                mutations: vec![Mutation::Put {
                    expected: None,
                    record,
                }],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        assert!(crate::rpc::acceptance(&engine, &access, &receipt).is_err());
        engine.into_store().close().await.unwrap();
    }
}

#[tokio::test]
async fn retained_budget_requires_original_public_genesis_and_proves_legacy_absence() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let (mut engine, access, connection, token) = fixture(temp.path(), backend).await;
        let prepared = prepare(&engine, &access, &connection, &token);
        let trigger = trigger(&engine);
        engine
            .commit_public_start(prepared, &access, &facts(), &trigger, Timestamp::new(100))
            .await
            .unwrap();
        let selected = scope(&request()).unwrap();
        let state = engine.store().state().clone();
        let genesis = state
            .events
            .iter()
            .position(|event| event.event.kind == EventKind::TaskCreated)
            .unwrap();
        let retained = retained_start_budget(&state, &selected).unwrap().unwrap();
        assert_eq!(retained.budget, request().budget);
        assert_eq!(retained.accepted_at, Timestamp::new(100));
        for case in [
            "missing", "marker", "redacted", "masked", "limit", "receipt", "scope",
        ] {
            let mut changed = state.clone();
            match case {
                "missing" => {
                    changed.events.remove(genesis);
                }
                "marker" => {
                    changed.events[genesis]
                        .event
                        .data
                        .as_object_mut()
                        .unwrap()
                        .remove("public_start");
                }
                "redacted" => {
                    changed.events[genesis].redaction =
                        Some(vcp_domain::redaction::ContentRedaction {
                            deletion: DeletionEpoch::new(1),
                            original_digest: "d".repeat(64),
                        });
                    changed.events[genesis].event.data = serde_json::Value::Null;
                }
                "masked" => {
                    let mask = vcp_domain::retention::RetentionMask {
                        schema_version: 1,
                        workspace: access.workspace.clone(),
                        session: access.session.clone(),
                        first: changed.events[genesis].sequence,
                        last: changed.events[genesis].sequence,
                        artifacts: vec![],
                        deletion: DeletionEpoch::ZERO,
                        reason: "logical genesis exclusion".into(),
                    };
                    let row = Record::typed(
                        Collection::Tombstone,
                        "genesis-mask",
                        access.workspace.clone(),
                        Revision::ZERO,
                        &mask,
                    )
                    .unwrap();
                    changed.records.insert(row.key(), row);
                }
                "limit" => {
                    changed.events[genesis].event.data["public_start"]["budget"]["max_requests"] =
                        serde_json::json!(30)
                }
                "receipt" => changed.commands.clear(),
                "scope" => changed.events[genesis].event.session = SessionId::new(),
                _ => unreachable!(),
            }
            assert!(
                retained_start_budget(&changed, &selected).is_err(),
                "{case}"
            );
        }
        engine.into_store().close().await.unwrap();

        // Real legacy CreateTask normalizes its objective source to genesis;
        // proving that genesis permits None, while deleting it does not.
        let legacy_temp = tempfile::tempdir().unwrap();
        let (mut engine, access, _, _) = fixture(legacy_temp.path(), backend).await;
        let envelope = |engine: &Engine<Store>, payload| CommandEnvelope {
            version: 1,
            id: CommandId::new(),
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            task: Some(selected.task.clone()),
            caller: access.actor.clone(),
            controller: engine.controller().clone(),
            owner_epoch: engine.owner_epoch(),
            expected: Revision::ZERO,
            steering: SteeringRevision::ZERO,
            payload,
        };
        let command = envelope(
            &engine,
            Command::CreateTask {
                root: selected.task.clone(),
                parent: None,
                fork_origin: None,
                objective: Objective {
                    text: "legacy objective".into(),
                    constraints: vec![],
                    acceptance: vec![],
                    source: EventId::new(),
                    steering: SteeringRevision::ZERO,
                },
                fingerprint: facts().fingerprint,
                editing: false,
                required_checks: vec![],
            },
        );
        engine
            .handle(command, &access, &HostFacts::inspect(Timestamp::new(2)))
            .await
            .unwrap();
        assert_eq!(
            retained_start_budget(engine.store().state(), &selected).unwrap(),
            None
        );
        let mut missing = engine.store().state().clone();
        missing
            .events
            .retain(|event| event.event.kind != EventKind::TaskCreated);
        assert!(retained_start_budget(&missing, &selected).is_err());
        let trigger = super::tests::trigger(&engine);
        let record = Record::typed(
            Collection::Artifact,
            trigger.spec.id.as_str(),
            access.workspace.clone(),
            Revision::ZERO,
            &trigger,
        )
        .unwrap();
        engine
            .store_mut()
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: missing.watermark,
                mutations: vec![Mutation::Put {
                    expected: None,
                    record,
                }],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        let command = envelope(
            &engine,
            Command::StartTurn {
                id: TurnId::new(),
                trigger: trigger.spec.id,
            },
        );
        let receipt = engine
            .handle(command, &access, &HostFacts::inspect(Timestamp::new(3)))
            .await
            .unwrap();
        let event = engine
            .store()
            .state()
            .events
            .iter()
            .find(|event| event.watermark == receipt.watermark)
            .unwrap()
            .event
            .id
            .clone();
        let cancelled = envelope(
            &engine,
            Command::Transition {
                next: TaskState::Cancelled,
                reason: "legacy fixture finished before retention".into(),
                verification: None,
            },
        );
        engine
            .handle(cancelled, &access, &HostFacts::inspect(Timestamp::new(4)))
            .await
            .unwrap();
        loop {
            let turn: Turn = engine
                .store()
                .state()
                .records
                .values()
                .find(|row| row.collection == Collection::Turn)
                .unwrap()
                .decode()
                .unwrap();
            if turn.state == TurnState::Cancelled {
                break;
            }
            let mut cancel = envelope(
                &engine,
                Command::AdvanceTurn {
                    id: turn.id,
                    next: if turn.state == TurnState::Cancelling {
                        TurnState::Cancelled
                    } else {
                        TurnState::Cancelling
                    },
                    reason: "legacy turn fully drained before retention".into(),
                },
            );
            cancel.expected = turn.revision;
            engine
                .handle(cancel, &access, &HostFacts::inspect(Timestamp::new(4)))
                .await
                .unwrap();
        }
        let mut workspace: Workspace = engine
            .store()
            .state()
            .record(
                Collection::Workspace,
                access.workspace.as_str(),
                &access.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let expected = workspace.revision;
        workspace.revision = expected.next().unwrap();
        workspace.deletion = DeletionEpoch::new(1);
        let record = Record::typed(
            Collection::Workspace,
            workspace.id.as_str(),
            workspace.id.clone(),
            workspace.revision,
            &workspace,
        )
        .unwrap();
        let watermark = engine.store().state().watermark;
        engine
            .store_mut()
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: watermark,
                mutations: vec![Mutation::Put {
                    expected: Some(expected),
                    record,
                }],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        let candidate = engine
            .store()
            .retention_candidate(
                &Default::default(),
                &std::collections::BTreeSet::from([event]),
                &Default::default(),
            )
            .unwrap();
        engine
            .store_mut()
            .rewrite_base(candidate, &[])
            .await
            .unwrap();
        let methods::ResultValue::Acceptance(projected) =
            crate::rpc::acceptance(&engine, &access, &receipt).unwrap()
        else {
            panic!("legacy receipt")
        };
        assert!(projected.turn.is_none());
        engine.into_store().close().await.unwrap();
    }
}

#[tokio::test]
async fn accepted_receipt_does_not_reauthorize_construction_after_task_changes() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let (mut engine, access, connection, token) = fixture(temp.path(), backend).await;
        let prepared = prepare(&engine, &access, &connection, &token);
        let trigger = trigger(&engine);
        let PublicStartOutcome::Accepted(receipt) = engine
            .commit_public_start(prepared, &access, &facts(), &trigger, Timestamp::new(3))
            .await
            .unwrap()
        else {
            panic!("fresh acceptance")
        };
        let mut denied = access.clone();
        denied.write = false;
        assert!(matches!(
            engine.check_accepted_public_start(&request(), &receipt, &denied, &connection, &token),
            Err(PublicError::Access)
        ));
        let mut changed = request();
        changed.objective = "changed objective".into();
        assert!(matches!(
            engine.check_accepted_public_start(&changed, &receipt, &access, &connection, &token),
            Err(PublicError::CommandConflict)
        ));
        engine
            .handle(
                CommandEnvelope {
                    version: 1,
                    id: CommandId::new(),
                    workspace: access.workspace.clone(),
                    session: access.session.clone(),
                    task: Some(TaskId::parse(request().task.as_str()).unwrap()),
                    caller: access.actor.clone(),
                    controller: engine.controller().clone(),
                    owner_epoch: engine.owner_epoch(),
                    expected: Revision::ZERO,
                    steering: SteeringRevision::ZERO,
                    payload: Command::Transition {
                        next: TaskState::Paused,
                        reason: "retained construction failed; explicit resume required".into(),
                        verification: None,
                    },
                },
                &access,
                &HostFacts::inspect(Timestamp::new(4)),
            )
            .await
            .unwrap();
        let before = engine.store().state().clone();
        assert!(matches!(
            engine.check_accepted_public_start(&request(), &receipt, &access, &connection, &token),
            Err(PublicError::StaleState)
        ));
        let PublicStartAdmission::Replay(replayed) = engine
            .prepare_public_start(request(), &access, &connection, &token)
            .unwrap()
        else {
            panic!("receipt remains reconcilable")
        };
        let retained = retained_start_budget(engine.store().state(), &scope(&request()).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(retained.budget, request().budget);
        assert_eq!(retained.accepted_at, Timestamp::new(3));
        assert_eq!(replayed, receipt);
        assert_eq!(*engine.store().state(), before);
        engine.into_store().close().await.unwrap();
    }
}

#[tokio::test]
async fn invalid_capture_policy_scope_revision_and_lost_lease_cannot_partially_accept() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let (mut engine, access, connection, token) = fixture(temp.path(), backend).await;
        let trigger = trigger(&engine);
        let before = engine.store().state().clone();
        for case in 0..6 {
            let prepared = prepare(&engine, &access, &connection, &token);
            let mut trigger = trigger.clone();
            let mut facts = facts();
            match case {
                0 => trigger.spec.scope.task = TaskId::new(),
                1 => trigger.sha256 = "0".repeat(64),
                2 => trigger.spec.omissions.push(Omission::UnobservedTail),
                3 => facts.fingerprint.repository = "invalid".into(),
                4 => facts.policy = PolicyRevision::new(1),
                5 => trigger.state = CaptureState::Aborted,
                _ => unreachable!(),
            }
            assert!(engine
                .commit_public_start(prepared, &access, &facts, &trigger, Timestamp::new(3))
                .await
                .is_err());
            assert_eq!(*engine.store().state(), before);
        }
        for case in 0..3 {
            let mut wrong = request();
            match case {
                0 => wrong.scope.session = id("foreign"),
                1 => wrong.mutation.expected_revision = 1.into(),
                2 => wrong.mutation.steering_revision = 1.into(),
                _ => unreachable!(),
            }
            assert!(engine
                .prepare_public_start(wrong, &access, &connection, &token)
                .is_err());
            assert_eq!(*engine.store().state(), before);
        }
        // CanonicalStore rejects the whole multi-record transaction if any
        // reference is invalid; neither earlier ledger nor artifact inserts leak.
        let mut invalid = transaction(
            engine.store().state(),
            &request(),
            &access,
            &facts(),
            &trigger,
            Timestamp::new(3),
        )
        .unwrap();
        invalid.mutations.retain(|mutation| !matches!(mutation, Mutation::Put { record, .. } if record.collection == Collection::Task));
        assert!(engine.store_mut().transact(invalid).await.is_err());
        assert_eq!(*engine.store().state(), before);
        let prepared = prepare(&engine, &access, &connection, &token);
        engine
            .release_controller(
                &access,
                &connection,
                CommandId::new(),
                &token,
                token.revision(),
                Reason::Released,
                Timestamp::new(4),
            )
            .await
            .unwrap();
        let released = engine.store().state().clone();
        assert!(matches!(
            engine
                .commit_public_start(prepared, &access, &facts(), &trigger, Timestamp::new(5))
                .await,
            Err(PublicError::Access)
        ));
        assert_eq!(*engine.store().state(), released);
        engine.into_store().close().await.unwrap();
    }
}
