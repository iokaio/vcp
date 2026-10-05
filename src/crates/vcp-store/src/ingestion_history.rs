// SPDX-License-Identifier: Apache-2.0
//! One full event pass retaining only current cursor/job validation metadata.
//! Event payloads and unrelated historical identities never enter this index.
use super::{job_id, Cursor, Job};
use crate::{
    contract::{key, Collection, Record},
    Error, Result,
};
use std::{
    borrow::Borrow,
    collections::{BTreeMap, BTreeSet},
};
use vcp_domain::{task::Task, CommandId, EventId, SessionId, TaskId, Watermark, WorkspaceId};
use vcp_protocol::event::EventEnvelope;

pub(super) struct Origin {
    pub offset: usize,
    pub watermark: Watermark,
    pub workspace: WorkspaceId,
    pub session: SessionId,
    pub task: Option<TaskId>,
    pub kind: serde_json::Value,
}
#[derive(Default)]
pub(super) struct CursorProgress {
    pub boundary: Option<Watermark>,
    pub first_error: Option<Error>,
}
pub(crate) struct History {
    pub(super) count: usize,
    pub(super) origins: BTreeMap<EventId, Origin>,
    pub(super) cursors: BTreeMap<CommandId, CursorProgress>,
}
pub(crate) struct IngestionHistory<'a> {
    records: &'a BTreeMap<String, Record>,
    active_cursors: Vec<&'a Cursor>,
    jobs: &'a BTreeMap<CommandId, Job>,
    required_origins: BTreeSet<EventId>,
    expected_count: usize,
    history: History,
    failed: bool,
}
impl<'a> IngestionHistory<'a> {
    pub(super) fn new(
        records: &'a BTreeMap<String, Record>,
        cursors: &'a BTreeMap<CommandId, Cursor>,
        jobs: &'a BTreeMap<CommandId, Job>,
        expected_count: usize,
    ) -> Self {
        Self {
            records,
            active_cursors: cursors
                .values()
                .filter(|cursor| cursor.after.get() != 0)
                .collect(),
            jobs,
            expected_count,
            required_origins: jobs.values().map(|job| job.origin.clone()).collect(),
            history: History {
                count: 0,
                origins: BTreeMap::new(),
                cursors: cursors
                    .keys()
                    .map(|id| (id.clone(), CursorProgress::default()))
                    .collect(),
            },
            failed: false,
        }
    }

    pub(crate) fn extend<E: Borrow<EventEnvelope>>(
        &mut self,
        rows: impl IntoIterator<Item = Result<E>>,
    ) -> Result<()> {
        if self.failed {
            return Err(Error::Corruption("ingestion history validation failed"));
        }
        for row in rows {
            self.failed = true;
            let row = row?;
            let event = row.borrow();
            let offset = self.history.count;
            let next = offset
                .checked_add(1)
                .ok_or(Error::Corruption("ingestion cursor offset"))?;
            for cursor in &self.active_cursors {
                let progress = self
                    .history
                    .cursors
                    .get_mut(&cursor.id)
                    .ok_or(Error::Corruption("ingestion history cursor"))?;
                if cursor.after.get() == next as u64 {
                    progress.boundary = Some(event.watermark);
                }
                if offset as u64 >= cursor.after.get() || progress.first_error.is_some() {
                    continue;
                }
                // Save each cursor's first semantic error instead of returning
                // it now: canonical validation checks cursors in key order,
                // including root and boundary checks before their prefix scan.
                progress.first_error = prefix_event(self.records, self.jobs, cursor, event).err();
            }
            // A cursor only needs its prefix. Later unrelated history must not
            // add repeated work for cursors that already reached their cut.
            self.active_cursors
                .retain(|cursor| cursor.after.get() > next as u64);
            if self.required_origins.contains(&event.event.id) {
                self.history.origins.insert(
                    event.event.id.clone(),
                    Origin {
                        offset,
                        watermark: event.watermark,
                        workspace: event.event.workspace.clone(),
                        session: event.event.session.clone(),
                        task: event.event.task.clone(),
                        kind: serde_json::to_value(&event.event.kind)?,
                    },
                );
            }
            self.history.count = next;
            self.failed = false;
        }
        Ok(())
    }

    pub(crate) fn finish(self) -> Result<History> {
        if self.failed || self.history.count != self.expected_count {
            return Err(Error::Corruption("ingestion history validation failed"));
        }
        Ok(self.history)
    }
}

fn prefix_event(
    records: &BTreeMap<String, Record>,
    jobs: &BTreeMap<CommandId, Job>,
    cursor: &Cursor,
    event: &EventEnvelope,
) -> Result<()> {
    if event.event.workspace != cursor.scope.workspace
        || event.event.session != cursor.scope.session
    {
        return Ok(());
    }
    let Some(task_id) = &event.event.task else {
        return Ok(());
    };
    let record = records
        .get(&key(Collection::Task, task_id.as_str()))
        .ok_or(Error::Conflict("record not found"))?;
    if record.workspace != cursor.scope.workspace {
        return Err(Error::Access);
    }
    let task: Task = record.decode()?;
    let kind = serde_json::to_value(&event.event.kind)?;
    if task.root == cursor.scope.task
        && kind
            .as_str()
            .is_some_and(|kind| cursor.extractor.event_kinds.iter().any(|item| item == kind))
    {
        let id = CommandId::parse(job_id(&cursor.id, &event.event.id)?)?;
        if !jobs.contains_key(&id) {
            return Err(Error::Corruption(
                "ingestion cursor advanced without durable job",
            ));
        }
    }
    Ok(())
}
