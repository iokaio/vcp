// SPDX-License-Identifier: Apache-2.0
//! Payload-free evidence for predicates that require historical event identity.
//! Fallible lookup never confuses unread/unavailable evidence with absence.
use crate::{contract::State, Result};
use vcp_domain::{EventId, SessionId, TaskId, WorkspaceId};
use vcp_protocol::event::{EventEnvelope, EventKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EventFact {
    pub(crate) id: EventId,
    pub(crate) workspace: WorkspaceId,
    pub(crate) session: SessionId,
    pub(crate) task: Option<TaskId>,
    pub(crate) kind: EventKind,
}
impl From<&EventEnvelope> for EventFact {
    fn from(event: &EventEnvelope) -> Self {
        Self {
            id: event.event.id.clone(),
            workspace: event.event.workspace.clone(),
            session: event.event.session.clone(),
            task: event.event.task.clone(),
            kind: event.event.kind.clone(),
        }
    }
}

/// Both operations are intentional: the complete reference's forecast map
/// selects the last duplicate, while send-intent and origin predicates accept
/// any exact match. Valid canonical history has unique identities; retaining
/// these distinctions also preserves standalone invalid-input behavior.
pub(crate) trait EventFacts {
    fn last(&mut self, id: &EventId) -> Result<Option<EventFact>>;
    fn any(&mut self, id: &EventId, predicate: &dyn Fn(&EventFact) -> bool) -> Result<bool>;
}

pub(crate) struct StateEventFacts<'a> {
    state: &'a State,
}
impl<'a> StateEventFacts<'a> {
    pub(crate) fn new(state: &'a State) -> Self {
        Self { state }
    }
}
impl EventFacts for StateEventFacts<'_> {
    fn last(&mut self, id: &EventId) -> Result<Option<EventFact>> {
        Ok(self
            .state
            .events
            .iter()
            .rev()
            .find(|event| &event.event.id == id)
            .map(EventFact::from))
    }
    fn any(&mut self, id: &EventId, predicate: &dyn Fn(&EventFact) -> bool) -> Result<bool> {
        Ok(self
            .state
            .events
            .iter()
            .filter(|event| &event.event.id == id)
            .any(|event| predicate(&EventFact::from(event))))
    }
}
