// SPDX-License-Identifier: Apache-2.0
//! Storage/retention shape only; observer scheduling remains in vcp-engine.
use crate::{contract::*, Error, Result};
use serde::Deserialize;
use std::collections::BTreeSet;
use vcp_domain::{workspace::Scope, *};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema_version: u32,
    document_type: String,
    scope: Scope,
    revision: Revision,
    owner: String,
    state: Observer,
    notice: String,
    elapsed_millis: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Observer {
    version: u32,
    root: TaskId,
    enabled: bool,
    cursor: Watermark,
    queued: Option<Input>,
    attempts: Vec<Attempt>,
    proposals: Vec<Proposal>,
    limits: Limits,
    budget: Budget,
    last_started: Option<Timestamp>,
    coalesced: u64,
    duplicates: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    root: TaskId,
    task: TaskId,
    steering: SteeringRevision,
    task_revision: Revision,
    authority: AuthorityRevision,
    deletion: DeletionEpoch,
    input_digest: String,
    pattern_digest: String,
    watermark: Watermark,
    deadline: Timestamp,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Work {
    id: u32,
    key: String,
    input: Input,
    deadline: Timestamp,
    reserved_steps: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Attempt {
    work: Work,
    status: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Proposal {
    attempt: u32,
    key: String,
    input: Input,
    source_evidence: String,
    verifications: Vec<VerificationId>,
    disposition: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Limits {
    max_attempts: u32,
    steps_per_attempt: u32,
    max_total_steps: u64,
    deadline_ms: u64,
    debounce_ms: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Budget {
    attempts: u32,
    reserved_steps: u64,
}
pub(crate) fn kind(row: &Record) -> bool {
    row.value["document_type"] == vcp_domain::redaction::OBSERVER_SOURCE
}
fn input(input: &Input, root: &TaskId) -> Result<()> {
    if &input.root != root
        || !accounting::valid_hash(&input.input_digest)
        || !accounting::valid_hash(&input.pattern_digest)
        || input.deadline == Timestamp::ZERO
    {
        return Err(Error::Corruption("observer input shape"));
    }
    let _ = (
        &input.task,
        input.steering,
        input.task_revision,
        input.authority,
        input.deletion,
        input.watermark,
    );
    Ok(())
}
fn document(row: &Record) -> Result<Document> {
    let d: Document = row.decode()?;
    let s = &d.state;
    let l = &s.limits;
    if row.collection != Collection::Projection
        || d.schema_version != 1
        || d.document_type != redaction::OBSERVER_SOURCE
        || d.scope.workspace != row.workspace
        || d.revision != row.revision
        || s.version != 1
        || s.root != d.scope.task
        || row.id
            != format!(
                "observer-{}",
                vcp_protocol::digest_bytes(s.root.as_str().as_bytes())
            )
        || !accounting::valid_hash(&d.owner)
        || d.notice.len() > 4096
        || l.max_attempts == 0
        || l.max_attempts > 64
        || l.steps_per_attempt == 0
        || l.steps_per_attempt > 4096
        || l.max_total_steps == 0
        || l.max_total_steps > 262144
        || l.deadline_ms == 0
        || l.deadline_ms > 2000
        || l.debounce_ms > 1000
        || s.attempts.len() > l.max_attempts as usize
        || s.proposals.len() > s.attempts.len()
        || s.budget.attempts as usize != s.attempts.len()
        || s.budget.reserved_steps > l.max_total_steps
        || vcp_protocol::canonical_bytes(&row.value)?.len() > 1024 * 1024
    {
        return Err(Error::Corruption("observer source shape"));
    }
    if let Some(q) = &s.queued {
        input(q, &s.root)?;
    }
    let mut keys = BTreeSet::new();
    let mut steps = 0u64;
    let mut running = 0;
    for (index, a) in s.attempts.iter().enumerate() {
        input(&a.work.input, &s.root)?;
        if a.work.id as usize != index + 1
            || !accounting::valid_hash(&a.work.key)
            || !keys.insert(&a.work.key)
            || a.work.reserved_steps != l.steps_per_attempt
            || a.work.deadline > a.work.input.deadline
            || !matches!(
                a.status.as_str(),
                "running" | "completed" | "interrupted" | "expired"
            )
        {
            return Err(Error::Corruption("observer attempt shape"));
        }
        steps += u64::from(a.work.reserved_steps);
        running += usize::from(a.status == "running");
    }
    if steps != s.budget.reserved_steps || running > 1 {
        return Err(Error::Corruption("observer budget shape"));
    }
    for p in &s.proposals {
        input(&p.input, &s.root)?;
        if p.attempt == 0
            || p.attempt as usize > s.attempts.len()
            || !accounting::valid_hash(&p.key)
            || p.source_evidence.is_empty()
            || p.source_evidence.len() > 256
            || p.verifications.len() < 3
            || p.verifications.len() > 4096
            || !matches!(p.disposition.as_str(), "current" | "historical")
        {
            return Err(Error::Corruption("observer proposal shape"));
        }
    }
    let _ = (
        d.elapsed_millis,
        s.enabled,
        s.cursor,
        s.last_started,
        s.coalesced,
        s.duplicates,
    );
    Ok(d)
}
pub(crate) fn shape(row: &Record) -> Result<()> {
    document(row).map(|_| ())
}
pub(crate) fn scope(row: &Record) -> Result<Scope> {
    document(row).map(|d| d.scope)
}
pub(crate) fn references(row: &Record) -> Result<BTreeSet<String>> {
    let d = document(row)?;
    let mut refs = BTreeSet::from([key(Collection::Task, d.scope.task.as_str())]);
    for a in &d.state.attempts {
        refs.insert(key(Collection::Task, a.work.input.task.as_str()));
    }
    for p in &d.state.proposals {
        for id in &p.verifications {
            refs.insert(key(Collection::Verification, id.as_str()));
        }
    }
    Ok(refs)
}
