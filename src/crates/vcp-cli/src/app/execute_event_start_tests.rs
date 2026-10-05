// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{
    collections::BTreeMap,
    io::Write,
    sync::{Arc, Mutex},
};
use vcp_domain::{
    artifact::{ArtifactSpec, Channel},
    *,
};
use vcp_protocol::command::CommandReceipt;
use vcp_store::{artifact::ArtifactWriter, contract::CanonicalStore};

#[derive(Clone)]
struct Writer(Arc<Mutex<Vec<u8>>>);
impl Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn config(temp: &tempfile::TempDir, backend: BackendKind) -> Config {
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let currency: Currency = "USD".to_owned().try_into().unwrap();
    Config {
        canonical_root: temp.path().join("canonical"),
        backend,
        workspace: WorkspaceId::new(),
        session: SessionId::new(),
        actor: ActorId::new(),
        root_task: TaskId::new(),
        binding: Binding {
            host: HostId::new(),
            root: workspace
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            repository: "invocation-events".into(),
            worktree: "main".into(),
            revision: Revision::ZERO,
        },
        cap: Money {
            currency: currency.clone(),
            micros: Micros::new(1000),
        }
        .into(),
        protected: Micros::ZERO,
        price: PriceSnapshot {
            id: "a".repeat(64),
            provider: "fixture".into(),
            model: "fixture/model".into(),
            currency,
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
                        micros: Micros::ZERO,
                        per_units: Units::new(1),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>(),
        },
        input_ceiling: Units::new(1000),
        output_ceiling: Units::new(100),
        max_transport_retries: 0,
        artifact_limit: ByteCount::new(16 * 1024 * 1024),
        host_tool_denials: vec![],
    }
}
fn create(host: &CanonicalHost, task: &TaskId, fork_origin: Option<TaskId>) -> CommandReceipt {
    host.command(
        Command::CreateTask {
            root: task.clone(),
            parent: None,
            fork_origin,
            objective: Objective {
                text: "offline invocation boundary".into(),
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
        Some(task.clone()),
        Revision::ZERO,
    )
    .unwrap()
}
fn cancel(host: &CanonicalHost, task: &TaskId) -> CommandReceipt {
    host.command(
        Command::Transition {
            next: TaskState::Cancelled,
            reason: "offline fixture finished".into(),
            verification: None,
        },
        Some(task.clone()),
        Revision::ZERO,
    )
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn new_invocation_stream_keeps_acceptance_and_final_receipt_without_prior_tasks() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for fork in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let mut config = config(&temp, backend);
            let (mut host, mut owner) = CanonicalHost::open(config.clone()).unwrap();
            assert_eq!(
                invocation_event_start(&host, &SessionId::new(), true).unwrap(),
                SessionSeq::ZERO
            );
            for _ in 0..8 {
                let task = TaskId::new();
                create(&host, &task, None);
                cancel(&host, &task);
            }
            let old_session = config.session.clone();
            let old_end = invocation_event_start(&host, &old_session, true).unwrap();
            assert!(old_end.get() >= 16);
            let mut origin = None;
            if fork {
                let source = config.root_task.clone();
                owner.close().await.unwrap();
                drop(host);
                // Capture through the actual store while no canonical owner is open.
                let store = Store::open(&config.canonical_root, backend, &[])
                    .await
                    .unwrap();
                let mut capture = store
                    .spool()
                    .create(ArtifactSpec {
                        id: ArtifactId::new(),
                        scope: Scope {
                            workspace: config.workspace.clone(),
                            session: config.session.clone(),
                            task: source.clone(),
                        },
                        media_type: "text/plain".into(),
                        schema: "coding-turn-input/1".into(),
                        source: "offline boundary fixture".into(),
                        channel: Channel::Evidence,
                        retention: "history".into(),
                        omissions: vec![],
                    })
                    .unwrap();
                capture.write_chunk(b"completed fork boundary").unwrap();
                let descriptor = capture.finalize().unwrap();
                drop(capture);
                store.close().await.unwrap();
                (host, owner) = CanonicalHost::open(config.clone()).unwrap();
                // Create the source only after its input is available, so setup
                // does not introduce an unrelated owner-loss/resume transition.
                create(&host, &source, None);
                let source_task: vcp_domain::task::Task = host
                    .current_state()
                    .unwrap()
                    .record(Collection::Task, source.as_str(), &config.workspace)
                    .unwrap()
                    .decode()
                    .unwrap();
                host.command(
                    Command::Transition {
                        next: TaskState::Running,
                        reason: "offline completed fork boundary".into(),
                        verification: None,
                    },
                    Some(source.clone()),
                    source_task.revision,
                )
                .unwrap();
                host.command(
                    Command::AttachArtifact {
                        descriptor: descriptor.clone(),
                    },
                    Some(source.clone()),
                    Revision::ZERO,
                )
                .unwrap();
                let turn = TurnId::new();
                let source_task: vcp_domain::task::Task = host
                    .current_state()
                    .unwrap()
                    .record(Collection::Task, source.as_str(), &config.workspace)
                    .unwrap()
                    .decode()
                    .unwrap();
                host.command(
                    Command::StartTurn {
                        id: turn.clone(),
                        trigger: descriptor.spec.id,
                    },
                    Some(source.clone()),
                    source_task.revision,
                )
                .unwrap();
                for (revision, next) in [
                    TurnState::AssemblingContext,
                    TurnState::ReservingBudget,
                    TurnState::RequestingModel,
                    TurnState::ProcessingResponse,
                    TurnState::Verifying,
                    TurnState::Completed,
                ]
                .into_iter()
                .enumerate()
                {
                    host.command(
                        Command::AdvanceTurn {
                            id: turn.clone(),
                            next,
                            reason: "offline recorded completion".into(),
                        },
                        Some(source.clone()),
                        Revision::new(revision as u64),
                    )
                    .unwrap();
                }
                let session = SessionId::new();
                host.command(
                    Command::CreateSession {
                        id: session.clone(),
                        fork_through: Some(turn),
                    },
                    None,
                    Revision::ZERO,
                )
                .unwrap();
                owner.close().await.unwrap();
                drop(host);
                config.session = session;
                config.root_task = TaskId::new();
                (host, owner) = CanonicalHost::open(config.clone()).unwrap();
                origin = Some(source);
            }
            let after = invocation_event_start(&host, &config.session, true).unwrap();
            if fork {
                assert!(
                    after < old_end,
                    "fork must select its new session, not source history"
                );
            } else {
                assert_eq!(after, old_end);
            }
            assert_eq!(
                invocation_event_start(&host, &config.session, false).unwrap(),
                SessionSeq::ZERO,
                "resume semantics remain unchanged"
            );
            let setup_receipt = host
                .command(
                    Command::SetWorkspaceTrust {
                        trust: Trust::Trusted,
                    },
                    None,
                    Revision::ZERO,
                )
                .unwrap();
            let task = TaskId::new();
            let accepted = create(&host, &task, origin);
            assert!(accepted.first_event > after);
            let scope = Scope {
                workspace: config.workspace.clone(),
                session: config.session.clone(),
                task: task.clone(),
            };
            let final_receipt = cancel(&host, &task);
            let writer = Writer(Arc::new(Mutex::new(vec![])));
            let retained = writer.clone();
            let mut output = OwnedJsonl::new(writer, owner).unwrap();
            output
                .emit(
                    &accepted.command,
                    Some(&scope),
                    crate::jsonl::Payload::Accepted { receipt: &accepted },
                )
                .await
                .unwrap();
            assert_eq!(
                output
                    .finish(&host, &accepted.command, &scope, after)
                    .await
                    .unwrap(),
                6
            );
            let bytes = retained.0.lock().unwrap();
            let frames: Vec<serde_json::Value> = std::str::from_utf8(&bytes)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            assert_eq!(frames.first().unwrap()["type"], "accepted");
            assert_eq!(frames.last().unwrap()["type"], "result");
            let sequences: Vec<u64> = frames
                .iter()
                .filter(|f| f["type"] == "event")
                .map(|f| {
                    assert_eq!(f["event"]["event"]["session"], config.session.as_str());
                    assert!(
                        f["event"]["event"]["task"].is_null()
                            || f["event"]["event"]["task"] == task.as_str()
                    );
                    f["event"]["sequence"].as_str().unwrap().parse().unwrap()
                })
                .collect();
            assert_eq!(
                sequences,
                (after.get() + 1..=final_receipt.last_event.get()).collect::<Vec<_>>()
            );
            assert!(sequences.contains(&accepted.first_event.get()));
            assert!(sequences.contains(&setup_receipt.first_event.get()));
            assert!(sequences.contains(&final_receipt.last_event.get()));
            assert_eq!(
                frames.last().unwrap()["receipt"]["command"],
                final_receipt.command.as_str()
            );
        }
    }
}

