// SPDX-License-Identifier: Apache-2.0
//! Read-only, unqualified action-path forecasts from complete retained episodes.
use super::{authorize, decision_name, err, observations, transitions, HistoryWindow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{
    accounting::{RequestRole, ReservationState},
    task::TaskState,
    *,
};
use vcp_memory::{
    access::Access,
    retention::{purged, Target},
};
use vcp_models::{markov::Chain, routing::RoutingDecision};
use vcp_protocol::{canonical_bytes, digest_bytes};
use vcp_store::{
    contract::{key, Collection},
    Store,
};

pub const MAX_EPISODES: usize = 512;
pub const MAX_COHORTS: usize = 64;
pub const MINIMUM_EPISODES: u64 = 20;
pub const MINIMUM_ROW_SAMPLES: u64 = 5;
const MAX_VISITS: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Main,
    Retry,
    Handoff,
    Support,
    Verification,
    Completed,
    Failed,
    Cancelled,
}
impl State {
    fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub role: String,
    pub model: String,
    pub endpoint: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cohort {
    pub task_class: String,
    pub policy: String,
    pub catalog: String,
    pub authority_policy: PolicyRevision,
    pub currency: String,
    pub root_task: bool,
    /// Exact historical endpoint/role footprint, not the current catalog.
    pub endpoints: Vec<Endpoint>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Visit {
    pub state: State,
    pub watermark: Watermark,
    /// Charge belongs to this attempt only. Check-only visits add no charge.
    pub reward_micros: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Episode {
    pub task: TaskId,
    pub cohort: String,
    pub visits: Vec<Visit>,
    pub known_cost_micros: u64,
    pub unknown_attempts: u64,
    pub liability_micros: u64,
    pub unknown_liabilities: u64,
    pub source_events: BTreeSet<EventId>,
    pub references: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExcludedEpisode {
    pub task: TaskId,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum ForecastStatus {
    Forecast {
        probabilities: Vec<Vec<f64>>,
        transient_states: Vec<State>,
        absorbing_states: Vec<State>,
        expected_visits: Vec<Vec<f64>>,
        outcome_probabilities: Vec<Vec<f64>>,
        /// Approximate descriptive micros, never a reservation or spending bound.
        expected_cost_micros: Vec<Option<f64>>,
    },
    Abstained {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Loop {
    pub from: State,
    pub to: State,
    pub count: u64,
    pub reverse_count: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CohortReport {
    pub id: String,
    pub identity: Cohort,
    pub episodes: u64,
    pub alphabet: Vec<State>,
    pub counts: Vec<Vec<u64>>,
    pub row_samples: Vec<u64>,
    pub known_cost_micros: u64,
    pub unknown_attempts: u64,
    pub liability_micros: u64,
    pub unknown_liabilities: u64,
    pub dominant_loops: Vec<Loop>,
    pub status: ForecastStatus,
    pub uncertainty: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub schema_version: u32,
    pub algorithm: String,
    pub state_definition: String,
    pub reward_definition: String,
    pub id: String,
    pub workspace: WorkspaceId,
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
    pub cutoff: Watermark,
    pub window: HistoryWindow,
    pub source_actions: String,
    pub source_transitions: String,
    pub excluded_pruned_action_records: u64,
    pub excluded_pruned_tasks: u64,
    pub source_tasks: Option<BTreeSet<TaskId>>,
    pub episodes: Vec<Episode>,
    pub excluded: Vec<ExcludedEpisode>,
    pub cohorts: Vec<CohortReport>,
    pub source_events: BTreeSet<EventId>,
    pub references: BTreeSet<String>,
    pub minimum_episodes: u64,
    pub minimum_row_samples: u64,
    pub serving_qualified: bool,
    pub limitations: Vec<String>,
}

fn add(total: &mut u64, value: u64) -> Result<()> {
    *total = total
        .checked_add(value)
        .ok_or("forecast integer overflow")?;
    Ok(())
}

fn role_name(role: RequestRole) -> &'static str {
    match role {
        RequestRole::Main => "main",
        RequestRole::Helper => "helper",
        RequestRole::Compaction => "compaction",
        RequestRole::Reviewer => "reviewer",
        RequestRole::Child => "child",
        RequestRole::Optimizer => "optimizer",
        RequestRole::Memory => "memory",
        RequestRole::Verification => "verification",
    }
}

fn routing(
    store: &Store,
    access: &Access,
    attempt: &observations::AttemptTrace,
) -> Result<Option<RoutingDecision>> {
    let name = decision_name(&attempt.attempt);
    let Some(record) = store
        .state()
        .records
        .get(&key(Collection::Projection, &name))
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
        return Ok(None);
    }
    // M1 validates normalized class/scope; verify the exact immutable request
    // commitment before using historical policy/catalog labels.
    if attempt.cohort.task_class.is_none() {
        return Ok(None);
    }
    let decision: RoutingDecision =
        serde_json::from_value(record.value["decision"].clone()).map_err(err)?;
    decision.validate().map_err(err)?;
    let admitted: vcp_domain::accounting::Attempt = store
        .state()
        .record(
            Collection::Attempt,
            attempt.attempt.as_str(),
            &access.workspace,
        )
        .map_err(err)?
        .decode()
        .map_err(err)?;
    if record.workspace != access.workspace
        || record.value["request_digest"].as_str() != Some(admitted.request_digest.as_str())
        || record.value["attempt"].as_str() != Some(admitted.id.as_str())
        || record.value["scope"] != serde_json::to_value(&admitted.scope).map_err(err)?
        || admitted.scope.task != attempt.task
        || decision.input.task != attempt.task
        || decision.input.root != admitted.root
        || decision.input.workspace != access.workspace
        || decision.input.role != admitted.role
        || admitted.role != attempt.cohort.role
        || decision.selected.as_ref().is_none_or(|selected| {
            selected.model != admitted.quote.price.model
                || selected.endpoint != admitted.quote.price.provider
        })
    {
        return Err("historical routing request or scope identity differs".into());
    }
    Ok(Some(decision))
}

/// Analyze one coherent retained view without persisting, dispatching, or
/// changing routing/verification/budget state. Unknown evidence is not zero.
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
    authorize(store, access, false)?;
    let actions = observations::observe_with_check(store, access, window.clone(), cooperate)?;
    let lifecycle = transitions::observe_with_check(store, access, window.clone(), cooperate)?;
    if lifecycle.traces.len() > MAX_EPISODES {
        return Err("forecast exceeds 512 tasks; narrow the window".into());
    }
    let mut episodes = Vec::new();
    let mut excluded = Vec::new();
    let mut identities = BTreeMap::new();
    let mut total_visits = 0usize;
    let mut unsupported_roots = BTreeSet::new();
    for record in store
        .state()
        .records
        .values()
        .filter(|r| r.collection == Collection::Task && r.workspace == access.workspace)
    {
        let task: vcp_domain::task::Task = record.decode().map_err(err)?;
        if task.parent.is_some() || task.fork_origin.is_some() || task.root != task.scope.task {
            unsupported_roots.insert(task.root);
            unsupported_roots.insert(task.scope.task);
        }
    }
    for trace in &lifecycle.traces {
        cooperate()?;
        if unsupported_roots.contains(&trace.task) {
            excluded.push(ExcludedEpisode {
                task: trace.task.clone(),
                reason: "child, fork or decomposed root requires a root-level forecast".into(),
            });
            continue;
        }
        match episode(store, access, &actions, trace) {
            Ok((episode, cohort)) => {
                total_visits += episode.visits.len();
                if total_visits > MAX_VISITS {
                    return Err("forecast exceeds 4096 action visits".into());
                }
                identities.insert(episode.cohort.clone(), cohort);
                if identities.len() > MAX_COHORTS {
                    return Err("forecast exceeds 64 cohorts; narrow the window".into());
                }
                episodes.push(episode);
            }
            Err(reason) => excluded.push(ExcludedEpisode {
                task: trace.task.clone(),
                reason,
            }),
        }
    }
    // Attempts without any retained task trajectory are explicit exclusions.
    let seen: BTreeSet<_> = lifecycle.traces.iter().map(|t| &t.task).collect();
    for task in actions
        .attempts
        .iter()
        .map(|a| &a.task)
        .collect::<BTreeSet<_>>()
    {
        if !seen.contains(task) {
            excluded.push(ExcludedEpisode {
                task: task.clone(),
                reason: "missing task trajectory".into(),
            });
        }
    }
    let cohorts = identities
        .into_iter()
        .map(|(id, identity)| {
            cooperate()?;
            let selected: Vec<_> = episodes.iter().filter(|e| e.cohort == id).collect();
            aggregate(id, identity, &selected)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut report = Report {
        schema_version: 1, algorithm: "first-order-observed-action-support-zero-prior/1".into(),
        state_definition: "complete-serial-single-task-action-episode/1".into(),
        reward_definition: "exact-final-attempt-charge-once-per-admission/1".into(),
        id: String::new(), workspace: access.workspace.clone(), authority: access.authority,
        deletion: actions.deletion, cutoff: actions.cutoff, window, source_actions: actions.id,
        source_transitions: lifecycle.id,
        excluded_pruned_action_records: actions.excluded_pruned_records,
        excluded_pruned_tasks: lifecycle.excluded_pruned_tasks,
        source_tasks: access.tasks.clone(),
        source_events: episodes.iter().flat_map(|e| e.source_events.iter().cloned()).collect(),
        references: episodes.iter().flat_map(|e| e.references.iter().cloned()).collect(),
        episodes, excluded, cohorts, minimum_episodes: MINIMUM_EPISODES, minimum_row_samples: MINIMUM_ROW_SAMPLES,
        serving_qualified: false,
        limitations: vec![
            "Complete serial single-task episodes only; censored, blocked/paused, concurrent or missing-provenance trajectories are excluded with reasons. Exclusions can bias forecasts.".into(),
            "Action order uses retained admission/closure and explicit ancestry. Handoffs require canonical escalation records; a changed endpoint alone never establishes causation.".into(),
            "First-order observed-support estimates with zero prior. Sample gates and raw denominators are uncertainty evidence, not calibrated confidence intervals or proof of a Markov process.".into(),
            "Known charges are counted once per attempt and currencies never mix. Unknown liabilities suppress cost predictions; approximate expected cost is not a reservation bound or probability of completion within remaining funds.".into(),
            "Endpoint/role footprints and historical routing policy/catalog define cohorts; task size, language, context strategy and intervention confounding remain unqualified. No causal policy or compaction effect is established.".into(),
            "Children are not root rollups: episodes and rewards are task-local; delegation/integration costs require separate qualified root analysis.".into(),
        ],
    };
    // Exclusion counts/reasons also derive from retained history. Carry their
    // sources so a future saved report cannot outlive a pruned rejected cohort.
    for trace in &lifecycle.traces {
        cooperate()?;
        report
            .references
            .insert(key(Collection::Task, trace.task.as_str()));
        report
            .source_events
            .extend(trace.observations.iter().map(|o| o.event.clone()));
        report
            .source_events
            .extend(trace.gaps.iter().map(|g| g.event.clone()));
    }
    for trace in &actions.attempts {
        cooperate()?;
        report
            .references
            .insert(key(Collection::Task, trace.task.as_str()));
        report
            .references
            .insert(key(Collection::Attempt, trace.attempt.as_str()));
        let decision = key(Collection::Projection, &decision_name(&trace.attempt));
        if store.state().records.contains_key(&decision) {
            report.references.insert(decision);
        }
        report
            .source_events
            .extend(trace.observations.iter().map(|o| o.event.clone()));
        report
            .source_events
            .extend(trace.gaps.iter().map(|g| g.event.clone()));
        for settlement in &trace.charge.settlements {
            cooperate()?;
            report
                .references
                .insert(key(Collection::Settlement, settlement.settlement.as_str()));
            report.source_events.insert(settlement.event.clone());
        }
    }
    for trace in &actions.turns {
        cooperate()?;
        report
            .references
            .insert(key(Collection::Task, trace.task.as_str()));
        report
            .references
            .insert(key(Collection::Turn, trace.turn.as_str()));
        report
            .source_events
            .extend(trace.observations.iter().map(|o| o.event.clone()));
        report
            .source_events
            .extend(trace.gaps.iter().map(|g| g.event.clone()));
    }
    for trace in &actions.effects {
        cooperate()?;
        report
            .references
            .insert(key(Collection::Task, trace.task.as_str()));
        report
            .references
            .insert(key(Collection::Effect, trace.effect.as_str()));
        report
            .source_events
            .extend(trace.observations.iter().map(|o| o.event.clone()));
        report
            .source_events
            .extend(trace.gaps.iter().map(|g| g.event.clone()));
    }
    for verification in &actions.verifications {
        cooperate()?;
        report
            .references
            .insert(key(Collection::Task, verification.task.as_str()));
        report.references.insert(key(
            Collection::Verification,
            verification.verification.as_str(),
        ));
        report.source_events.insert(verification.event.clone());
    }
    for gap in &actions.gaps {
        cooperate()?;
        report
            .references
            .insert(key(Collection::Task, gap.task.as_str()));
        report.source_events.insert(gap.event.clone());
    }
    if report.references.len() > 16_384 || report.source_events.len() > 8192 {
        return Err("forecast source references exceed bounded report capacity".into());
    }
    report.id = format!(
        "action-forecast-{}",
        digest_bytes(&canonical_bytes(&report).map_err(err)?)
    );
    Ok(report)
}

fn episode(
    store: &Store,
    access: &Access,
    actions: &observations::Evidence,
    trace: &transitions::Trace,
) -> Result<(Episode, Cohort)> {
    if actions.excluded_pruned_records > 0 {
        return Err("pruned action evidence".into());
    }
    if trace.left_censored
        || trace.right_censored
        || !trace.gaps.is_empty()
        || actions.gaps.iter().any(|g| g.task == trace.task)
    {
        return Err("censored or gapped trajectory".into());
    }
    if trace.observations.iter().any(|o| {
        matches!(
            o.state,
            TaskState::Paused | TaskState::Blocked | TaskState::WaitingForInput
        )
    }) {
        return Err("held task trajectory is outside the serial action alphabet".into());
    }
    let start = trace.observations.first().ok_or("empty task trajectory")?;
    let terminal = trace
        .observations
        .last()
        .ok_or("missing terminal observation")?;
    let terminal_state = match terminal.state {
        TaskState::Completed => State::Completed,
        TaskState::Failed => State::Failed,
        TaskState::Cancelled => State::Cancelled,
        _ => return Err("missing terminal outcome".into()),
    };
    let mut attempts: Vec<_> = actions
        .attempts
        .iter()
        .filter(|a| a.task == trace.task)
        .collect();
    attempts.sort_by_key(|a| a.observations.first().map(|o| o.watermark));
    let first = *attempts.first().ok_or("no retained model attempts")?;
    if first.cohort.role != RequestRole::Main || first.previous.is_some() {
        return Err("episode lacks original main attempt".into());
    }
    let original =
        routing(store, access, first)?.ok_or("historical main routing cohort unavailable")?;
    let currency = first.charge.currency.clone();
    let mut endpoints = BTreeSet::new();
    let mut visits = Vec::new();
    let mut source_events: BTreeSet<_> =
        trace.observations.iter().map(|o| o.event.clone()).collect();
    let mut references = BTreeSet::from([key(Collection::Task, trace.task.as_str())]);
    let mut known_cost = 0;
    let mut unknown = 0;
    let mut liability = 0;
    let mut unknown_liabilities = 0;
    let mut previous_end = start.watermark;
    let mut previous_main: Option<&observations::AttemptTrace> = None;
    let mut intervals = Vec::new();
    for attempt in attempts {
        let admitted = attempt
            .observations
            .first()
            .ok_or("attempt has no admission")?;
        let end = attempt
            .observations
            .iter()
            .find(|o| {
                !matches!(
                    o.phase,
                    ReservationState::Created | ReservationState::Submitted
                )
            })
            .ok_or("attempt has no retained causal closure")?;
        if attempt.left_censored
            || !attempt.gaps.is_empty()
            || admitted.phase != ReservationState::Created
            || admitted.watermark <= previous_end
            || admitted.watermark >= terminal.watermark
            || end.watermark >= terminal.watermark
            || matches!(
                end.phase,
                ReservationState::Created | ReservationState::Submitted
            )
        {
            return Err("incomplete or overlapping attempt trajectory".into());
        }
        if attempt.charge.currency != currency
            || attempt.cohort.authority_policy != first.cohort.authority_policy
            || attempt.cohort.root_task != first.cohort.root_task
        {
            return Err("mixed currency or authority cohort".into());
        }
        let decision = routing(store, access, attempt)?;
        if attempt.cohort.role == RequestRole::Main && decision.is_none() {
            return Err("historical main routing cohort unavailable".into());
        }
        if let Some(decision) = decision {
            if decision.input.policy != original.input.policy
                || decision.input.catalog != original.input.catalog
                || decision.input.task_class != original.input.task_class
            {
                return Err("routing cohort changed within episode".into());
            }
            references.insert(key(
                Collection::Projection,
                &decision_name(&attempt.attempt),
            ));
        }
        let mut state = match attempt.cohort.role {
            RequestRole::Main => State::Main,
            RequestRole::Verification => State::Verification,
            _ => State::Support,
        };
        if attempt.cohort.role == RequestRole::Main {
            if previous_main.is_some() && attempt.previous.is_none() {
                return Err("subsequent main attempt lacks canonical ancestry".into());
            }
            if let Some(previous) = &attempt.previous {
                let prior = previous_main
                    .filter(|p| &p.attempt == previous)
                    .ok_or("main retry ancestry is not adjacent")?;
                let escalation_id = format!(
                    "routing-escalation-{}",
                    digest_bytes(attempt.attempt.as_str().as_bytes())
                );
                let escalation = store
                    .state()
                    .records
                    .get(&key(Collection::Projection, &escalation_id));
                if let Some(record) = escalation {
                    if purged(
                        store.state(),
                        &access.workspace,
                        &Target::Record(record.key()),
                    )
                    .map_err(err)?
                    {
                        return Err("pruned escalation source".into());
                    }
                    let admission: super::EscalationAdmission = super::decode_record(record)?;
                    if record.workspace != access.workspace
                        || admission.document_type != "vcp_escalation_admission_v1"
                        || admission.attempt != attempt.attempt
                        || admission.scope.workspace != access.workspace
                        || admission.scope.task != trace.task
                        || admission.plan.previous_attempt != *previous
                        || admission.handoff.previous_attempt != *previous
                        || admission.plan.previous.model != prior.cohort.model
                        || admission.plan.previous.endpoint != prior.cohort.endpoint
                        || admission.plan.selected.model != attempt.cohort.model
                        || admission.plan.selected.endpoint != attempt.cohort.endpoint
                    {
                        return Err("escalation source identity differs".into());
                    }
                    state = match admission.plan.trigger.class() {
                        vcp_models::escalation::Class::TransportRetry => State::Retry,
                        vcp_models::escalation::Class::QualitySwitch => State::Handoff,
                        _ => return Err("decomposition requires root-level forecast".into()),
                    };
                    references.insert(record.key());
                } else {
                    if prior.cohort.model != attempt.cohort.model
                        || prior.cohort.endpoint != attempt.cohort.endpoint
                    {
                        return Err("endpoint change has no canonical handoff".into());
                    }
                    state = State::Retry;
                }
            }
            previous_main = Some(attempt);
        } else if attempt.previous.is_some() {
            return Err("support retry chain requires a separate supported alphabet".into());
        }
        let reward = if attempt.charge.complete && !attempt.charge.unknown_remainder {
            attempt.charge.final_charge_micros
        } else {
            None
        };
        if let Some(cost) = reward {
            add(&mut known_cost, cost)?;
        } else {
            add(&mut unknown, 1)?;
        }
        if let Some(amount) = attempt.charge.liability_micros {
            add(&mut liability, amount)?;
        } else {
            add(&mut unknown_liabilities, 1)?;
        }
        visits.push(Visit {
            state,
            watermark: admitted.watermark,
            reward_micros: reward,
        });
        endpoints.insert(Endpoint {
            role: role_name(attempt.cohort.role).into(),
            model: attempt.cohort.model.clone(),
            endpoint: attempt.cohort.endpoint.clone(),
        });
        source_events.extend(attempt.observations.iter().map(|o| o.event.clone()));
        references.insert(key(Collection::Attempt, attempt.attempt.as_str()));
        for settlement in &attempt.charge.settlements {
            source_events.insert(settlement.event.clone());
            references.insert(key(Collection::Settlement, settlement.settlement.as_str()));
        }
        intervals.push((admitted.watermark, end.watermark));
        previous_end = end.watermark;
    }
    for check in actions
        .verifications
        .iter()
        .filter(|v| v.task == trace.task)
    {
        if check.watermark <= start.watermark
            || check.watermark >= terminal.watermark
            || intervals
                .iter()
                .any(|(start, end)| check.watermark > *start && check.watermark < *end)
        {
            return Err("verification ordering overlaps attempt or task boundary".into());
        }
        visits.push(Visit {
            state: State::Verification,
            watermark: check.watermark,
            reward_micros: Some(0),
        });
        source_events.insert(check.event.clone());
        references.insert(key(Collection::Verification, check.verification.as_str()));
    }
    visits.sort_by_key(|v| v.watermark);
    if visits.windows(2).any(|v| v[0].watermark == v[1].watermark) {
        return Err("ambiguous same-event action order".into());
    }
    visits.push(Visit {
        state: terminal_state,
        watermark: terminal.watermark,
        reward_micros: Some(0),
    });
    let identity = Cohort {
        task_class: original.input.task_class,
        policy: original.input.policy,
        catalog: original.input.catalog,
        authority_policy: first.cohort.authority_policy,
        currency,
        root_task: first.cohort.root_task,
        endpoints: endpoints.into_iter().collect(),
    };
    let cohort = digest_bytes(&canonical_bytes(&identity).map_err(err)?);
    Ok((
        Episode {
            task: trace.task.clone(),
            cohort,
            visits,
            known_cost_micros: known_cost,
            unknown_attempts: unknown,
            liability_micros: liability,
            unknown_liabilities,
            source_events,
            references,
        },
        identity,
    ))
}

fn aggregate(id: String, identity: Cohort, episodes: &[&Episode]) -> Result<CohortReport> {
    let alphabet: Vec<_> = episodes
        .iter()
        .flat_map(|e| e.visits.iter().map(|v| v.state))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let n = alphabet.len();
    let mut counts = vec![vec![0; n]; n];
    let mut rewards = vec![0u64; n];
    let mut samples = vec![0u64; n];
    let mut missing = vec![false; n];
    let mut known_cost = 0;
    let mut unknown = 0;
    let mut liability = 0;
    let mut unknown_liabilities = 0;
    for episode in episodes {
        add(&mut known_cost, episode.known_cost_micros)?;
        add(&mut unknown, episode.unknown_attempts)?;
        add(&mut liability, episode.liability_micros)?;
        add(&mut unknown_liabilities, episode.unknown_liabilities)?;
        for visit in &episode.visits {
            let index = alphabet
                .binary_search(&visit.state)
                .map_err(|_| "forecast alphabet mismatch")?;
            add(&mut samples[index], 1)?;
            if let Some(reward) = visit.reward_micros {
                add(&mut rewards[index], reward)?;
            } else {
                missing[index] = true;
            }
        }
        for pair in episode.visits.windows(2) {
            let from = alphabet
                .binary_search(&pair[0].state)
                .map_err(|_| "forecast alphabet mismatch")?;
            let to = alphabet
                .binary_search(&pair[1].state)
                .map_err(|_| "forecast alphabet mismatch")?;
            add(&mut counts[from][to], 1)?;
        }
    }
    let row_samples = counts.iter().map(|row| row.iter().sum()).collect();
    let absorbing: Vec<_> = alphabet.iter().map(|s| s.terminal()).collect();
    let mut loops = Vec::new();
    for i in 0..n {
        for j in i..n {
            if counts[i][j] > 0 && (i == j || counts[j][i] > 0) && !absorbing[i] && !absorbing[j] {
                loops.push(Loop {
                    from: alphabet[i],
                    to: alphabet[j],
                    count: counts[i][j],
                    reverse_count: if i == j { 0 } else { counts[j][i] },
                });
            }
        }
    }
    loops.sort_by_key(|l| std::cmp::Reverse(l.count + l.reverse_count));
    loops.truncate(8);
    let status = if (episodes.len() as u64) < MINIMUM_EPISODES {
        ForecastStatus::Abstained {
            reason: "fewer than twenty complete comparable episodes".into(),
        }
    } else {
        let legal: Vec<Vec<bool>> = alphabet
            .iter()
            .map(|from| {
                alphabet
                    .iter()
                    .map(|to| !from.terminal() || from == to)
                    .collect()
            })
            .collect();
        match Chain::fit(&counts, &legal, absorbing, 0.0, MINIMUM_ROW_SAMPLES) {
            Err(error) => ForecastStatus::Abstained {
                reason: format!("action chain unavailable: {error:?}"),
            },
            Ok(chain) => {
                let zero = vec![Some(0.0); n];
                match chain.analyze(&zero) {
                    Err(error) => ForecastStatus::Abstained {
                        reason: format!("action analysis unavailable: {error:?}"),
                    },
                    Ok(analysis) => {
                        let reward: Vec<_> = (0..n)
                            .map(|i| {
                                if missing[i] {
                                    None
                                } else {
                                    Some(rewards[i] as f64 / samples[i] as f64)
                                }
                            })
                            .collect();
                        let costs = if unknown == 0 && unknown_liabilities == 0 {
                            chain.analyze(&reward).ok().map(|a| a.expected_rewards)
                        } else {
                            None
                        };
                        ForecastStatus::Forecast {
                            probabilities: chain.probabilities().to_vec(),
                            transient_states: analysis
                                .transient_states
                                .iter()
                                .map(|&i| alphabet[i])
                                .collect(),
                            absorbing_states: analysis
                                .absorbing_states
                                .iter()
                                .map(|&i| alphabet[i])
                                .collect(),
                            expected_cost_micros: analysis
                                .transient_states
                                .iter()
                                .enumerate()
                                .map(|(i, _)| costs.as_ref().map(|v| v[i]))
                                .collect(),
                            expected_visits: analysis.expected_visits,
                            outcome_probabilities: analysis.outcome_probabilities,
                        }
                    }
                }
            }
        }
    };
    Ok(CohortReport { id, identity, episodes: episodes.len() as u64, alphabet, counts, row_samples,
        known_cost_micros: known_cost, unknown_attempts: unknown, liability_micros: liability, unknown_liabilities, dominant_loops: loops, status,
        uncertainty: vec!["Uncalibrated first-order point estimates; raw sample support and exclusions are visible. Confidence intervals and held-out outcome qualification are unavailable.".into(),
            "Expected visits include the starting transient visit. Terminal visits add zero provider charge. Unsupported terminal outcomes have no invented probability mass.".into()] })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cohort() -> Cohort {
        Cohort {
            task_class: "fixture".into(),
            policy: "p".into(),
            catalog: "c".into(),
            authority_policy: PolicyRevision::ZERO,
            currency: "USD".into(),
            root_task: true,
            endpoints: vec![],
        }
    }
    fn episode(states: &[State], cost: Option<u64>) -> Episode {
        Episode {
            task: TaskId::new(),
            cohort: "c".into(),
            visits: states
                .iter()
                .enumerate()
                .map(|(i, &state)| Visit {
                    state,
                    watermark: Watermark::new(i as u64),
                    reward_micros: if state.terminal() { Some(0) } else { cost },
                })
                .collect(),
            known_cost_micros: cost.unwrap_or(0) * (states.len() as u64 - 1),
            unknown_attempts: if cost.is_none() {
                states.len() as u64 - 1
            } else {
                0
            },
            liability_micros: 0,
            unknown_liabilities: 0,
            source_events: BTreeSet::new(),
            references: BTreeSet::new(),
        }
    }
    #[test]
    fn hand_calculated_loop_visits_rewards_and_unknown_liability() {
        let mut episodes: Vec<_> = (0..20)
            .map(|_| {
                episode(
                    &[State::Main, State::Retry, State::Retry, State::Completed],
                    Some(10),
                )
            })
            .collect();
        let report = aggregate("c".into(), cohort(), &episodes.iter().collect::<Vec<_>>()).unwrap();
        let ForecastStatus::Forecast {
            expected_visits,
            expected_cost_micros,
            outcome_probabilities,
            ..
        } = report.status
        else {
            panic!("forecast unavailable")
        };
        assert_eq!(expected_visits, vec![vec![1.0, 2.0], vec![0.0, 2.0]]);
        assert_eq!(expected_cost_micros, vec![Some(30.0), Some(20.0)]);
        assert_eq!(outcome_probabilities, vec![vec![1.0], vec![1.0]]);
        assert_eq!(report.known_cost_micros, 600);
        episodes[0] = episode(
            &[State::Main, State::Retry, State::Retry, State::Completed],
            None,
        );
        let report = aggregate("c".into(), cohort(), &episodes.iter().collect::<Vec<_>>()).unwrap();
        let ForecastStatus::Forecast {
            expected_cost_micros,
            ..
        } = report.status
        else {
            panic!("visits should remain available")
        };
        assert!(expected_cost_micros.iter().all(Option::is_none));
        assert_eq!(report.unknown_attempts, 3);
        episodes[0] = episode(
            &[State::Main, State::Retry, State::Retry, State::Completed],
            Some(10),
        );
        episodes[0].unknown_liabilities = 1;
        let report = aggregate("c".into(), cohort(), &episodes.iter().collect::<Vec<_>>()).unwrap();
        let ForecastStatus::Forecast {
            expected_cost_micros,
            ..
        } = report.status
        else {
            panic!("visits should remain available")
        };
        assert!(expected_cost_micros.iter().all(Option::is_none));
    }
    #[test]
    fn sparse_episode_and_row_gates_do_not_invent_support() {
        let mut episodes: Vec<_> = (0..19)
            .map(|_| episode(&[State::Main, State::Completed], Some(10)))
            .collect();
        assert!(matches!(
            aggregate("c".into(), cohort(), &episodes.iter().collect::<Vec<_>>())
                .unwrap()
                .status,
            ForecastStatus::Abstained { .. }
        ));
        episodes.push(episode(
            &[State::Main, State::Support, State::Failed],
            Some(10),
        ));
        assert!(matches!(
            aggregate("c".into(), cohort(), &episodes.iter().collect::<Vec<_>>())
                .unwrap()
                .status,
            ForecastStatus::Abstained { .. }
        ));
    }
}
