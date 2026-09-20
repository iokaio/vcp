// SPDX-License-Identifier: Apache-2.0
//! Durable queue invariants independent of the host-side scheduler.
use super::*;
use vcp_domain::{
    ingestion::{Cursor, Job, JobState},
    workspace::Scope,
};

pub(super) fn kind(record: &Record) -> Result<Option<&str>> {
    let Some(kind) = record.value["document_type"].as_str() else {
        return Ok(None);
    };
    match kind {
        "vcp_ingestion_cursor_v1" | "vcp_ingestion_job_v1"
            if record.collection == Collection::Claim =>
        {
            Ok(Some(kind))
        }
        _ if kind.starts_with("vcp_ingestion_") => {
            Err(Error::Corruption("ingestion document type or collection"))
        }
        _ => Ok(None),
    }
}
pub(super) fn scope(record: &Record) -> Result<Scope> {
    match kind(record)? {
        Some("vcp_ingestion_cursor_v1") => Ok(record.decode::<Cursor>()?.scope),
        Some("vcp_ingestion_job_v1") => Ok(record.decode::<Job>()?.scope),
        _ => Err(Error::Corruption("ingestion document expected")),
    }
}
fn cursor_id(cursor: &Cursor) -> Result<String> {
    Ok(digest_bytes(&canonical_bytes(&(
        "ingestion-cursor/1",
        &cursor.scope,
        &cursor.extractor,
    ))?))
}
fn job_id(cursor: &CommandId, event: &EventId) -> Result<String> {
    Ok(digest_bytes(&canonical_bytes(&(
        "ingestion-job/1",
        cursor,
        event,
    ))?))
}
pub(super) fn shape(record: &Record) -> Result<()> {
    let (id, workspace, revision) = match kind(record)? {
        Some("vcp_ingestion_cursor_v1") => {
            let value: Cursor = record.decode()?;
            value.validate()?;
            if value.id.as_str() != cursor_id(&value)? {
                return Err(Error::Corruption("ingestion cursor identity"));
            }
            (value.id, value.scope.workspace, value.revision)
        }
        Some("vcp_ingestion_job_v1") => {
            let value: Job = record.decode()?;
            value.validate()?;
            if value.id.as_str() != job_id(&value.cursor, &value.origin)?
                || (value.state != JobState::Completed
                    && (!value.results.is_empty() || value.finding.is_some()))
                || (value.state == JobState::Pending
                    && (value.attempts != Units::ZERO || value.last_failure.is_some()))
                || (value.state == JobState::Leased && value.attempts == Units::ZERO)
                || (matches!(
                    value.state,
                    JobState::Deferred | JobState::Failed | JobState::Cancelled
                ) && value.last_failure.is_none())
            {
                return Err(Error::Corruption("ingestion job identity or state"));
            }
            (value.id, value.scope.workspace, value.revision)
        }
        _ => return Err(Error::Corruption("ingestion document expected")),
    };
    if id.as_str() != record.id || workspace != record.workspace || revision != record.revision {
        return Err(Error::Corruption("ingestion record identity or revision"));
    }
    Ok(())
}
pub(super) fn references(record: &Record) -> Result<BTreeSet<String>> {
    let mut refs = BTreeSet::from([key(Collection::Task, scope(record)?.task.as_str())]);
    if kind(record)? == Some("vcp_ingestion_job_v1") {
        let job: Job = record.decode()?;
        refs.insert(key(Collection::Claim, job.cursor.as_str()));
        refs.insert(key(Collection::Task, job.root.as_str()));
        refs.extend(
            job.results
                .iter()
                .map(|id| key(Collection::Projection, id.as_str())),
        );
    }
    Ok(refs)
}
pub(super) fn insert(record: &Record) -> Result<()> {
    if kind(record)? == Some("vcp_ingestion_job_v1") {
        let job: Job = record.decode()?;
        if job.state != JobState::Pending || job.attempts != Units::ZERO {
            return Err(Error::Conflict("ingestion job must start pending"));
        }
    }
    Ok(())
}
pub(super) fn transition(previous: &Record, record: &Record) -> Result<()> {
    let previous_kind = kind(previous)?;
    if previous_kind != kind(record)? {
        return Err(Error::Conflict("ingestion document type changed"));
    }
    match previous_kind {
        Some("vcp_ingestion_cursor_v1") => {
            let mut before: Cursor = previous.decode()?;
            let after: Cursor = record.decode()?;
            if after.after < before.after || after.scanned_through < before.scanned_through {
                return Err(Error::Conflict("ingestion cursor regressed"));
            }
            before.revision = after.revision;
            before.after = after.after;
            before.scanned_through = after.scanned_through;
            if before != after {
                return Err(Error::Conflict("ingestion stream identity changed"));
            }
        }
        Some("vcp_ingestion_job_v1") => {
            let mut before: Job = previous.decode()?;
            let after: Job = record.decode()?;
            if before.state.finished() {
                return Err(Error::Conflict("finished ingestion job is immutable"));
            }
            let allowed = match after.state {
                JobState::Leased => {
                    matches!(
                        before.state,
                        JobState::Pending | JobState::Deferred | JobState::Leased
                    ) && after.attempts == before.attempts.next()?
                        && after.lease.as_ref().is_some_and(|next| {
                            before.lease.as_ref().is_none_or(|old| {
                                old.token != next.token && next.expires_at > old.expires_at
                            })
                        })
                }
                JobState::Completed | JobState::Deferred => {
                    before.state == JobState::Leased && after.attempts == before.attempts
                }
                JobState::Failed => {
                    after.attempts == before.attempts
                        && (before.state == JobState::Leased
                            || before.attempts == before.max_attempts)
                }
                JobState::Cancelled => after.attempts == before.attempts,
                JobState::Pending => false,
            };
            if !allowed {
                return Err(Error::Conflict("ingestion job transition"));
            }
            before.revision = after.revision;
            before.state = after.state;
            before.attempts = after.attempts;
            before.lease = after.lease.clone();
            before.not_before = after.not_before;
            before.last_failure = after.last_failure.clone();
            before.results = after.results.clone();
            before.finding = after.finding.clone();
            if before != after {
                return Err(Error::Conflict(
                    "ingestion job origin or specification changed",
                ));
            }
        }
        _ => (),
    }
    Ok(())
}

