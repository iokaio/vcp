// SPDX-License-Identifier: Apache-2.0
//! A bounded maintenance step, called only by an admitted canonical owner.
use crate::{access::Access, extractors, ingest, repository, Error, Result};
use vcp_domain::{ingestion::*, memory::ProposalRecord, workspace::Scope, *};
use vcp_store::{contract::Collection, Store};

pub fn specification() -> ExtractorSpec {
    ExtractorSpec {
        name: "deterministic".into(),
        version: 1,
        event_kinds: [
            "task_created",
            "objective_changed",
            "artifact_attached",
            "verification_recorded",
            "fingerprint_observed",
            "task_transition",
            "turn_transition",
            "attempt_submitted",
            "usage_reconciled",
            "effect_transition",
            "commentary",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
    }
}

pub fn limits() -> ingest::Limits {
    ingest::Limits {
        scanned_events: 32,
        new_jobs: 16,
        batch_bytes: 256 * 1024,
        pending_jobs: 256,
        concurrency: 1,
        attempts: 3,
        lease_ms: 30_000,
    }
}

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub completed: usize,
    pub deferred: usize,
    pub failed: usize,
    /// The cursor covers the event stream after processing this step. Jobs may
    /// still be pending; callers must also inspect the queue before declaring idle.
    pub caught_up: bool,
    pub quota_reached: bool,
}

#[derive(serde::Serialize)]
pub struct Status {
    pub watermark: Watermark,
    pub pending: usize,
    pub leased: usize,
    pub deferred: usize,
    pub failed: usize,
    pub completed: usize,
    pub cancelled: usize,
    pub oldest_pending_watermark: Option<Watermark>,
    pub accepted: usize,
    pub disputed: usize,
    pub rejected: usize,
}

/// Current authorized queue metadata; this never leases work or advances a cursor.
pub fn status(store: &Store, access: &Access) -> Result<Status> {
    let jobs = ingest::inspect(store, access)?;
    let mut status = Status {
        watermark: store.state().watermark,
        pending: 0,
        leased: 0,
        deferred: 0,
        failed: 0,
        completed: 0,
        cancelled: 0,
        oldest_pending_watermark: None,
        accepted: 0,
        disputed: 0,
        rejected: 0,
    };
    let mut results = std::collections::BTreeSet::new();
    for job in jobs {
        match job.state {
            JobState::Pending => status.pending += 1,
            JobState::Leased => status.leased += 1,
            JobState::Deferred => status.deferred += 1,
            JobState::Failed => status.failed += 1,
            JobState::Completed => status.completed += 1,
            JobState::Cancelled => status.cancelled += 1,
        }
        if !job.state.finished() {
            status.oldest_pending_watermark = Some(
                status
                    .oldest_pending_watermark
                    .map_or(job.origin_watermark, |old| old.min(job.origin_watermark)),
            );
        }
        results.extend(job.results);
    }
    for id in results {
        let result: vcp_domain::memory::ProposalResult = store
            .state()
            .record(Collection::Projection, id.as_str(), &access.workspace)?
            .decode()?;
        match result.resolution.outcome {
            vcp_domain::memory::Outcome::Accepted => status.accepted += 1,
            vcp_domain::memory::Outcome::Disputed => status.disputed += 1,
            vcp_domain::memory::Outcome::Rejected => status.rejected += 1,
            vcp_domain::memory::Outcome::AwaitingReview => (),
        }
    }
    Ok(status)
}

