// SPDX-License-Identifier: Apache-2.0
//! Retained causal action observations. This is source evidence, not a fitted
//! model, inferred regime, forecast or permission to influence routing.
use super::{authorize, decision_name, err, HistoryWindow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{
    accounting::{
        valid_hash, AdjustmentDirection, Attempt, RequestRole, Reservation, ReservationState,
        Settlement,
    },
    effect::{Effect, EffectState},
    task::{Task, Turn, TurnState},
    verification::{CheckOutcome, CostCertainty, Verification},
    workspace::Workspace,
    *,
};
use vcp_memory::{
    access::Access,
    retention::{purged, Target},
};
use vcp_protocol::{
    canonical_bytes, digest_bytes,
    event::{EventKind, EventMetadata},
};
use vcp_store::{
    contract::{key, Collection, Record},
    Store,
};

const MAX_SCAN: usize = 100_000;
const MAX_OBSERVATIONS: usize = 4096;
const MAX_LINEAGE_DEPTH: u16 = 256;
const MAX_DIAGNOSTIC_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GapReason {
    RedactedEvent,
    MissingFacts,
    InvalidFact,
    RevisionGap,
    DuplicateObservation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gap {
    pub task: TaskId,
    pub event: EventId,
    pub reason: GapReason,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnObservation {
    pub revision: Revision,
    pub state: TurnState,
    pub event: EventId,
    pub watermark: Watermark,
    pub timestamp: Timestamp,
    pub connected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnTrace {
    pub turn: TurnId,
    pub task: TaskId,
    pub steering: SteeringRevision,
    pub observations: Vec<TurnObservation>,
    pub gaps: Vec<Gap>,
    pub left_censored: bool,
    pub right_censored: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectObservation {
    pub revision: Revision,
    pub state: EffectState,
    pub exit_code: Option<i32>,
    pub has_execution: bool,
    pub observed_changes: u32,
    pub event: EventId,
    pub watermark: Watermark,
    pub timestamp: Timestamp,
    pub connected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectTrace {
    pub effect: ToolRunId,
    pub task: TaskId,
    pub steering: SteeringRevision,
    /// Canonical operation digest; command arguments and output remain absent.
    pub operation: String,
    pub observations: Vec<EffectObservation>,
    pub gaps: Vec<Gap>,
    pub left_censored: bool,
    pub right_censored: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptObservation {
    pub revision: Revision,
    pub phase: ReservationState,
    pub event: EventId,
    pub watermark: Watermark,
    pub timestamp: Timestamp,
    pub connected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChargeObservation {
    pub settlement: ObservationId,
    pub direction: AdjustmentDirection,
    pub adjustment_micros: u64,
    pub total_micros: u64,
    pub applied: bool,
    pub final_usage: bool,
    pub event: EventId,
    pub watermark: Watermark,
    pub timestamp: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChargeAttribution {
    pub currency: String,
    /// Cumulative charge at the last retained observation. Unavailable when a
    /// settlement source or the attempt prefix is missing.
    pub charged_micros: Option<u64>,
    /// Canonical reserved liability at the last retained observation.
    pub liability_micros: Option<u64>,
    /// Exact terminal cost-reward component. None is unknown, never zero.
    pub final_charge_micros: Option<u64>,
    pub unknown_remainder: bool,
    pub complete: bool,
    pub settlements: Vec<ChargeObservation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptCohort {
    pub role: RequestRole,
    pub model: String,
    /// Exact qualified endpoint tag captured by accounting.
    pub endpoint: String,
    pub authority_policy: PolicyRevision,
    pub task_class: Option<String>,
    pub root_task: bool,
    pub retry_depth: Option<u16>,
    pub decomposition_depth: Option<u16>,
    pub prior_attempts: Option<u16>,
    pub prior_failed_checks: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptTrace {
    pub attempt: AttemptId,
    pub previous: Option<AttemptId>,
    pub task: TaskId,
    pub cohort: AttemptCohort,
    pub charge: ChargeAttribution,
    pub observations: Vec<AttemptObservation>,
    pub gaps: Vec<Gap>,
    pub left_censored: bool,
    pub right_censored: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckResult {
    Passed,
    Failed,
    NotRun,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckObservation {
    /// Digest of the normalized check specification; raw command/prose is absent.
    pub check: String,
    pub result: CheckResult,
    pub exit_code: Option<i32>,
    /// Exact repeated failure identity. None means unavailable, not a new failure.
    pub failure_signature: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationObservation {
    pub verification: VerificationId,
    pub task: TaskId,
    pub steering: SteeringRevision,
    pub observed_task_revision: Option<Revision>,
    pub input_fingerprint: String,
    pub event: EventId,
    pub watermark: Watermark,
    pub timestamp: Timestamp,
    pub checks: Vec<CheckObservation>,
    pub unresolved_effects: u32,
    pub outstanding_issues: u32,
    pub cost_known: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub schema_version: u32,
    pub alphabet: String,
    pub id: String,
    pub workspace: WorkspaceId,
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
    pub cutoff: Watermark,
    pub window: HistoryWindow,
    pub source_tasks: Option<BTreeSet<TaskId>>,
    pub turns: Vec<TurnTrace>,
    pub effects: Vec<EffectTrace>,
    pub attempts: Vec<AttemptTrace>,
    pub verifications: Vec<VerificationObservation>,
    pub gaps: Vec<Gap>,
    pub excluded_pruned_records: u64,
    pub limitations: Vec<String>,
}

#[derive(Clone)]
struct AttemptIdentity {
    scope: workspace::Scope,
    root: TaskId,
    reservation: ReservationId,
    role: RequestRole,
    agent: AgentId,
    previous: Option<AttemptId>,
    model: String,
    endpoint: String,
    authority_policy: PolicyRevision,
}

struct AccountingSnapshot {
    charged_micros: Option<u64>,
    liability_micros: Option<u64>,
    unknown_remainder: bool,
    complete: bool,
    settlement: Option<ChargeObservation>,
}

struct Build {
    turns: BTreeMap<TurnId, TurnTrace>,
    effects: BTreeMap<ToolRunId, EffectTrace>,
    attempts: BTreeMap<AttemptId, (AttemptIdentity, AttemptTrace)>,
    verifications: Vec<VerificationObservation>,
    gaps: Vec<Gap>,
    excluded: BTreeSet<String>,
    tainted: BTreeSet<TaskId>,
    observations: usize,
    facts: usize,
    attempts_seen: BTreeMap<TaskId, u16>,
    failures_seen: BTreeMap<TaskId, u16>,
    settlements_seen: BTreeMap<ObservationId, AttemptId>,
}

impl Build {
    fn new() -> Self {
        Self {
            turns: BTreeMap::new(),
            effects: BTreeMap::new(),
            attempts: BTreeMap::new(),
            verifications: vec![],
            gaps: vec![],
            excluded: BTreeSet::new(),
            tainted: BTreeSet::new(),
            observations: 0,
            facts: 0,
            attempts_seen: BTreeMap::new(),
            failures_seen: BTreeMap::new(),
            settlements_seen: BTreeMap::new(),
        }
    }
    fn add(&mut self, count: usize) -> Result<()> {
        self.observations = self
            .observations
            .checked_add(count)
            .ok_or("action evidence count overflow")?;
        if self.observations > MAX_OBSERVATIONS {
            return Err("action evidence exceeds 4096 observations; narrow the window".into());
        }
        Ok(())
    }
    fn gap(&mut self, task: &TaskId, event: &EventId, reason: GapReason) -> Result<()> {
        self.add(1)?;
        self.tainted.insert(task.clone());
        self.gaps.push(Gap {
            task: task.clone(),
            event: event.clone(),
            reason,
        });
        Ok(())
    }
}

/// Build one read-only retained view. Independent turns and attempts remain
/// separate traces; callers may feed each uninterrupted trace to pure kernels.
pub fn observe(store: &Store, access: &Access, window: HistoryWindow) -> Result<Evidence> {
    authorize(store, access, false)?;
    let state = store.state();
    if state.events.len() > MAX_SCAN || state.records.len() > MAX_SCAN {
        return Err("action evidence exceeds 100000 canonical rows/events".into());
    }
    if window.from.is_some_and(|from| from >= window.until) {
        return Err("invalid action evidence window".into());
    }
    let workspace: Workspace = state
        .record(
            Collection::Workspace,
            access.workspace.as_str(),
            &access.workspace,
        )
        .map_err(err)?
        .decode()
        .map_err(err)?;
    let mut build = Build::new();
    let mut seen_events = BTreeSet::new();
    for envelope in &state.events {
        let event = &envelope.event;
        if event.workspace != access.workspace
            || event.timestamp >= window.until
            || window.from.is_some_and(|from| event.timestamp < from)
            || !event
                .task
                .as_ref()
                .is_some_and(|task| access.allows_task(task))
        {
            continue;
        }
        let Some(task) = event.task.as_ref() else {
            continue;
        };
        if !eligible_task(store, access, task, &mut build.excluded)? {
            continue;
        }
        if !seen_events.insert(event.id.clone()) {
            return Err("duplicate canonical event identity".into());
        }
        let unavailable = envelope.redaction.is_some()
            || purged(state, &access.workspace, &Target::Event(event.id.clone())).map_err(err)?;
        if unavailable {
            if relevant(&event.kind) {
                build.gap(task, &event.id, GapReason::RedactedEvent)?;
            }
            continue;
        }
        if matches!(
            event.kind,
            EventKind::ReservationCreated
                | EventKind::AttemptSubmitted
                | EventKind::UsageReconciled
                | EventKind::ReservationReleased
                | EventKind::LiabilityRetained
        ) {
            if envelope.version != 1 || event.data["schema_version"] != 1 {
                build.gap(task, &event.id, GapReason::MissingFacts)?;
                continue;
            }
            observe_attempt(store, access, envelope, task, &window, &mut build)?;
        }
        if matches!(
            event.kind,
            EventKind::TurnTransition
                | EventKind::EffectTransition
                | EventKind::TaskTransition
                | EventKind::VerificationRecorded
        ) {
            let facts = (envelope.version == 1 && event.data["schema_version"] == 1)
                .then(|| event.data["facts"].as_array())
                .flatten();
            let Some(facts) = facts else {
                build.gap(task, &event.id, GapReason::MissingFacts)?;
                continue;
            };
            build.facts = build
                .facts
                .checked_add(facts.len())
                .ok_or("action fact count overflow")?;
            if build.facts > MAX_SCAN {
                return Err("action evidence exceeds 100000 facts".into());
            }
            let mut found = false;
            for fact in facts {
                match fact["collection"].as_str() {
                    Some("turn") => {
                        found = true;
                        observe_turn(store, access, envelope, task, fact, &mut build)?;
                    }
                    Some("effect") => {
                        found = true;
                        observe_effect(store, access, envelope, task, fact, &mut build)?;
                    }
                    Some("verification") => {
                        found = true;
                        observe_verification(store, access, envelope, task, fact, &mut build)?;
                    }
                    _ => {}
                }
            }
            if !found
                && matches!(
                    event.kind,
                    EventKind::TurnTransition
                        | EventKind::EffectTransition
                        | EventKind::VerificationRecorded
                )
            {
                build.gap(task, &event.id, GapReason::InvalidFact)?;
            }
        }
    }
    finalize_attempts(store, access, &mut build)?;
    let mut evidence = Evidence {
        schema_version: 2,
        alphabet: "canonical-action-observation/2".into(),
        id: String::new(), workspace: access.workspace.clone(), authority: access.authority,
        deletion: workspace.deletion, cutoff: state.watermark, window, source_tasks: access.tasks.clone(),
        turns: build.turns.into_values().collect(),
        effects: build.effects.into_values().collect(),
        attempts: build.attempts.into_values().map(|(_, trace)| trace).collect(),
        verifications: build.verifications, gaps: build.gaps,
        excluded_pruned_records: build.excluded.len() as u64,
        limitations: vec![
            "Observed retained actions and exact terminal charge rewards only; no inferred regime, fitted probability or routing input.".into(),
            "Independent turn, effect and attempt traces are never joined. Missing/pruned/out-of-window revisions break continuity.".into(),
            "Older verification events lack an applicable task revision; their failure signature remains unavailable.".into(),
            "Task class and counters remain unavailable when their retained source or complete prefix is absent.".into(),
            "Charge rewards preserve currency and owning attempt. Unknown or reserved liability has no point estimate; parent/root rollups are never added.".into(),
        ],
    };
    let encoded = canonical_bytes(&evidence).map_err(err)?;
    if encoded.len() > 512 * 1024 {
        return Err("action evidence exceeds 512 KiB; narrow the window or task scope".into());
    }
    evidence.id = format!("action-evidence-{}", digest_bytes(&encoded));
    Ok(evidence)
}

fn relevant(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::TurnTransition
            | EventKind::EffectTransition
            | EventKind::TaskTransition
            | EventKind::VerificationRecorded
            | EventKind::ReservationCreated
            | EventKind::AttemptSubmitted
            | EventKind::UsageReconciled
            | EventKind::ReservationReleased
            | EventKind::LiabilityRetained
    )
}

fn eligible_task(
    store: &Store,
    access: &Access,
    id: &TaskId,
    excluded: &mut BTreeSet<String>,
) -> Result<bool> {
    if !access.allows_task(id) {
        return Ok(false);
    }
    let Some(record) = store
        .state()
        .records
        .get(&key(Collection::Task, id.as_str()))
        .filter(|record| record.workspace == access.workspace)
    else {
        return Ok(false);
    };
    if purged(
        store.state(),
        &access.workspace,
        &Target::Record(record.key()),
    )
    .map_err(err)?
    {
        excluded.insert(record.key());
        return Ok(false);
    }
    let task: Task = record.decode().map_err(err)?;
    if task.redaction.is_some() {
        excluded.insert(record.key());
        return Ok(false);
    }
    Ok(true)
}

fn eligible_record(
    store: &Store,
    access: &Access,
    collection: Collection,
    id: &str,
    excluded: &mut BTreeSet<String>,
) -> Result<Option<Record>> {
    let Some(record) = store
        .state()
        .records
        .get(&key(collection, id))
        .filter(|record| record.workspace == access.workspace)
        .cloned()
    else {
        return Ok(None);
    };
    if purged(
        store.state(),
        &access.workspace,
        &Target::Record(record.key()),
    )
    .map_err(err)?
    {
        excluded.insert(record.key());
        return Ok(None);
    }
    Ok(Some(record))
}

fn revision(fact: &serde_json::Value, expected: Revision) -> bool {
    serde_json::to_value(expected).ok().as_ref() == Some(&fact["revision"])
}

fn turn_transition(from: TurnState, to: TurnState) -> bool {
    use TurnState::*;
    match (from, to) {
        (Completed | Failed | Cancelled, _) => false,
        (Cancelling, Cancelled) => true,
        (Cancelling, _) => false,
        (_, Paused | Blocked | BudgetExhausted | Failed | Cancelling | WaitingForInput) => {
            from != to
        }
        (Queued, AssemblingContext)
        | (AssemblingContext, ReservingBudget)
        | (ReservingBudget, RequestingModel)
        | (RequestingModel, ProcessingResponse)
        | (ProcessingResponse, AwaitingApproval | ExecutingTools | Verifying)
        | (AwaitingApproval, ExecutingTools)
        | (ExecutingTools, AssemblingContext)
        | (Verifying, Completed) => true,
        (WaitingForInput | Paused | Blocked | BudgetExhausted, AssemblingContext) => true,
        _ => false,
    }
}

fn effect_transition(from: EffectState, to: EffectState) -> bool {
    use EffectState::*;
    matches!(
        (from, to),
        (Proposed, Validated)
            | (Validated, Authorized)
            | (Authorized, DispatchRecorded)
            | (DispatchRecorded, Running | OutcomeUnknown)
            | (Running, Succeeded | Failed | Cancelled | OutcomeUnknown)
            | (OutcomeUnknown, Succeeded | Failed | Cancelled)
            | (Proposed | Validated | Authorized, Cancelled)
    )
}

fn observe_turn(
    store: &Store,
    access: &Access,
    envelope: &vcp_protocol::event::EventEnvelope,
    task_id: &TaskId,
    fact: &serde_json::Value,
    build: &mut Build,
) -> Result<()> {
    let event = &envelope.event;
    let Some(id) = fact["id"].as_str().and_then(|id| TurnId::parse(id).ok()) else {
        return build.gap(task_id, &event.id, GapReason::InvalidFact);
    };
    let record_key = key(Collection::Turn, id.as_str());
    let Some(current) = eligible_record(
        store,
        access,
        Collection::Turn,
        id.as_str(),
        &mut build.excluded,
    )?
    else {
        return if build.excluded.contains(&record_key) {
            Ok(())
        } else {
            build.gap(task_id, &event.id, GapReason::InvalidFact)
        };
    };
    let current: Turn = current.decode().map_err(err)?;
    if current.redaction.is_some() {
        build.excluded.insert(key(Collection::Turn, id.as_str()));
        return Ok(());
    }
    let turn = serde_json::from_value::<Turn>(fact["value"].clone())
        .ok()
        .filter(|turn| {
            turn.id == id
                && turn.scope.workspace == access.workspace
                && turn.scope.session == event.session
                && turn.scope.task == *task_id
                && turn.cause == event.id
                && revision(fact, turn.revision)
                && turn.redaction.is_none()
                && !turn.reason.trim().is_empty()
                && (turn.revision != Revision::ZERO || turn.state == TurnState::Queued)
        });
    let Some(turn) = turn else {
        return build.gap(task_id, &event.id, GapReason::InvalidFact);
    };
    let prior = build
        .turns
        .get(&id)
        .and_then(|trace| trace.observations.last());
    let connected =
        prior.is_some_and(|value| value.revision.get().checked_add(1) == Some(turn.revision.get()));
    let legal = prior.is_none_or(|value| turn_transition(value.state, turn.state));
    let revision_gap = prior.is_some() && (!connected || !legal);
    build.add(1 + usize::from(revision_gap))?;
    let trace = build.turns.entry(id.clone()).or_insert_with(|| TurnTrace {
        turn: id,
        task: task_id.clone(),
        steering: turn.steering,
        observations: vec![],
        gaps: vec![],
        left_censored: true,
        right_censored: true,
    });
    if trace.task != *task_id || trace.steering != turn.steering {
        build.tainted.insert(task_id.clone());
        trace.gaps.push(Gap {
            task: task_id.clone(),
            event: event.id.clone(),
            reason: GapReason::InvalidFact,
        });
        return Ok(());
    }
    if trace
        .observations
        .iter()
        .any(|value| value.revision == turn.revision)
    {
        build.tainted.insert(task_id.clone());
        trace.gaps.push(Gap {
            task: task_id.clone(),
            event: event.id.clone(),
            reason: GapReason::DuplicateObservation,
        });
        return Ok(());
    }
    if revision_gap {
        build.tainted.insert(task_id.clone());
        trace.gaps.push(Gap {
            task: task_id.clone(),
            event: event.id.clone(),
            reason: GapReason::RevisionGap,
        });
    }
    if trace.observations.is_empty() {
        trace.left_censored = turn.revision != Revision::ZERO;
    }
    trace.observations.push(TurnObservation {
        revision: turn.revision,
        state: turn.state,
        event: event.id.clone(),
        watermark: envelope.watermark,
        timestamp: event.timestamp,
        connected,
    });
    trace.right_censored = !matches!(
        turn.state,
        TurnState::Completed | TurnState::Failed | TurnState::Cancelled
    );
    Ok(())
}

fn observe_effect(
    store: &Store,
    access: &Access,
    envelope: &vcp_protocol::event::EventEnvelope,
    task_id: &TaskId,
    fact: &serde_json::Value,
    build: &mut Build,
) -> Result<()> {
    let event = &envelope.event;
    let Some(id) = fact["id"].as_str().and_then(|id| ToolRunId::parse(id).ok()) else {
        return build.gap(task_id, &event.id, GapReason::InvalidFact);
    };
    let record_key = key(Collection::Effect, id.as_str());
    let Some(current) = eligible_record(
        store,
        access,
        Collection::Effect,
        id.as_str(),
        &mut build.excluded,
    )?
    else {
        return if build.excluded.contains(&record_key) {
            Ok(())
        } else {
            build.gap(task_id, &event.id, GapReason::InvalidFact)
        };
    };
    let current: Effect = current.decode().map_err(err)?;
    if current.redaction.is_some() {
        build.excluded.insert(record_key);
        return Ok(());
    }
    let effect = serde_json::from_value::<Effect>(fact["value"].clone())
        .ok()
        .filter(|effect| {
            effect.id == id
                && effect.scope.workspace == access.workspace
                && effect.scope.session == event.session
                && effect.scope.task == *task_id
                && effect.cause == event.id
                && revision(fact, effect.revision)
                && effect.redaction.is_none()
                && valid_hash(&effect.operation_digest)
                && !effect.reason.trim().is_empty()
                && (effect.revision != Revision::ZERO || effect.state == EffectState::Proposed)
        });
    let Some(effect) = effect else {
        return build.gap(task_id, &event.id, GapReason::InvalidFact);
    };
    let prior = build
        .effects
        .get(&id)
        .and_then(|trace| trace.observations.last());
    let connected = prior
        .is_some_and(|value| value.revision.get().checked_add(1) == Some(effect.revision.get()));
    let legal = prior.is_none_or(|value| effect_transition(value.state, effect.state));
    let revision_gap = prior.is_some() && (!connected || !legal);
    build.add(1 + usize::from(revision_gap))?;
    let trace = build
        .effects
        .entry(id.clone())
        .or_insert_with(|| EffectTrace {
            effect: id,
            task: task_id.clone(),
            steering: effect.steering,
            operation: effect.operation_digest.clone(),
            observations: vec![],
            gaps: vec![],
            left_censored: true,
            right_censored: true,
        });
    if trace.task != *task_id
        || trace.steering != effect.steering
        || trace.operation != effect.operation_digest
    {
        build.tainted.insert(task_id.clone());
        trace.gaps.push(Gap {
            task: task_id.clone(),
            event: event.id.clone(),
            reason: GapReason::InvalidFact,
        });
        return Ok(());
    }
    if trace
        .observations
        .iter()
        .any(|value| value.revision == effect.revision)
    {
        build.tainted.insert(task_id.clone());
        trace.gaps.push(Gap {
            task: task_id.clone(),
            event: event.id.clone(),
            reason: GapReason::DuplicateObservation,
        });
        return Ok(());
    }
    if revision_gap {
        build.tainted.insert(task_id.clone());
        trace.gaps.push(Gap {
            task: task_id.clone(),
            event: event.id.clone(),
            reason: GapReason::RevisionGap,
        });
    }
    if trace.observations.is_empty() {
        trace.left_censored = effect.revision != Revision::ZERO;
    }
    trace.observations.push(EffectObservation {
        revision: effect.revision,
        state: effect.state,
        exit_code: effect.exit_code,
        has_execution: effect.execution.is_some(),
        observed_changes: effect
            .observed_changes
            .len()
            .try_into()
            .map_err(|_| "too many observed effect changes")?,
        event: event.id.clone(),
        watermark: envelope.watermark,
        timestamp: event.timestamp,
        connected,
    });
    trace.right_censored = !matches!(
        effect.state,
        EffectState::Succeeded | EffectState::Failed | EffectState::Cancelled
    );
    Ok(())
}

fn observe_attempt(
    store: &Store,
    access: &Access,
    envelope: &vcp_protocol::event::EventEnvelope,
    task_id: &TaskId,
    window: &HistoryWindow,
    build: &mut Build,
) -> Result<()> {
    let event = &envelope.event;
    let Some(attempt) = event
        .data
        .get("attempt")
        .and_then(|value| serde_json::from_value::<Attempt>(value.clone()).ok())
    else {
        return build.gap(task_id, &event.id, GapReason::InvalidFact);
    };
    let record_key = key(Collection::Attempt, attempt.id.as_str());
    let Some(current) = eligible_record(
        store,
        access,
        Collection::Attempt,
        attempt.id.as_str(),
        &mut build.excluded,
    )?
    else {
        return if build.excluded.contains(&record_key) {
            Ok(())
        } else {
            build.gap(task_id, &event.id, GapReason::InvalidFact)
        };
    };
    let current: Attempt = current.decode().map_err(err)?;
    if current.redaction.is_some() || current.redacted_at_revision.is_some() {
        build
            .excluded
            .insert(key(Collection::Attempt, attempt.id.as_str()));
        return Ok(());
    }
    if attempt.validate().is_err()
        || attempt.scope.workspace != access.workspace
        || attempt.scope.session != event.session
        || attempt.scope.task != *task_id
        || event
            .metadata
            .as_ref()
            .is_some_and(|metadata| !metadata_matches(metadata, &attempt))
        || !phase_matches(&event.kind, &attempt, &event.id)
    {
        return build.gap(task_id, &event.id, GapReason::InvalidFact);
    }
    let Some(accounting) = accounting_snapshot(store, access, envelope, &attempt, build)? else {
        return build.gap(task_id, &event.id, GapReason::InvalidFact);
    };
    let prior = build
        .attempts
        .get(&attempt.id)
        .and_then(|(_, trace)| trace.observations.last());
    let connected = prior
        .is_some_and(|value| value.revision.get().checked_add(1) == Some(attempt.revision.get()));
    let revision_gap = prior.is_some() && !connected;
    build.add(1 + usize::from(revision_gap) + usize::from(accounting.settlement.is_some()))?;
    let identity = AttemptIdentity {
        scope: attempt.scope.clone(),
        root: attempt.root.clone(),
        reservation: attempt.reservation.clone(),
        role: attempt.role,
        agent: attempt.agent.clone(),
        previous: attempt.previous.clone(),
        model: attempt.quote.price.model.clone(),
        endpoint: attempt.quote.price.provider.clone(),
        authority_policy: attempt.admitted_policy,
    };
    let prior_attempts = window
        .from
        .is_none()
        .then(|| *build.attempts_seen.get(task_id).unwrap_or(&0));
    let prior_failed_checks = window
        .from
        .is_none()
        .then(|| *build.failures_seen.get(task_id).unwrap_or(&0));
    let (saved, trace) = build.attempts.entry(attempt.id.clone()).or_insert_with(|| {
        *build.attempts_seen.entry(task_id.clone()).or_default() =
            prior_attempts.unwrap_or_default().saturating_add(1);
        let cohort = AttemptCohort {
            role: attempt.role,
            model: identity.model.clone(),
            endpoint: identity.endpoint.clone(),
            authority_policy: attempt.admitted_policy,
            task_class: None,
            root_task: attempt.root == *task_id,
            retry_depth: None,
            decomposition_depth: None,
            prior_attempts,
            prior_failed_checks,
        };
        (
            identity.clone(),
            AttemptTrace {
                attempt: attempt.id.clone(),
                previous: attempt.previous.clone(),
                task: task_id.clone(),
                cohort,
                charge: ChargeAttribution {
                    currency: attempt.quote.amount.currency.code().to_owned(),
                    charged_micros: None,
                    liability_micros: None,
                    final_charge_micros: None,
                    unknown_remainder: true,
                    complete: attempt.revision == Revision::ZERO,
                    settlements: vec![],
                },
                observations: vec![],
                gaps: vec![],
                left_censored: true,
                right_censored: true,
            },
        )
    });
    if !identity_matches(saved, &identity)
        || trace.charge.currency != attempt.quote.amount.currency.code()
    {
        build.tainted.insert(task_id.clone());
        trace.gaps.push(Gap {
            task: task_id.clone(),
            event: event.id.clone(),
            reason: GapReason::InvalidFact,
        });
        return Ok(());
    }
    if trace
        .observations
        .iter()
        .any(|value| value.revision == attempt.revision)
    {
        build.tainted.insert(task_id.clone());
        trace.gaps.push(Gap {
            task: task_id.clone(),
            event: event.id.clone(),
            reason: GapReason::DuplicateObservation,
        });
        return Ok(());
    }
    if revision_gap {
        build.tainted.insert(task_id.clone());
        trace.gaps.push(Gap {
            task: task_id.clone(),
            event: event.id.clone(),
            reason: GapReason::RevisionGap,
        });
    }
    trace.charge.complete &= accounting.complete && !revision_gap;
    trace.charge.charged_micros = trace
        .charge
        .complete
        .then_some(accounting.charged_micros)
        .flatten();
    trace.charge.liability_micros = accounting.liability_micros;
    trace.charge.unknown_remainder = accounting.unknown_remainder;
    if let Some(settlement) = accounting.settlement {
        trace.charge.settlements.push(settlement);
    }
    if trace.observations.is_empty() {
        trace.left_censored = attempt.revision != Revision::ZERO;
    }
    trace.observations.push(AttemptObservation {
        revision: attempt.revision,
        phase: attempt.phase,
        event: event.id.clone(),
        watermark: envelope.watermark,
        timestamp: event.timestamp,
        connected,
    });
    trace.right_censored = !matches!(
        attempt.phase,
        ReservationState::Settled
            | ReservationState::Released
            | ReservationState::ExplicitlyResolved
    );
    Ok(())
}

fn accounting_snapshot(
    store: &Store,
    access: &Access,
    envelope: &vcp_protocol::event::EventEnvelope,
    attempt: &Attempt,
    build: &mut Build,
) -> Result<Option<AccountingSnapshot>> {
    let event = &envelope.event;
    let Some(reservation) = event
        .data
        .get("reservation")
        .and_then(|value| serde_json::from_value::<Reservation>(value.clone()).ok())
    else {
        return Ok(None);
    };
    if reservation.validate().is_err()
        || reservation.id != attempt.reservation
        || reservation.scope != attempt.scope
        || reservation.attempt != attempt.id
        || reservation.phase != attempt.phase
        || reservation.role != attempt.role
        || reservation.charged != attempt.charged
        || reservation.amount.currency != attempt.quote.amount.currency
    {
        return Ok(None);
    }
    let retained_reservation = eligible_record(
        store,
        access,
        Collection::Reservation,
        reservation.id.as_str(),
        &mut build.excluded,
    )?;
    let reservation_available = retained_reservation
        .and_then(|record| record.decode::<Reservation>().ok())
        .is_some_and(|current| {
            current.validate().is_ok()
                && current.id == reservation.id
                && current.scope == reservation.scope
                && current.root == reservation.root
                && current.attempt == reservation.attempt
                && current.role == reservation.role
                && current.amount == reservation.amount
                && current.day == reservation.day
                && current.revision >= reservation.revision
        });
    let mut complete = reservation_available;
    let mut charge = None;
    match (&event.kind, event.data.get("settlement")) {
        (EventKind::UsageReconciled, None) => complete = false,
        (EventKind::UsageReconciled, Some(value)) => {
            let Some(reference) = value.as_object() else {
                return Ok(None);
            };
            if reference.len() != 2
                || reference
                    .get("schema_version")
                    .and_then(serde_json::Value::as_u64)
                    != Some(1)
            {
                return Ok(None);
            }
            let Some(id) = reference
                .get("id")
                .and_then(serde_json::Value::as_str)
                .and_then(|id| ObservationId::parse(id).ok())
            else {
                return Ok(None);
            };
            let record_key = key(Collection::Settlement, id.as_str());
            let retained = eligible_record(
                store,
                access,
                Collection::Settlement,
                id.as_str(),
                &mut build.excluded,
            )?;
            let Some(record) = retained else {
                complete = false;
                return Ok(Some(AccountingSnapshot {
                    charged_micros: None,
                    liability_micros: reservation_available.then_some(reservation.liability.get()),
                    unknown_remainder: true,
                    complete,
                    settlement: None,
                }));
            };
            let Ok(settlement) = record.decode::<Settlement>() else {
                return Ok(None);
            };
            if settlement.redaction.is_some() {
                build.excluded.insert(record_key);
                complete = false;
                return Ok(Some(AccountingSnapshot {
                    charged_micros: None,
                    liability_micros: reservation_available.then_some(reservation.liability.get()),
                    unknown_remainder: true,
                    complete,
                    settlement: None,
                }));
            }
            if settlement.schema_version != 1
                || settlement.normalization_version != 1
                || settlement.id != id
                || settlement.id != settlement.observation.id
                || settlement.scope != attempt.scope
                || settlement.attempt != attempt.id
                || settlement.observation.scope != attempt.scope
                || settlement.observation.attempt != attempt.id
                || settlement.observation.amount.currency != attempt.quote.amount.currency
                || settlement.observation.provider_request.trim().is_empty()
                || attempt.provider_request.as_ref()
                    != Some(&settlement.observation.provider_request)
                || !event.artifacts.contains(&settlement.observation.raw)
                || settlement.total != attempt.charged
                || (settlement.applied
                    && match settlement.direction {
                        AdjustmentDirection::Debit | AdjustmentDirection::Credit => {
                            settlement.adjustment == Micros::ZERO
                        }
                        AdjustmentDirection::None => settlement.adjustment != Micros::ZERO,
                    })
                || (!settlement.applied
                    && (settlement.direction != AdjustmentDirection::None
                        || settlement.adjustment != Micros::ZERO))
            {
                return Ok(None);
            }
            if let Some(previous) = build
                .attempts
                .get(&attempt.id)
                .and_then(|(_, trace)| trace.charge.charged_micros)
            {
                let valid_delta = match settlement.direction {
                    AdjustmentDirection::Debit => {
                        previous.checked_add(settlement.adjustment.get())
                            == Some(settlement.total.get())
                    }
                    AdjustmentDirection::Credit => {
                        settlement
                            .total
                            .get()
                            .checked_add(settlement.adjustment.get())
                            == Some(previous)
                    }
                    AdjustmentDirection::None => previous == settlement.total.get(),
                };
                if !valid_delta {
                    return Ok(None);
                }
            }
            if build
                .settlements_seen
                .insert(settlement.id.clone(), attempt.id.clone())
                .is_some()
            {
                return Ok(None);
            }
            charge = Some(ChargeObservation {
                settlement: settlement.id,
                direction: settlement.direction,
                adjustment_micros: settlement.adjustment.get(),
                total_micros: settlement.total.get(),
                applied: settlement.applied,
                final_usage: settlement.observation.final_usage,
                event: event.id.clone(),
                watermark: envelope.watermark,
                timestamp: event.timestamp,
            });
        }
        (_, Some(_)) => return Ok(None),
        (_, None) => {}
    }
    let unknown_remainder = attempt.uncertain.is_some()
        || !matches!(
            attempt.phase,
            ReservationState::Settled | ReservationState::Released
        )
        || reservation.liability != Micros::ZERO;
    Ok(Some(AccountingSnapshot {
        charged_micros: reservation_available.then_some(attempt.charged.get()),
        liability_micros: reservation_available.then_some(reservation.liability.get()),
        unknown_remainder,
        complete,
        settlement: charge,
    }))
}

fn metadata_matches(metadata: &EventMetadata, attempt: &Attempt) -> bool {
    metadata
        .agent
        .as_ref()
        .is_none_or(|value| value == &attempt.agent)
        && metadata
            .provider
            .as_ref()
            .is_none_or(|value| value == &attempt.quote.price.provider)
        && metadata
            .model
            .as_ref()
            .is_none_or(|value| value == &attempt.quote.price.model)
}
fn phase_matches(kind: &EventKind, attempt: &Attempt, event: &EventId) -> bool {
    match kind {
        EventKind::ReservationCreated => attempt.phase == ReservationState::Created,
        EventKind::AttemptSubmitted => {
            attempt.phase == ReservationState::Submitted
                && attempt.send_intent.as_ref() == Some(event)
        }
        EventKind::ReservationReleased => attempt.phase == ReservationState::Released,
        EventKind::LiabilityRetained => attempt.phase == ReservationState::ReconciliationPending,
        EventKind::UsageReconciled => !matches!(
            attempt.phase,
            ReservationState::Created | ReservationState::Released
        ),
        _ => false,
    }
}
fn identity_matches(a: &AttemptIdentity, b: &AttemptIdentity) -> bool {
    a.scope == b.scope
        && a.root == b.root
        && a.reservation == b.reservation
        && a.role == b.role
        && a.agent == b.agent
        && a.previous == b.previous
        && a.model == b.model
        && a.endpoint == b.endpoint
        && a.authority_policy == b.authority_policy
}

fn observe_verification(
    store: &Store,
    access: &Access,
    envelope: &vcp_protocol::event::EventEnvelope,
    task_id: &TaskId,
    fact: &serde_json::Value,
    build: &mut Build,
) -> Result<()> {
    let event = &envelope.event;
    let Some(id) = fact["id"]
        .as_str()
        .and_then(|id| VerificationId::parse(id).ok())
    else {
        return build.gap(task_id, &event.id, GapReason::InvalidFact);
    };
    let record_key = key(Collection::Verification, id.as_str());
    let Some(current) = eligible_record(
        store,
        access,
        Collection::Verification,
        id.as_str(),
        &mut build.excluded,
    )?
    else {
        return if build.excluded.contains(&record_key) {
            Ok(())
        } else {
            build.gap(task_id, &event.id, GapReason::InvalidFact)
        };
    };
    let current: Verification = current.decode().map_err(err)?;
    if current.redaction.is_some() {
        build
            .excluded
            .insert(key(Collection::Verification, id.as_str()));
        return Ok(());
    }
    let verification = serde_json::from_value::<Verification>(fact["value"].clone())
        .ok()
        .filter(|value| {
            value.id == id
                && value.scope.workspace == access.workspace
                && value.scope.session == event.session
                && value.scope.task == *task_id
                && revision(fact, Revision::ZERO)
                && value.redaction.is_none()
                && value.fingerprint.validate().is_ok()
        });
    let Some(verification) = verification else {
        return build.gap(task_id, &event.id, GapReason::InvalidFact);
    };
    if current != verification {
        return build.gap(task_id, &event.id, GapReason::InvalidFact);
    }
    let observed_task_revision = match event.data.get("observed_task_revision") {
        None => None,
        Some(value) => match serde_json::from_value(value.clone()) {
            Ok(revision) => Some(revision),
            Err(_) => return build.gap(task_id, &event.id, GapReason::InvalidFact),
        },
    };
    if observed_task_revision.is_some_and(|revision| {
        store
            .state()
            .records
            .get(&key(Collection::Task, task_id.as_str()))
            .and_then(|record| record.decode::<Task>().ok())
            .is_none_or(|task| revision > task.revision)
    }) {
        return build.gap(task_id, &event.id, GapReason::InvalidFact);
    }
    let input_fingerprint = digest_bytes(&canonical_bytes(&verification.fingerprint).map_err(err)?);
    let mut checks = Vec::new();
    for check in &verification.checks {
        let Some(specification) = normalize(&check.specification) else {
            return build.gap(task_id, &event.id, GapReason::InvalidFact);
        };
        let identity = digest_bytes(specification.as_bytes());
        let (result, failure_signature) = match &check.outcome {
            CheckOutcome::Passed => (CheckResult::Passed, None),
            CheckOutcome::NotRun { .. } => (CheckResult::NotRun, None),
            CheckOutcome::Failed { reason } => {
                let failures = build.failures_seen.entry(task_id.clone()).or_default();
                *failures = failures.saturating_add(1);
                let signature = observed_task_revision.and_then(|revision| normalize(reason).and_then(|diagnostic| {
                    canonical_bytes(&serde_json::json!({"check":identity,"task_revision":revision,"steering":verification.steering,
                        "fingerprint":input_fingerprint,"exit_code":check.exit_code,"diagnostic":digest_bytes(diagnostic.as_bytes())}))
                        .ok().map(|bytes| digest_bytes(&bytes))
                }));
                (CheckResult::Failed, signature)
            }
        };
        checks.push(CheckObservation {
            check: identity,
            result,
            exit_code: check.exit_code,
            failure_signature,
        });
    }
    build.add(1 + checks.len())?;
    build.verifications.push(VerificationObservation {
        verification: id,
        task: task_id.clone(),
        steering: verification.steering,
        observed_task_revision,
        input_fingerprint,
        event: event.id.clone(),
        watermark: envelope.watermark,
        timestamp: event.timestamp,
        checks,
        unresolved_effects: verification
            .unresolved_effects
            .len()
            .try_into()
            .map_err(|_| "too many unresolved effects")?,
        outstanding_issues: verification
            .outstanding_issues
            .len()
            .try_into()
            .map_err(|_| "too many outstanding issues")?,
        cost_known: matches!(verification.cost, CostCertainty::Known),
    });
    Ok(())
}

fn normalize(value: &str) -> Option<String> {
    if value.is_empty() || value.len() > MAX_DIAGNOSTIC_BYTES || value.contains('\0') {
        return None;
    }
    let normalized = value
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ");
    (!normalized.is_empty() && normalized.len() <= MAX_DIAGNOSTIC_BYTES).then_some(normalized)
}

fn finalize_attempts(store: &Store, access: &Access, build: &mut Build) -> Result<()> {
    let ids: Vec<_> = build.attempts.keys().cloned().collect();
    for id in ids {
        let depth = retry_depth(&id, &build.attempts);
        let Some((identity, trace)) = build.attempts.get_mut(&id) else {
            return Err("attempt observation disappeared during finalization".into());
        };
        let incomplete =
            build.tainted.contains(&trace.task) || trace.left_censored || !trace.gaps.is_empty();
        trace.cohort.retry_depth = (!incomplete).then_some(depth).flatten();
        if incomplete {
            trace.cohort.prior_attempts = None;
            trace.cohort.prior_failed_checks = None;
        }
        trace.charge.complete &= !incomplete;
        if !trace.charge.complete {
            trace.charge.charged_micros = None;
            trace.charge.final_charge_micros = None;
            trace.charge.unknown_remainder = true;
        } else {
            trace.charge.final_charge_micros =
                trace
                    .observations
                    .last()
                    .and_then(|observation| match observation.phase {
                        ReservationState::Settled if !trace.charge.unknown_remainder => {
                            trace.charge.charged_micros
                        }
                        ReservationState::Released if !trace.charge.unknown_remainder => Some(0),
                        _ => None,
                    });
        }
        trace.cohort.decomposition_depth = task_depth(
            store,
            access,
            &trace.task,
            &identity.root,
            &mut build.excluded,
        )?;
        let decision = store
            .state()
            .records
            .get(&key(Collection::Projection, &decision_name(&id)))
            .filter(|record| record.workspace == access.workspace);
        if let Some(record) = decision {
            if !purged(
                store.state(),
                &access.workspace,
                &Target::Record(record.key()),
            )
            .map_err(err)?
            {
                let task = record.value["task"].as_str();
                let attempt = record.value["attempt"].as_str();
                let class = record.value["decision"]["input"]["task_class"].as_str();
                let selected = &record.value["decision"]["selected"];
                let valid = record.value["routing_encoding"] == "object_v1"
                    && record.value["schema_version"] == 1
                    && record.value["document_type"] == "vcp_routing_decision_v1"
                    && task == Some(trace.task.as_str())
                    && attempt == Some(id.as_str())
                    && record.value["root"].as_str() == Some(identity.root.as_str())
                    && record.value["request_digest"]
                        .as_str()
                        .is_some_and(|digest| {
                            digest.len() == 64
                                && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
                        })
                    && record.value["decision"]["input"]["workspace"].as_str()
                        == Some(access.workspace.as_str())
                    && record.value["decision"]["input"]["task"].as_str()
                        == Some(trace.task.as_str())
                    && record.value["decision"]["input"]["root"].as_str()
                        == Some(identity.root.as_str())
                    && selected["model"].as_str() == Some(identity.model.as_str())
                    && selected["endpoint"].as_str() == Some(identity.endpoint.as_str())
                    && record
                        .references
                        .contains(&key(Collection::Attempt, id.as_str()))
                    && record
                        .references
                        .contains(&key(Collection::Task, trace.task.as_str()));
                if valid {
                    trace.cohort.task_class = class
                        .filter(|value| !value.is_empty() && value.len() <= 128)
                        .map(str::to_owned);
                }
            } else {
                build.excluded.insert(record.key());
            }
        }
    }
    Ok(())
}

fn retry_depth(
    id: &AttemptId,
    attempts: &BTreeMap<AttemptId, (AttemptIdentity, AttemptTrace)>,
) -> Option<u16> {
    let mut depth = 0u16;
    let mut current = id.clone();
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(current.clone()) {
            return None;
        }
        let (identity, trace) = attempts.get(&current)?;
        if trace.left_censored || !trace.gaps.is_empty() {
            return None;
        }
        let Some(previous) = &identity.previous else {
            return Some(depth);
        };
        let (_, prior) = attempts.get(previous)?;
        if prior.task != trace.task || depth == MAX_LINEAGE_DEPTH {
            return None;
        }
        depth += 1;
        current = previous.clone();
    }
}

fn task_depth(
    store: &Store,
    access: &Access,
    task: &TaskId,
    root: &TaskId,
    excluded: &mut BTreeSet<String>,
) -> Result<Option<u16>> {
    let mut depth = 0u16;
    let mut current = task.clone();
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(current.clone())
            || depth > MAX_LINEAGE_DEPTH
            || !access.allows_task(&current)
        {
            return Ok(None);
        }
        let Some(record) =
            eligible_record(store, access, Collection::Task, current.as_str(), excluded)?
        else {
            return Ok(None);
        };
        let value: Task = record.decode().map_err(err)?;
        if value.redaction.is_some() || value.scope.task != current || value.root != *root {
            return Ok(None);
        }
        let Some(parent) = value.parent else {
            return Ok((current == *root).then_some(depth));
        };
        depth += 1;
        current = parent;
    }
}
