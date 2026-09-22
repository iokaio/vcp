// SPDX-License-Identifier: Apache-2.0
//! Real native sharing failure after an earlier parent integration write.
use super::*;
use std::{fs, os::windows::fs::OpenOptionsExt, path::Path};
use vcp_domain::{
    agents::ChildMode,
    effect::{Effect, EffectState},
};
use vcp_lifecycle::foundation::ToolProposal;

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
    let reports = host.reconcile_effects().unwrap();
    assert!(!reports.is_empty());
    let state = host.snapshot().unwrap();
    let after: Effect = state
        .record(Collection::Effect, effect_id.as_str(), &scope.workspace)
        .unwrap()
        .decode()
        .unwrap();
    assert_ne!(after.state, EffectState::Succeeded);
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
        linked_report,
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
    assert_eq!(parent.state, TaskState::Paused);
}
