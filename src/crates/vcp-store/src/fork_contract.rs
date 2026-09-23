// SPDX-License-Identifier: Apache-2.0
//! The only receipted cross-session transaction is exact metadata-fork genesis.
//! This is deliberately not a general relaxation of receipt/event scope.
use crate::{contract::*, Error, Result};
use vcp_domain::{
    revision::*,
    task::{Task, TaskState, Turn, TurnState},
    workspace::Session,
};
use vcp_protocol::{
    command::{
        fork::{self, Acceptance},
        CommandResult,
    },
    event::EventKind,
};

pub(crate) fn validate(state: &State, transaction: &Transaction) -> Result<()> {
    let receipt = transaction.command.as_ref().ok_or(Error::Access)?;
    if transaction.events.len() != 2
        || transaction.mutations.len() != 2
        || receipt.result
            != (CommandResult::Accepted {
                revision: Revision::ZERO,
            })
    {
        return Err(Error::Access);
    }
    let source = &transaction.events[0];
    let target = &transaction.events[1];
    let accepted: Acceptance =
        serde_json::from_value(source.data.clone()).map_err(|_| Error::Access)?;
    if accepted.schema_version != 1
        || accepted.source.workspace != receipt.workspace
        || accepted.source.session != receipt.session
        || accepted.new_session == receipt.session
        || accepted.new_task == accepted.source.task
        || state.sequences.contains_key(&accepted.new_session)
        || state.events.iter().any(|event| event.event.session == accepted.new_session)
        || source.workspace != receipt.workspace
        || source.session != receipt.session
        || source.correlation != receipt.command
        || source.kind != EventKind::SessionStarted
        || source.task.is_some()
        || source.causation.is_some()
        || !source.artifacts.is_empty()
        || source.metadata.is_some()
        || target.workspace != receipt.workspace
        || target.session != accepted.new_session
        || target.task.as_ref() != Some(&accepted.new_task)
        || target.kind != EventKind::TaskCreated
        || target.correlation != fork::target_correlation(&receipt.command, &accepted)?
        || target.correlation == source.correlation
        || target.causation.as_ref() != Some(&source.id)
        || target.actor != source.actor
        || target.timestamp != source.timestamp
        || target.id == source.id
        || !target.artifacts.is_empty()
        || target.metadata.is_some()
    {
        return Err(Error::Access);
    }
    let old_session: Session = state
        .record(
            Collection::Session,
            receipt.session.as_str(),
            &receipt.workspace,
        )?
        .decode()?;
    let turn: Turn = state
        .record(
            Collection::Turn,
            accepted.through_turn.as_str(),
            &receipt.workspace,
        )?
        .decode()?;
    let old_task: Task = state
        .record(
            Collection::Task,
            accepted.source.task.as_str(),
            &receipt.workspace,
        )?
        .decode()?;
    if old_session.workspace != receipt.workspace
        || old_task.scope != accepted.source
        || old_task.redaction.is_some()
        || turn.scope != accepted.source
        || turn.redaction.is_some()
        || turn.state != TurnState::Completed
        || !state.events.iter().any(|event| {
            event.event.id == turn.cause
                && event.watermark == accepted.through_watermark
                && event.event.workspace == receipt.workspace
                && event.event.session == receipt.session
                && event.event.task.as_ref() == Some(&accepted.source.task)
                && event.redaction.is_none()
        })
    {
        return Err(Error::Access);
    }
    let mut session = None;
    let mut task = None;
    for mutation in &transaction.mutations {
        let Mutation::Put {
            expected: None,
            record,
        } = mutation
        else {
            return Err(Error::Access);
        };
        if record.workspace != receipt.workspace
            || record.revision != Revision::ZERO
            || !record.references.is_empty()
            || state.records.contains_key(&record.key())
        {
            return Err(Error::Access);
        }
        match record.collection {
            Collection::Session
                if session.is_none() && record.id == accepted.new_session.as_str() =>
            {
                session = Some(record.decode::<Session>()?);
            }
            Collection::Task if task.is_none() && record.id == accepted.new_task.as_str() => {
                task = Some(record.decode::<Task>()?);
            }
            _ => return Err(Error::Access),
        }
    }
    let session = session.ok_or(Error::Access)?;
    let task = task.ok_or(Error::Access)?;
    if session.id != accepted.new_session
        || session.workspace != receipt.workspace
        || session.revision != Revision::ZERO
        || session.configuration != old_session.configuration
        || session.fork_origin.as_ref() != Some(&receipt.session)
        || session.fork_through.as_ref() != Some(&accepted.through_turn)
        || task.scope.workspace != receipt.workspace
        || task.scope.session != accepted.new_session
        || task.scope.task != accepted.new_task
        || task.root != accepted.new_task
        || task.parent.is_some()
        || task.fork_origin.as_ref() != Some(&accepted.source.task)
        || task.revision != Revision::ZERO
        || task.steering != SteeringRevision::ZERO
        || task.state != TaskState::Pending
        || task.redaction.is_some()
        || task.cause != target.id
        || task.objectives.len() != 1
        || task.objectives[0].source != target.id
        || task.objectives[0].steering != SteeringRevision::ZERO
        || target.data != fork::genesis_facts(&session, &task)
    {
        return Err(Error::Access);
    }
    task.validate()?;
    Ok(())
}
