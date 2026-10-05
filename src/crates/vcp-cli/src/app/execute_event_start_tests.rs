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
