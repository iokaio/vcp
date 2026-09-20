// SPDX-License-Identifier: Apache-2.0
use vcp_domain::{task::*, verification::*, workspace::*, *};
use vcp_protocol::{command::CommandResult, event::*, redaction};
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
        redaction: None,
    }
}
#[test]
fn explicit_erasure_preserves_identity_without_payload_or_normal_entity_forgery() {
    let marker = "closed-private-objective-918";
    let mut task = task();
    task.objectives[0].text = marker.into();
    task.reason = marker.into();
    assert!(redaction::task(&task, DeletionEpoch::new(1)).is_err());
    task.state = TaskState::Cancelled;
    let before = task.clone();
    let purged = redaction::task(&task, DeletionEpoch::new(1)).unwrap();
    assert_eq!(purged.scope, task.scope);
    assert_eq!(purged.revision, task.revision);
    assert!(purged.objectives.is_empty());
    assert!(purged.validate().is_ok());
    let mut disguised = purged.clone();
    disguised.redaction = None;
    assert!(disguised.validate().is_err());
    let receipt = redaction::inspection(
        &CommandResult::Inspection {
            task: Some(before.clone()),
        },
        DeletionEpoch::new(1),
    )
    .unwrap();
    assert!(
        matches!(&receipt, CommandResult::InspectionRedacted { task: Some(id), .. } if *id == before.scope.task)
    );
    let event = EventEnvelope {
        version: 1,
        sequence: SessionSeq::new(3),
        watermark: Watermark::new(9),
        redaction: None,
        event: EventInput {
            id: EventId::new(),
            workspace: task.scope.workspace.clone(),
            session: task.scope.session.clone(),
            task: Some(task.scope.task.clone()),
            actor: ActorId::new(),
            correlation: CommandId::new(),
            causation: None,
            timestamp: Timestamp::new(1),
            kind: EventKind::TaskCreated,
            artifacts: vec![],
            data: serde_json::json!({"objective":marker}),
            metadata: Some(EventMetadata {
                paths: vec![marker.into()],
                ..Default::default()
            }),
        },
    };
    let erased = redaction::event(&event, DeletionEpoch::new(1)).unwrap();
    assert_eq!(erased.event.id, event.event.id);
    assert_eq!(erased.sequence, event.sequence);
    assert!(erased.event.data.is_null());
    redaction::validate_event(&erased).unwrap();
    for bytes in [
        vcp_protocol::canonical_bytes(&purged).unwrap(),
        vcp_protocol::canonical_bytes(&receipt).unwrap(),
        vcp_protocol::canonical_bytes(&erased).unwrap(),
    ] {
        assert!(!bytes.windows(marker.len()).any(|v| v == marker.as_bytes()));
    }
    let mut malformed = erased;
    malformed.event.data = serde_json::json!({"objective":marker});
    assert!(redaction::validate_event(&malformed).is_err());
}
