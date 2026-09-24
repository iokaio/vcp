// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::HostFacts;
use vcp_domain::{task::Objective, verification::Fingerprint, workspace::Binding};
use vcp_protocol::command::{Command, CommandEnvelope};
use vcp_store::{BackendKind, Store};
fn wid(value: &str) -> methods::Id {
    value.to_owned().try_into().unwrap()
}
fn scope(access: &Access) -> methods::Scope {
    methods::Scope {
        workspace: wid(access.workspace.as_str()),
        session: wid(access.session.as_str()),
    }
}
fn mutation(name: &str, revision: u64) -> methods::Mutation {
    methods::Mutation {
        command_id: wid(name),
        expected_revision: revision.into(),
        steering_revision: 0.into(),
    }
}
async fn command(
    engine: &mut Engine<Store>,
    access: &Access,
    payload: Command,
    task: Option<TaskId>,
    expected: Revision,
) {
    let mut host = HostFacts::inspect(Timestamp::new(1));
    if matches!(
        payload,
        Command::Transition {
            next: TaskState::Running,
            ..
        }
    ) {
        host.may_execute = true;
        host.resume = Some(vcp_domain::task::ResumeEvidence {
            workspace_current: true,
            policy_current: true,
            budget_current: true,
            effects_reconciled: true,
            owner_current: true,
        });
    }
    engine
        .handle(
            CommandEnvelope {
                version: 1,
                id: CommandId::new(),
                workspace: access.workspace.clone(),
                session: access.session.clone(),
                task,
                caller: access.actor.clone(),
                controller: engine.controller().clone(),
                owner_epoch: engine.owner_epoch(),
                expected,
                steering: SteeringRevision::ZERO,
                payload,
            },
            access,
            &host,
        )
        .await
        .unwrap();
}
async fn put<T: serde::Serialize>(
    engine: &mut Engine<Store>,
    access: &Access,
    collection: Collection,
    name: &str,
    revision: Revision,
    value: &T,
    expected: Option<Revision>,
) {
    let watermark = engine.store().state().watermark;
    engine
        .store_mut()
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: watermark,
            mutations: vec![Mutation::Put {
                record: Record::typed(collection, name, access.workspace.clone(), revision, value)
                    .unwrap(),
                expected,
            }],
            events: vec![],
            command: None,
        })
        .await
        .unwrap();
}
async fn fixture(
    path: &std::path::Path,
    backend: BackendKind,
) -> (Engine<Store>, Access, ControllerId, ControllerToken, Task) {
    let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
    let mut access = Access {
        actor: ActorId::new(),
        workspace: WorkspaceId::new(),
        session: SessionId::new(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: true,
    };
    command(
        &mut engine,
        &access,
        Command::Initialize {
            binding: Binding {
                host: HostId::new(),
                root: "C:/editor-fixture".into(),
                repository: "fixture".into(),
                worktree: "main".into(),
                revision: Revision::ZERO,
            },
        },
        None,
        Revision::ZERO,
    )
    .await;
    command(
        &mut engine,
        &access,
        Command::SetWorkspaceTrust {
            trust: Trust::Trusted,
        },
        None,
        Revision::ZERO,
    )
    .await;
    access.authority = AuthorityRevision::new(1);
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
    let task_id = TaskId::new();
    command(
        &mut engine,
        &access,
        Command::CreateTask {
            root: task_id.clone(),
            parent: None,
            fork_origin: None,
            objective: Objective {
                text: "editor fixture".into(),
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
            editing: true,
            required_checks: vec![],
        },
        Some(task_id.clone()),
        Revision::ZERO,
    )
    .await;
    let mut task: Task = engine
        .store()
        .state()
        .record(Collection::Task, task_id.as_str(), &access.workspace)
        .unwrap()
        .decode()
        .unwrap();
    task.state = TaskState::Running;
    task.revision = Revision::new(1);
    put(
        &mut engine,
        &access,
        Collection::Task,
        task_id.as_str(),
        task.revision,
        &task,
        Some(Revision::ZERO),
    )
    .await;
    (engine, access, connection, token, task)
}
fn document(observed: &Observation, content: &str) -> wire::DocumentObservation {
    wire::DocumentObservation {
        host: wid(&observed.host),
        open_id: wid(&observed.open_id),
        uri: observed.uri.clone(),
        relative_path: observed.path.clone(),
        version: observed.version.get().into(),
        content_sha256: observed.content_sha256.clone(),
        dirty: observed.dirty,
        content: Some(content.into()),
        disk_sha256: Some(observed.disk_sha256.clone()),
        language: "plaintext".into(),
        eol: wire::EndOfLine::Lf,
        encoding: wire::Encoding::Unknown,
        selections: vec![],
        capture: false,
        diagnostics: None,
    }
}
#[tokio::test]
async fn editor_durable_dispatch_receipt_replay_and_ephemeral_text_on_both_stores() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for outcome in [wire::EditorOutcome::Applied, wire::EditorOutcome::Unknown] {
            let dir = tempfile::tempdir().unwrap();
            let (mut engine, mut access, connection, token, task) =
                fixture(dir.path(), backend).await;
            let original = "private unsaved draft never retained";
            let observation = Observation {
                id: "observation".into(),
                host: "editor-host".into(),
                open_id: "open-file".into(),
                uri: "file:///C:/editor-fixture/file.txt".into(),
                path: "file.txt".into(),
                version: Revision::new(7),
                content_sha256: vcp_protocol::digest_bytes(original.as_bytes()),
                dirty: true,
                root: RootId::new(),
                disk_sha256: "d".repeat(64),
                disk_fingerprint: "e".repeat(64),
                eol: "lf".into(),
                encoding: "unknown".into(),
            };
            let context = wire::EditorContext {
                scope: scope(&access),
                task: wid(task.scope.task.as_str()),
                mutation: mutation("context", task.revision.get()),
                closed: vec![],
                documents: vec![document(&observation, original)],
            };
            let receipt = engine
                .editor_observe(
                    &context,
                    &access,
                    &connection,
                    &token,
                    &EditorObserveFacts {
                        closed: vec![],
                        observations: vec![(observation.clone(), false)],
                        now: Timestamp::new(3),
                    },
                )
                .await
                .unwrap();
            assert_eq!(
                engine
                    .editor_observe(
                        &context,
                        &access,
                        &connection,
                        &token,
                        &EditorObserveFacts {
                            closed: vec![],
                            observations: vec![],
                            now: Timestamp::new(3)
                        }
                    )
                    .await
                    .unwrap(),
                receipt
            );
            assert!(
                buffer_status(engine.store().state(), &task.scope)
                    .unwrap()
                    .1
            );
            let edits = vec![wire::TextEdit {
                range: wire::Range {
                    start: wire::Position {
                        line: 0,
                        character: 0,
                    },
                    end: wire::Position {
                        line: 0,
                        character: 0,
                    },
                },
                text: "replacement secret".into(),
            }];
            let effect = Effect {
                redaction: None,
                id: ToolRunId::new(),
                scope: task.scope.clone(),
                revision: Revision::ZERO,
                steering: task.steering,
                state: EffectState::Validated,
                operation_digest: "f".repeat(64),
                execution: None,
                exit_code: None,
                observed_changes: vec![],
                cause: EventId::new(),
                reason: "prepared editor metadata".into(),
            };
            put(
                &mut engine,
                &access,
                Collection::Effect,
                effect.id.as_str(),
                effect.revision,
                &effect,
                None,
            )
            .await;
            let mut second_effect = effect.clone();
            second_effect.id = ToolRunId::new();
            put(
                &mut engine,
                &access,
                Collection::Effect,
                second_effect.id.as_str(),
                second_effect.revision,
                &second_effect,
                None,
            )
            .await;
            let mut second_observation = observation.clone();
            second_observation.id = "second-observation".into();
            second_observation.path = "second.txt".into();
            second_observation.uri = "file:///C:/editor-fixture/second.txt".into();
            let mut request = wire::EditorPrepare {
                scope: scope(&access),
                task: wid(task.scope.task.as_str()),
                mutation: mutation("change", 2),
                generation: wid("generation"),
                files: vec![wire::FileEdits {
                    observation: wid("observation"),
                    edits: edits.clone(),
                }],
            };
            let workspace = engine.editor_workspace(&access).unwrap();
            let after = "replacement secretprivate unsaved draft never retained";
            request.files.push(wire::FileEdits {
                observation: wid("second-observation"),
                edits: edits.clone(),
            });
            let mut facts = EditorPrepareFacts {
                generation: "generation".into(),
                root: observation.root.clone(),
                host: workspace.binding.host,
                binding: workspace.binding.revision,
                policy: PolicyRevision::ZERO,
                now: Timestamp::new(4),
                files: vec![domain::File {
                    effect: effect.id.clone(),
                    observation: observation.clone(),
                    after_sha256: vcp_protocol::digest_bytes(after.as_bytes()),
                    edits_digest: vcp_protocol::digest_bytes(
                        &vcp_protocol::canonical_bytes(&edits).unwrap(),
                    ),
                    operation_digest: effect.operation_digest.clone(),
                    state: FileState::Prepared,
                    execution: None,
                    observed: None,
                }],
            };
            let mut second_file = facts.files[0].clone();
            second_file.effect = second_effect.id.clone();
            second_file.observation = second_observation.clone();
            facts.files.push(second_file);
            let prepared = engine
                .editor_prepare(&request, &access, &connection, &token, &facts)
                .await
                .unwrap();
            assert!(!prepared.replay);
            assert_eq!(prepared.change.id, "change");
            assert!(
                engine
                    .editor_prepare(&request, &access, &connection, &token, &facts)
                    .await
                    .unwrap()
                    .replay
            );
            let dispatch = wire::EditorDispatch {
                scope: scope(&access),
                task: wid(task.scope.task.as_str()),
                mutation: mutation("dispatch", 0),
                change: wid("change"),
                generation: wid("generation"),
                file: 0,
            };
            let dispatch_facts = EditorDispatchFacts {
                generation: "generation".into(),
                observation: observation.clone(),
                policy: PolicyRevision::ZERO,
                now: Timestamp::new(5),
            };
            assert!(
                engine
                    .editor_dispatch(&dispatch, &access, &connection, &token, &dispatch_facts)
                    .await
                    .is_err(),
                "Validated is not authorized"
            );
            let mut authorized = effect.clone();
            authorized.revision = Revision::new(1);
            authorized.state = EffectState::Authorized;
            put(
                &mut engine,
                &access,
                Collection::Effect,
                effect.id.as_str(),
                authorized.revision,
                &authorized,
                Some(effect.revision),
            )
            .await;
            let mut stale_dispatch = dispatch.clone();
            stale_dispatch.generation = wid("obsolete-generation");
            assert!(matches!(
                engine
                    .editor_dispatch(
                        &stale_dispatch,
                        &access,
                        &connection,
                        &token,
                        &dispatch_facts
                    )
                    .await,
                Err(PublicError::StaleState)
            ));
            let committed = engine
                .editor_dispatch(&dispatch, &access, &connection, &token, &dispatch_facts)
                .await
                .unwrap();
            assert!(!committed.replay);
            assert_eq!(committed.change.files[0].state, FileState::Dispatched);
            assert!(
                engine
                    .editor_dispatch(&dispatch, &access, &connection, &token, &dispatch_facts)
                    .await
                    .unwrap()
                    .replay
            );
            let mut observed = observation.clone();
            observed.version = Revision::new(8);
            observed.content_sha256 = vcp_protocol::digest_bytes(after.as_bytes());
            let result = wire::EditorChangeResult {
                scope: scope(&access),
                task: wid(task.scope.task.as_str()),
                mutation: mutation("receipt", 1),
                change: wid("change"),
                generation: wid("generation"),
                file: 0,
                execution: wid(committed.change.files[0]
                    .execution
                    .as_ref()
                    .unwrap()
                    .as_str()),
                outcome: outcome.clone(),
                document: document(&observed, after),
            };
            let result_facts = EditorResultFacts {
                observation: observed,
                matched_disk: false,
                now: Timestamp::new(6),
            };
            let applied = engine
                .editor_result(&result, &access, &connection, &token, &result_facts)
                .await
                .unwrap();
            assert_eq!(
                applied.change.files[0].state,
                if outcome == wire::EditorOutcome::Applied {
                    FileState::Applied
                } else {
                    FileState::Unknown
                }
            );
            assert_eq!(applied.change.files[1].state, FileState::Prepared);
            assert!(
                engine
                    .editor_result(&result, &access, &connection, &token, &result_facts)
                    .await
                    .unwrap()
                    .replay
            );
            let serialized = serde_json::to_string(engine.store().state()).unwrap();
            assert!(!serialized.contains(original));
            assert!(!serialized.contains("replacement secret"));
            assert!(
                buffer_status(engine.store().state(), &task.scope)
                    .unwrap()
                    .1
            );
            let current = engine
                .editor_task(&access, &scope(&access), &wid(task.scope.task.as_str()))
                .unwrap();
            assert_ne!(current.fingerprint.buffers, task.fingerprint.buffers);
            let final_effect: Effect = engine
                .store()
                .state()
                .record(Collection::Effect, effect.id.as_str(), &access.workspace)
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(
                final_effect.state,
                if outcome == wire::EditorOutcome::Applied {
                    EffectState::Succeeded
                } else {
                    EffectState::OutcomeUnknown
                }
            );
            assert!(
                engine
                    .store()
                    .state()
                    .events
                    .iter()
                    .any(|event| event.event.id == final_effect.cause)
            );
            assert!(engine.store().state().events.iter().any(|event| {
                event.event.kind == EventKind::EffectTransition
                    && event.event.data.to_string().contains("outcome_unknown")
            }));
            let approval = vcp_protocol::command::Approval {
                id: ApprovalId::new(),
                scope: task.scope.clone(),
                effect: second_effect.id.clone(),
                effect_revision: second_effect.revision,
                steering: task.steering,
                operation_digest: second_effect.operation_digest.clone(),
                actor: access.actor.clone(),
                policy: PolicyRevision::ZERO,
                expires_at: Timestamp::new(100),
                state: vcp_protocol::command::ApprovalState::Pending,
                revision: Revision::ZERO,
                controller: Some(engine.controller().clone()),
                owner_epoch: Some(engine.owner_epoch()),
                authority: Some(access.authority),
                binding: Some(facts.binding),
            };
            command(
                &mut engine,
                &access,
                Command::Ask {
                    approval: approval.clone(),
                },
                Some(task.scope.task.clone()),
                second_effect.revision,
            )
            .await;
            assert!(
                crate::questions::actionable(engine.store().state(), &approval, Timestamp::new(7))
                    .unwrap()
            );
            let waiting = engine
                .editor_task(&access, &scope(&access), &wid(task.scope.task.as_str()))
                .unwrap();
            let mut replacement = second_observation.clone();
            replacement.id = "fresh-observation".into();
            replacement.version = Revision::new(8);
            let refresh = wire::EditorContext {
                scope: scope(&access),
                task: wid(task.scope.task.as_str()),
                mutation: mutation("refresh", waiting.revision.get()),
                closed: vec![],
                documents: vec![document(&replacement, original)],
            };
            engine
                .editor_observe(
                    &refresh,
                    &access,
                    &connection,
                    &token,
                    &EditorObserveFacts {
                        closed: vec![],
                        observations: vec![(replacement.clone(), false)],
                        now: Timestamp::new(7),
                    },
                )
                .await
                .unwrap();
            assert!(
                !crate::questions::actionable(engine.store().state(), &approval, Timestamp::new(7))
                    .unwrap()
            );
            let retired = engine
                .editor_read(
                    &access,
                    &wire::EditorChangeRead {
                        scope: scope(&access),
                        task: wid(task.scope.task.as_str()),
                        change: wid("change"),
                    },
                )
                .unwrap();
            assert_eq!(
                retired.files[0], applied.change.files[0],
                "active or unknown receipt is not cancelled"
            );
            assert_eq!(retired.files[1].state, FileState::Rejected);
            assert!(retired.files[1].execution.is_none());
            let cancelled: Effect = engine
                .store()
                .state()
                .record(
                    Collection::Effect,
                    second_effect.id.as_str(),
                    &access.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(cancelled.state, EffectState::Cancelled);
            let retired_dispatch = wire::EditorDispatch {
                file: 1,
                mutation: mutation("retired-dispatch", retired.revision.get()),
                ..dispatch.clone()
            };
            let retired_facts = EditorDispatchFacts {
                generation: "generation".into(),
                observation: second_observation.clone(),
                policy: PolicyRevision::ZERO,
                now: Timestamp::new(7),
            };
            assert!(matches!(
                engine
                    .editor_dispatch(
                        &retired_dispatch,
                        &access,
                        &connection,
                        &token,
                        &retired_facts
                    )
                    .await,
                Err(PublicError::StaleState)
            ));
            let waiting = engine
                .editor_task(&access, &scope(&access), &wid(task.scope.task.as_str()))
                .unwrap();
            assert_eq!(
                waiting.state,
                TaskState::WaitingForInput,
                "observation does not resume work"
            );
            if outcome == wire::EditorOutcome::Applied {
                command(
                    &mut engine,
                    &access,
                    Command::Transition {
                        next: TaskState::Running,
                        reason: "explicit replan after stale question".into(),
                        verification: None,
                    },
                    Some(task.scope.task.clone()),
                    waiting.revision,
                )
                .await;
                let mut fresh_effect = second_effect.clone();
                fresh_effect.id = ToolRunId::new();
                put(
                    &mut engine,
                    &access,
                    Collection::Effect,
                    fresh_effect.id.as_str(),
                    fresh_effect.revision,
                    &fresh_effect,
                    None,
                )
                .await;
                let running = engine
                    .editor_task(&access, &scope(&access), &wid(task.scope.task.as_str()))
                    .unwrap();
                let fresh_request = wire::EditorPrepare {
                    scope: scope(&access),
                    task: wid(task.scope.task.as_str()),
                    mutation: mutation("fresh-change", running.revision.get()),
                    generation: wid("fresh-generation"),
                    files: vec![wire::FileEdits {
                        observation: wid("fresh-observation"),
                        edits: edits.clone(),
                    }],
                };
                let mut fresh_file = facts.files[1].clone();
                fresh_file.effect = fresh_effect.id;
                fresh_file.observation = replacement;
                engine
                    .editor_prepare(
                        &fresh_request,
                        &access,
                        &connection,
                        &token,
                        &EditorPrepareFacts {
                            generation: "fresh-generation".into(),
                            root: facts.root.clone(),
                            host: facts.host.clone(),
                            binding: facts.binding,
                            policy: facts.policy,
                            files: vec![fresh_file],
                            now: Timestamp::new(8),
                        },
                    )
                    .await
                    .unwrap();
            }
            let workspace_revision = engine.editor_workspace(&access).unwrap().revision;
            command(
                &mut engine,
                &access,
                Command::SetWorkspaceTrust {
                    trust: Trust::Untrusted,
                },
                None,
                workspace_revision,
            )
            .await;
            let second_dispatch = wire::EditorDispatch {
                file: 1,
                mutation: mutation("second-dispatch", 2),
                ..dispatch.clone()
            };
            let second_facts = EditorDispatchFacts {
                generation: "generation".into(),
                observation: second_observation,
                policy: PolicyRevision::ZERO,
                now: Timestamp::new(7),
            };
            let revoked_watermark = engine.store().state().watermark;
            assert!(matches!(
                engine
                    .editor_dispatch(
                        &second_dispatch,
                        &access,
                        &connection,
                        &token,
                        &second_facts
                    )
                    .await,
                Err(PublicError::Unavailable)
            ));
            assert_eq!(engine.store().state().watermark, revoked_watermark);
            access.authority = engine.editor_workspace(&access).unwrap().authority;
            let protected = engine
                .store()
                .state()
                .records
                .values()
                .filter(|row| {
                    row.collection == Collection::Projection
                        && row.value["document_type"]
                            .as_str()
                            .is_some_and(|kind| kind.starts_with("vcp_editor_"))
                })
                .cloned()
                .collect::<Vec<_>>();
            assert!(
                protected
                    .iter()
                    .any(|row| row.value["document_type"] == domain::BUFFERS)
            );
            assert!(
                protected
                    .iter()
                    .any(|row| row.value["document_type"] == domain::CHANGE)
            );
            for row in protected {
                assert_eq!(row.value["schema_version"], 2);
                row.validate_shape().unwrap();
                let mut old = row.clone();
                old.value["document_type"] =
                    serde_json::json!(if row.value["document_type"] == domain::CHANGE {
                        "vcp_editor_change_v1"
                    } else {
                        "vcp_editor_buffers_v1"
                    });
                old.value["schema_version"] = serde_json::json!(1);
                assert!(
                    matches!(old.validate_shape(), Err(vcp_store::Error::Incompatible)),
                    "unqualified old editor records are not silently accepted"
                );
                let mut generic = row.clone();
                generic.value["document_type"] = serde_json::json!("unknown_editor_fixture");
                assert!(
                    matches!(
                        generic.validate_shape(),
                        Err(vcp_store::Error::Incompatible)
                    ),
                    "generic fallback shared with pre-editor reader rejects schema2"
                );
                let before = engine.store().state().watermark;
                assert!(
                    engine
                        .store_mut()
                        .transact(Transaction {
                            id: TransactionId::new(),
                            expected_watermark: before,
                            mutations: vec![Mutation::DropProjection {
                                id: row.id,
                                expected: row.revision
                            }],
                            events: vec![],
                            command: None
                        })
                        .await
                        .is_err()
                );
                assert_eq!(engine.store().state().watermark, before);
            }
            engine.into_store().close().await.unwrap();
            let reopened =
                Engine::new(Store::open(dir.path(), backend, &[]).await.unwrap()).unwrap();
            assert_eq!(
                reopened
                    .editor_read(
                        &access,
                        &wire::EditorChangeRead {
                            scope: scope(&access),
                            task: wid(task.scope.task.as_str()),
                            change: wid("change")
                        }
                    )
                    .unwrap(),
                retired
            );
            assert!(
                buffer_status(reopened.store().state(), &task.scope)
                    .unwrap()
                    .1
            );
            let target = tempfile::tempdir().unwrap();
            let destination = target.path().join("converted");
            let other = if backend == BackendKind::Files {
                BackendKind::Sqlite
            } else {
                BackendKind::Files
            };
            let converted = reopened
                .store()
                .convert(&destination, other, &[])
                .await
                .unwrap();
            assert_eq!(converted.state(), reopened.store().state());
            converted.close().await.unwrap();
            let converted = Store::open(&destination, other, &[]).await.unwrap();
            assert_eq!(converted.state(), reopened.store().state());
            assert!(buffer_status(converted.state(), &task.scope).unwrap().1);
        }
    }
}

#[tokio::test]
async fn editor_close_retires_only_exact_observations_on_both_stores() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let dir = tempfile::tempdir().unwrap();
        let (mut engine, mut access, connection, token, task) = fixture(dir.path(), backend).await;
        let observation = Observation {
            id: "observation".into(),
            host: "host".into(),
            open_id: "open".into(),
            uri: "file:///C:/editor-fixture/file.txt".into(),
            path: "file.txt".into(),
            version: Revision::new(1),
            content_sha256: vcp_protocol::digest_bytes(b"clean"),
            dirty: false,
            root: RootId::new(),
            disk_sha256: "d".repeat(64),
            disk_fingerprint: "e".repeat(64),
            eol: "lf".into(),
            encoding: "utf8".into(),
        };
        let mut request = wire::EditorContext {
            scope: scope(&access),
            task: wid(task.scope.task.as_str()),
            mutation: mutation("observe", task.revision.get()),
            documents: vec![document(&observation, "clean")],
            closed: vec![],
        };
        engine
            .editor_observe(
                &request,
                &access,
                &connection,
                &token,
                &EditorObserveFacts {
                    observations: vec![(observation.clone(), true)],
                    closed: vec![],
                    now: Timestamp::new(3),
                },
            )
            .await
            .unwrap();
        // A native-matched clean observation still tracks a live buffer and is
        // not equivalent to coverage by a disk-only verification.
        assert!(
            buffer_status(engine.store().state(), &task.scope)
                .unwrap()
                .1
        );
        let effect = Effect {
            id: ToolRunId::new(),
            scope: task.scope.clone(),
            revision: Revision::ZERO,
            steering: task.steering,
            state: EffectState::Authorized,
            operation_digest: "a".repeat(64),
            execution: None,
            exit_code: None,
            observed_changes: vec![],
            cause: EventId::new(),
            reason: "authorized close fixture".into(),
            redaction: None,
        };
        put(
            &mut engine,
            &access,
            Collection::Effect,
            effect.id.as_str(),
            effect.revision,
            &effect,
            None,
        )
        .await;
        let workspace = engine.editor_workspace(&access).unwrap();
        let change = ChangeSet {
            document_type: domain::CHANGE.into(),
            schema_version: domain::SCHEMA_VERSION,
            id: "close-change".into(),
            scope: task.scope.clone(),
            revision: Revision::ZERO,
            actor: access.actor.clone(),
            connection: connection.clone(),
            generation: "close-generation".into(),
            root: observation.root.clone(),
            host: workspace.binding.host,
            binding: workspace.binding.revision,
            authority: access.authority,
            policy: PolicyRevision::ZERO,
            steering: task.steering,
            files: vec![domain::File {
                effect: effect.id.clone(),
                observation: observation.clone(),
                after_sha256: "b".repeat(64),
                edits_digest: "c".repeat(64),
                operation_digest: effect.operation_digest.clone(),
                state: FileState::Prepared,
                execution: None,
                observed: None,
            }],
        };
        put(
            &mut engine,
            &access,
            Collection::Projection,
            &change.id,
            change.revision,
            &change,
            None,
        )
        .await;
        request.documents.clear();
        request.closed = vec![wid("stale")];
        request.mutation = mutation("close", task.revision.get() + 1);
        let before = engine.store().state().watermark;
        let mut facts = EditorObserveFacts {
            observations: vec![],
            closed: vec!["stale".into()],
            now: Timestamp::new(4),
        };
        assert!(matches!(
            engine
                .editor_observe(&request, &access, &connection, &token, &facts)
                .await,
            Err(PublicError::StaleState)
        ));
        assert_eq!(engine.store().state().watermark, before);
        request.closed = vec![wid("observation")];
        facts.closed = vec!["observation".into()];
        let mut reader = access.clone();
        reader.write = false;
        assert!(matches!(
            engine
                .editor_observe(&request, &reader, &connection, &token, &facts)
                .await,
            Err(PublicError::Access)
        ));
        engine
            .editor_observe(&request, &access, &connection, &token, &facts)
            .await
            .unwrap();
        assert!(
            !buffer_status(engine.store().state(), &task.scope)
                .unwrap()
                .1
        );
        let closed = engine
            .editor_read(
                &access,
                &wire::EditorChangeRead {
                    scope: scope(&access),
                    task: wid(task.scope.task.as_str()),
                    change: wid(&change.id),
                },
            )
            .unwrap();
        assert_eq!(closed.files[0].state, FileState::Rejected);
        assert!(closed.files[0].execution.is_none());
        assert_eq!(closed.files[0].observed.as_ref(), Some(&observation));
        let cancelled: Effect = engine
            .store()
            .state()
            .record(Collection::Effect, effect.id.as_str(), &access.workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(cancelled.state, EffectState::Cancelled);
        // Exact-command replay remains harmless; a new stale close is rejected.
        engine
            .editor_observe(&request, &access, &connection, &token, &facts)
            .await
            .unwrap();
        request.mutation = mutation("close-again", task.revision.get() + 2);
        assert!(matches!(
            engine
                .editor_observe(&request, &access, &connection, &token, &facts)
                .await,
            Err(PublicError::StaleState)
        ));
        access.authority = engine.editor_workspace(&access).unwrap().authority;
        engine.into_store().close().await.unwrap();
        let reopened = Engine::new(Store::open(dir.path(), backend, &[]).await.unwrap()).unwrap();
        assert!(
            !buffer_status(reopened.store().state(), &task.scope)
                .unwrap()
                .1
        );
    }
}
