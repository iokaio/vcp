// SPDX-License-Identifier: Apache-2.0
//! Native integration interruption, durable receipts and recovery without replay.
use super::*;
#[cfg(feature = "qualification")]
use codex_protocol::ThreadId;
use std::{
    fs,
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};
use vcp_domain::{
    agents::ChildMode,
    effect::{Effect, EffectState},
};
use vcp_lifecycle::foundation::ToolProposal;

#[derive(Clone)]
pub(super) enum Schedule {
    Control {
        before_write: bool,
        index: usize,
        state: TaskState,
    },
    ProcessExit {
        root: PathBuf,
        index: usize,
    },
    Verification {
        state: TaskState,
    },
}
impl Schedule {
    pub(super) fn process_root(&self) -> Option<&Path> {
        match self {
            Self::ProcessExit { root, .. } => Some(root),
            _ => None,
        }
    }
    pub(super) fn control_state(&self) -> Option<TaskState> {
        match self {
            Self::Control { state, .. } | Self::Verification { state } => Some(*state),
            _ => None,
        }
    }
}

#[cfg(feature = "qualification")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn integrated_parent_verification_pause_and_cancel_before_publish() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for state in [TaskState::Paused, TaskState::Cancelled] {
            super::child_agents::child_case_with_schedule(
                backend,
                ChildMode::IsolatedWrite,
                false,
                Some(false),
                false,
                false,
                None,
                false,
                Some(Schedule::Verification { state }),
            )
            .await;
        }
    }
}

#[cfg(feature = "qualification")]
pub(super) fn configure_parent_verification(
    host: &CanonicalHost,
    thread: ThreadId,
    fixture: &Path,
) {
    use vcp_tools::{
        process::{Mode, Profile},
        verification::{Requirement, Runner},
    };
    let tools = fixture.join("tools");
    fs::create_dir(&tools).unwrap();
    let exe = tools.join("node.exe");
    fs::copy(
        std::env::var_os("VCP_TEST_NODE").expect("native Node dependency"),
        &exe,
    )
    .unwrap();
    host.configure_process_profile(
        Profile::new(
            "node".into(),
            exe,
            Mode::Direct,
            std::collections::BTreeMap::from([(
                "SystemRoot".into(),
                std::env::var("SystemRoot").unwrap(),
            )]),
            std::collections::BTreeSet::new(),
            true,
        )
        .unwrap(),
    )
    .unwrap();
    host.configure_verification(
        thread,
        vcp_lifecycle::foundation::verification::VerificationConfig {
            requirements: vec![Requirement {
                timeout_ms: None,
                manifest: "package.json".into(),
                runner: Runner::Node,
                profile: "node".into(),
                expected_tests: vec!["integrated_parent".into()],
                rationale: "Read the actual integrated parent bytes".into(),
            }],
            rationale: "Independent parent acceptance after child integration".into(),
        },
    )
    .unwrap();
}

#[cfg(feature = "qualification")]
pub(super) async fn interrupt_parent_verification(
    host: &CanonicalHost,
    thread: ThreadId,
    scope: &Scope,
    next: TaskState,
) {
    let before = host.snapshot().unwrap();
    let verification = host
        .verify_with_publish_observer(thread, vec![], || {
            let task: Task = host
                .snapshot()
                .unwrap()
                .record(Collection::Task, scope.task.as_str(), &scope.workspace)
                .unwrap()
                .decode()
                .unwrap();
            host.stop(
                host.control_envelope(
                    CommandId::new(),
                    scope.task.clone(),
                    task.revision,
                    Command::Transition {
                        next,
                        reason: "interrupt integrated-parent verification before publication"
                            .into(),
                        verification: None,
                    },
                )
                .unwrap(),
            )
            .unwrap();
        })
        .await
        .unwrap();
    assert_eq!(verification.checks.len(), 1);
    assert_eq!(
        verification.checks[0].outcome,
        vcp_domain::verification::CheckOutcome::Passed
    );
    assert!(verification
        .outstanding_issues
        .iter()
        .any(|issue| issue.contains("stale")));
    assert!(host.complete_verified(thread, verification.id).is_err());
    let after = host.snapshot().unwrap();
    assert_eq!(
        vcp_budget::ledger(&before, scope).unwrap(),
        vcp_budget::ledger(&after, scope).unwrap()
    );
    assert_eq!(
        vcp_engine::agents::graph(&before, scope, &scope.task)
            .unwrap()
            .unwrap()
            .results,
        vcp_engine::agents::graph(&after, scope, &scope.task)
            .unwrap()
            .unwrap()
            .results
    );
}