pub(super) fn validate(state: &State) -> Result<()> {
    let mut cursors = BTreeMap::<CommandId, Cursor>::new();
    let mut jobs = BTreeMap::<CommandId, Job>::new();
    for record in state.records.values() {
        match kind(record)? {
            Some("vcp_ingestion_cursor_v1") => {
                let cursor: Cursor = record.decode()?;
                cursors.insert(cursor.id.clone(), cursor);
            }
            Some("vcp_ingestion_job_v1") => {
                let job: Job = record.decode()?;
                jobs.insert(job.id.clone(), job);
            }
            _ => (),
        }
    }
    for cursor in cursors.values() {
        let root: Task = state
            .record(
                Collection::Task,
                cursor.scope.task.as_str(),
                &cursor.scope.workspace,
            )?
            .decode()?;
        if root.scope != cursor.scope || root.parent.is_some() || root.root != cursor.scope.task {
            return Err(Error::Corruption("ingestion root scope"));
        }
        let after = usize::try_from(cursor.after.get())
            .map_err(|_| Error::Corruption("ingestion cursor offset"))?;
        if after > state.events.len()
            || cursor.scanned_through > state.watermark
            || (after == 0 && cursor.scanned_through != Watermark::ZERO)
            || (after > 0 && state.events[after - 1].watermark != cursor.scanned_through)
        {
            return Err(Error::Corruption("ingestion cursor progress"));
        }
        for event in state.events.iter().take(after) {
            if event.event.workspace != cursor.scope.workspace
                || event.event.session != cursor.scope.session
            {
                continue;
            }
            let Some(task_id) = &event.event.task else {
                continue;
            };
            let task: Task = state
                .record(Collection::Task, task_id.as_str(), &cursor.scope.workspace)?
                .decode()?;
            let kind = serde_json::to_value(&event.event.kind)?;
            if task.root == cursor.scope.task
                && kind.as_str().is_some_and(|kind| {
                    cursor.extractor.event_kinds.iter().any(|item| item == kind)
                })
            {
                let id = CommandId::parse(job_id(&cursor.id, &event.event.id)?)?;
                if !jobs.contains_key(&id) {
                    return Err(Error::Corruption(
                        "ingestion cursor advanced without durable job",
                    ));
                }
            }
        }
    }
    let events: BTreeMap<_, _> = state
        .events
        .iter()
        .enumerate()
        .map(|(i, e)| (&e.event.id, (i, e)))
        .collect();
    for job in jobs.values() {
        let cursor = cursors
            .get(&job.cursor)
            .ok_or(Error::Corruption("ingestion job cursor"))?;
        let task: Task = state
            .record(
                Collection::Task,
                job.scope.task.as_str(),
                &job.scope.workspace,
            )?
            .decode()?;
        let (offset, event) = events
            .get(&job.origin)
            .ok_or(Error::Corruption("ingestion origin missing"))?;
        let kind = serde_json::to_value(&event.event.kind)?;
        if job.scope != task.scope
            || task.root != job.root
            || job.root != cursor.scope.task
            || job.scope.workspace != cursor.scope.workspace
            || job.scope.session != cursor.scope.session
            || job.extractor != cursor.extractor
            || event.watermark != job.origin_watermark
            || event.event.workspace != job.scope.workspace
            || event.event.session != job.scope.session
            || event.event.task.as_ref() != Some(&job.scope.task)
            || *offset as u64 >= cursor.after.get()
            || !kind
                .as_str()
                .is_some_and(|kind| job.extractor.event_kinds.iter().any(|item| item == kind))
        {
            return Err(Error::Corruption("ingestion job origin scope or stream"));
        }
        for id in &job.results {
            let row = state.record(Collection::Projection, id.as_str(), &job.scope.workspace)?;
            let (scope, proposal_id) =
                if row.value["document_type"] == vcp_domain::redaction::RESULT {
                    let result: vcp_domain::redaction::RedactedResult = row.decode()?;
                    result.validate()?;
                    (result.scope, result.proposal)
                } else {
                    let result: vcp_domain::memory::ProposalResult = row.decode()?;
                    result.validate()?;
                    (result.scope, result.proposal)
                };
            let row = state.record(
                Collection::Claim,
                proposal_id.as_str(),
                &job.scope.workspace,
            )?;
            let associated = if row.value["document_type"] == vcp_domain::redaction::PROPOSAL {
                let proposal: vcp_domain::redaction::RedactedProposal = row.decode()?;
                proposal.validate()?;
                proposal.sources.origins.contains(&job.origin)
                    && proposal.extractor_digest
                        == vcp_protocol::digest_bytes(job.extractor.identity().as_bytes())
            } else {
                let proposal: vcp_domain::memory::ProposalRecord = row.decode()?;
                proposal.proposal.origins.contains(&job.origin)
                    && proposal.proposal.extractor == job.extractor.identity()
            };
            if scope != job.scope || !associated {
                return Err(Error::Corruption("ingestion completion provenance"));
            }
        }
    }
    Ok(())
}
