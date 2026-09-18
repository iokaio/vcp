// SPDX-License-Identifier: Apache-2.0
use vcp_domain::{task::*, verification::*, workspace::*, *};

fn task() -> Task {
    let id = TaskId::new();
    Task {
        scope: Scope {
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
            task: id.clone(),
        },
        root: id,
        parent: None,
        fork_origin: None,
        revision: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        objectives: vec![Objective {
            text: "Fix the synthetic result".into(),
            constraints: vec!["preserve user edits".into()],
            acceptance: vec!["fixture passes".into()],
            source: EventId::new(),
            steering: SteeringRevision::ZERO,
        }],
        state: TaskState::Pending,
        fingerprint: Fingerprint {
            repository: "a".repeat(64),
            buffers: "b".repeat(64),
            environment: "c".repeat(64),
        },
        editing: true,
        required_checks: vec!["fixture".into()],
        cause: EventId::new(),
        reason: "created".into(),
    }
}
fn evidence(task: &Task) -> Verification {
    Verification {
        id: VerificationId::new(),
        scope: task.scope.clone(),
        steering: task.steering,
        fingerprint: task.fingerprint.clone(),
        outputs: vec![ArtifactId::new()],
        checks: vec![Check {
            specification: "fixture".into(),
            outcome: CheckOutcome::Passed,
            output: ArtifactId::new(),
            exit_code: Some(0),
        }],
        unresolved_effects: vec![],
        outstanding_issues: vec![],
        cost: CostCertainty::Known,
    }
}
fn transition(task: &Task, next: TaskState, verification: Option<&Verification>) -> Result<Task> {
    task.transition(
        &task.scope,
        task.revision,
        task.steering,
        next,
        EventId::new(),
        "test transition".into(),
        verification,
        None,
    )
}

#[test]
fn completion_requires_current_objective_fingerprint_and_real_applicable_checks() {
    let original = task();
    original.validate().unwrap();
    let active = transition(&original, TaskState::Running, None).unwrap();
    assert_eq!(
        transition(&active, TaskState::Completed, None),
        Err(Error::Evidence)
    );
    let verified = evidence(&active);
    assert_eq!(
        transition(&active, TaskState::Completed, Some(&verified))
            .unwrap()
            .state,
        TaskState::Completed
    );
    for failed in [
        CheckOutcome::Failed {
            reason: "assertion".into(),
        },
        CheckOutcome::NotRun {
            reason: "tool missing".into(),
        },
    ] {
        let mut invalid = verified.clone();
        invalid.checks[0].outcome = failed;
        assert_eq!(
            transition(&active, TaskState::Completed, Some(&invalid)),
            Err(Error::Evidence)
        );
    }
    let mut changed = active.fingerprint.clone();
    changed.repository = "d".repeat(64);
    let edited = active
        .observe_fingerprint(active.revision, changed, EventId::new())
        .unwrap();
    assert_eq!(
        transition(&edited, TaskState::Completed, Some(&verified)),
        Err(Error::Evidence)
    );
    let steered = active
        .steer(
            active.revision,
            Objective {
                text: "Also verify the edge case".into(),
                ..active.objectives[0].clone()
            },
        )
        .unwrap();
    assert_eq!(
        transition(&steered, TaskState::Completed, Some(&verified)),
        Err(Error::Evidence)
    );
    assert_eq!(steered.objectives.len(), 2);
    assert_eq!(original.state, TaskState::Pending);
}

#[test]
fn generated_sequences_cannot_complete_without_evidence_or_dispatch_stale_work() {
    // A deterministic generated state walk exercises invariants independently of
    // an expected transition table. Failure must leave every byte unchanged.
    for seed in 1u64..=64 {
        let mut random = seed;
        let mut current = task();
        for _ in 0..96 {
            random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
            let states = [
                TaskState::Running,
                TaskState::Paused,
                TaskState::WaitingForInput,
                TaskState::Blocked,
                TaskState::Completed,
                TaskState::Cancelled,
                TaskState::Failed,
            ];
            let next = states[(random >> 32) as usize % states.len()];
            let before = current.clone();
            let result = transition(&current, next, None);
            assert_eq!(current, before);
            if let Ok(updated) = result {
                current = updated;
            }
            assert_ne!(current.state, TaskState::Completed);
            assert!(!current.can_dispatch(&current.scope, current.steering.next().unwrap(), true));
            let mut foreign = current.scope.clone();
            foreign.workspace = WorkspaceId::new();
            assert!(!current.can_dispatch(&foreign, current.steering, true));
        }
    }
}

#[test]
fn pause_preserves_objective_and_revalidation_is_required_to_resume() {
    let active = transition(&task(), TaskState::Running, None).unwrap();
    let paused = transition(&active, TaskState::Paused, None).unwrap();
    assert_eq!(paused.objectives, active.objectives);
    assert_eq!(paused.root, active.root);
    assert_eq!(
        transition(&paused, TaskState::Running, None),
        Err(Error::Revalidation)
    );
    let proof = ResumeEvidence {
        workspace_current: true,
        policy_current: true,
        budget_current: true,
        effects_reconciled: true,
        owner_current: true,
    };
    assert_eq!(
        paused
            .transition(
                &paused.scope,
                paused.revision,
                paused.steering,
                TaskState::Running,
                EventId::new(),
                "revalidated".into(),
                None,
                Some(&proof)
            )
            .unwrap()
            .state,
        TaskState::Running
    );
    assert_eq!(
        paused.transition(
            &paused.scope,
            active.revision,
            paused.steering,
            TaskState::Cancelled,
            EventId::new(),
            "stale".into(),
            None,
            None
        ),
        Err(Error::Stale)
    );
}

#[test]
fn counters_round_trip_full_u64_range_as_strings_and_rebinding_retains_identity() {
    for value in [0, 1, 9_007_199_254_740_993, u64::MAX] {
        let revision = Revision::new(value);
        let json = serde_json::to_string(&revision).unwrap();
        assert_eq!(json, format!("\"{value}\""));
        assert_eq!(serde_json::from_str::<Revision>(&json).unwrap(), revision);
    }
    for invalid in ["1", "\"01\"", "\"-1\"", "\"18446744073709551616\""] {
        assert!(serde_json::from_str::<Revision>(invalid).is_err());
    }
    assert_eq!(Revision::new(u64::MAX).next(), Err(Error::Overflow));
    let workspace = Workspace {
        id: WorkspaceId::new(),
        binding: Binding {
            host: HostId::new(),
            root: "C:/fixture".into(),
            repository: "repo".into(),
            worktree: "main".into(),
            revision: Revision::ZERO,
        },
        trust: Trust::Trusted,
        revision: Revision::ZERO,
        authority: AuthorityRevision::ZERO,
        deletion: DeletionEpoch::ZERO,
    };
    let rebound = workspace
        .rebind(
            workspace.revision,
            Binding {
                root: "D:/renamed".into(),
                ..workspace.binding.clone()
            },
        )
        .unwrap();
    assert_eq!(workspace.id, rebound.id);
    assert_eq!(rebound.trust, Trust::Untrusted);
    assert_eq!(rebound.authority.get(), 1);
    assert_eq!(workspace.binding.root, "C:/fixture");
}