#[cfg(feature = "qualification")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn child_integration_pause_and_cancel_before_and_after_each_write() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for state in [TaskState::Paused, TaskState::Cancelled] {
            for before_write in [true, false] {
                for index in 0..2 {
                    super::child_agents::child_case_with_schedule(
                        backend,
                        ChildMode::IsolatedWrite,
                        false,
                        Some(false),
                        false,
                        true,
                        None,
                        false,
                        Some(Schedule::Control {
                            before_write,
                            index,
                            state,
                        }),
                    )
                    .await;
                }
            }
        }
    }
}

#[cfg(feature = "qualification")]
pub(super) fn apply_scheduled(
    host: &CanonicalHost,
    proposal: ToolProposal,
    workspace: &Path,
    scope: &Scope,
    child: &TaskId,
    schedule: &Schedule,
) -> ToolRunId {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use vcp_lifecycle::foundation::FileDispatchPoint;
    let effect_id = proposal.effect().clone();
    if let Schedule::ProcessExit { root, .. } = schedule {
        fs::write(
            root.join("integration-before.json"),
            serde_json::to_vec(&host.snapshot().unwrap()).unwrap(),
        )
        .unwrap();
    }
    let host_copy = host.clone();
    let observed_scope = scope.clone();
    let child = child.clone();
    let effect = effect_id.clone();
    let configured = schedule.clone();
    let fired = Arc::new(AtomicUsize::new(0));
    let observed = fired.clone();
    let output = host.qualification_dispatch_tool_with_file_observer(proposal, move |point| {
        match &configured {
            Schedule::Control { before_write, index, state } => {
                let selected = if *before_write { FileDispatchPoint::BeforeWrite(*index) } else { FileDispatchPoint::AfterReceipt(*index) };
                if point == selected {
                    assert_eq!(observed.fetch_add(1, Ordering::SeqCst), 0);
                    let task: Task = host_copy.snapshot().unwrap().record(Collection::Task, observed_scope.task.as_str(), &observed_scope.workspace).unwrap().decode().unwrap();
                    host_copy.stop(host_copy.control_envelope(CommandId::new(), observed_scope.task.clone(), task.revision, Command::Transition { next: *state, reason: format!("qualification interruption at {point:?}"), verification: None }).unwrap()).unwrap();
                }
            }
            Schedule::ProcessExit { root, index } if point == FileDispatchPoint::BeforeReceipt(*index) => {
                // This callback runs on the canonical worker. Persist only the
                // supervisor handshake; no reentrant host calls are allowed.
                let marker = serde_json::json!({"phase":"native_write_before_receipt", "index":index, "effect":effect, "child":child, "scope":observed_scope});
                let temporary = root.join("integration-barrier.tmp");
                let mut file = fs::File::create(&temporary).unwrap();
                std::io::Write::write_all(&mut file, &serde_json::to_vec(&marker).unwrap()).unwrap();
                file.sync_all().unwrap();
                drop(file);
                fs::rename(temporary, root.join("integration-barrier.json")).unwrap();
                loop { std::thread::park(); }
            }
            _ => {}
        }
        Ok(())
    }).unwrap();
    let Schedule::Control {
        before_write,
        index,
        state,
    } = schedule
    else {
        panic!("process interruption unexpectedly returned")
    };
    assert_eq!(fired.load(Ordering::SeqCst), 1);
    let written = *index + usize::from(!*before_write);
    assert_eq!(output.result["complete"], written == 2);
    assert_eq!(output.result["files"].as_array().unwrap().len(), written);
    assert_eq!(
        fs::read(workspace.join("file.txt")).unwrap(),
        if written > 0 {
            b"child changed\n".as_slice()
        } else {
            b"captured\n".as_slice()
        }
    );
    assert_eq!(
        fs::read(workspace.join("second.txt")).unwrap(),
        if written > 1 {
            b"second child changed\n".as_slice()
        } else {
            b"second base\n".as_slice()
        }
    );
    let snapshot = host.snapshot().unwrap();
    let task: Task = snapshot
        .record(Collection::Task, scope.task.as_str(), &scope.workspace)
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(task.state, *state);
    let effect: Effect = snapshot
        .record(Collection::Effect, effect_id.as_str(), &scope.workspace)
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(
        effect.state,
        if written == 2 {
            EffectState::Succeeded
        } else {
            EffectState::OutcomeUnknown
        }
    );
    let receipts = effect
        .observed_changes
        .iter()
        .filter(|id| {
            snapshot
                .record(Collection::Artifact, id.as_str(), &scope.workspace)
                .unwrap()
                .value["spec"]["schema"]
                == "vcp-file-outcome-v1"
        })
        .count();
    assert_eq!(receipts, written);
    let ledger = vcp_budget::ledger(&snapshot, scope).unwrap();
    assert_eq!(ledger.settled, Micros::new(100));
    assert_eq!(ledger.active, Micros::ZERO);
    assert_eq!(ledger.unresolved, Micros::ZERO);
    effect_id
}

