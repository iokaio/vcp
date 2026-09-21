// SPDX-License-Identifier: Apache-2.0
//! Explicit offline fitting and frozen, task-local repeated-strategy inspection.
//! Nothing here dispatches a provider, persists a fit, or changes an action.
use super::{current_policy, current_registry, cycles, err, observations, HistoryWindow, Result};
use observations::{CheckResult, VerificationObservation};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{
    task::{Task, TaskState},
    *,
};
use vcp_memory::access::Access;
use vcp_models::stall;
use vcp_protocol::{canonical_bytes, digest_bytes};
use vcp_store::{contract::Collection, Store};

const ALPHABET: &str = "exact-failed-verification-checks/1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventPin {
    pub event: EventId,
    pub digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fit {
    pub schema_version: u32,
    pub id: String,
    pub workspace: WorkspaceId,
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
    pub task: TaskId,
    pub steering: SteeringRevision,
    pub task_revision: Revision,
    pub input_fingerprint: String,
    pub source_evidence: String,
    pub source_digest: String,
    pub source_cutoff: Watermark,
    pub source_window: HistoryWindow,
    pub source_tasks: Option<BTreeSet<TaskId>>,
    pub source_events: Vec<EventPin>,
    pub source_verifications: Vec<VerificationId>,
    pub policy: Option<String>,
    pub catalog: Option<String>,
    pub alphabet_version: String,
    pub alphabet: Vec<String>,
    pub model: stall::Model,
    pub serving_qualified: bool,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Abstention {
    MissingObservations,
    ChangedInput,
    UnknownSymbol,
    OversizedWindow,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    pub fit: String,
    pub task: TaskId,
    pub source_evidence: String,
    pub source_cutoff: Watermark,
    pub source_window: HistoryWindow,
    pub exact_cycles: cycles::Evidence,
    pub signal: Option<stall::Signal>,
    pub abstention: Option<Abstention>,
    pub serving_qualified: bool,
}

fn training_digest(source: &observations::Evidence) -> Result<String> {
    // The global cutoff may advance after fitting. Only the frozen source view
    // is compared; this does not estimate or update any model parameters.
    Ok(digest_bytes(
        &canonical_bytes(&(
            &source.verifications,
            &source.gaps,
            source.excluded_pruned_records,
        ))
        .map_err(err)?,
    ))
}

fn event_pins(store: &Store, source: &observations::Evidence) -> Result<Vec<EventPin>> {
    let requested: BTreeSet<_> = source.verifications.iter().map(|v| &v.event).collect();
    let retained: BTreeMap<_, _> = store
        .state()
        .events
        .iter()
        .filter(|event| event.redaction.is_none() && requested.contains(&event.event.id))
        .map(|event| (&event.event.id, &event.event))
        .collect();
    source
        .verifications
        .iter()
        .map(|observation| {
            let event = retained
                .get(&observation.event)
                .ok_or("local fit source event unavailable")?;
            Ok(EventPin {
                event: observation.event.clone(),
                digest: digest_bytes(&canonical_bytes(event).map_err(err)?),
            })
        })
        .collect()
}

fn symbol(observation: &VerificationObservation) -> Result<Option<String>> {
    if observation.observed_task_revision.is_none()
        || observation.checks.is_empty()
        || observation
            .checks
            .iter()
            .any(|check| check.result != CheckResult::Failed || check.failure_signature.is_none())
    {
        return Ok(None);
    }
    Ok(Some(digest_bytes(
        &canonical_bytes(&observation.checks).map_err(err)?,
    )))
}

fn connected(left: &VerificationObservation, right: &VerificationObservation) -> bool {
    left.task == right.task
        && left.steering == right.steering
        && left.observed_task_revision == right.observed_task_revision
        && left.input_fingerprint == right.input_fingerprint
        && left.unresolved_effects == right.unresolved_effects
        && left.outstanding_issues == right.outstanding_issues
}

fn segments(source: &observations::Evidence) -> Result<Vec<Vec<String>>> {
    if source.excluded_pruned_records != 0 || !source.gaps.is_empty() {
        return Err("local fit requires complete retained observations".into());
    }
    let mut result = Vec::new();
    let mut segment = Vec::new();
    let mut prior = None;
    for observation in &source.verifications {
        let current = symbol(observation)?;
        if current.is_none() || prior.is_some_and(|old| !connected(old, observation)) {
            if !segment.is_empty() {
                result.push(std::mem::take(&mut segment));
            }
        }
        if let Some(current) = current {
            segment.push(current);
        }
        prior = Some(observation);
    }
    if !segment.is_empty() {
        result.push(segment);
    }
    Ok(result)
}

fn fit_id(value: &Fit) -> Result<String> {
    let mut identity = value.clone();
    identity.id.clear();
    Ok(format!(
        "local-stall-fit-{}",
        digest_bytes(&canonical_bytes(&identity).map_err(err)?)
    ))
}

/// Explicit offline training. Mixed task cohorts are rejected, not pooled.
pub fn fit(
    store: &Store,
    access: &Access,
    window: HistoryWindow,
    minimum_samples: u64,
) -> Result<Fit> {
    let source = observations::observe(store, access, window.clone())?;
    // Reuse the exact-cycle boundary's validation of ordered identities, check
    // hashes, source alphabet and bounds. Its result is not a training label.
    cycles::observe(store, access, window)?;
    let last = source
        .verifications
        .last()
        .ok_or("no local training observations")?;
    if source.verifications.iter().any(|v| v.task != last.task) {
        return Err("local training requires one task cohort".into());
    }
    let segments = segments(&source)?;
    let alphabet: Vec<String> = segments
        .iter()
        .flatten()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let indexed: Vec<Vec<usize>> = segments
        .iter()
        .map(|segment| {
            segment
                .iter()
                .map(|s| {
                    alphabet
                        .binary_search(s)
                        .map_err(|_| "training alphabet mismatch".to_owned())
                })
                .collect()
        })
        .collect::<Result<_>>()?;
    let model = stall::fit(alphabet.len(), &indexed, minimum_samples)?;
    let mut value = Fit {
        schema_version: 1, id: String::new(), workspace: source.workspace.clone(),
        authority: source.authority, deletion: source.deletion, task: last.task.clone(),
        steering: last.steering, task_revision: last.observed_task_revision.ok_or("training task revision unavailable")?,
        input_fingerprint: last.input_fingerprint.clone(), source_evidence: source.id.clone(),
        source_digest: training_digest(&source)?, source_cutoff: source.cutoff,
        source_window: source.window.clone(), source_tasks: source.source_tasks.clone(),
        source_events: event_pins(store, &source)?,
        source_verifications: source.verifications.iter().map(|v| v.verification.clone()).collect(),
        policy: current_policy(store, access)?.map(|p| p.value.id),
        catalog: current_registry(store, access)?.map(|r| r.value.catalog.id),
        alphabet_version: ALPHABET.into(), alphabet, model, serving_qualified: false,
        limitations: vec![
            "Frozen single-task failed-verification model; no endpoint cohort, calibrated stall probability, review triage or next-action authority.".into(),
            "Repeated failures may be productive, flaky or environment-caused. Entropy is a raw statistic; exact cycles remain a separate observation.".into(),
            "Only explicit fitting updates parameters. New input, steering, missing sources, policy/catalog or authority/deletion changes require a new fit.".into(),
        ],
    };
    value.id = fit_id(&value)?;
    Ok(value)
}

/// Check frozen training provenance without refitting. This is for a fit already
/// accepted by `validate_install`, not proof that untrusted model counts were
/// learned from the source. Appended events are allowed; mutated, newly
/// backdated, hidden or pruned training evidence is not.
pub fn validate(store: &Store, access: &Access, value: &Fit) -> Result<()> {
    if value.schema_version != 1
        || value.alphabet_version != ALPHABET
        || value.serving_qualified
        || value.workspace != access.workspace
        || value.authority != access.authority
        || value.source_tasks != access.tasks
        || !access.allows_task(&value.task)
        || value.id != fit_id(value)?
        || value.alphabet.len() != value.model.states()
        || value.alphabet.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err("local fit identity or access changed".into());
    }
    let source = observations::observe(store, access, value.source_window.clone())?;
    let last = source
        .verifications
        .last()
        .ok_or("local training source unavailable")?;
    if source.deletion != value.deletion
        || training_digest(&source)? != value.source_digest
        || event_pins(store, &source)? != value.source_events
        || source
            .verifications
            .iter()
            .map(|v| &v.verification)
            .ne(value.source_verifications.iter())
        || source.verifications.iter().any(|v| v.task != value.task)
        || last.steering != value.steering
        || last.observed_task_revision != Some(value.task_revision)
        || last.input_fingerprint != value.input_fingerprint
        || current_policy(store, access)?.map(|p| p.value.id) != value.policy
        || current_registry(store, access)?.map(|r| r.value.catalog.id) != value.catalog
    {
        return Err("local fit training evidence or policy changed".into());
    }
    Ok(())
}

