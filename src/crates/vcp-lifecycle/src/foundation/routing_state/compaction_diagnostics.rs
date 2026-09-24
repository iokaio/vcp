// SPDX-License-Identifier: Apache-2.0
//! Descriptive transitions around submitted contexts that consumed a compacted
//! summary. Creating a projection or manifest alone is not consumption.
use super::{err, transitions, HistoryWindow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vcp_context::{compaction::Projection, manifest::Manifest};
use vcp_domain::{
    accounting::{Attempt, RequestRole},
    artifact::{ArtifactDescriptor, CaptureState},
    *,
};
use vcp_memory::{
    access::Access,
    retention::{recall_allowed, Target},
};
use vcp_protocol::{canonical_bytes, digest_bytes, event::EventKind};
use vcp_store::{
    contract::{key, Collection},
    Store,
};

const MAX_ARTIFACTS: usize = 128;
const MAX_BYTES: usize = 512 * 1024;
const MAX_TOTAL_BYTES: usize = 8 * 1024 * 1024;
const MINIMUM_TRANSITIONS: u64 = 5;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactPin {
    pub artifact: ArtifactId,
    pub digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    pub from: Watermark,
    pub until: Watermark,
    pub duration_millis: u64,
    pub transitions: Vec<transitions::TransitionCount>,
    pub observations: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub projection: ArtifactPin,
    pub task: TaskId,
    pub summary: Option<ArtifactId>,
    pub consumed_by: Option<AttemptId>,
    pub context_manifest: Option<ArtifactId>,
    pub context_revisions: Option<vcp_context::manifest::Revisions>,
    pub model: Option<String>,
    pub before: Option<Counts>,
    pub after: Option<Counts>,
    pub abstention: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub schema_version: u32,
    pub id: String,
    pub workspace: WorkspaceId,
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
    pub window: HistoryWindow,
    pub cutoff: Watermark,
    pub source_tasks: Option<BTreeSet<TaskId>>,
    pub transition_evidence: String,
    pub source_artifacts: Vec<ArtifactPin>,
    pub source_events: Vec<EventId>,
    pub source_attempts: Vec<AttemptId>,
    pub entries: Vec<Entry>,
    pub unavailable_artifacts: u64,
    pub minimum_transition_samples: u64,
    pub serving_qualified: bool,
    pub limitations: Vec<String>,
}

struct Captured<T> {
    descriptor: ArtifactDescriptor,
    value: T,
    watermark: Watermark,
}
struct Submitted {
    manifest: Captured<Manifest>,
    attempt: AttemptId,
    event: EventId,
    watermark: Watermark,
    timestamp: Timestamp,
}

fn available(store: &Store, access: &Access, collection: Collection, id: &str) -> Result<bool> {
    recall_allowed(
        store.state(),
        &access.workspace,
        &Target::Record(key(collection, id)),
    )
    .map_err(err)
}

fn read(
    store: &Store,
    access: &Access,
    descriptor: &ArtifactDescriptor,
    total: &mut usize,
) -> Result<Option<Vec<u8>>> {
    if descriptor.state != CaptureState::Complete
        || descriptor.length.get() > MAX_BYTES as u64
        || !available(
            store,
            access,
            Collection::Artifact,
            descriptor.spec.id.as_str(),
        )?
    {
        return Ok(None);
    }
    *total = total
        .checked_add(descriptor.length.get() as usize)
        .ok_or("compaction byte count overflow")?;
    if *total > MAX_TOTAL_BYTES {
        return Err("compaction inspection exceeds 8 MiB; narrow the window".into());
    }
    let mut bytes = Vec::new();
    if vcp_audit::history::History::read_artifact(
        store,
        &access.history(),
        &descriptor.spec.id,
        &mut bytes,
    )
    .is_err()
        || digest_bytes(&bytes) != descriptor.sha256
    {
        return Ok(None);
    }
    Ok(Some(bytes))
}

fn pin(descriptor: &ArtifactDescriptor) -> ArtifactPin {
    ArtifactPin {
        artifact: descriptor.spec.id.clone(),
        digest: descriptor.sha256.clone(),
    }
}

/// Rebuild from authorized artifacts and canonical submission intents. This is
/// a read-only description, not evidence that a provider received the request.
pub fn observe(store: &Store, access: &Access, window: HistoryWindow) -> Result<Report> {
    observe_with_check(store, access, window, &|| Ok(()))
}
pub fn observe_with_check(
    store: &Store,
    access: &Access,
    window: HistoryWindow,
    cooperate: &dyn Fn() -> Result<()>,
) -> Result<Report> {
    cooperate()?;
    let source = transitions::observe_with_check(store, access, window.clone(), cooperate)?;
    let mut report = Report {
        schema_version: 1, id: String::new(), workspace: source.workspace.clone(), authority: source.authority,
        deletion: source.deletion, window, cutoff: source.cutoff, source_tasks: source.source_tasks.clone(), transition_evidence: source.id.clone(),
        source_artifacts: vec![], source_events: vec![], source_attempts: vec![], entries: vec![],
        unavailable_artifacts: 0, minimum_transition_samples: MINIMUM_TRANSITIONS, serving_qualified: false,
        limitations: vec![
            "Consumption means canonical submission intent with exact request digest and included summary identity, never provider receipt. Projection or context creation alone does not qualify.".into(),
            "Adjacent submitted contexts must match exact task/session, revisions and model. Counts describe connected task transitions with explicit interval exposures, not causal regression or calibrated uncertainty.".into(),
            "Missing, pruned, ambiguous or changed contexts abstain. Windows without two matched adjacent submissions have no paired comparison; no prose or summary content is returned.".into(),
        ],
    };
    let mut projections = Vec::new();
    let mut manifests = Vec::new();
    let mut total = 0usize;
    let mut seen = BTreeSet::new();
    for envelope in &store.state().events {
        cooperate()?;
        let event = &envelope.event;
        if event.workspace != access.workspace
            || event.timestamp >= report.window.until
            || report
                .window
                .from
                .is_some_and(|from| event.timestamp < from)
            || event.task.as_ref().is_none_or(|t| !access.allows_task(t))
            || event.kind != EventKind::ArtifactAttached
            || envelope.redaction.is_some()
            || !recall_allowed(
                store.state(),
                &access.workspace,
                &Target::Event(event.id.clone()),
            )
            .map_err(err)?
        {
            continue;
        }
        for id in &event.artifacts {
            cooperate()?;
            let Ok(record) =
                store
                    .state()
                    .record(Collection::Artifact, id.as_str(), &access.workspace)
            else {
                continue;
            };
            let descriptor: ArtifactDescriptor = record.decode().map_err(err)?;
            if !matches!(
                descriptor.spec.schema.as_str(),
                "canonical-compaction-projection/1" | "context-manifest/1"
            ) || event.task.as_ref() != Some(&descriptor.spec.scope.task)
                || event.session != descriptor.spec.scope.session
                || !available(
                    store,
                    access,
                    Collection::Task,
                    descriptor.spec.scope.task.as_str(),
                )?
            {
                continue;
            }
            if !seen.insert(id.clone()) {
                return Err("duplicate compaction artifact attachment".into());
            }
            if seen.len() > MAX_ARTIFACTS {
                return Err(
                    "compaction inspection exceeds 128 artifacts; narrow the window".into(),
                );
            }
            report.source_events.push(event.id.clone());
            report.source_artifacts.push(pin(&descriptor));
            let Some(bytes) = read(store, access, &descriptor, &mut total)? else {
                report.unavailable_artifacts += 1;
                continue;
            };
            if descriptor.spec.schema == "context-manifest/1" {
                match serde_json::from_slice::<Manifest>(&bytes) {
                    Ok(value)
                        if value.version == 1 && value.revisions.scope == descriptor.spec.scope =>
                    {
                        manifests.push(Captured {
                            descriptor,
                            value,
                            watermark: envelope.watermark,
                        })
                    }
                    _ => report.unavailable_artifacts += 1,
                }
            } else {
                projections.push(Captured {
                    descriptor,
                    value: serde_json::from_slice::<serde_json::Value>(&bytes).map_err(err)?,
                    watermark: envelope.watermark,
                });
            }
        }
    }
    let mut submitted = Vec::new();
    let events: BTreeMap<_, _> = store
        .state()
        .events
        .iter()
        .map(|e| (&e.event.id, e))
        .collect();
    if events.len() != store.state().events.len() {
        return Err("duplicate canonical event identity".into());
    }
    let mut attempts = Vec::new();
    for record in store
        .state()
        .records
        .values()
        .filter(|r| r.collection == Collection::Attempt && r.workspace == access.workspace)
    {
        let attempt: Attempt = record.decode().map_err(err)?;
        if access.allows_task(&attempt.scope.task)
            && manifests
                .iter()
                .any(|m| m.value.revisions.scope == attempt.scope)
        {
            attempts.push(attempt);
            if attempts.len() > 512 {
                return Err("compaction inspection exceeds 512 scoped attempts".into());
            }
        }
    }
    for manifest in manifests {
        cooperate()?;
        let mut matches = Vec::new();
        for attempt in &attempts {
            cooperate()?;
            if attempt.redaction.is_some()
                || attempt.role != RequestRole::Main
                || attempt.scope != manifest.value.revisions.scope
                || attempt.request_digest != manifest.value.request_sha256
                || attempt.steering != manifest.value.revisions.steering
                || attempt.admitted_policy != manifest.value.revisions.policy
                || !available(store, access, Collection::Attempt, attempt.id.as_str())?
            {
                continue;
            }
            let Some(send) = attempt.send_intent.as_ref() else {
                continue;
            };
            let Some(envelope) = events.get(send) else {
                continue;
            };
            if envelope.redaction.is_some()
                || envelope.event.kind != EventKind::AttemptSubmitted
                || envelope.watermark <= manifest.watermark
                || envelope.event.timestamp >= report.window.until
                || envelope.event.task.as_ref() != Some(&attempt.scope.task)
                || envelope.event.workspace != access.workspace
                || !envelope.event.artifacts.contains(&attempt.request)
                || !recall_allowed(
                    store.state(),
                    &access.workspace,
                    &Target::Event(send.clone()),
                )
                .map_err(err)?
                || !available(
                    store,
                    access,
                    Collection::Artifact,
                    attempt.request.as_str(),
                )?
            {
                continue;
            }
            let request: ArtifactDescriptor = store
                .state()
                .record(
                    Collection::Artifact,
                    attempt.request.as_str(),
                    &access.workspace,
                )
                .map_err(err)?
                .decode()
                .map_err(err)?;
            if request.sha256 != attempt.request_digest
                || request.spec.scope != attempt.scope
                || read(store, access, &request, &mut total)?.is_none()
            {
                continue;
            }
            matches.push((
                attempt.id.clone(),
                send.clone(),
                envelope.watermark,
                envelope.event.timestamp,
                request,
            ));
        }
        // Repeated identical requests cannot be assigned to a specific captured
        // manifest without an explicit identity link. Do not guess.
        if matches.len() != 1 {
            continue;
        }
        let (attempt, event, watermark, timestamp, request) = matches.remove(0);
        report.source_artifacts.push(pin(&request));
        report.source_attempts.push(attempt.clone());
        report.source_events.push(event.clone());
        submitted.push(Submitted {
            manifest,
            attempt,
            event,
            watermark,
            timestamp,
        });
    }
    let mut use_counts = BTreeMap::new();
    for item in &submitted {
        cooperate()?;
        *use_counts.entry(item.attempt.clone()).or_insert(0usize) += 1;
    }
    submitted.retain(|item| use_counts[&item.attempt] == 1);
    submitted.sort_by_key(|s| s.watermark);
    for projection in projections {
        cooperate()?;
        let mut entry = Entry {
            projection: pin(&projection.descriptor),
            task: projection.descriptor.spec.scope.task.clone(),
            summary: None,
            consumed_by: None,
            context_manifest: None,
            context_revisions: None,
            model: None,
            before: None,
            after: None,
            abstention: Some("unconsumed_or_unavailable_context".into()),
        };
        let parsed = serde_json::from_value::<Projection>(projection.value["projection"].clone());
        let summary =
            serde_json::from_value::<ArtifactId>(projection.value["summary_artifact"].clone());
        if let (Ok(value), Ok(summary)) = (parsed, summary) {
            entry.summary = Some(summary.clone());
            if value.version == 1
                && value.revisions.scope == projection.descriptor.spec.scope
                && value.sources.len() <= 4096
                && value.retained.len() <= 4096
            {
                let summary_digest = digest_bytes(value.summary.as_bytes());
                let mut source_ids = BTreeSet::new();
                let mut sources_complete = value
                    .sources
                    .iter()
                    .all(|s| source_ids.insert(s.artifact.clone()));
                for (id, hash) in value
                    .sources
                    .iter()
                    .map(|s| (&s.artifact, &s.sha256))
                    .chain(std::iter::once((&summary, &summary_digest)))
                {
                    let descriptor = store
                        .state()
                        .record(Collection::Artifact, id.as_str(), &access.workspace)
                        .ok()
                        .and_then(|r| r.decode::<ArtifactDescriptor>().ok());
                    match descriptor {
                        Some(d)
                            if d.sha256 == *hash
                                && d.spec.scope == projection.descriptor.spec.scope =>
                        {
                            report.source_artifacts.push(pin(&d));
                            if report.source_artifacts.len() > 4096 {
                                return Err(
                                    "compaction source dependencies exceed 4096; narrow the window"
                                        .into(),
                                );
                            }
                            sources_complete &= read(store, access, &d, &mut total)?.is_some();
                        }
                        _ => sources_complete = false,
                    }
                }
                if sources_complete {
                    if let Some(index) = submitted.iter().position(|s| {
                        s.manifest.watermark > projection.watermark
                            && s.manifest.value.revisions == value.revisions
                            && s.manifest.value.included.iter().any(|part| {
                                part.artifact == summary && part.source_hash == summary_digest
                            })
                    }) {
                        let current = &submitted[index];
                        entry.consumed_by = Some(current.attempt.clone());
                        entry.context_manifest = Some(current.manifest.descriptor.spec.id.clone());
                        entry.context_revisions = Some(current.manifest.value.revisions.clone());
                        entry.model = Some(current.manifest.value.envelope.model.clone());
                        let same_task: Vec<_> = submitted
                            .iter()
                            .filter(|s| s.manifest.value.revisions.scope == value.revisions.scope)
                            .collect();
                        let position = same_task
                            .iter()
                            .position(|s| s.event == current.event)
                            .ok_or("consumed context disappeared")?;
                        let trace = source.traces.iter().find(|t| t.task == entry.task);
                        if position > 0
                            && position + 1 < same_task.len()
                            && trace.is_some_and(|t| t.gaps.is_empty())
                        {
                            let before = same_task[position - 1];
                            let after = same_task[position + 1];
                            if matching(before, current) && matching(current, after) {
                                let trace = trace.ok_or("task trace disappeared")?;
                                entry.before = Some(counts(trace, before, current));
                                entry.after = Some(counts(trace, current, after));
                                entry.abstention =
                                    sample_abstention(entry.before.as_ref(), entry.after.as_ref());
                            } else {
                                entry.abstention = Some("adjacent_context_or_model_changed".into());
                            }
                        } else {
                            entry.abstention =
                                Some("missing_adjacent_context_or_transition_gap".into());
                        }
                    }
                } else {
                    entry.abstention = Some("projection_source_unavailable".into());
                }
            } else {
                entry.abstention = Some("invalid_projection_scope_or_version".into());
            }
        } else {
            entry.abstention = Some("invalid_projection".into());
        }
        report.entries.push(entry);
    }
    report
        .source_artifacts
        .sort_by(|a, b| a.artifact.cmp(&b.artifact));
    report.source_artifacts.dedup();
    report.source_events.extend(
        source
            .traces
            .iter()
            .flat_map(|t| t.observations.iter().map(|o| o.event.clone())),
    );
    report.source_events.extend(
        source
            .traces
            .iter()
            .flat_map(|t| t.gaps.iter().map(|g| g.event.clone())),
    );
    report.source_events.sort();
    report.source_events.dedup();
    report.source_attempts.sort();
    report.source_attempts.dedup();
    report.id = format!(
        "compaction-diagnostics-{}",
        digest_bytes(&canonical_bytes(&report).map_err(err)?)
    );
    Ok(report)
}

fn matching(left: &Submitted, right: &Submitted) -> bool {
    left.manifest.value.revisions == right.manifest.value.revisions
        && left.manifest.value.envelope == right.manifest.value.envelope
        && left.timestamp <= right.timestamp
}

fn sample_abstention(before: Option<&Counts>, after: Option<&Counts>) -> Option<String> {
    (!(before.is_some_and(|c| c.observations >= MINIMUM_TRANSITIONS)
        && after.is_some_and(|c| c.observations >= MINIMUM_TRANSITIONS)))
    .then(|| "insufficient_transition_samples".into())
}

fn counts(trace: &transitions::Trace, from: &Submitted, until: &Submitted) -> Counts {
    count_interval(
        trace,
        from.watermark,
        until.watermark,
        until.timestamp.get().saturating_sub(from.timestamp.get()),
    )
}

fn count_interval(
    trace: &transitions::Trace,
    from: Watermark,
    until: Watermark,
    duration_millis: u64,
) -> Counts {
    let mut rows: Vec<transitions::TransitionCount> = Vec::new();
    let mut observations = 0;
    for pair in trace.observations.windows(2) {
        if pair[1].connected && pair[0].watermark >= from && pair[1].watermark < until {
            observations += 1;
            if let Some(row) = rows
                .iter_mut()
                .find(|r| r.from == pair[0].state && r.to == pair[1].state)
            {
                row.count += 1;
            } else {
                rows.push(transitions::TransitionCount {
                    from: pair[0].state,
                    to: pair[1].state,
                    count: 1,
                });
            }
        }
    }
    Counts {
        from,
        until,
        duration_millis,
        transitions: rows,
        observations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vcp_domain::task::TaskState;

    #[test]
    fn compaction_intervals_require_samples_on_both_sides() {
        let mut counts = Counts {
            from: Watermark::new(1),
            until: Watermark::new(10),
            duration_millis: 9,
            transitions: vec![],
            observations: MINIMUM_TRANSITIONS,
        };
        assert!(sample_abstention(Some(&counts), Some(&counts)).is_none());
        assert!(sample_abstention(None, Some(&counts)).is_some());
        let supported = counts.clone();
        counts.observations -= 1;
        assert!(sample_abstention(Some(&counts), Some(&supported)).is_some());
        assert!(sample_abstention(Some(&supported), Some(&counts)).is_some());
    }

    #[test]
    fn compaction_intervals_never_bridge_boundary_or_missing_observation() {
        let observations = [
            TaskState::Running,
            TaskState::Paused,
            TaskState::Running,
            TaskState::Paused,
            TaskState::Running,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, state)| transitions::Observation {
            revision: Revision::new(i as u64),
            state,
            event: EventId::new(),
            watermark: Watermark::new(i as u64 + 1),
            timestamp: Timestamp::new(i as u64),
            connected: i != 3,
        })
        .collect();
        let trace = transitions::Trace {
            task: TaskId::new(),
            observations,
            gaps: vec![],
            left_censored: false,
            right_censored: false,
        };
        let result = count_interval(&trace, Watermark::new(2), Watermark::new(5), 40);
        assert_eq!(result.observations, 1);
        assert_eq!(result.duration_millis, 40);
        assert_eq!(
            result.transitions,
            vec![transitions::TransitionCount {
                from: TaskState::Paused,
                to: TaskState::Running,
                count: 1
            }]
        );
        assert_eq!(
            count_interval(&trace, Watermark::new(5), Watermark::new(6), 1).observations,
            0
        );
    }
}