#[cfg(feature = "qualification")]
#[test]
#[ignore = "supervisor launches this integration fault as a real child process"]
fn integration_receipt_fault_process_child() {
    let root = PathBuf::from(
        std::env::var_os("VCP_INTEGRATION_FAULT_ROOT").expect("supervised integration root"),
    );
    let backend = match std::env::var("VCP_INTEGRATION_FAULT_BACKEND")
        .unwrap()
        .as_str()
    {
        "sqlite" => BackendKind::Sqlite,
        "files" => BackendKind::Files,
        _ => panic!("unknown integration backend"),
    };
    let index = std::env::var("VCP_INTEGRATION_FAULT_INDEX")
        .unwrap()
        .parse::<usize>()
        .unwrap();
    assert!(index < 2);
    tokio::runtime::Runtime::new().unwrap().block_on(
        super::child_agents::child_case_with_schedule(
            backend,
            ChildMode::IsolatedWrite,
            false,
            Some(false),
            false,
            true,
            None,
            false,
            Some(Schedule::ProcessExit { root, index }),
        ),
    );
}

#[cfg(feature = "qualification")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn child_integration_process_kill_after_each_write_reopens_without_replay() {
    use std::{process::Stdio, time::Instant};
    struct Supervised(std::process::Child);
    impl Drop for Supervised {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for index in 0..2 {
            let temporary = tempfile::tempdir().unwrap();
            let root = temporary.path();
            let mut process = Supervised(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "child_integration_fault::integration_receipt_fault_process_child",
                        "--ignored",
                        "--nocapture",
                    ])
                    .env("VCP_INTEGRATION_FAULT_ROOT", root)
                    .env(
                        "VCP_INTEGRATION_FAULT_BACKEND",
                        if backend == BackendKind::Sqlite {
                            "sqlite"
                        } else {
                            "files"
                        },
                    )
                    .env("VCP_INTEGRATION_FAULT_INDEX", index.to_string())
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .unwrap(),
            );
            let barrier = root.join("integration-barrier.json");
            let deadline = Instant::now() + Duration::from_secs(60);
            while !barrier.exists() {
                if let Some(status) = process.0.try_wait().unwrap() {
                    panic!("integration child exited before native receipt barrier: {status}");
                }
                assert!(
                    Instant::now() < deadline,
                    "integration native receipt barrier timed out"
                );
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            let marker: serde_json::Value =
                serde_json::from_slice(&fs::read(barrier).unwrap()).unwrap();
            assert_eq!(marker["phase"], "native_write_before_receipt");
            assert_eq!(marker["index"], index);
            let effect_id = ToolRunId::parse(marker["effect"].as_str().unwrap()).unwrap();
            let child_id = TaskId::parse(marker["child"].as_str().unwrap()).unwrap();
            let scope: Scope = serde_json::from_value(marker["scope"].clone()).unwrap();
            let workspace = root.join("workspace").canonicalize().unwrap();
            assert_eq!(
                fs::read(workspace.join("file.txt")).unwrap(),
                b"child changed\n"
            );
            assert_eq!(
                fs::read(workspace.join("second.txt")).unwrap(),
                if index == 0 {
                    b"second base\n".as_slice()
                } else {
                    b"second child changed\n".as_slice()
                }
            );
            process.0.kill().unwrap();
            assert!(!process.0.wait().unwrap().success());

            // Later human edits are independent of the interrupted native write.
            // Reopening/reconciliation must observe them, never roll them back.
            fs::write(
                workspace.join("file.txt"),
                b"human after terminated integration\n",
            )
            .unwrap();
            fs::write(
                workspace.join("second.txt"),
                b"human second after termination\n",
            )
            .unwrap();
            let before: vcp_store::contract::State =
                serde_json::from_slice(&fs::read(root.join("integration-before.json")).unwrap())
                    .unwrap();
            let config = config(&root.join("canonical"), &workspace, backend);
            let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
            let reports = host.reconcile_effects().unwrap();
            let state = host.snapshot().unwrap();
            let effect: Effect = state
                .record(Collection::Effect, effect_id.as_str(), &scope.workspace)
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(effect.state, EffectState::OutcomeUnknown);
            assert!(effect.execution.is_some());
            let task: Task = state
                .record(Collection::Task, scope.task.as_str(), &scope.workspace)
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(task.state, TaskState::Paused);
            assert!(host
                .command(
                    Command::Transition {
                        next: TaskState::Completed,
                        reason: "partial integration is not parent acceptance".into(),
                        verification: None
                    },
                    Some(scope.task.clone()),
                    task.revision
                )
                .is_err());
            assert!(state
                .records
                .values()
                .filter(|row| row.collection == Collection::Task)
                .all(|row| row.value["state"] != "completed"));
            let ledger = vcp_budget::ledger(&state, &scope).unwrap();
            assert_eq!(ledger.settled, Micros::new(100));
            assert_eq!(ledger.active, Micros::ZERO);
            assert_eq!(ledger.unresolved, Micros::ZERO);
            let before_attempts: Vec<_> = before
                .records
                .values()
                .filter(|row| row.collection == Collection::Attempt)
                .collect();
            let attempts: Vec<_> = state
                .records
                .values()
                .filter(|row| row.collection == Collection::Attempt)
                .collect();
            assert_eq!(
                attempts, before_attempts,
                "recovery cannot resubmit child provider work"
            );
            assert_eq!(attempts.len(), 1);
            let old_graph = vcp_engine::agents::graph(&before, &scope, &scope.task)
                .unwrap()
                .unwrap();
            let graph = vcp_engine::agents::graph(&state, &scope, &scope.task)
                .unwrap()
                .unwrap();
            assert_eq!(graph.results[&child_id], old_graph.results[&child_id]);
            assert_eq!(graph.results[&child_id].len(), 2);
            for row in before
                .records
                .values()
                .filter(|row| row.collection == Collection::Artifact)
            {
                assert_eq!(
                    state
                        .record(Collection::Artifact, &row.id, &scope.workspace)
                        .unwrap(),
                    row,
                    "history artifact changed during recovery"
                );
            }
            let mut intents = 0;
            let mut receipts = 0;
            for row in state
                .records
                .values()
                .filter(|row| row.collection == Collection::Artifact)
            {
                let artifact: ArtifactDescriptor = row.decode().unwrap();
                if matches!(
                    artifact.spec.schema.as_str(),
                    "vcp-file-intent-v1" | "vcp-file-outcome-v1"
                ) {
                    let value: serde_json::Value =
                        serde_json::from_slice(&host.read_artifact(artifact.spec.id).unwrap())
                            .unwrap();
                    if value["effect"] == serde_json::json!(effect_id) {
                        if artifact.spec.schema == "vcp-file-intent-v1" {
                            intents += 1;
                        } else {
                            receipts += 1;
                        }
                    }
                }
            }
            assert_eq!(intents, index + 1);
            assert_eq!(
                receipts, index,
                "terminated write cannot invent its missing receipt"
            );
            assert!(reports.iter().any(|report| {
                let value: serde_json::Value =
                    serde_json::from_slice(&host.read_artifact(report.spec.id.clone()).unwrap())
                        .unwrap();
                value["effect"] == serde_json::json!(effect_id)
                    && value["replayed"] == false
                    && value["outcome"] == "outcome_unknown"
                    && !value["observations"].as_array().unwrap().is_empty()
            }));
            assert_eq!(
                fs::read(workspace.join("file.txt")).unwrap(),
                b"human after terminated integration\n"
            );
            assert_eq!(
                fs::read(workspace.join("second.txt")).unwrap(),
                b"human second after termination\n"
            );
            assert_eq!(
                fs::read(
                    root.join("children")
                        .join(child_id.as_str())
                        .join("file.txt")
                )
                .unwrap(),
                b"child changed\n"
            );
            assert_eq!(
                fs::read(
                    root.join("children")
                        .join(child_id.as_str())
                        .join("second.txt")
                )
                .unwrap(),
                b"second child changed\n"
            );
            owner.close().await.unwrap();
        }
    }
}