fn assert_live_status(host: &CanonicalHost, scope: &Scope, state: TaskState, required_input: bool) {
    let live = crate::outcome::LiveStatus::read(host, scope).unwrap();
    let full = crate::outcome::Outcome::read(host, scope).unwrap();
    assert_eq!(live.state, state);
    assert_eq!(live.required_input, required_input);
    assert_eq!(live.state, full.task.state);
    assert_eq!(live.required_input, full.conditions.required_input);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn live_status_preserves_task_states_and_owner_approval() {
    use vcp_protocol::command::{Approval, ApprovalState};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let config = config(&temp, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        for state in [
            TaskState::Pending,
            TaskState::Running,
            TaskState::WaitingForInput,
            TaskState::Blocked,
            TaskState::Paused,
            TaskState::Failed,
            TaskState::Cancelled,
        ] {
            let task = TaskId::new();
            create(&host, &task, None);
            let scope = Scope {
                workspace: config.workspace.clone(),
                session: config.session.clone(),
                task: task.clone(),
            };
            if state != TaskState::Pending {
                host.command(
                    Command::Transition {
                        next: state,
                        reason: "live status fixture".into(),
                        verification: None,
                    },
                    Some(task),
                    Revision::ZERO,
                )
                .unwrap();
            }
            assert_live_status(&host, &scope, state, state == TaskState::WaitingForInput);
        }
        let task = TaskId::new();
        create(&host, &task, None);
        host.command(
            Command::Transition {
                next: TaskState::Paused,
                reason: "approval on paused task".into(),
                verification: None,
            },
            Some(task.clone()),
            Revision::ZERO,
        )
        .unwrap();
        let scope = Scope {
            workspace: config.workspace.clone(),
            session: config.session.clone(),
            task: task.clone(),
        };
        assert_live_status(&host, &scope, TaskState::Paused, false);
        let effect = ToolRunId::new();
        host.command(
            Command::ProposeEffect {
                id: effect.clone(),
                operation_digest: "a".repeat(64),
            },
            Some(task.clone()),
            Revision::new(1),
        )
        .unwrap();
        let envelope = host
            .control_envelope(
                CommandId::new(),
                task.clone(),
                Revision::new(1),
                Command::Inspect,
            )
            .unwrap();
        let workspace: vcp_domain::workspace::Workspace = host
            .current_state()
            .unwrap()
            .record(
                Collection::Workspace,
                config.workspace.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let approval = Approval {
            id: ApprovalId::new(),
            scope: scope.clone(),
            effect: effect.clone(),
            effect_revision: Revision::ZERO,
            steering: SteeringRevision::ZERO,
            operation_digest: "a".repeat(64),
            actor: envelope.caller,
            policy: PolicyRevision::ZERO,
            expires_at: Timestamp::new(u64::MAX),
            state: ApprovalState::Pending,
            revision: Revision::ZERO,
            controller: Some(envelope.controller),
            owner_epoch: Some(envelope.owner_epoch),
            authority: Some(workspace.authority),
            binding: Some(workspace.binding.revision),
        };
        host.command(
            Command::Ask {
                approval: approval.clone(),
            },
            Some(task.clone()),
            Revision::ZERO,
        )
        .unwrap();
        assert_live_status(&host, &scope, TaskState::Paused, true);
        assert_eq!(
            crate::outcome::Outcome::read(&host, &scope)
                .unwrap()
                .approvals,
            vec![approval.clone()]
        );
        host.command(
            Command::Decide {
                id: approval.id,
                operation_digest: approval.operation_digest,
                effect_revision: Revision::ZERO,
                allow: false,
            },
            Some(task),
            Revision::ZERO,
        )
        .unwrap();
        assert_live_status(&host, &scope, TaskState::Paused, false);
        owner.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn live_status_preserves_latest_turn_input_and_final_verification() {
    use vcp_domain::verification::{CostCertainty, Verification};
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let config = config(&temp, backend);
        let scope = Scope {
            workspace: config.workspace.clone(),
            session: config.session.clone(),
            task: config.root_task.clone(),
        };
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        owner.close().await.unwrap();
        drop(host);
        let store = Store::open(&config.canonical_root, backend, &[])
            .await
            .unwrap();
        let mut capture = store
            .spool()
            .create(ArtifactSpec {
                id: ArtifactId::new(),
                scope: scope.clone(),
                media_type: "text/plain".into(),
                schema: "coding-turn-input/1".into(),
                source: "live status fixture".into(),
                channel: Channel::Evidence,
                retention: "history".into(),
                omissions: vec![],
            })
            .unwrap();
        capture
            .write_chunk(b"live status input and verification evidence")
            .unwrap();
        let descriptor = capture.finalize().unwrap();
        drop(capture);
        store.close().await.unwrap();
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        create(&host, &scope.task, None);
        host.command(
            Command::AttachArtifact {
                descriptor: descriptor.clone(),
            },
            Some(scope.task.clone()),
            Revision::ZERO,
        )
        .unwrap();
        host.command(
            Command::Transition {
                next: TaskState::Running,
                reason: "live turn fixture".into(),
                verification: None,
            },
            Some(scope.task.clone()),
            Revision::ZERO,
        )
        .unwrap();
        let turn = TurnId::new();
        host.command(
            Command::StartTurn {
                id: turn.clone(),
                trigger: descriptor.spec.id.clone(),
            },
            Some(scope.task.clone()),
            Revision::new(1),
        )
        .unwrap();
        host.command(
            Command::AdvanceTurn {
                id: turn.clone(),
                next: TurnState::WaitingForInput,
                reason: "input needed".into(),
            },
            Some(scope.task.clone()),
            Revision::ZERO,
        )
        .unwrap();
        assert_live_status(&host, &scope, TaskState::Running, true);
        host.command(
            Command::AdvanceTurn {
                id: turn,
                next: TurnState::Failed,
                reason: "old turn failed".into(),
            },
            Some(scope.task.clone()),
            Revision::new(1),
        )
        .unwrap();
        assert_live_status(&host, &scope, TaskState::Running, false);
        assert!(
            crate::outcome::Outcome::read(&host, &scope)
                .unwrap()
                .conditions
                .incomplete
        );
        host.command(
            Command::StartTurn {
                id: TurnId::new(),
                trigger: descriptor.spec.id.clone(),
            },
            Some(scope.task.clone()),
            Revision::new(1),
        )
        .unwrap();
        assert_live_status(&host, &scope, TaskState::Running, false);
        let full = crate::outcome::Outcome::read(&host, &scope).unwrap();
        assert!(
            !full.conditions.incomplete,
            "older failed turn must not override the latest turn"
        );
        let report = Verification {
            redaction: None,
            id: VerificationId::new(),
            scope: scope.clone(),
            steering: full.task.steering,
            fingerprint: full.task.fingerprint,
            outputs: vec![descriptor.spec.id],
            checks: vec![],
            unresolved_effects: vec![],
            outstanding_issues: vec!["failed fixture check".into()],
            cost: CostCertainty::Known,
        };
        host.command(
            Command::RecordVerification {
                verification: report.clone(),
            },
            Some(scope.task.clone()),
            Revision::new(1),
        )
        .unwrap();
        assert_live_status(&host, &scope, TaskState::Running, false);
        assert!(
            crate::outcome::Outcome::read(&host, &scope)
                .unwrap()
                .conditions
                .incomplete
        );
        let repaired = Verification {
            id: VerificationId::new(),
            outstanding_issues: vec![],
            ..report
        };
        host.command(
            Command::RecordVerification {
                verification: repaired,
            },
            Some(scope.task.clone()),
            Revision::new(1),
        )
        .unwrap();
        assert_live_status(&host, &scope, TaskState::Running, false);
        assert!(
            !crate::outcome::Outcome::read(&host, &scope)
                .unwrap()
                .conditions
                .incomplete
        );
        owner.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn live_status_does_not_scan_verification_history_but_final_outcome_rejects_malformed_events()
{
    use vcp_protocol::event::{EventInput, EventKind};
    use vcp_store::contract::Transaction;
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for (data, expected_error) in [
            (
                serde_json::json!({}),
                Some("verification event facts missing"),
            ),
            (
                serde_json::json!({"facts":[]}),
                Some("verification event reference missing"),
            ),
            (
                serde_json::json!({"facts":[{"collection":"verification","id":"missing-report"}]}),
                None,
            ),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let config = config(&temp, backend);
            let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
            create(&host, &config.root_task, None);
            let scope = Scope {
                workspace: config.workspace.clone(),
                session: config.session.clone(),
                task: config.root_task.clone(),
            };
            assert_live_status(&host, &scope, TaskState::Pending, false);
            owner.close().await.unwrap();
            drop(host);
            let mut store = Store::open(&config.canonical_root, backend, &[])
                .await
                .unwrap();
            store
                .transact(Transaction {
                    id: TransactionId::new(),
                    expected_watermark: store.current().watermark,
                    mutations: vec![],
                    events: vec![EventInput {
                        id: EventId::new(),
                        workspace: scope.workspace.clone(),
                        session: scope.session.clone(),
                        task: Some(scope.task.clone()),
                        actor: config.actor.clone(),
                        correlation: CommandId::new(),
                        causation: None,
                        timestamp: crate::settings::now(),
                        kind: EventKind::VerificationRecorded,
                        artifacts: vec![],
                        data,
                        metadata: None,
                    }],
                    command: None,
                })
                .await
                .unwrap();
            store.close().await.unwrap();
            let (host, owner) = CanonicalHost::open(config).unwrap();
            let live = crate::outcome::LiveStatus::read(&host, &scope).unwrap();
            assert!(!live.required_input);
            let error = match crate::outcome::Outcome::read(&host, &scope) {
                Ok(_) => panic!("final outcome accepted malformed verification history"),
                Err(error) => error,
            };
            if let Some(expected) = expected_error {
                assert_eq!(error, expected);
            }
            owner.close().await.unwrap();
        }
    }
}
