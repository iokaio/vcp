// SPDX-License-Identifier: Apache-2.0
//! Bounded exact repetition facts, never a stall diagnosis or action permission.
use super::{current_policy, current_registry, err, observations, HistoryWindow, Result};
use observations::{CheckResult, VerificationObservation};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use vcp_domain::{accounting::valid_hash, *};
use vcp_memory::access::Access;
use vcp_protocol::{canonical_bytes, digest_bytes};
use vcp_store::Store;

const MAX_OBSERVATIONS: usize = 4096;
const MAX_PERIOD: usize = 8;
const MIN_REPETITIONS: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repetition {
    pub task: TaskId,
    pub steering: SteeringRevision,
    pub task_revision: Revision,
    pub input_fingerprint: String,
    pub pattern_digest: String,
    pub period: u16,
    pub repetition_count: u16,
    pub first_watermark: Watermark,
    pub last_watermark: Watermark,
    pub verifications: Vec<VerificationId>,
    pub events: Vec<EventId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub schema_version: u32,
    pub id: String,
    pub workspace: WorkspaceId,
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
    pub source_evidence: String,
    pub source_digest: String,
    pub source_cutoff: Watermark,
    pub source_window: HistoryWindow,
    pub source_tasks: Option<BTreeSet<TaskId>>,
    pub source_alphabet: String,
    pub source_gaps: u64,
    pub excluded_pruned_records: u64,
    pub policy: Option<String>,
    pub catalog: Option<String>,
    pub algorithm: String,
    pub maximum_observations: u16,
    pub maximum_period: u16,
    pub minimum_repetitions: u16,
    pub repetitions: Vec<Repetition>,
    pub serving_qualified: bool,
    pub limitations: Vec<String>,
}

/// Rebuild from currently authorized retained evidence. No artifact is persisted,
/// no provider is called, and no task, counter or scheduling state is modified.
pub fn observe(store: &Store, access: &Access, window: HistoryWindow) -> Result<Evidence> {
    prepare(store, access, window, &|| Ok(()))?.compute()
}

/// Authorized immutable inputs for bounded owner-driven local computation.
pub(crate) struct Prepared {
    source: observations::Evidence,
    policy: Option<String>,
    catalog: Option<String>,
}

pub(crate) fn prepare(
    store: &Store,
    access: &Access,
    window: HistoryWindow,
    cooperate: &dyn Fn() -> Result<()>,
) -> Result<Prepared> {
    Ok(Prepared {
        source: observations::observe_with_check(store, access, window, cooperate)?,
        policy: current_policy(store, access)?.map(|p| p.value.id),
        catalog: current_registry(store, access)?.map(|r| r.value.catalog.id),
    })
}

impl Prepared {
    pub(crate) fn compute(self) -> Result<Evidence> {
        let source = self.source;
        let repetitions = analyze(&source)?;
        let mut evidence = Evidence {
        schema_version: 1,
        id: String::new(),
        workspace: source.workspace.clone(),
        authority: source.authority,
        deletion: source.deletion,
        source_digest: digest_bytes(&canonical_bytes(&source).map_err(err)?),
        source_evidence: source.id,
        source_cutoff: source.cutoff,
        source_window: source.window,
        source_tasks: source.source_tasks,
        source_alphabet: source.alphabet,
        source_gaps: source.gaps.len() as u64,
        excluded_pruned_records: source.excluded_pruned_records,
        policy: self.policy,
        catalog: self.catalog,
        algorithm: "exact-verification-ngram/1".into(),
        maximum_observations: MAX_OBSERVATIONS as u16,
        maximum_period: MAX_PERIOD as u16,
        minimum_repetitions: MIN_REPETITIONS as u16,
        repetitions,
        serving_qualified: false,
        limitations: vec![
            "Exact retained failed-verification patterns only; normalized M1 check/failure identities do not establish a stall, cause, probability or next action.".into(),
            "Only consecutive verification observations with identical task, task revision, steering and input are joined. Success, unavailable checks and identity changes reset continuity; any M1 gap excludes its entire task, and any pruned source record excludes the entire window.".into(),
            "Earliest nonoverlapping patterns use the shortest period (1–8), with at least three complete repetitions. New diagnostics break an exact match; alternating diagnostics can themselves repeat.".into(),
            "Productive work, flaky checks and environment failures can repeat. Non-verification actions and partial cycles are not classified. Rebuild after pause, steering, retention or reopen; this evidence cannot authorize consumption.".into(),
        ],
    };
        evidence.id = format!(
            "cycle-evidence-{}",
            digest_bytes(&canonical_bytes(&evidence).map_err(err)?)
        );
        Ok(evidence)
    }
}

