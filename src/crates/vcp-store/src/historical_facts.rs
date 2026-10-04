// SPDX-License-Identifier: Apache-2.0
//! Payload-free evidence for predicates that require historical event identity.
//! Fallible lookup never confuses unread/unavailable evidence with absence.
use crate::{
    contract::{RecordView, State},
    Result,
};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{EventId, SessionId, TaskId, TransactionId, Watermark, WorkspaceId};
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
    /// Optional non-fallible prefetch hint. Implementations must preserve the
    /// result/error order of subsequent lookups; fallible readers may ignore it.
    fn prepare_last(&mut self, _ids: &BTreeSet<EventId>) {}
    fn last(&mut self, id: &EventId) -> Result<Option<EventFact>>;
    fn any(&mut self, id: &EventId, predicate: &dyn Fn(&EventFact) -> bool) -> Result<bool>;
}

pub(crate) struct StateEventFacts<'a> {
    state: &'a State,
    prepared: BTreeMap<EventId, Option<EventFact>>,
    #[cfg(test)]
    scanned: usize,
}
impl<'a> StateEventFacts<'a> {
    pub(crate) fn new(state: &'a State) -> Self {
        Self {
            state,
            prepared: BTreeMap::new(),
            #[cfg(test)]
            scanned: 0,
        }
    }
}
impl EventFacts for StateEventFacts<'_> {
    fn prepare_last(&mut self, ids: &BTreeSet<EventId>) {
        // Replace the cache for each current forecast. Explicit None entries
        // retain absence without rescanning; no unrelated identity is retained.
        self.prepared = ids.iter().cloned().map(|id| (id, None)).collect();
        if ids.is_empty() {
            return;
        }
        for event in self.state.events.iter() {
            #[cfg(test)]
            {
                self.scanned += 1;
            }
            if let Some(value) = self.prepared.get_mut(&event.event.id) {
                *value = Some(EventFact::from(event));
            }
        }
    }
    fn last(&mut self, id: &EventId) -> Result<Option<EventFact>> {
        if let Some(value) = self.prepared.get(id) {
            return Ok(value.clone());
        }
        for event in self.state.events.iter().rev() {
            #[cfg(test)]
            {
                self.scanned += 1;
            }
            if &event.event.id == id {
                return Ok(Some(EventFact::from(event)));
            }
        }
        Ok(None)
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

/// Receipt watermark needed by current search coverage and receipt-existence
/// checks. A durable implementation must authenticate the exact receipt key;
/// unavailable history must return an error, never an absent receipt.
pub(crate) trait TransactionFacts {
    fn watermark(&mut self, id: &TransactionId) -> Result<Option<Watermark>>;
}

pub(crate) struct StateTransactionFacts<'a> {
    state: &'a State,
}
impl<'a> StateTransactionFacts<'a> {
    pub(crate) fn new(state: &'a State) -> Self {
        Self { state }
    }
}
impl TransactionFacts for StateTransactionFacts<'_> {
    fn watermark(&mut self, id: &TransactionId) -> Result<Option<Watermark>> {
        Ok(self
            .state
            .transactions
            .get(id)
            .map(|receipt| receipt.watermark))
    }
}

pub(crate) struct RecordTransactionFacts<'a> {
    state: RecordView<'a>,
}
impl<'a> RecordTransactionFacts<'a> {
    pub(crate) fn new(state: RecordView<'a>) -> Self {
        Self { state }
    }
}
impl TransactionFacts for RecordTransactionFacts<'_> {
    fn watermark(&mut self, id: &TransactionId) -> Result<Option<Watermark>> {
        // Preserve the preparation view's exclusion of the transaction being
        // admitted; its new receipt is not evidence in the prior snapshot.
        Ok(self.state.transaction(id).map(|receipt| receipt.watermark))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vcp_protocol::event::EventInput;

    #[test]
    fn forecast_prefetch_retains_only_requested_facts_and_caches_absence() {
        let mut state = State::default();
        for index in 0..1000 {
            state.events.push(EventEnvelope {
                version: 1,
                redaction: None,
                watermark: Watermark::new(index + 1),
                sequence: vcp_domain::SessionSeq::new(index + 1),
                event: EventInput {
                    id: EventId::parse(format!("event-{index}")).unwrap(),
                    workspace: WorkspaceId::parse("workspace").unwrap(),
                    session: SessionId::parse("session").unwrap(),
                    task: Some(TaskId::parse("task").unwrap()),
                    actor: vcp_domain::ActorId::parse("human").unwrap(),
                    correlation: vcp_domain::CommandId::parse("command").unwrap(),
                    causation: None,
                    timestamp: vcp_domain::Timestamp::new(index),
                    kind: EventKind::TaskCreated,
                    artifacts: vec![],
                    metadata: None,
                    data: serde_json::json!({"payload":"x".repeat(1024)}),
                },
            });
        }
        let selected = EventId::parse("event-1").unwrap();
        let missing = EventId::parse("missing").unwrap();
        let mut duplicate = state.events[1].clone();
        duplicate.event.task = Some(TaskId::parse("last-task").unwrap());
        state.events.push(duplicate.clone());
        let mut facts = StateEventFacts::new(&state);
        facts.prepare_last(&BTreeSet::from([selected.clone(), missing.clone()]));
        assert_eq!(facts.scanned, state.events.len());
        assert_eq!(facts.prepared.len(), 2);
        for _ in 0..50 {
            assert_eq!(
                facts.last(&selected).unwrap(),
                Some(EventFact::from(&duplicate))
            );
            assert_eq!(facts.last(&missing).unwrap(), None);
        }
        assert_eq!(
            facts.scanned,
            state.events.len(),
            "found and absent lookups never rescan"
        );
        assert!(
            facts
                .any(&selected, &|event| event
                    .task
                    .as_ref()
                    .is_some_and(|task| task.as_str() == "task"))
                .unwrap(),
            "any retains the earlier matching duplicate"
        );
        let next = EventId::parse("event-999").unwrap();
        facts.prepare_last(&BTreeSet::from([next.clone()]));
        assert_eq!(facts.prepared.len(), 1, "a new forecast replaces the cache");
        assert!(!facts.prepared.contains_key(&selected));
        assert_eq!(
            facts.last(&next).unwrap(),
            Some(EventFact::from(&state.events[999]))
        );
        assert_eq!(facts.scanned, 2 * state.events.len());
        facts.prepare_last(&BTreeSet::new());
        assert!(facts.prepared.is_empty());
        assert_eq!(
            facts.scanned,
            2 * state.events.len(),
            "empty selection needs no scan"
        );
    }
}
