// SPDX-License-Identifier: Apache-2.0
//! Canonical chronology proof shared by archived and bounded live history readers.
use super::PublicError;
use std::collections::BTreeMap;
use vcp_domain::{
    ids::*,
    revision::*,
    task::{Turn, TurnState},
    workspace::Scope,
};
use vcp_protocol::event::EventEnvelope;
use vcp_store::{
    contract::{CanonicalStore, Collection, State},
    CurrentStateView,
};

pub fn current_public_turn(state: &State, scope: &Scope) -> Result<Option<Turn>, PublicError> {
    let mut proof = Chronology::new(state.into(), scope)?;
    if !proof.turns.is_empty() {
        for event in &state.events {
            proof.observe(event)?;
        }
    }
    proof.finish()
}
pub async fn current_public_turn_store<S: CanonicalStore>(
    store: &S,
    scope: &Scope,
) -> Result<Option<Turn>, PublicError> {
    current_public_turn_checked(store, scope)
        .await
        .map_err(|error| match error {
            HistoryVisitError::Read => PublicError::Unavailable,
            HistoryVisitError::Evidence(error) => error,
        })
}
/// Queries may render absent chronology as unknown, but must not disguise a
/// failed authenticated read as proven absence.
pub(crate) enum HistoryVisitError {
    Read,
    Evidence(PublicError),
}
pub(crate) async fn current_public_turn_checked<S: CanonicalStore>(
    store: &S,
    scope: &Scope,
) -> Result<Option<Turn>, HistoryVisitError> {
    let mut proof = Chronology::new(store.current(), scope).map_err(HistoryVisitError::Evidence)?;
    if !proof.turns.is_empty() {
        try_visit_history(store, |event| proof.observe(event)).await?;
    }
    proof.finish().map_err(HistoryVisitError::Evidence)
}
/// Full proofs must visit every ordinal, including after the first matching row.
/// Short nonempty pages are byte boundaries, never evidence of absence.
pub(crate) async fn visit_history<S: CanonicalStore>(
    store: &S,
    visit: impl FnMut(&EventEnvelope) -> Result<(), PublicError>,
) -> Result<(), PublicError> {
    try_visit_history(store, visit)
        .await
        .map_err(|error| match error {
            HistoryVisitError::Read => PublicError::Unavailable,
            HistoryVisitError::Evidence(error) => error,
        })
}
async fn try_visit_history<S: CanonicalStore>(
    store: &S,
    mut visit: impl FnMut(&EventEnvelope) -> Result<(), PublicError>,
) -> Result<(), HistoryVisitError> {
    let watermark = store.current().watermark;
    let end = store
        .history_event_count()
        .await
        .map_err(|_| HistoryVisitError::Read)?;
    let mut next = 0u64;
    while next < end {
        let limit = (end - next).min(4096) as usize;
        let rows = store
            .history_events(next.checked_sub(1), limit)
            .await
            .map_err(|_| HistoryVisitError::Read)?;
        if rows.is_empty() || rows.len() > limit {
            return Err(HistoryVisitError::Read);
        }
        for row in &rows {
            if row.watermark > watermark {
                return Err(HistoryVisitError::Read);
            }
            visit(row).map_err(HistoryVisitError::Evidence)?;
        }
        next += rows.len() as u64;
    }
    if store.current().watermark != watermark {
        return Err(HistoryVisitError::Read);
    }
    Ok(())
}
pub(crate) struct Chronology<'a> {
    scope: &'a Scope,
    turns: BTreeMap<TurnId, Turn>,
    created: BTreeMap<TurnId, SessionSeq>,
}
impl<'a> Chronology<'a> {
    pub(crate) fn new(state: CurrentStateView<'_>, scope: &'a Scope) -> Result<Self, PublicError> {
        let mut turns = std::collections::BTreeMap::new();
        for row in state
            .records
            .values()
            .filter(|row| row.collection == Collection::Turn && row.workspace == scope.workspace)
        {
            let turn: Turn = row.decode().map_err(|_| PublicError::Unavailable)?;
            if &turn.scope == scope {
                if turn.id.as_str() != row.id
                    || turn.revision != row.revision
                    || turn.redaction.is_some()
                {
                    return Err(PublicError::Unavailable);
                }
                turns.insert(turn.id.clone(), turn);
            }
        }
        Ok(Self {
            scope,
            turns,
            created: BTreeMap::new(),
        })
    }
    pub(crate) fn observe(&mut self, envelope: &EventEnvelope) -> Result<(), PublicError> {
        let scope = self.scope;
        let created = &mut self.created;
        let event = &envelope.event;
        if event.workspace != scope.workspace
            || event.session != scope.session
            || event.task.as_ref() != Some(&scope.task)
            || event.kind != vcp_protocol::event::EventKind::TurnTransition
            || envelope.redaction.is_some()
        {
            return Ok(());
        }
        if event
            .data
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            != Some(1)
        {
            return Err(PublicError::Unavailable);
        }
        let facts = event
            .data
            .get("facts")
            .and_then(serde_json::Value::as_array)
            .ok_or(PublicError::Unavailable)?;
        for fact in facts {
            if fact.get("collection").and_then(serde_json::Value::as_str) != Some("turn") {
                continue;
            }
            let revision: Revision = serde_json::from_value(
                fact.get("revision")
                    .cloned()
                    .ok_or(PublicError::Unavailable)?,
            )
            .map_err(|_| PublicError::Unavailable)?;
            if revision != Revision::ZERO {
                continue;
            }
            let original: Turn =
                serde_json::from_value(fact.get("value").cloned().ok_or(PublicError::Unavailable)?)
                    .map_err(|_| PublicError::Unavailable)?;
            if &original.scope != scope
                || original.revision != Revision::ZERO
                || original.state != TurnState::Queued
                || original.cause != event.id
                || fact.get("id").and_then(serde_json::Value::as_str) != Some(original.id.as_str())
            {
                return Err(PublicError::Unavailable);
            }
            if created.insert(original.id, envelope.sequence).is_some() {
                return Err(PublicError::Unavailable);
            }
        }
        Ok(())
    }
    pub(crate) fn finish(self) -> Result<Option<Turn>, PublicError> {
        let Self {
            mut turns, created, ..
        } = self;
        if turns.is_empty() {
            return Ok(None);
        }

        if turns.keys().any(|id| !created.contains_key(id)) {
            return Err(PublicError::Unavailable);
        }
        let newest = created
            .iter()
            .max_by_key(|(_, sequence)| **sequence)
            .map(|(id, _)| id)
            .ok_or(PublicError::Unavailable)?;
        turns
            .remove(newest)
            .map(Some)
            .ok_or(PublicError::Unavailable)
    }
}
