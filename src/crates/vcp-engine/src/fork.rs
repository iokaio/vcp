// SPDX-License-Identifier: Apache-2.0
//! Atomic metadata forks. Historical evidence is not execution authority.
use crate::{Error, Result};
use vcp_domain::{
    artifact::{ArtifactDescriptor, CaptureState},
    ids::*,
    retention::RetentionMask,
    revision::*,
    task::{Task, TaskState, Turn, TurnState},
    workspace::{Scope, Session, Workspace},
};
use vcp_protocol::{
    command::{
        fork::{self, Acceptance, Format},
        CommandEnvelope, CommandResult,
    },
    event::{EventInput, EventKind},
};
use vcp_store::contract::{key, Collection, Mutation, ReceiptInput, Record, State, Transaction};

pub(crate) struct Source {
    pub session: Session,
    pub turn: Turn,
    pub historical_task: Task,
    pub through_watermark: Watermark,
}

/// Both the immutable completed boundary and its historical task snapshot must
/// remain visible. A newer current task cannot supply old fingerprint evidence.
pub(crate) fn source(
    state: &State,
    workspace: &WorkspaceId,
    session: &SessionId,
    through: &TurnId,
) -> Result<Source> {
    let unavailable = || Error::Unavailable;
    let current_workspace: Workspace = state
        .record(Collection::Workspace, workspace.as_str(), workspace)
        .map_err(|_| unavailable())?
        .decode()
        .map_err(|_| unavailable())?;
    let source_session: Session = state
        .record(Collection::Session, session.as_str(), workspace)
        .map_err(|_| unavailable())?
        .decode()
        .map_err(|_| unavailable())?;
    if &source_session.id != session || &source_session.workspace != workspace {
        return Err(unavailable());
    }
    let turn: Turn = state
        .record(Collection::Turn, through.as_str(), workspace)
        .map_err(|_| unavailable())?
        .decode()
        .map_err(|_| unavailable())?;
    if &turn.id != through
        || &turn.scope.workspace != workspace
        || &turn.scope.session != session
        || turn.state != TurnState::Completed
        || turn.redaction.is_some()
    {
        return Err(unavailable());
    }
    let current: Task = state
        .record(Collection::Task, turn.scope.task.as_str(), workspace)
        .map_err(|_| unavailable())?
        .decode()
        .map_err(|_| unavailable())?;
    current.validate().map_err(|_| unavailable())?;
    if current.scope != turn.scope || current.redaction.is_some() {
        return Err(unavailable());
    }
    let trigger: ArtifactDescriptor = state
        .record(Collection::Artifact, turn.trigger.as_str(), workspace)
        .map_err(|_| unavailable())?
        .decode()
        .map_err(|_| unavailable())?;
    trigger.validate().map_err(|_| unavailable())?;
    if trigger.spec.scope != turn.scope
        || trigger.spec.id != turn.trigger
        || trigger.state != CaptureState::Complete
    {
        return Err(unavailable());
    }
    let boundary = state
        .events
        .iter()
        .find(|event| event.event.id == turn.cause)
        .ok_or_else(unavailable)?;
    if boundary.redaction.is_some()
        || boundary.event.workspace != *workspace
        || boundary.event.session != *session
        || boundary.event.task.as_ref() != Some(&turn.scope.task)
        || !matches!(
            boundary.event.kind,
            EventKind::TurnTransition | EventKind::TaskTransition
        )
        || boundary.event.data["schema_version"] != 1
    {
        return Err(unavailable());
    }
    let facts = boundary.event.data["facts"]
        .as_array()
        .ok_or_else(unavailable)?;
    let mut proved = false;
    for fact in facts
        .iter()
        .filter(|fact| fact["collection"] == "turn" && fact["id"] == through.as_str())
    {
        let retained: Turn =
            serde_json::from_value(fact["value"].clone()).map_err(|_| unavailable())?;
        if retained != turn || fact["revision"] != serde_json::to_value(turn.revision)? || proved {
            return Err(unavailable());
        }
        proved = true;
    }
    if !proved {
        return Err(unavailable());
    }
    // Logical suppression is effective before physical rewriting. Reject a gap
    // rather than falling back to an older snapshot with apparently valid data.
    for row in state
        .records
        .values()
        .filter(|row| row.collection == Collection::Tombstone && &row.workspace == workspace)
    {
        let mask: RetentionMask = row.decode().map_err(|_| unavailable())?;
        mask.validate().map_err(|_| unavailable())?;
        if &mask.workspace != workspace || mask.deletion > current_workspace.deletion {
            return Err(unavailable());
        }
        if mask.artifacts.contains(&turn.trigger)
            || (&mask.session == session
                && state.events.iter().any(|event| {
                    event.watermark <= boundary.watermark
                        && event.event.workspace == *workspace
                        && event.event.session == *session
                        && event.event.task.as_ref() == Some(&turn.scope.task)
                        && mask.first <= event.sequence
                        && event.sequence <= mask.last
                }))
        {
            return Err(unavailable());
        }
    }
    let mut historical = None;
    for event in state.events.iter().filter(|event| {
        event.watermark <= boundary.watermark
            && event.event.workspace == *workspace
            && event.event.session == *session
            && event.event.task.as_ref() == Some(&turn.scope.task)
    }) {
        if event.redaction.is_some() {
            return Err(unavailable());
        }
        // Only these internal command events are task-snapshot producers.
        if !matches!(
            event.event.kind,
            EventKind::TaskCreated
                | EventKind::TaskTransition
                | EventKind::ObjectiveChanged
                | EventKind::FingerprintObserved
        ) {
            continue;
        }
        if event.event.data["schema_version"] != 1 {
            return Err(unavailable());
        }
        let facts = event.event.data["facts"]
            .as_array()
            .ok_or_else(unavailable)?;
        for fact in facts
            .iter()
            .filter(|fact| fact["collection"] == "task" && fact["id"] == turn.scope.task.as_str())
        {
            let task: Task =
                serde_json::from_value(fact["value"].clone()).map_err(|_| unavailable())?;
            task.validate().map_err(|_| unavailable())?;
            if task.scope != turn.scope
                || task.redaction.is_some()
                || fact["revision"] != serde_json::to_value(task.revision)?
            {
                return Err(unavailable());
            }
            historical = Some(task);
        }
    }
    let historical_task = historical.ok_or_else(unavailable)?;
    if historical_task.steering != turn.steering
        || !historical_task
            .objectives
            .iter()
            .any(|objective| objective.steering == turn.steering)
    {
        return Err(unavailable());
    }
    Ok(Source {
        session: source_session,
        turn,
        historical_task,
        through_watermark: boundary.watermark,
    })
}