/// Runs at most two jobs and yields to the host. It never starts a model call,
/// executes a remembered command, or runs in response to a read-only query.
pub async fn step(
    store: &mut Store,
    access: &Access,
    scope: &Scope,
    now: Timestamp,
) -> Result<Progress> {
    let through = store.state().watermark;
    let spec = specification();
    let queued = ingest::enqueue(store, access, scope, &spec, through, limits()).await?;
    let mut progress = Progress {
        caught_up: queued.caught_up,
        quota_reached: queued.quota_reached,
        ..Progress::default()
    };
    let mut jobs = ingest::inspect(store, access)?;
    jobs.sort_by(|a, b| (a.origin_watermark, &a.id).cmp(&(b.origin_watermark, &b.id)));
    for pending in jobs
        .into_iter()
        .filter(|job| {
            job.root == scope.task
                && job.extractor == spec
                && !job.state.finished()
                && job.not_before <= now
                && job
                    .lease
                    .as_ref()
                    .is_none_or(|lease| lease.expires_at <= now)
        })
        .take(limits().pending_jobs)
    {
        if progress.completed + progress.deferred + progress.failed >= 2 {
            break;
        }
        let job = match ingest::lease(store, access, &pending.id, pending.revision, now, limits())
            .await
        {
            Ok(job) => job,
            Err(Error::Conflict("ingestion task or ancestor is held")) => continue,
            Err(error) => return Err(error),
        };
        let Some(lease) = job.lease else {
            progress.failed += 1;
            continue;
        };
        let outcome = process(store, access, &job.origin, now).await;
        match outcome {
            Ok((results, finding)) => {
                ingest::complete(
                    store,
                    access,
                    &job.id,
                    &lease.token,
                    now,
                    results,
                    Some(finding),
                )
                .await?;
                progress.completed += 1;
            }
            Err(error) => {
                let mut reason = error.to_string();
                while reason.len() > 4000 {
                    reason.pop();
                }
                ingest::defer(
                    store,
                    access,
                    &job.id,
                    &lease.token,
                    now,
                    Timestamp::new(now.get().saturating_add(1000)),
                    reason,
                )
                .await?;
                progress.deferred += 1;
            }
        }
    }
    // Processing may attach evidence or resolve proposals after enqueue's fixed
    // cut. Do not report fresh progress while those observations remain unqueued.
    progress.caught_up &= queued.cursor.after.get() == store.state().events.len() as u64;
    Ok(progress)
}

async fn process(
    store: &mut Store,
    access: &Access,
    origin: &EventId,
    now: Timestamp,
) -> Result<(Vec<CommandId>, String)> {
    let event = store
        .state()
        .events
        .iter()
        .find(|event| event.event.id == *origin)
        .cloned()
        .ok_or(Error::Conflict("ingestion origin missing"))?;
    let preference = crate::preferences::materialize(store, access, &event).await?;
    let mut extraction = extractors::extract(store, access, &event)?;
    if let Some(proposal) = preference {
        extraction.proposals.push(proposal);
        extraction
            .findings
            .retain(|finding| finding.code != "unsupported_observation");
    }
    let mut results = Vec::new();
    let mut candidates: std::collections::BTreeMap<_, _> = extraction
        .proposals
        .into_iter()
        .map(|proposal| (proposal.id.clone(), proposal))
        .collect();
    for row in store.state().records.values().filter(|row| {
        row.workspace == access.workspace
            && row.collection == Collection::Claim
            && row.value["document_type"] == "vcp_memory_proposal_v1"
    }) {
        let saved: ProposalRecord = row.decode()?;
        if saved.proposal.extractor == extractors::SPEC && saved.proposal.origins.contains(origin) {
            candidates.insert(saved.id, saved.proposal);
        }
    }
    for candidate in candidates.into_values() {
        // A crash after one output committed must not refresh that output's
        // epochs or evidence based on today's state. Recheck the original
        // proposal through the same repository retry/retention/access boundary.
        let previous = store
            .state()
            .records
            .get(&vcp_store::contract::key(
                Collection::Claim,
                candidate.id.as_str(),
            ))
            .map(|row| row.decode::<ProposalRecord>())
            .transpose()?;
        let proposal = previous.map_or(candidate, |row| row.proposal);
        results.push(
            repository::propose(store, access, proposal, now)
                .await?
                .result
                .id,
        );
    }
    let total = extraction.findings.len();
    let mut retained = extraction.findings;
    let finding = loop {
        let serialized = serde_json::to_string(&serde_json::json!({"outputs":results.len(),
            "findings":retained,"total_findings":total,"truncated":retained.len() < total}))?;
        if serialized.len() <= 4096 {
            break serialized;
        }
        retained.pop();
    };
    Ok((results, finding))
}