#[cfg(feature = "qualification")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn child_integration_pause_between_writes_retains_receipts_and_human_edits() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        super::child_agents::child_case_with_helper(
            backend,
            ChildMode::IsolatedWrite,
            false,
            Some(false),
            false,
            true,
            None,
            true,
        )
        .await;
    }
}

#[cfg(feature = "qualification")]
pub(super) fn apply_paused(
    host: &CanonicalHost,
    proposal: ToolProposal,
    workspace: &Path,
    scope: &Scope,
) -> ToolRunId {
    let mut boundaries = 0;
    let output = host
        .dispatch_tool_with_receipt_observer(proposal, |count| {
            boundaries += 1;
            assert_eq!(count, 1);
            let state = host.snapshot().unwrap();
            let task: Task = state
                .record(Collection::Task, scope.task.as_str(), &scope.workspace)
                .unwrap()
                .decode()
                .unwrap();
            host.stop(
                host.control_envelope(
                    CommandId::new(),
                    scope.task.clone(),
                    task.revision,
                    Command::Transition {
                        next: TaskState::Paused,
                        reason: "qualification pause after first durable file receipt".into(),
                        verification: None,
                    },
                )
                .unwrap(),
            )
            .unwrap();
        })
        .unwrap();
    assert_eq!(boundaries, 1);
    assert_eq!(output.result["complete"], false);
    assert_eq!(output.result["files"].as_array().unwrap().len(), 1);
    assert_eq!(
        fs::read(workspace.join("file.txt")).unwrap(),
        b"child changed\n"
    );
    assert_eq!(
        fs::read(workspace.join("second.txt")).unwrap(),
        b"second base\n"
    );
    let state = host.snapshot().unwrap();
    let effect: Effect = state
        .record(Collection::Effect, output.effect.as_str(), &scope.workspace)
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(effect.state, EffectState::OutcomeUnknown);
    assert!(effect.observed_changes.iter().any(|id| {
        let descriptor: ArtifactDescriptor = state
            .record(Collection::Artifact, id.as_str(), &scope.workspace)
            .unwrap()
            .decode()
            .unwrap();
        descriptor.spec.schema == "vcp-file-outcome-v1"
            && descriptor.state == CaptureState::Complete
    }));
    output.effect
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn child_integration_partial_native_failure_retains_receipts_and_human_edits() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        super::child_agents::child_case(
            backend,
            ChildMode::IsolatedWrite,
            false,
            Some(false),
            false,
            true,
        )
        .await;
    }
}

