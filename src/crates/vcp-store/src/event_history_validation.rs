// SPDX-License-Identifier: Apache-2.0
//! Event predicates fed by bounded, fallible row reads. Full passes retain
//! identity metadata for their history; the incremental adapter seeds admitted
//! frontiers and checks only new rows when prefix dependencies are unchanged.
use super::{key, Collection, Record};
use crate::{Error, Result};
use std::{
    borrow::Borrow,
    collections::{BTreeMap, BTreeSet},
};
use vcp_domain::{
    artifact::ArtifactDescriptor, task::Task, EventId, SessionId, SessionSeq, TaskId, Watermark,
    WorkspaceId,
};
use vcp_protocol::event::EventEnvelope;

// These facts are decoded from one immutable current record map. Reaching the
// cap falls back to the original decode; it never changes acceptance.
const MAX_SCOPE_FACTS: usize = 512;

#[cfg(test)]
#[path = "event_replay_proof.rs"]
pub(crate) mod replay_proof;

#[path = "event_history_incremental.rs"]
pub(crate) mod incremental;

pub(crate) struct EventHistoryValidator<'a> {
    watermark: Watermark,
    records: &'a BTreeMap<String, Record>,
    expected_sequences: &'a BTreeMap<SessionId, SessionSeq>,
    event_ids: BTreeSet<EventId>,
    sequences: BTreeMap<SessionId, SessionSeq>,
    previous_watermark: Watermark,
    examined: u64,
    task_sessions: BTreeMap<&'a str, SessionId>,
    artifact_scopes: BTreeMap<&'a str, (SessionId, TaskId)>,
    #[cfg(test)]
    task_decodes: usize,
    #[cfg(test)]
    artifact_decodes: usize,
    failed: bool,
}
impl<'a> EventHistoryValidator<'a> {
    pub(crate) fn new(
        watermark: Watermark,
        records: &'a BTreeMap<String, Record>,
        expected_sequences: &'a BTreeMap<SessionId, SessionSeq>,
    ) -> Self {
        Self {
            watermark,
            records,
            expected_sequences,
            event_ids: BTreeSet::new(),
            sequences: BTreeMap::new(),
            previous_watermark: Watermark::ZERO,
            examined: 0,
            task_sessions: BTreeMap::new(),
            artifact_scopes: BTreeMap::new(),
            #[cfg(test)]
            task_decodes: 0,
            #[cfg(test)]
            artifact_decodes: 0,
            failed: false,
        }
    }

    /// Accept owned rows or references into a page that can be dropped after
    /// this call. A reader or semantic error permanently invalidates this run.
    pub(crate) fn extend<E: Borrow<EventEnvelope>>(
        &mut self,
        rows: impl IntoIterator<Item = Result<E>>,
    ) -> Result<()> {
        if self.failed {
            return Err(Error::Corruption("event history validation failed"));
        }
        for row in rows {
            self.failed = true;
            self.event(row?.borrow())?;
            self.failed = false;
        }
        Ok(())
    }

    fn event(&mut self, event: &EventEnvelope) -> Result<()> {
        self.examined = self.examined.saturating_add(1);
        if event.version != 1
            || event.watermark > self.watermark
            || event.watermark < self.previous_watermark
            || !self.event_ids.insert(event.event.id.clone())
        {
            return Err(Error::Corruption("event identity"));
        }
        self.previous_watermark = event.watermark;
        self.record(
            Collection::Session,
            event.event.session.as_str(),
            &event.event.workspace,
        )?;
        if let Some(task) = &event.event.task {
            // Keep lookup and workspace validation before every memo hit. The
            // key is the actual map key, not an unverified field in the record.
            let (key, record) =
                self.record(Collection::Task, task.as_str(), &event.event.workspace)?;
            let session = match self.task_sessions.get(key) {
                Some(session) => session.clone(),
                None => {
                    #[cfg(test)]
                    {
                        self.task_decodes += 1;
                    }
                    let task: Task = record.decode()?;
                    let session = task.scope.session;
                    if self.task_sessions.len() < MAX_SCOPE_FACTS {
                        self.task_sessions.insert(key, session.clone());
                    }
                    session
                }
            };
            if session != event.event.session {
                return Err(Error::Access);
            }
        }
        for artifact in &event.event.artifacts {
            let (key, record) = self.record(
                Collection::Artifact,
                artifact.as_str(),
                &event.event.workspace,
            )?;
            let (session, task) = match self.artifact_scopes.get(key) {
                Some(scope) => scope.clone(),
                None => {
                    #[cfg(test)]
                    {
                        self.artifact_decodes += 1;
                    }
                    let artifact: ArtifactDescriptor = record.decode()?;
                    let scope = (artifact.spec.scope.session, artifact.spec.scope.task);
                    if self.artifact_scopes.len() < MAX_SCOPE_FACTS {
                        self.artifact_scopes.insert(key, scope.clone());
                    }
                    scope
                }
            };
            if session != event.event.session
                || event.event.task.as_ref().is_some_and(|t| t != &task)
            {
                return Err(Error::Access);
            }
        }
        let expected = self
            .sequences
            .get(&event.event.session)
            .copied()
            .unwrap_or_default()
            .next()?;
        if event.sequence != expected {
            return Err(Error::Corruption("event sequence gap or duplicate"));
        }
        self.sequences.insert(event.event.session.clone(), expected);
        Ok(())
    }

    fn record(
        &self,
        collection: Collection,
        id: &str,
        workspace: &WorkspaceId,
    ) -> Result<(&'a str, &'a Record)> {
        let (key, record) = self
            .records
            .get_key_value(&key(collection, id))
            .ok_or(Error::Conflict("record not found"))?;
        if &record.workspace != workspace {
            return Err(Error::Access);
        }
        Ok((key.as_str(), record))
    }

    pub(crate) fn finish(self) -> Result<()> {
        if self.failed {
            return Err(Error::Corruption("event history validation failed"));
        }
        if &self.sequences != self.expected_sequences {
            return Err(Error::Corruption("session watermark"));
        }
        Ok(())
    }

    pub(crate) fn examined(&self) -> u64 {
        self.examined
    }
}

#[cfg(test)]
#[path = "event_history_validation_tests.rs"]
mod tests;