fn analyze(source: &observations::Evidence) -> Result<Vec<Repetition>> {
    if source.schema_version != 2
        || source.alphabet != "canonical-action-observation/2"
        || source.verifications.len() > MAX_OBSERVATIONS
        || source.gaps.len() > MAX_OBSERVATIONS
    {
        return Err("invalid or oversized cycle source evidence".into());
    }
    // M1 cannot assign every pruned row to a precise position. Never join over
    // an invisible verification; a narrower complete window may remain useful.
    if source.excluded_pruned_records > 0 {
        return Ok(Vec::new());
    }
    let excluded: BTreeSet<_> = source.gaps.iter().map(|g| &g.task).collect();
    let mut identities = BTreeSet::new();
    let mut events = BTreeSet::new();
    let mut previous = Watermark::ZERO;
    let mut check_count = 0usize;
    let mut segments: Vec<Vec<(&VerificationObservation, String)>> = Vec::new();
    let mut segment = Vec::new();
    for observation in &source.verifications {
        check_count = check_count
            .checked_add(observation.checks.len())
            .ok_or("cycle check count overflow")?;
        if check_count > MAX_OBSERVATIONS
            || observation.watermark <= previous
            || observation.watermark > source.cutoff
            || !identities.insert(&observation.verification)
            || !events.insert(&observation.event)
            || !valid_hash(&observation.input_fingerprint)
            || observation.timestamp >= source.window.until
            || source
                .window
                .from
                .is_some_and(|from| observation.timestamp < from)
            || source
                .source_tasks
                .as_ref()
                .is_some_and(|tasks| !tasks.contains(&observation.task))
        {
            return Err("duplicate, unordered or invalid cycle observation".into());
        }
        previous = observation.watermark;
        let mut checks = BTreeSet::new();
        for check in &observation.checks {
            if !valid_hash(&check.check)
                || !checks.insert(&check.check)
                || check
                    .failure_signature
                    .as_ref()
                    .is_some_and(|s| !valid_hash(s))
                || (check.result != CheckResult::Failed && check.failure_signature.is_some())
            {
                return Err("invalid cycle check identity".into());
            }
        }
        let available = !excluded.contains(&observation.task)
            && observation.observed_task_revision.is_some()
            && !observation.checks.is_empty()
            && observation
                .checks
                .iter()
                .all(|c| c.result == CheckResult::Failed && c.failure_signature.is_some());
        let connected =
            segment
                .last()
                .is_none_or(|(prior, _): &(&VerificationObservation, String)| {
                    prior.task == observation.task
                        && prior.steering == observation.steering
                        && prior.observed_task_revision == observation.observed_task_revision
                        && prior.input_fingerprint == observation.input_fingerprint
                        && prior.unresolved_effects == observation.unresolved_effects
                        && prior.outstanding_issues == observation.outstanding_issues
                });
        if !available || !connected {
            if !segment.is_empty() {
                segments.push(std::mem::take(&mut segment));
            }
        }
        if available {
            let symbol = digest_bytes(&canonical_bytes(&observation.checks).map_err(err)?);
            segment.push((observation, symbol));
        }
    }
    if !segment.is_empty() {
        segments.push(segment);
    }
    let mut repetitions = Vec::new();
    for segment in segments {
        let mut start = 0;
        while start < segment.len() {
            let mut consumed = 1;
            for period in 1..=MAX_PERIOD.min((segment.len() - start) / MIN_REPETITIONS) {
                let mut end = start + period;
                while end < segment.len()
                    && segment[end].1 == segment[start + (end - start) % period].1
                {
                    end += 1;
                }
                let count = (end - start) / period;
                if count < MIN_REPETITIONS {
                    continue;
                }
                consumed = count * period;
                let items = &segment[start..start + consumed];
                let first = items[0].0;
                repetitions.push(Repetition {
                    task: first.task.clone(),
                    steering: first.steering,
                    task_revision: first
                        .observed_task_revision
                        .ok_or("cycle revision disappeared")?,
                    input_fingerprint: first.input_fingerprint.clone(),
                    pattern_digest: digest_bytes(
                        &canonical_bytes(
                            &items[..period]
                                .iter()
                                .map(|(_, symbol)| symbol)
                                .collect::<Vec<_>>(),
                        )
                        .map_err(err)?,
                    ),
                    period: period as u16,
                    repetition_count: count as u16,
                    first_watermark: first.watermark,
                    last_watermark: items[consumed - 1].0.watermark,
                    verifications: items.iter().map(|(o, _)| o.verification.clone()).collect(),
                    events: items.iter().map(|(o, _)| o.event.clone()).collect(),
                });
                break;
            }
            start += consumed;
        }
    }
    Ok(repetitions)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn evidence(pattern: &[u8]) -> observations::Evidence {
        let task = TaskId::new();
        observations::Evidence {
            schema_version: 2,
            alphabet: "canonical-action-observation/2".into(),
            id: "fixture".into(),
            workspace: WorkspaceId::new(),
            authority: AuthorityRevision::ZERO,
            deletion: DeletionEpoch::ZERO,
            cutoff: Watermark::new(5000),
            window: HistoryWindow {
                from: None,
                until: Timestamp::new(100),
            },
            source_tasks: None,
            turns: vec![],
            effects: vec![],
            attempts: vec![],
            verifications: pattern
                .iter()
                .enumerate()
                .map(|(i, symbol)| VerificationObservation {
                    verification: VerificationId::new(),
                    task: task.clone(),
                    steering: SteeringRevision::ZERO,
                    observed_task_revision: Some(Revision::new(3)),
                    input_fingerprint: digest_bytes(b"input"),
                    event: EventId::new(),
                    watermark: Watermark::new(i as u64 + 1),
                    timestamp: Timestamp::new(1),
                    checks: vec![observations::CheckObservation {
                        check: digest_bytes(b"check"),
                        result: CheckResult::Failed,
                        exit_code: Some(1),
                        failure_signature: Some(digest_bytes(&[*symbol])),
                    }],
                    unresolved_effects: 0,
                    outstanding_issues: 0,
                    cost_known: false,
                })
                .collect(),
            gaps: vec![],
            excluded_pruned_records: 0,
            limitations: vec![],
        }
    }

    #[test]
    fn exact_cycles_detect_minimal_period_and_complete_repetitions() {
        let source = evidence(&[1, 2, 1, 2, 1, 2, 1]);
        let result = analyze(&source).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!((result[0].period, result[0].repetition_count), (2, 3));
        assert_eq!(result[0].events.len(), 6);
        assert_eq!(analyze(&source).unwrap(), result);
        let constant = analyze(&evidence(&[1; 20])).unwrap();
        assert_eq!((constant[0].period, constant[0].repetition_count), (1, 20));
        assert!(analyze(&evidence(&[1, 1])).unwrap().is_empty());
        assert!(analyze(&evidence(&[1, 1, 2, 2, 3, 3])).unwrap().is_empty());
    }

    #[test]
    fn exact_cycles_reset_on_productive_unknown_and_changed_context() {
        for reset in 0..9 {
            let mut source = evidence(&[1; 5]);
            let middle = &mut source.verifications[2];
            match reset {
                0 => {
                    middle.checks[0].result = CheckResult::Passed;
                    middle.checks[0].failure_signature = None;
                }
                1 => {
                    middle.checks[0].result = CheckResult::NotRun;
                    middle.checks[0].failure_signature = None;
                }
                2 => middle.checks[0].failure_signature = None,
                3 => middle.observed_task_revision = None,
                4 => middle.observed_task_revision = Some(Revision::new(4)),
                5 => middle.input_fingerprint = digest_bytes(b"edited"),
                6 => middle.steering = SteeringRevision::new(1),
                7 => middle.task = TaskId::new(),
                _ => middle.outstanding_issues = 1,
            }
            assert!(analyze(&source).unwrap().is_empty(), "reset {reset}");
        }
        let mut gap = evidence(&[1; 8]);
        gap.gaps.push(observations::Gap {
            task: gap.verifications[0].task.clone(),
            event: EventId::new(),
            reason: observations::GapReason::MissingFacts,
        });
        assert!(analyze(&gap).unwrap().is_empty());
        let mut pruned = evidence(&[1; 8]);
        pruned.excluded_pruned_records = 1;
        assert!(analyze(&pruned).unwrap().is_empty());
    }

    #[test]
    fn exact_cycles_reject_adversarial_order_identity_and_bounds() {
        for mutation in 0..6 {
            let mut source = evidence(&[1; 3]);
            match mutation {
                0 => source.verifications.swap(0, 1),
                1 => {
                    source.verifications[1].verification =
                        source.verifications[0].verification.clone()
                }
                2 => source.verifications[1].event = source.verifications[0].event.clone(),
                3 => source.verifications[0].checks[0].failure_signature = Some("forged".into()),
                4 => {
                    let check = source.verifications[0].checks[0].clone();
                    source.verifications[0].checks.push(check);
                }
                _ => source.cutoff = Watermark::new(1),
            }
            assert!(analyze(&source).is_err(), "mutation {mutation}");
        }
        assert!(analyze(&evidence(&vec![1; MAX_OBSERVATIONS + 1])).is_err());
        let bounded = analyze(&evidence(&vec![1; MAX_OBSERVATIONS])).unwrap();
        assert_eq!(bounded[0].repetition_count, MAX_OBSERVATIONS as u16);
    }
}

#[cfg(test)]
#[path = "cycles_campaign.rs"]
mod campaign;