pub(super) fn apply_partial(
    host: &CanonicalHost,
    proposal: ToolProposal,
    workspace: &Path,
    scope: &Scope,
) -> ToolRunId {
    // Read access remains possible for initial manifest revalidation; mutation
    // of the second path is denied deterministically by the OS sharing handle.
    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(workspace.join("second.txt"))
        .unwrap();
    let output = host.dispatch_tool(proposal).unwrap();
    assert_eq!(output.result["complete"], false);
    assert_eq!(output.result["files"].as_array().unwrap().len(), 1);
    assert_eq!(
        fs::read(workspace.join("file.txt")).unwrap(),
        b"child changed\n"
    );
    assert_eq!(
        fs::read(workspace.join("second.txt")).unwrap(),
        b"second base\n"
    );
    let state = host.snapshot().unwrap();
    let effect: Effect = state
        .record(Collection::Effect, output.effect.as_str(), &scope.workspace)
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(effect.state, EffectState::OutcomeUnknown);
    let receipts: Vec<ArtifactDescriptor> = effect
        .observed_changes
        .iter()
        .map(|id| {
            state
                .record(Collection::Artifact, id.as_str(), &scope.workspace)
                .unwrap()
                .decode()
                .unwrap()
        })
        .collect();
    assert_eq!(
        receipts
            .iter()
            .filter(|artifact| artifact.spec.schema == "vcp-file-outcome-v1")
            .count(),
        1
    );
    assert!(receipts
        .iter()
        .any(|artifact| artifact.spec.id == output.evidence.spec.id
            && artifact.state == CaptureState::Complete));
    drop(locked);
    output.effect
}

