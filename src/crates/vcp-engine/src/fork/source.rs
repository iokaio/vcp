// SPDX-License-Identifier: Apache-2.0
//! Shared current-source and ordered historical snapshot proof.
use super::*;
use vcp_protocol::event::EventEnvelope;
use vcp_store::{contract::CanonicalStore, CurrentStateView};

struct Current {
    workspace: Workspace,
    session: Session,
    turn: Turn,
}
impl Current {
    fn new(
        state: CurrentStateView<'_>,
        workspace: &WorkspaceId,
        session: &SessionId,
        through: &TurnId,
    ) -> Result<Self> {
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
        Ok(Self {
            workspace: current_workspace,
            session: source_session,
            turn,
        })
    }

    fn boundary(
        self,
        state: CurrentStateView<'_>,
        boundary: &EventEnvelope,
    ) -> Result<Accumulator> {
        let unavailable = || Error::Unavailable;
        let turn = &self.turn;
        if boundary.event.id != turn.cause
            || boundary.redaction.is_some()
            || boundary.event.workspace != turn.scope.workspace
            || boundary.event.session != turn.scope.session
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
            .filter(|fact| fact["collection"] == "turn" && fact["id"] == turn.id.as_str())
        {
            let retained: Turn =
                serde_json::from_value(fact["value"].clone()).map_err(|_| unavailable())?;
            if retained != *turn
                || fact["revision"] != serde_json::to_value(turn.revision)?
                || proved
            {
                return Err(unavailable());
            }
            proved = true;
        }
        if !proved {
            return Err(unavailable());
        }
        // Preserve logical suppression before task-snapshot selection. The
        // accumulator needs only current sequence ranges, not tombstone payloads.
        let mut masks = Vec::new();
        for row in state.records.values().filter(|row| {
            row.collection == Collection::Tombstone && row.workspace == turn.scope.workspace
        }) {
            let mask: RetentionMask = row.decode().map_err(|_| unavailable())?;
            mask.validate().map_err(|_| unavailable())?;
            if mask.workspace != turn.scope.workspace || mask.deletion > self.workspace.deletion {
                return Err(unavailable());
            }
            if mask.artifacts.contains(&turn.trigger) {
                return Err(unavailable());
            }
            if mask.session == turn.scope.session {
                masks.push((mask.first, mask.last));
            }
        }
        Ok(Accumulator {
            current: self,
            through: boundary.watermark,
            masks,
            historical: None,
        })
    }
}

struct Accumulator {
    current: Current,
    through: Watermark,
    masks: Vec<(SessionSeq, SessionSeq)>,
    historical: Option<Task>,
}
impl Accumulator {
    fn observe(&mut self, event: &EventEnvelope) -> Result<()> {
        let unavailable = || Error::Unavailable;
        let turn = &self.current.turn;
        if event.watermark > self.through
            || event.event.workspace != turn.scope.workspace
            || event.event.session != turn.scope.session
            || event.event.task.as_ref() != Some(&turn.scope.task)
        {
            return Ok(());
        }
        if self
            .masks
            .iter()
            .any(|(first, last)| *first <= event.sequence && event.sequence <= *last)
            || event.redaction.is_some()
        {
            return Err(unavailable());
        }
        if !matches!(
            event.event.kind,
            EventKind::TaskCreated
                | EventKind::TaskTransition
                | EventKind::ObjectiveChanged
                | EventKind::FingerprintObserved
        ) {
            return Ok(());
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
            self.historical = Some(task);
        }
        Ok(())
    }
    fn finish(self) -> Result<Source> {
        let historical_task = self.historical.ok_or(Error::Unavailable)?;
        if historical_task.steering != self.current.turn.steering
            || !historical_task
                .objectives
                .iter()
                .any(|objective| objective.steering == self.current.turn.steering)
        {
            return Err(Error::Unavailable);
        }
        Ok(Source {
            session: self.current.session,
            turn: self.current.turn,
            historical_task,
            through_watermark: self.through,
        })
    }
}

#[cfg(test)]
pub(crate) fn source(
    state: &State,
    workspace: &WorkspaceId,
    session: &SessionId,
    through: &TurnId,
) -> Result<Source> {
    let current = Current::new(state.into(), workspace, session, through)?;
    let boundary = state
        .events
        .iter()
        .find(|row| row.event.id == current.turn.cause)
        .ok_or(Error::Unavailable)?;
    let mut proof = current.boundary(state.into(), boundary)?;
    for row in &state.events {
        proof.observe(row)?;
    }
    proof.finish()
}

pub(crate) async fn source_store<S: CanonicalStore>(
    store: &S,
    workspace: &WorkspaceId,
    session: &SessionId,
    through: &TurnId,
) -> Result<Source> {
    let watermark = store.current().watermark;
    let current = Current::new(store.current(), workspace, session, through)?;
    let boundary = store
        .history_event(&current.turn.cause)
        .await
        .map_err(|_| Error::Unavailable)?
        .ok_or(Error::Unavailable)?;
    if boundary.watermark > watermark {
        return Err(Error::Unavailable);
    }
    let mut proof = current.boundary(store.current(), &boundary)?;
    crate::public::visit_history(store, |row| {
        proof
            .observe(row)
            .map_err(|_| crate::public::PublicError::Unavailable)
    })
    .await
    .map_err(|_| Error::Unavailable)?;
    if store.current().watermark != watermark {
        return Err(Error::Unavailable);
    }
    proof.finish()
}
