// SPDX-License-Identifier: Apache-2.0
use std::{
    io::{self, Write},
    sync::mpsc,
    time::Duration,
};
use vcp_cli::{jsonl::Payload, output::OwnedJsonl};
use vcp_domain::{
    accounting::*, ids::*, revision::*, task::*, verification::Fingerprint, workspace::*,
};
use vcp_lifecycle::foundation::{CanonicalHost, Config};
use vcp_protocol::{command::Command, subscription::EventPage};
use vcp_store::{contract::Collection, BackendKind};

fn config(root: &std::path::Path, backend: BackendKind) -> Config {
    let currency: Currency = "USD".to_owned().try_into().unwrap();
    Config {
        canonical_root: root.join("canonical"),
        backend,
        workspace: WorkspaceId::new(),
        session: SessionId::new(),
        binding: Binding {
            host: HostId::new(),
            root: root.to_string_lossy().into_owned(),
            repository: "output-fixture".into(),
            worktree: "main".into(),
            revision: Revision::ZERO,
        },
        actor: ActorId::new(),
        root_task: TaskId::new(),
        cap: Money {
            currency: currency.clone(),
            micros: Micros::new(1000),
        },
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
            .collect(),
        },
        input_ceiling: Units::new(4096),
        output_ceiling: Units::new(1024),
        artifact_limit: ByteCount::new(vcp_store::artifact::DEFAULT_ARTIFACT_LIMIT),
        host_tool_denials: vec![],
    }
}