#[cfg(test)]
mod tests;

pub(crate) fn available_targets(state: &State, session: &SessionId, task: &TaskId) -> Result<()> {
    if state
        .records
        .contains_key(&key(Collection::Session, session.as_str()))
        || state
            .records
            .contains_key(&key(Collection::Task, task.as_str()))
    {
        return Err(Error::Target);
    }
    Ok(())
}

pub(crate) fn transaction(
    state: &State,
    command: &CommandEnvelope,
    digest: String,
    new_session: &SessionId,
    new_task: &TaskId,
    through: &TurnId,
    now: Timestamp,
) -> Result<Transaction> {
    if command.task.is_some()
        || command.expected != Revision::ZERO
        || command.steering != SteeringRevision::ZERO
        || new_session == &command.session
    {
        return Err(Error::Target);
    }
    available_targets(state, new_session, new_task)?;
    let source = source(state, &command.workspace, &command.session, through)?;
    let accepted = Acceptance {
        document_type: Format::V1,
        schema_version: 1,
        source: source.turn.scope.clone(),
        through_turn: through.clone(),
        through_watermark: source.through_watermark,
        new_session: new_session.clone(),
        new_task: new_task.clone(),
    };
    let source_event = EventId::new();
    let target_event = EventId::new();
    let mut objective = source
        .historical_task
        .objectives
        .iter()
        .find(|objective| objective.steering == source.turn.steering)
        .cloned()
        .ok_or(Error::Unavailable)?;
    objective.source = target_event.clone();
    objective.steering = SteeringRevision::ZERO;
    let session = Session {
        id: new_session.clone(),
        workspace: command.workspace.clone(),
        revision: Revision::ZERO,
        configuration: source.session.configuration,
        fork_origin: Some(command.session.clone()),
        fork_through: Some(through.clone()),
    };
    let task = Task {
        scope: Scope {
            workspace: command.workspace.clone(),
            session: new_session.clone(),
            task: new_task.clone(),
        },
        root: new_task.clone(),
        parent: None,
        fork_origin: Some(source.turn.scope.task),
        revision: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        objectives: vec![objective],
        state: TaskState::Pending,
        fingerprint: source.historical_task.fingerprint,
        editing: source.historical_task.editing,
        required_checks: source.historical_task.required_checks,
        cause: target_event.clone(),
        reason: "metadata fork; explicit profile, budget and execution admission required".into(),
        redaction: None,
    };
    task.validate()?;
    let mutations = vec![
        Mutation::Put {
            expected: None,
            record: Record::typed(
                Collection::Session,
                new_session.as_str(),
                command.workspace.clone(),
                Revision::ZERO,
                &session,
            )?,
        },
        Mutation::Put {
            expected: None,
            record: Record::typed(
                Collection::Task,
                new_task.as_str(),
                command.workspace.clone(),
                Revision::ZERO,
                &task,
            )?,
        },
    ];
    let events = vec![
        EventInput {
            id: source_event.clone(),
            workspace: command.workspace.clone(),
            session: command.session.clone(),
            task: None,
            actor: command.caller.clone(),
            correlation: command.id.clone(),
            causation: None,
            timestamp: now,
            kind: EventKind::SessionStarted,
            artifacts: vec![],
            data: serde_json::to_value(&accepted)?,
            metadata: None,
        },
        EventInput {
            id: target_event,
            workspace: command.workspace.clone(),
            session: new_session.clone(),
            task: Some(new_task.clone()),
            actor: command.caller.clone(),
            correlation: fork::target_correlation(&command.id, &accepted)?,
            causation: Some(source_event),
            timestamp: now,
            kind: EventKind::TaskCreated,
            artifacts: vec![],
            data: fork::genesis_facts(&session, &task),
            metadata: None,
        },
    ];
    Ok(Transaction {
        id: TransactionId::new(),
        expected_watermark: state.watermark,
        mutations,
        events,
        command: Some(ReceiptInput {
            command: command.id.clone(),
            workspace: command.workspace.clone(),
            session: command.session.clone(),
            digest,
            result: CommandResult::Accepted {
                revision: Revision::ZERO,
            },
        }),
    })
}
