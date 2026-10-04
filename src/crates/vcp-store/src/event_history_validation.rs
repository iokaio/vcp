// SPDX-License-Identifier: Apache-2.0
//! Full historical validation fed by bounded, fallible row reads. No event
//! payload is retained here, and no previously validated prefix is skipped.
//! Identity metadata still grows with history; this is not bounded reopen.
use super::{key, Collection, Record};
use crate::{Error, Result};
use std::{
    borrow::Borrow,
    collections::{BTreeMap, BTreeSet},
};
use vcp_domain::{
    artifact::ArtifactDescriptor, task::Task, EventId, SessionId, SessionSeq, Watermark,
    WorkspaceId,
};
use vcp_protocol::event::EventEnvelope;

pub(crate) struct EventHistoryValidator<'a> {
    watermark: Watermark,
    records: &'a BTreeMap<String, Record>,
    expected_sequences: &'a BTreeMap<SessionId, SessionSeq>,
    event_ids: BTreeSet<EventId>,
    sequences: BTreeMap<SessionId, SessionSeq>,
    previous_watermark: Watermark,
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
            let task: Task = self
                .record(Collection::Task, task.as_str(), &event.event.workspace)?
                .decode()?;
            if task.scope.session != event.event.session {
                return Err(Error::Access);
            }
        }
        for artifact in &event.event.artifacts {
            let artifact: ArtifactDescriptor = self
                .record(
                    Collection::Artifact,
                    artifact.as_str(),
                    &event.event.workspace,
                )?
                .decode()?;
            if artifact.spec.scope.session != event.event.session
                || event
                    .event
                    .task
                    .as_ref()
                    .is_some_and(|t| t != &artifact.spec.scope.task)
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

    fn record(&self, collection: Collection, id: &str, workspace: &WorkspaceId) -> Result<&Record> {
        let record = self
            .records
            .get(&key(collection, id))
            .ok_or(Error::Conflict("record not found"))?;
        if &record.workspace != workspace {
            return Err(Error::Access);
        }
        Ok(record)
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
}

#[cfg(test)]
#[path = "event_history_validation_tests.rs"]
mod tests;