/// Explicit installation verifies that deserialized parameters really derive
/// from retained training data. Only this offline boundary rebuilds a model;
/// ordinary evaluation checks provenance and keeps the installed parameters.
pub fn validate_install(store: &Store, access: &Access, value: &Fit) -> Result<()> {
    validate(store, access, value)?;
    let rebuilt = fit(
        store,
        access,
        value.source_window.clone(),
        value.model.minimum_samples(),
    )?;
    if rebuilt.model != value.model || rebuilt.alphabet != value.alphabet {
        return Err("local fitted parameters do not match training evidence".into());
    }
    Ok(())
}

/// Read-only inference from strictly later observations; never fits or updates.
pub fn evaluate(
    store: &Store,
    access: &Access,
    value: &Fit,
    task: &TaskId,
    window: HistoryWindow,
) -> Result<Outcome> {
    validate(store, access, value)?;
    if task != &value.task
        || window
            .from
            .is_none_or(|from| from < value.source_window.until)
    {
        return Err("local inference must follow training in the same task".into());
    }
    let current: Task = store
        .state()
        .record(Collection::Task, task.as_str(), &access.workspace)
        .map_err(err)?
        .decode()
        .map_err(err)?;
    if current.state != TaskState::Running
        || current.steering != value.steering
        || current.revision != value.task_revision
        || digest_bytes(&canonical_bytes(&current.fingerprint).map_err(err)?)
            != value.input_fingerprint
    {
        return Err("local inference task is paused, terminal, steered or revised".into());
    }
    let source = observations::observe(store, access, window.clone())?;
    let exact_cycles = cycles::observe(store, access, window.clone())?;
    let mut outcome = Outcome {
        fit: value.id.clone(),
        task: task.clone(),
        source_evidence: source.id.clone(),
        source_cutoff: source.cutoff,
        source_window: window,
        exact_cycles,
        signal: None,
        abstention: None,
        serving_qualified: false,
    };
    let rows: Vec<_> = source
        .verifications
        .iter()
        .filter(|v| &v.task == task)
        .collect();
    if source.excluded_pruned_records > 0
        || source.gaps.iter().any(|g| &g.task == task)
        || rows.is_empty()
    {
        outcome.abstention = Some(Abstention::MissingObservations);
    } else if rows.len() > stall::MAX_WINDOW {
        outcome.abstention = Some(Abstention::OversizedWindow);
    } else if rows.iter().any(|v| {
        v.watermark <= value.source_cutoff
            || v.steering != value.steering
            || v.observed_task_revision != Some(value.task_revision)
            || v.input_fingerprint != value.input_fingerprint
    }) || rows.windows(2).any(|pair| !connected(pair[0], pair[1]))
    {
        outcome.abstention = Some(Abstention::ChangedInput);
    } else {
        let mut sequence = Vec::new();
        for row in rows {
            let Some(symbol) = symbol(row)? else {
                outcome.abstention = Some(Abstention::MissingObservations);
                break;
            };
            let Ok(index) = value.alphabet.binary_search(&symbol) else {
                outcome.abstention = Some(Abstention::UnknownSymbol);
                break;
            };
            sequence.push(index);
        }
        if outcome.abstention.is_none() {
            outcome.signal = Some(value.model.evaluate(&sequence)?);
        }
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_order_is_exact_and_success_cannot_be_a_failure_symbol() {
        let mut observation = VerificationObservation {
            verification: VerificationId::new(),
            task: TaskId::new(),
            steering: SteeringRevision::ZERO,
            observed_task_revision: Some(Revision::ZERO),
            input_fingerprint: digest_bytes(b"source"),
            event: EventId::new(),
            watermark: Watermark::new(1),
            timestamp: Timestamp::new(1),
            checks: vec![observations::CheckObservation {
                check: digest_bytes(b"check"),
                result: CheckResult::Failed,
                exit_code: Some(1),
                failure_signature: Some(digest_bytes(b"failure")),
            }],
            unresolved_effects: 0,
            outstanding_issues: 0,
            cost_known: false,
        };
        let original = symbol(&observation).unwrap().unwrap();
        observation.checks[0].failure_signature = Some(digest_bytes(b"different"));
        assert_ne!(symbol(&observation).unwrap().unwrap(), original);
        observation.checks[0].result = CheckResult::Passed;
        observation.checks[0].failure_signature = None;
        assert!(symbol(&observation).unwrap().is_none());
        let mut changed = observation.clone();
        changed.input_fingerprint = digest_bytes(b"new source");
        assert!(!connected(&observation, &changed));
        changed = observation.clone();
        changed.steering = SteeringRevision::new(1);
        assert!(!connected(&observation, &changed));
    }
}