pub(super) fn reconcile_preserves_edits(
    host: &CanonicalHost,
    thread: codex_protocol::ThreadId,
    workspace: &Path,
    scope: &Scope,
    effect_id: &ToolRunId,
) {
    fs::write(workspace.join("file.txt"), b"human after first applied\n").unwrap();
    fs::write(
        workspace.join("second.txt"),
        b"human after second blocked\n",
    )
    .unwrap();
    let state = host.snapshot().unwrap();
    let parent: Task = state
        .record(Collection::Task, scope.task.as_str(), &scope.workspace)
        .unwrap()
        .decode()
        .unwrap();
    let before: Effect = state
        .record(Collection::Effect, effect_id.as_str(), &scope.workspace)
        .unwrap()
        .decode()
        .unwrap();
    if !matches!(parent.state, TaskState::Paused | TaskState::Cancelled) {
        host.stop(
            host.control_envelope(
                CommandId::new(),
                scope.task.clone(),
                parent.revision,
                Command::Transition {
                    next: TaskState::Paused,
                    reason: "inspect partial child integration without replay".into(),
                    verification: None,
                },
            )
            .unwrap(),
        )
        .unwrap();
    }
    let reports = host.reconcile_effects().unwrap();
    if before.state != EffectState::Succeeded {
        assert!(!reports.is_empty());
    }
    let state = host.snapshot().unwrap();
    let after: Effect = state
        .record(Collection::Effect, effect_id.as_str(), &scope.workspace)
        .unwrap()
        .decode()
        .unwrap();
    if before.state == EffectState::Succeeded {
        assert_eq!(after.state, EffectState::Succeeded);
    } else {
        assert_ne!(after.state, EffectState::Succeeded);
    }
    for id in &before.observed_changes {
        assert!(after.observed_changes.contains(id));
    }
    // An already unknown effect need not transition again. Recovery publishes
    // its observation separately instead of claiming a new effect outcome.
    let mut linked_report = false;
    for report in reports {
        let persisted: ArtifactDescriptor = state
            .record(
                Collection::Artifact,
                report.spec.id.as_str(),
                &scope.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(persisted, report);
        assert_eq!(report.state, CaptureState::Complete);
        assert_eq!(report.spec.schema, "vcp-effect-reconciliation-v1");
        let bytes = host.read_artifact(report.spec.id).unwrap();
        assert_eq!(vcp_protocol::digest_bytes(&bytes), report.sha256);
        let observation: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        if observation["effect"] == serde_json::json!(effect_id) {
            linked_report = true;
            assert_eq!(
                observation["execution"],
                serde_json::json!(before.execution)
            );
            assert_eq!(observation["replayed"], false);
            assert_eq!(
                observation["outcome"],
                serde_json::json!(EffectState::OutcomeUnknown)
            );
            assert!(!observation["observations"].as_array().unwrap().is_empty());
        }
    }
    assert!(
        linked_report || before.state == EffectState::Succeeded,
        "durable reconciliation must identify this partial integration"
    );
    assert_eq!(
        fs::read(workspace.join("file.txt")).unwrap(),
        b"human after first applied\n"
    );
    assert_eq!(
        fs::read(workspace.join("second.txt")).unwrap(),
        b"human after second blocked\n"
    );
    assert!(host.complete_coding_turn(thread).is_err());
    let parent: Task = host
        .snapshot()
        .unwrap()
        .record(Collection::Task, scope.task.as_str(), &scope.workspace)
        .unwrap()
        .decode()
        .unwrap();
    assert!(matches!(
        parent.state,
        TaskState::Paused | TaskState::Cancelled
    ));
}