fn create(host: &CanonicalHost, config: &Config) {
    host.command(
        Command::CreateTask {
            root: config.root_task.clone(),
            parent: None,
            fork_origin: None,
            objective: Objective {
                text: "observe output owner".into(),
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
        Some(config.root_task.clone()),
        Revision::ZERO,
    )
    .unwrap();
    host.command(
        Command::Transition {
            next: TaskState::Running,
            reason: "start fixture".into(),
            verification: None,
        },
        Some(config.root_task.clone()),
        Revision::ZERO,
    )
    .unwrap();
}

#[tokio::test]
async fn independent_process_consumes_durable_jsonl_without_duplicates() {
    use std::process::{Command as Process, Stdio};
    let node = std::env::var_os("VCP_TEST_NODE").expect("native runner supplies Node");
    let mut consumer = Process::new(node)
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/consume-jsonl.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = config(temp.path(), BackendKind::Files);
    let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
    create(&host, &config);
    let mut output = OwnedJsonl::new(consumer.stdin.take().unwrap(), owner).unwrap();
    let correlation = CommandId::new();
    let after = output
        .drain_events(&host, &correlation, SessionSeq::ZERO)
        .await
        .unwrap();
    assert_eq!(
        output
            .drain_events(&host, &correlation, after)
            .await
            .unwrap(),
        after
    );
    let scope = Scope {
        workspace: config.workspace.clone(),
        session: config.session.clone(),
        task: config.root_task.clone(),
    };
    assert_eq!(
        output
            .finish(&host, &correlation, &scope, after)
            .await
            .unwrap(),
        8
    );
    assert!(output
        .finish(&host, &correlation, &scope, after)
        .await
        .is_err());
    drop(output);
    let result = consumer.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(result["count"].as_u64().unwrap(), after.get() + 1);
}

struct Broken;

#[tokio::test]
async fn failed_current_verification_takes_precedence_over_pause_but_not_new_steering() {
    use vcp_domain::verification::{CostCertainty, Verification};
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let config = config(temp.path(), backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        create(&host, &config);
        let scope = Scope {
            workspace: config.workspace.clone(),
            session: config.session.clone(),
            task: config.root_task.clone(),
        };
        let task: Task = host
            .snapshot()
            .unwrap()
            .record(Collection::Task, scope.task.as_str(), &scope.workspace)
            .unwrap()
            .decode()
            .unwrap();
        host.command(
            Command::RecordVerification {
                verification: Verification {
                    id: VerificationId::new(),
                    scope: scope.clone(),
                    steering: task.steering,
                    fingerprint: task.fingerprint,
                    outputs: vec![],
                    checks: vec![],
                    unresolved_effects: vec![],
                    outstanding_issues: vec!["acceptance failed".into()],
                    cost: CostCertainty::Known,
                },
            },
            Some(scope.task.clone()),
            task.revision,
        )
        .unwrap();
        host.command(
            Command::Transition {
                next: TaskState::Paused,
                reason: "saved after verification".into(),
                verification: None,
            },
            Some(scope.task.clone()),
            task.revision,
        )
        .unwrap();
        let outcome = vcp_cli::outcome::Outcome::read(&host, &scope).unwrap();
        assert!(outcome.conditions.durably_paused);
        assert!(outcome.conditions.incomplete);
        assert_eq!(outcome.conditions.code(), 3);
        host.command(
            Command::Steer {
                objective: Objective {
                    text: "new acceptance contract".into(),
                    constraints: vec![],
                    acceptance: vec![],
                    source: EventId::new(),
                    steering: SteeringRevision::new(1),
                },
            },
            Some(scope.task.clone()),
            outcome.task.revision,
        )
        .unwrap();
        assert_eq!(
            vcp_cli::outcome::Outcome::read(&host, &scope)
                .unwrap()
                .conditions
                .code(),
            8
        );
        owner.close().await.unwrap();
    }
}

#[tokio::test]
async fn pending_question_is_required_input_after_durable_pause() {
    use vcp_protocol::command::{Approval, ApprovalState};
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let config = config(temp.path(), backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        create(&host, &config);
        let effect = ToolRunId::new();
        host.command(
            Command::ProposeEffect {
                id: effect.clone(),
                operation_digest: "a".repeat(64),
            },
            Some(config.root_task.clone()),
            Revision::new(1),
        )
        .unwrap();
        let identity = host
            .control_envelope(
                CommandId::new(),
                config.root_task.clone(),
                Revision::new(1),
                Command::Inspect,
            )
            .unwrap();
        let workspace: Workspace = host
            .snapshot()
            .unwrap()
            .record(
                Collection::Workspace,
                config.workspace.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let scope = Scope {
            workspace: config.workspace.clone(),
            session: config.session.clone(),
            task: config.root_task.clone(),
        };
        let approval = Approval {
            id: ApprovalId::new(),
            scope: scope.clone(),
            effect,
            effect_revision: Revision::ZERO,
            steering: SteeringRevision::ZERO,
            operation_digest: "a".repeat(64),
            actor: config.actor.clone(),
            policy: PolicyRevision::ZERO,
            expires_at: Timestamp::new(u64::MAX),
            state: ApprovalState::Pending,
            revision: Revision::ZERO,
            controller: Some(identity.controller),
            owner_epoch: Some(identity.owner_epoch),
            authority: Some(workspace.authority),
            binding: Some(workspace.binding.revision),
        };
        host.command(
            Command::Ask {
                approval: approval.clone(),
            },
            Some(config.root_task.clone()),
            Revision::ZERO,
        )
        .unwrap();
        let pause = host
            .control_envelope(
                CommandId::new(),
                config.root_task.clone(),
                Revision::new(2),
                Command::Transition {
                    next: TaskState::Paused,
                    reason: "noninteractive input required".into(),
                    verification: None,
                },
            )
            .unwrap();
        host.stop(pause).unwrap();
        owner.close().await.unwrap();
        let outcome = vcp_cli::outcome::Outcome::read(&host, &scope).unwrap();
        assert_eq!(outcome.approvals, vec![approval]);
        assert!(outcome.conditions.required_input);
        assert!(outcome.conditions.durably_paused);
        assert_eq!(outcome.conditions.code(), 4);
        assert!(!outcome.conditions.completed);
    }
}

#[tokio::test]
async fn durable_outcomes_preserve_ambiguous_effect_and_cancellation() {
    use vcp_domain::effect::EffectState;
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let config = config(temp.path(), backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        create(&host, &config);
        host.command(
            Command::SetWorkspaceTrust {
                trust: Trust::Trusted,
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        let effect = ToolRunId::new();
        host.command(
            Command::ProposeEffect {
                id: effect.clone(),
                operation_digest: "a".repeat(64),
            },
            Some(config.root_task.clone()),
            Revision::new(1),
        )
        .unwrap();
        let execution = ExecutionId::new();
        for (revision, next) in [
            EffectState::Validated,
            EffectState::Authorized,
            EffectState::DispatchRecorded,
            EffectState::OutcomeUnknown,
        ]
        .into_iter()
        .enumerate()
        {
            host.command(
                Command::AdvanceEffect {
                    id: effect.clone(),
                    next,
                    reason: "durable fixture observation".into(),
                    execution: (revision >= 2).then(|| execution.clone()),
                    exit_code: None,
                    observed_changes: vec![],
                },
                Some(config.root_task.clone()),
                Revision::new(revision as u64),
            )
            .unwrap();
        }
        host.command(
            Command::Transition {
                next: TaskState::Cancelled,
                reason: "cancel with unresolved dispatch".into(),
                verification: None,
            },
            Some(config.root_task.clone()),
            Revision::new(1),
        )
        .unwrap();
        let scope = Scope {
            workspace: config.workspace.clone(),
            session: config.session.clone(),
            task: config.root_task.clone(),
        };
        let outcome = vcp_cli::outcome::Outcome::read(&host, &scope).unwrap();
        assert!(outcome.conditions.cancelled);
        assert!(outcome.conditions.unresolved_effect);
        assert_eq!(outcome.conditions.code(), 7);
        let mut wrong = scope.clone();
        wrong.session = SessionId::new();
        assert!(vcp_cli::outcome::Outcome::read(&host, &wrong).is_err());
        host.command(
            Command::AdvanceEffect {
                id: effect,
                next: EffectState::Cancelled,
                reason: "dispatch reconciled".into(),
                execution: Some(execution),
                exit_code: None,
                observed_changes: vec![],
            },
            Some(config.root_task.clone()),
            Revision::new(4),
        )
        .unwrap();
        let reconciled = vcp_cli::outcome::Outcome::read(&host, &scope).unwrap();
        assert_eq!(reconciled.conditions.code(), 6);
        assert_eq!(reconciled.receipt, outcome.receipt);
        owner.close().await.unwrap();
    }
}
impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn broken_output_durably_pauses_owner_and_cannot_emit_again() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let config = config(temp.path(), backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        create(&host, &config);
        let mut output = OwnedJsonl::new(Broken, owner).unwrap();
        let id = CommandId::new();
        assert!(output
            .emit(&id, None, Payload::CursorGap { reason: "fixture" })
            .await
            .is_err());
        let task: Task = host
            .snapshot()
            .unwrap()
            .record(
                Collection::Task,
                config.root_task.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(task.state, TaskState::Paused);
        let watermark = host.snapshot().unwrap().watermark;
        assert!(output
            .emit(&id, None, Payload::CursorGap { reason: "retry" })
            .await
            .is_err());
        assert_eq!(host.snapshot().unwrap().watermark, watermark);
    }
}

struct Gated {
    entered: Option<tokio::sync::oneshot::Sender<()>>,
    release: mpsc::Receiver<()>,
}
impl Write for Gated {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Some(entered) = self.entered.take() {
            let _ = entered.send(());
            self.release
                .recv_timeout(Duration::from_secs(5))
                .map_err(|_| io::ErrorKind::TimedOut)?;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn slow_consumer_does_not_block_owner_controls_or_durable_event_cursor() {
    let temp = tempfile::tempdir().unwrap();
    let config = config(temp.path(), BackendKind::Files);
    let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
    create(&host, &config);
    host.command(
        Command::Transition {
            next: TaskState::Paused,
            reason: "saved task awaiting operator".into(),
            verification: None,
        },
        Some(config.root_task.clone()),
        Revision::new(1),
    )
    .unwrap();
    let cursor = host.subscribe_events(SessionSeq::ZERO, 1).unwrap();
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let (release, gate) = mpsc::channel();
    let mut output = OwnedJsonl::new(
        Gated {
            entered: Some(entered),
            release: gate,
        },
        owner,
    )
    .unwrap();
    let emission = tokio::spawn(async move {
        output
            .emit(
                &CommandId::new(),
                None,
                Payload::CursorGap { reason: "fixture" },
            )
            .await
            .unwrap();
        output
    });
    tokio::time::timeout(Duration::from_secs(2), waiting)
        .await
        .unwrap()
        .unwrap();
    let command = host
        .control_envelope(
            CommandId::new(),
            config.root_task.clone(),
            Revision::new(2),
            Command::Transition {
                next: TaskState::Cancelled,
                reason: "control while output blocked".into(),
                verification: None,
            },
        )
        .unwrap();
    let bytes = format!(
        "{}\n{}\n",
        serde_json::to_string(&command).unwrap(),
        serde_json::to_string(&command).unwrap()
    );
    let mut input = vcp_cli::input::ControlInput::new(io::Cursor::new(bytes)).unwrap();
    let first = tokio::time::timeout(Duration::from_secs(2), input.next(&host))
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .result
        .unwrap();
    let duplicate = input.next(&host).await.unwrap().unwrap().result.unwrap();
    assert_eq!(first, duplicate);
    assert!(input.next(&host).await.unwrap().is_none());
    let outcome = vcp_cli::outcome::Outcome::read(
        &host,
        &Scope {
            workspace: config.workspace.clone(),
            session: config.session.clone(),
            task: config.root_task.clone(),
        },
    )
    .unwrap();
    assert_eq!(outcome.conditions.code(), 6);
    assert_eq!(outcome.receipt, first);
    let EventPage::Events { events, at_end, .. } = host.events(cursor.clone()).unwrap() else {
        panic!("expected events")
    };
    assert_eq!(events.len(), 1);
    assert!(!at_end);
    host.unsubscribe_events(cursor.snapshot.clone()).unwrap();
    assert!(matches!(
        host.events(cursor).unwrap(),
        EventPage::Gap { .. }
    ));
    release.send(()).unwrap();
    let mut output = emission.await.unwrap();
    output.close().await.unwrap();
}
