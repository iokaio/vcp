// SPDX-License-Identifier: Apache-2.0
//! Canonical routing publications and local, scoped optimization evidence.
pub mod advisory;
pub mod consumption;
pub mod cycles;
pub mod fits;
pub mod local_stall;
pub mod observations;
pub mod rewards;
pub mod transitions;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{
    accounting::*,
    task::{Task, TaskState},
    workspace::{Session, Workspace},
    *,
};
use vcp_memory::access::Access;
use vcp_models::routing::{CatalogRevision, Policy, Preference, Profile, RoutingDecision};
use vcp_protocol::{
    canonical_bytes, digest_bytes,
    event::{EventInput, EventKind},
};
use vcp_store::{
    contract::{key, CanonicalStore, Collection, Mutation, Record, Transaction},
    Store,
};

pub type Result<T> = std::result::Result<T, String>;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn authorize(store: &Store, access: &Access, write: bool) -> Result<()> {
    if !access.read || (write && !access.write) {
        return Err("routing access denied".into());
    }
    let workspace: Workspace = store
        .state()
        .record(
            Collection::Workspace,
            access.workspace.as_str(),
            &access.workspace,
        )
        .map_err(err)?
        .decode()
        .map_err(err)?;
    if workspace.authority != access.authority {
        return Err("routing authority changed".into());
    }
    Ok(())
}
fn global_write(store: &Store, access: &Access) -> Result<()> {
    authorize(store, access, true)?;
    if access.tasks.is_some() {
        return Err("routing policy requires workspace authority".into());
    }
    Ok(())
}
fn name(kind: &str, access: &Access) -> String {
    let digest = digest_bytes(format!("{kind}:{}", access.workspace).as_bytes());
    if kind.starts_with("command-") {
        format!("routing-command-{digest}")
    } else {
        format!("routing-{kind}-{}", &digest[..32])
    }
}
fn decision_name(attempt: &AttemptId) -> String {
    format!(
        "routing-decision-{}",
        digest_bytes(attempt.as_str().as_bytes())
    )
}
fn read<T: DeserializeOwned>(store: &Store, access: &Access, id: &str) -> Result<Option<T>> {
    authorize(store, access, false)?;
    store
        .state()
        .records
        .get(&key(Collection::Projection, id))
        .map(|r| {
            if r.workspace != access.workspace {
                return Err("routing scope mismatch".into());
            }
            decode_record(r)
        })
        .transpose()
}
fn row<T: Serialize>(access: &Access, id: String, revision: Revision, value: &T) -> Result<Record> {
    let payload = serde_json::to_value(value).map_err(err)?;
    let mut encoded = match payload {
        serde_json::Value::Object(mut object) => {
            if object.contains_key("routing_encoding") || object.contains_key("schema_version") {
                return Err("routing payload shadows storage envelope".into());
            }
            object.insert("routing_encoding".into(), serde_json::json!("object_v1"));
            object
        }
        scalar => serde_json::Map::from_iter([
            ("routing_encoding".into(), serde_json::json!("scalar_v1")),
            ("routing_scalar".into(), scalar),
        ]),
    };
    encoded.insert("schema_version".into(), serde_json::json!(1));
    Record::typed(
        Collection::Projection,
        id,
        access.workspace.clone(),
        revision,
        &encoded,
    )
    .map_err(err)
}
fn decode_record<T: DeserializeOwned>(record: &Record) -> Result<T> {
    let mut value = record.value.clone();
    let object = value
        .as_object_mut()
        .ok_or("routing storage envelope must be an object")?;
    if object.remove("schema_version") != Some(serde_json::json!(1)) {
        return Err("unsupported routing storage schema".into());
    }
    let encoding = object
        .remove("routing_encoding")
        .and_then(|v| v.as_str().map(str::to_owned));
    if encoding.as_deref() == Some("scalar_v1") && object.len() == 1 {
        return serde_json::from_value(
            object
                .remove("routing_scalar")
                .ok_or("routing scalar missing")?,
        )
        .map_err(err);
    }
    if encoding.as_deref() != Some("object_v1") {
        return Err("unsupported routing storage encoding".into());
    }
    serde_json::from_value(value).map_err(err)
}
async fn commit(
    store: &mut Store,
    access: &Access,
    mutations: Vec<Mutation>,
    command: CommandId,
    now: Timestamp,
) -> Result<()> {
    authorize(store, access, true)?;
    let session: Session = store
        .state()
        .records
        .values()
        .find(|r| r.collection == Collection::Session && r.workspace == access.workspace)
        .ok_or("routing publication requires retained session")?
        .decode()
        .map_err(err)?;
    // Payload stays behind service access checks; generic history must not expose
    // a saved report or project interview to a narrower task reader.
    let event = EventInput {
        id: EventId::new(),
        workspace: access.workspace.clone(),
        session: session.id,
        task: None,
        actor: access.actor.clone(),
        correlation: command,
        causation: None,
        timestamp: now,
        kind: EventKind::Diagnostic,
        artifacts: vec![],
        data: serde_json::json!({"version":1,"routing_publication":true}),
        metadata: None,
    };
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations,
            events: vec![event],
            command: None,
        })
        .await
        .map_err(err)?;
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryWindow {
    pub from: Option<Timestamp>,
    pub until: Timestamp,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    pub tasks: u64,
    pub completed: u64,
    pub failed: u64,
    pub cancelled: u64,
    /// Nonterminal tasks at the cutoff: abandonment is not inferred from silence.
    pub unfinished: u64,
    pub abandoned: Option<u64>,
    pub attempts: u64,
    pub retries: u64,
    pub child_tasks: u64,
    pub supporting_attempts: u64,
    pub known_spend_micros: BTreeMap<String, u64>,
    pub uncertain_attempts: u64,
    pub reserved_liability_micros: BTreeMap<String, u64>,
    pub pruned_tasks: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptimizationReport {
    pub id: String,
    pub workspace: WorkspaceId,
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
    pub cutoff: Watermark,
    pub window: HistoryWindow,
    pub counts: Counts,
    pub cohorts: BTreeMap<String, u64>,
    pub cohort_denominator: String,
    pub tasks: BTreeSet<TaskId>,
    pub evidence: Vec<EventId>,
    pub uncertainty: Vec<String>,
    pub source_tasks: Option<BTreeSet<TaskId>>,
    pub observed: ObservedMetrics,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedMetrics {
    pub pruned_events: u64,
    pub pruned_attempts: u64,
    pub objective_change_events: u64,
    pub verification_checks_passed: u64,
    pub verification_checks_failed: u64,
    pub verification_checks_not_run: u64,
    pub pruned_verifications: u64,
    /// Matched canonical submit/final-usage timestamps, including observation lag.
    /// Sorted samples; missing submissions/final observations are not zero latency.
    pub submission_to_final_usage_ms: Vec<u64>,
    pub attempts_without_complete_latency: u64,
}
/// Snapshot of the current canonical view, restricted to tasks with retained
/// events in the window. It never fabricates historical state from current rows.
pub fn report(store: &Store, access: &Access, window: HistoryWindow) -> Result<OptimizationReport> {
    authorize(store, access, false)?;
    if store.state().records.len() > 100_000 || store.state().events.len() > 100_000 {
        return Err("optimization history scan exceeds 100000 canonical rows/events".into());
    }
    if window.from.is_some_and(|from| from >= window.until) {
        return Err("invalid history window".into());
    }
    let workspace: Workspace = store
        .state()
        .record(
            Collection::Workspace,
            access.workspace.as_str(),
            &access.workspace,
        )
        .map_err(err)?
        .decode()
        .map_err(err)?;
    let events: Vec<_> = store
        .state()
        .events
        .iter()
        .filter(|e| {
            e.event.workspace == access.workspace
                && e.event.timestamp < window.until
                && window.from.is_none_or(|from| e.event.timestamp >= from)
                && e.event.task.as_ref().is_some_and(|t| access.allows_task(t))
        })
        .collect();
    let tasks: BTreeSet<_> = events.iter().filter_map(|e| e.event.task.clone()).collect();
    let mut counts = Counts::default();
    let mut observed = ObservedMetrics {
        pruned_events: events
            .iter()
            .filter(|event| event.redaction.is_some())
            .count() as u64,
        ..Default::default()
    };
    let mut cohorts = BTreeMap::new();
    for task_id in &tasks {
        let task: Task = store
            .state()
            .record(Collection::Task, task_id.as_str(), &access.workspace)
            .map_err(err)?
            .decode()
            .map_err(err)?;
        counts.tasks += 1;
        counts.child_tasks += u64::from(task.parent.is_some());
        counts.pruned_tasks += u64::from(task.redaction.is_some());
        match task.state {
            TaskState::Completed => counts.completed += 1,
            TaskState::Failed => counts.failed += 1,
            TaskState::Cancelled => counts.cancelled += 1,
            _ => counts.unfinished += 1,
        }
    }
    for record in store
        .state()
        .records
        .values()
        .filter(|r| r.workspace == access.workspace && r.collection == Collection::Attempt)
    {
        let attempt: Attempt = record.decode().map_err(err)?;
        if !tasks.contains(&attempt.scope.task) {
            continue;
        }
        counts.attempts += 1;
        observed.pruned_attempts +=
            u64::from(attempt.redaction.is_some() || attempt.redacted_at_revision.is_some());
        counts.retries += u64::from(attempt.previous.is_some());
        counts.supporting_attempts += u64::from(attempt.role != RequestRole::Main);
        counts.uncertain_attempts += u64::from(
            attempt.uncertain.is_some()
                || !matches!(
                    attempt.phase,
                    ReservationState::Settled | ReservationState::Released
                ),
        );
        let amount = counts
            .known_spend_micros
            .entry(attempt.quote.amount.currency.code().to_owned())
            .or_default();
        *amount = amount
            .checked_add(attempt.charged.get())
            .ok_or("report spend overflow")?;
        let routing = store
            .state()
            .records
            .get(&key(Collection::Projection, &decision_name(&attempt.id)))
            .filter(|r| r.workspace == access.workspace);
        let routing_policy = routing
            .and_then(|r| r.value["decision"]["input"]["policy"].as_str())
            .unwrap_or("unknown");
        let task_class = routing
            .and_then(|r| r.value["decision"]["input"]["task_class"].as_str())
            .unwrap_or("unknown");
        let cohort = serde_json::to_string(&serde_json::json!({
            "provider": attempt.quote.price.provider, "model": attempt.quote.price.model,
            "catalog": attempt.quote.price.id, "routing_policy": routing_policy,
            "authority_policy": attempt.admitted_policy, "task_class": task_class,
            "size": "unknown", "role": attempt.role
        }))
        .map_err(err)?;
        *cohorts.entry(cohort).or_insert(0) += 1;
    }
    for record in store
        .state()
        .records
        .values()
        .filter(|r| r.workspace == access.workspace && r.collection == Collection::Reservation)
    {
        let reservation: Reservation = record.decode().map_err(err)?;
        if tasks.contains(&reservation.scope.task) {
            let amount = counts
                .reserved_liability_micros
                .entry(reservation.amount.currency.code().to_owned())
                .or_default();
            *amount = amount
                .checked_add(reservation.liability.get())
                .ok_or("report liability overflow")?;
        }
    }
    let mut submitted = BTreeMap::<String, Timestamp>::new();
    let mut finished = BTreeMap::<String, Timestamp>::new();
    for event in store.state().events.iter().filter(|e| {
        e.event.workspace == access.workspace
            && e.redaction.is_none()
            && e.event.task.as_ref().is_some_and(|t| tasks.contains(t))
    }) {
        if event.event.kind == EventKind::ObjectiveChanged {
            observed.objective_change_events += 1;
        }
        let Some(id) = event.event.data["attempt"]["id"].as_str() else {
            continue;
        };
        if event.event.kind == EventKind::AttemptSubmitted {
            submitted
                .entry(id.to_owned())
                .or_insert(event.event.timestamp);
        }
        if event.event.kind == EventKind::UsageReconciled
            && event.event.data["attempt"]["phase"] == "settled"
        {
            finished
                .entry(id.to_owned())
                .or_insert(event.event.timestamp);
        }
    }
    for (id, from) in submitted {
        if let Some(until) = finished.get(&id) {
            if let Some(elapsed) = until.get().checked_sub(from.get()) {
                observed.submission_to_final_usage_ms.push(elapsed);
            }
        }
    }
    observed.submission_to_final_usage_ms.sort_unstable();
    observed.attempts_without_complete_latency = counts
        .attempts
        .saturating_sub(observed.submission_to_final_usage_ms.len() as u64);
    for record in store
        .state()
        .records
        .values()
        .filter(|r| r.workspace == access.workspace && r.collection == Collection::Verification)
    {
        let verification: vcp_domain::verification::Verification = record.decode().map_err(err)?;
        if !tasks.contains(&verification.scope.task) {
            continue;
        }
        if verification.redaction.is_some() {
            observed.pruned_verifications += 1;
            continue;
        }
        for check in verification.checks {
            match check.outcome {
                vcp_domain::verification::CheckOutcome::Passed => {
                    observed.verification_checks_passed += 1
                }
                vcp_domain::verification::CheckOutcome::Failed { .. } => {
                    observed.verification_checks_failed += 1
                }
                vcp_domain::verification::CheckOutcome::NotRun { .. } => {
                    observed.verification_checks_not_run += 1
                }
            }
        }
    }
    let mut uncertainty = vec!["Observed associations are not causal improvement.".into(), "Language, size, intervention, retrieval contribution and abandonment have no normalized retained observation; unavailable, not zero. Task class is unknown without a retained routing decision. Objective changes count observed steering events, not inferred human interventions.".into(), "Task outcomes and attempt totals reflect canonical cutoff, including activity outside the event selection window. Latency samples cover submission to final usage observation, not complete task wall time.".into()];
    if counts.tasks < 10 {
        uncertainty.push("Small sample: fewer than ten tasks.".into());
    }
    if counts.pruned_tasks > 0 || events.iter().any(|e| e.redaction.is_some()) {
        uncertainty.push("Pruned evidence reduces coverage.".into());
    }
    if counts.uncertain_attempts > 0 {
        uncertainty.push(
            "Unresolved usage is excluded from known spend, with liability shown separately."
                .into(),
        );
    }
    Ok(OptimizationReport {
        id: format!("routing-report-{}", CommandId::new()),
        workspace: access.workspace.clone(),
        authority: access.authority,
        deletion: workspace.deletion,
        cutoff: store.state().watermark,
        window,
        counts,
        cohorts,
        cohort_denominator: "attempts, including support and unsuccessful attempts".into(),
        tasks,
        evidence: events
            .iter()
            .filter(|e| e.redaction.is_none())
            .map(|e| e.event.id.clone())
            .collect(),
        uncertainty,
        source_tasks: access.tasks.clone(),
        observed,
    })
}
pub async fn save_report(
    store: &mut Store,
    access: &Access,
    window: HistoryWindow,
    now: Timestamp,
) -> Result<OptimizationReport> {
    let value = report(store, access, window)?;
    let record = row(access, value.id.clone(), Revision::ZERO, &value)?;
    commit(
        store,
        access,
        vec![Mutation::Put {
            record,
            expected: None,
        }],
        CommandId::new(),
        now,
    )
    .await?;
    Ok(value)
}
pub fn load_report(store: &Store, access: &Access, id: &str) -> Result<OptimizationReport> {
    let report: OptimizationReport = read(store, access, id)?.ok_or("report not found")?;
    let workspace: Workspace = store
        .state()
        .record(
            Collection::Workspace,
            access.workspace.as_str(),
            &access.workspace,
        )
        .map_err(err)?
        .decode()
        .map_err(err)?;
    if report.workspace != access.workspace
        || report.authority != access.authority
        || report.deletion != workspace.deletion
        || report.tasks.iter().any(|t| !access.allows_task(t))
        || report
            .source_tasks
            .as_ref()
            .is_some_and(|tasks| tasks.iter().any(|t| !access.allows_task(t)))
    {
        return Err("report evidence access changed; refresh report".into());
    }
    for id in &report.evidence {
        if !store.state().events.iter().any(|e| {
            &e.event.id == id
                && e.event.workspace == access.workspace
                && e.redaction.is_none()
                && e.event.task.as_ref().is_none_or(|t| access.allows_task(t))
        }) {
            return Err("report evidence unavailable; refresh report".into());
        }
    }
    Ok(report)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Increased,
    Decreased,
    Unchanged,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricComparison {
    pub metric: String,
    pub baseline_numerator: u64,
    pub baseline_denominator: u64,
    pub current_numerator: u64,
    pub current_denominator: u64,
    pub direction: Direction,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegressionComparison {
    pub baseline: String,
    pub current: String,
    pub comparable: bool,
    pub baseline_cutoff: Watermark,
    pub current_cutoff: Watermark,
    pub baseline_counts: Counts,
    pub current_counts: Counts,
    pub baseline_policy_revisions: BTreeSet<String>,
    pub current_policy_revisions: BTreeSet<String>,
    pub metrics: Vec<MetricComparison>,
    pub reasons: Vec<String>,
    pub caveats: Vec<String>,
    pub recommendation: String,
    pub automatic_action: bool,
}
fn rate_comparison(
    metric: String,
    before: u64,
    before_count: u64,
    after: u64,
    after_count: u64,
) -> MetricComparison {
    let direction = match (u128::from(before) * u128::from(after_count))
        .cmp(&(u128::from(after) * u128::from(before_count)))
    {
        std::cmp::Ordering::Less => Direction::Increased,
        std::cmp::Ordering::Greater => Direction::Decreased,
        std::cmp::Ordering::Equal => Direction::Unchanged,
    };
    MetricComparison {
        metric,
        baseline_numerator: before,
        baseline_denominator: before_count,
        current_numerator: after,
        current_denominator: after_count,
        direction,
    }
}
fn comparison_cohorts(
    report: &OptimizationReport,
) -> Result<(BTreeMap<String, u64>, BTreeSet<String>)> {
    let mut cohorts = BTreeMap::new();
    let mut policies = BTreeSet::new();
    for (cohort, count) in &report.cohorts {
        let mut value: serde_json::Value = serde_json::from_str(cohort).map_err(err)?;
        let object = value.as_object_mut().ok_or("invalid saved cohort schema")?;
        if let Some(policy) = object.remove("routing_policy") {
            policies.insert(
                policy
                    .as_str()
                    .ok_or("invalid saved cohort policy")?
                    .to_owned(),
            );
        }
        let key = serde_json::to_string(&value).map_err(err)?;
        let total = cohorts.entry(key).or_insert(0u64);
        *total = total.checked_add(*count).ok_or("cohort sample overflow")?;
    }
    Ok((cohorts, policies))
}
/// Read-only comparison. The sole allowed cohort dimension change is routing
/// policy; every provider/model/catalog/class/size/role dimension remains visible.
/// Evidence loss or unlike source bounds yields an explicit non-comparison.
pub fn compare_reports(
    store: &Store,
    access: &Access,
    baseline: &str,
    current: &str,
) -> Result<RegressionComparison> {
    let baseline = load_report(store, access, baseline)?;
    let current = load_report(store, access, current)?;
    let (before_cohorts, baseline_policy_revisions) = comparison_cohorts(&baseline)?;
    let (after_cohorts, current_policy_revisions) = comparison_cohorts(&current)?;
    let mut reasons = Vec::new();
    if baseline.id == current.id || baseline.cutoff >= current.cutoff {
        reasons.push(
            "Comparison requires two distinct reports in increasing canonical cutoff order.".into(),
        );
    }
    if baseline.source_tasks != current.source_tasks {
        reasons.push("Authorized source task selectors differ.".into());
    }
    let duration = |w: &HistoryWindow| w.from.map(|from| w.until.get().saturating_sub(from.get()));
    if baseline.window != current.window
        && (duration(&baseline.window).is_none()
            || duration(&baseline.window) != duration(&current.window))
    {
        reasons.push("History source windows have unlike or unspecified bounds.".into());
    }
    if baseline.counts.tasks == 0
        || current.counts.tasks == 0
        || before_cohorts.is_empty()
        || after_cohorts.is_empty()
    {
        reasons.push("Empty task or declared attempt cohort prevents comparison.".into());
    }
    if before_cohorts.keys().ne(after_cohorts.keys()) {
        reasons.push(
            "Provider, model, catalog, task class, size, authority or role cohorts differ.".into(),
        );
    } else if before_cohorts.iter().any(|(key, before)| {
        u128::from(*before) * u128::from(current.counts.attempts)
            != u128::from(*after_cohorts.get(key).unwrap_or(&0))
                * u128::from(baseline.counts.attempts)
    }) {
        reasons.push(
            "Attempt cohort proportions differ; aggregate changes would mix unlike samples.".into(),
        );
    }
    if baseline.counts.pruned_tasks > 0
        || current.counts.pruned_tasks > 0
        || baseline.observed.pruned_events > 0
        || current.observed.pruned_events > 0
        || baseline.observed.pruned_attempts > 0
        || current.observed.pruned_attempts > 0
        || baseline.observed.pruned_verifications > 0
        || current.observed.pruned_verifications > 0
    {
        reasons.push("Pruned evidence prevents a complete comparison.".into());
    }
    let comparable = reasons.is_empty();
    let mut caveats = vec!["Counts and denominators are observations, not a causal policy effect or retraining result.".into(), "Unknown task size/language, incomplete outcomes and different tasks can confound even matched recorded cohorts. Verification checks are observed quality signals, not a complete quality grade.".into(), "Costs include failed attempts, support, optimization and children; latency remains submission-to-usage observation lag. No trials, policy changes or rollback are authorized by this comparison.".into()];
    if baseline.counts.tasks < 10 || current.counts.tasks < 10 {
        caveats.push("Small samples: retain uncertainty rather than assert improvement.".into());
    }
    if !baseline.tasks.is_disjoint(&current.tasks) {
        caveats.push("Reports share tasks; these are correlated follow-up observations, not independent trials.".into());
    }
    let mut metrics = Vec::new();
    if comparable {
        metrics.push(rate_comparison(
            "failed_tasks_per_selected_task".into(),
            baseline.counts.failed,
            baseline.counts.tasks,
            current.counts.failed,
            current.counts.tasks,
        ));
        metrics.push(rate_comparison(
            "cancelled_tasks_per_selected_task".into(),
            baseline.counts.cancelled,
            baseline.counts.tasks,
            current.counts.cancelled,
            current.counts.tasks,
        ));
        let before_checks = baseline.observed.verification_checks_passed
            + baseline.observed.verification_checks_failed;
        let after_checks = current.observed.verification_checks_passed
            + current.observed.verification_checks_failed;
        if before_checks > 0 && after_checks > 0 {
            metrics.push(rate_comparison(
                "failed_checks_per_executed_check".into(),
                baseline.observed.verification_checks_failed,
                before_checks,
                current.observed.verification_checks_failed,
                after_checks,
            ));
        }
        if baseline.counts.uncertain_attempts > 0 || current.counts.uncertain_attempts > 0 {
            caveats.push(
                "Unknown charges: no spend regression or saving direction is computed.".into(),
            );
        } else if baseline
            .counts
            .known_spend_micros
            .keys()
            .eq(current.counts.known_spend_micros.keys())
        {
            for (currency, before) in &baseline.counts.known_spend_micros {
                if let Some(after) = current.counts.known_spend_micros.get(currency) {
                    metrics.push(rate_comparison(
                        format!("known_{currency}_micros_per_selected_task"),
                        *before,
                        baseline.counts.tasks,
                        *after,
                        current.counts.tasks,
                    ));
                }
            }
        } else {
            caveats.push("Currency sets differ; spend comparisons are unavailable.".into());
        }
    }
    let recommendation = if !comparable { "Collect comparable retained evidence before judging regression." }
        else if metrics.iter().any(|m| m.direction == Direction::Increased) { "Review observed regressions and consider a separate, explicit policy preview or rollback; causation is unproven." }
        else { "No increase in compared observed metrics; retain monitoring and uncertainty rather than claim causal improvement." }.into();
    Ok(RegressionComparison {
        baseline: baseline.id,
        current: current.id,
        comparable,
        baseline_cutoff: baseline.cutoff,
        current_cutoff: current.cutoff,
        baseline_counts: baseline.counts,
        current_counts: current.counts,
        baseline_policy_revisions,
        current_policy_revisions,
        metrics,
        reasons,
        caveats,
        recommendation,
        automatic_action: false,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Published<T> {
    pub revision: Revision,
    pub parent: Option<Revision>,
    pub actor: ActorId,
    pub authority: AuthorityRevision,
    pub timestamp: Timestamp,
    pub value: T,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registry {
    pub catalog: CatalogRevision,
    pub raw: ArtifactId,
    pub raw_sha256: String,
}
fn revision_name(kind: &str, access: &Access, revision: Revision) -> String {
    format!("{}-{}", name(kind, access), revision.get())
}
pub fn current_registry(store: &Store, access: &Access) -> Result<Option<Published<Registry>>> {
    let registry: Option<Published<Registry>> = current(store, access, "registry")?;
    if let Some(value) = &registry {
        let artifact: vcp_domain::artifact::ArtifactDescriptor = store
            .state()
            .record(
                Collection::Artifact,
                value.value.raw.as_str(),
                &access.workspace,
            )
            .map_err(err)?
            .decode()
            .map_err(err)?;
        if !access.allows_task(&artifact.spec.scope.task) {
            return Err("catalog evidence scope denied".into());
        }
        value.value.catalog.validate().map_err(err)?;
        if artifact.sha256 != value.value.raw_sha256 {
            return Err("catalog evidence digest changed".into());
        }
    }
    Ok(registry)
}
pub fn current_policy(store: &Store, access: &Access) -> Result<Option<Published<Policy>>> {
    let policy: Option<Published<Policy>> = current(store, access, "policy")?;
    if let Some(value) = &policy {
        value.value.validate().map_err(err)?;
    }
    Ok(policy)
}
fn current<T: DeserializeOwned>(
    store: &Store,
    access: &Access,
    kind: &str,
) -> Result<Option<Published<T>>> {
    let Some(revision) = read::<Revision>(store, access, &name(kind, access))? else {
        return Ok(None);
    };
    read(store, access, &revision_name(kind, access, revision))?
        .ok_or_else(|| "routing head has no revision".into())
        .map(Some)
}
fn publication<T: Serialize>(
    store: &Store,
    access: &Access,
    kind: &str,
    expected: Option<Revision>,
    value: T,
    now: Timestamp,
) -> Result<(Published<T>, Vec<Mutation>)> {
    global_write(store, access)?;
    let current: Option<Revision> = read(store, access, &name(kind, access))?;
    if current != expected {
        return Err("routing revision changed; refresh preview".into());
    }
    let revision = expected.map_or(Ok(Revision::ZERO), |r| r.next().map_err(err))?;
    let published = Published {
        revision,
        parent: expected,
        actor: access.actor.clone(),
        authority: access.authority,
        timestamp: now,
        value,
    };
    let version = row(
        access,
        revision_name(kind, access, revision),
        Revision::ZERO,
        &published,
    )?;
    let head = row(access, name(kind, access), revision, &revision)?;
    Ok((
        published,
        vec![
            Mutation::Put {
                record: version,
                expected: None,
            },
            Mutation::Put {
                record: head,
                expected,
            },
        ],
    ))
}
pub async fn publish_registry(
    store: &mut Store,
    access: &Access,
    expected: Option<Revision>,
    catalog: CatalogRevision,
    raw: ArtifactId,
    now: Timestamp,
) -> Result<Published<Registry>> {
    catalog.validate().map_err(err)?;
    let artifact: vcp_domain::artifact::ArtifactDescriptor = store
        .state()
        .record(Collection::Artifact, raw.as_str(), &access.workspace)
        .map_err(err)?
        .decode()
        .map_err(err)?;
    if !access.allows_task(&artifact.spec.scope.task)
        || artifact.state != vcp_domain::artifact::CaptureState::Complete
    {
        return Err("catalog raw evidence unavailable".into());
    }
    let parent = current_registry(store, access)?.map(|r| r.value.catalog.id);
    if catalog.parent != parent {
        return Err("catalog parent changed".into());
    }
    let (published, mut mutations) = publication(
        store,
        access,
        "registry",
        expected,
        Registry {
            catalog,
            raw: raw.clone(),
            raw_sha256: artifact.sha256,
        },
        now,
    )?;
    if let Some(Mutation::Put { record, .. }) = mutations.first_mut() {
        record
            .references
            .insert(key(Collection::Artifact, raw.as_str()));
    }
    commit(store, access, mutations, CommandId::new(), now).await?;
    Ok(published)
}
/// A separately authorized initial configuration. Optimizer edits cannot create
/// policy, expand provider allowlists, enable evaluators or modify budgets.
pub async fn initialize_policy(
    store: &mut Store,
    access: &Access,
    policy: Policy,
    now: Timestamp,
) -> Result<Published<Policy>> {
    policy.validate().map_err(err)?;
    if policy.parent.is_some() {
        return Err("initial policy must have no parent".into());
    }
    let (published, mutations) = publication(store, access, "policy", None, policy, now)?;
    commit(store, access, mutations, CommandId::new(), now).await?;
    Ok(published)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "field",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Edit {
    Profile(Profile),
    Ordering(Vec<Preference>),
    QualityFloorBps(u16),
    MinimumSamples(u32),
    MaximumEvidenceAgeMs(u64),
}
impl Edit {
    fn key(&self) -> &'static str {
        match self {
            Self::Profile(_) => "profile",
            Self::Ordering(_) => "ordering",
            Self::QualityFloorBps(_) => "quality_floor_bps",
            Self::MinimumSamples(_) => "minimum_samples",
            Self::MaximumEvidenceAgeMs(_) => "maximum_evidence_age_ms",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preview {
    pub base: Revision,
    pub report: String,
    pub selected: Vec<Edit>,
    pub prior: Policy,
    pub persisted: Policy,
    pub effective: Policy,
    pub ceilings_digest: String,
    pub reason: String,
    pub uncertainty: Vec<String>,
    pub interview: Interview,
}
pub fn effective_policy(mut policy: Policy, ceilings: &Policy) -> Result<Policy> {
    policy.validate().map_err(err)?;
    ceilings.validate().map_err(err)?;
    policy.allowed_models = policy
        .allowed_models
        .intersection(&ceilings.allowed_models)
        .cloned()
        .collect();
    policy.allowed_endpoints = policy
        .allowed_endpoints
        .intersection(&ceilings.allowed_endpoints)
        .cloned()
        .collect();
    policy.allowed_groups = policy
        .allowed_groups
        .intersection(&ceilings.allowed_groups)
        .copied()
        .collect();
    policy.quality_floor_bps = policy.quality_floor_bps.max(ceilings.quality_floor_bps);
    policy.minimum_samples = policy.minimum_samples.max(ceilings.minimum_samples);
    policy.maximum_evidence_age_ms = policy
        .maximum_evidence_age_ms
        .min(ceilings.maximum_evidence_age_ms);
    policy.deny_data_collection |= ceilings.deny_data_collection;
    policy.require_zdr |= ceilings.require_zdr;
    if policy.broader_task_class != ceilings.broader_task_class {
        policy.broader_task_class = None;
    }
    if let Some(ceiling_pin) = &ceilings.pin {
        let mut allowed = ceiling_pin.fallback_candidates.clone();
        allowed.insert(ceiling_pin.candidate.clone());
        if let Some(pin) = &policy.pin {
            let mut selected = pin.fallback_candidates.clone();
            selected.insert(pin.candidate.clone());
            allowed = allowed.intersection(&selected).cloned().collect();
        }
        if !allowed.contains(&ceiling_pin.candidate) {
            policy.allowed_models.clear();
            policy.pin = Some(ceiling_pin.clone());
        } else {
            let preferred = ceiling_pin.candidate.clone();
            allowed.remove(&preferred);
            policy.pin = Some(vcp_models::routing::Pin {
                candidate: preferred,
                fallback_candidates: allowed,
            });
        }
    }
    policy.seal().map_err(err)
}
pub fn preview(
    store: &Store,
    access: &Access,
    report_id: &str,
    selected: Vec<Edit>,
    ceilings: &Policy,
) -> Result<Preview> {
    global_write(store, access)?;
    let report = load_report(store, access, report_id)?;
    let current = current_policy(store, access)?.ok_or("routing policy not configured")?;
    let mut policy = current.value.clone();
    let mut fields = BTreeSet::new();
    for edit in &selected {
        if !fields.insert(edit.key()) {
            return Err("duplicate proposal field".into());
        }
        match edit {
            Edit::Profile(value) => policy.profile = *value,
            Edit::Ordering(value) => policy.ordering = value.clone(),
            Edit::QualityFloorBps(value) => policy.quality_floor_bps = *value,
            Edit::MinimumSamples(value) => policy.minimum_samples = *value,
            Edit::MaximumEvidenceAgeMs(value) => policy.maximum_evidence_age_ms = *value,
        }
    }
    policy.parent = Some(current.value.id.clone());
    policy = policy.seal().map_err(err)?;
    Ok(Preview {
        base: current.revision,
        report: report.id,
        selected,
        prior: current.value,
        effective: effective_policy(policy.clone(), ceilings)?,
        persisted: policy,
        ceilings_digest: ceilings.digest().map_err(err)?,
        reason: "Explicit developer preferences; no causal improvement inferred.".into(),
        uncertainty: report.uncertainty,
        interview: interview(store, access)?,
    })
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyReceipt {
    pub command: CommandId,
    pub digest: String,
    pub published: Published<Policy>,
    pub effective: Policy,
}
pub async fn apply(
    store: &mut Store,
    access: &Access,
    command: CommandId,
    proposal: &Preview,
    ceilings: &Policy,
    now: Timestamp,
) -> Result<ApplyReceipt> {
    global_write(store, access)?;
    let digest = digest_bytes(&canonical_bytes(proposal).map_err(err)?);
    let receipt_id = name(&format!("command-{command}"), access);
    if let Some(receipt) = read::<ApplyReceipt>(store, access, &receipt_id)? {
        if receipt.digest != digest {
            return Err("command identity reused with different proposal".into());
        }
        return Ok(receipt);
    }
    if proposal.selected.is_empty() {
        return Err("no selected changes; policy unchanged".into());
    }
    let checked = preview(
        store,
        access,
        &proposal.report,
        proposal.selected.clone(),
        ceilings,
    )?;
    if &checked != proposal {
        return Err("preview changed; refresh before applying".into());
    }
    let (published, mut mutations) = publication(
        store,
        access,
        "policy",
        Some(proposal.base),
        proposal.persisted.clone(),
        now,
    )?;
    let receipt = ApplyReceipt {
        command: command.clone(),
        digest,
        published,
        effective: proposal.effective.clone(),
    };
    mutations.push(Mutation::Put {
        record: row(access, receipt_id, Revision::ZERO, &receipt)?,
        expected: None,
    });
    commit(store, access, mutations, command, now).await?;
    Ok(receipt)
}
pub async fn rollback(
    store: &mut Store,
    access: &Access,
    command: CommandId,
    expected: Revision,
    target: Revision,
    ceilings: &Policy,
    now: Timestamp,
) -> Result<ApplyReceipt> {
    global_write(store, access)?;
    let digest = digest_bytes(&canonical_bytes(&("rollback", expected, target)).map_err(err)?);
    let receipt_id = name(&format!("command-{command}"), access);
    if let Some(receipt) = read::<ApplyReceipt>(store, access, &receipt_id)? {
        if receipt.digest != digest {
            return Err("command identity reused".into());
        }
        return Ok(receipt);
    }
    let prior: Published<Policy> = read(store, access, &revision_name("policy", access, target))?
        .ok_or("rollback target unavailable")?;
    let current = current_policy(store, access)?.ok_or("policy unavailable")?;
    if target >= expected {
        return Err("rollback target must be a predecessor".into());
    }
    let mut restored = prior.value;
    restored.parent = Some(current.value.id);
    restored = restored.seal().map_err(err)?;
    let effective = effective_policy(restored.clone(), ceilings)?;
    let (published, mut mutations) =
        publication(store, access, "policy", Some(expected), restored, now)?;
    let receipt = ApplyReceipt {
        command: command.clone(),
        digest,
        published,
        effective,
    };
    mutations.push(Mutation::Put {
        record: row(access, receipt_id, Revision::ZERO, &receipt)?,
        expected: None,
    });
    commit(store, access, mutations, command, now).await?;
    Ok(receipt)
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Question {
    Priority,
    ExpectedSize,
    ReviewPreference,
    ModelRestrictions,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interview {
    pub version: u32,
    pub revision: Revision,
    pub answers: BTreeMap<Question, String>,
}
pub fn interview(store: &Store, access: &Access) -> Result<Interview> {
    // Project answers can contain restrictions/preferences for all tasks.
    authorize(store, access, false)?;
    if access.tasks.is_some() {
        return Err("project preferences require workspace access".into());
    }
    Ok(
        read(store, access, &name("interview", access))?.unwrap_or(Interview {
            version: 1,
            revision: Revision::ZERO,
            answers: BTreeMap::new(),
        }),
    )
}
pub fn next_question(interview: &Interview) -> Option<Question> {
    [
        Question::Priority,
        Question::ExpectedSize,
        Question::ReviewPreference,
        Question::ModelRestrictions,
    ]
    .into_iter()
    .find(|q| !interview.answers.contains_key(q))
}
/// Ask only about a decision supported by this report's gaps. Answers persist
/// across reports; optional questions never gate a manually selected policy edit.
pub fn next_question_for_report(
    interview: &Interview,
    report: &OptimizationReport,
) -> Option<Question> {
    let relevant = [
        (Question::Priority, true),
        (
            Question::ModelRestrictions,
            report.counts.uncertain_attempts > 0,
        ),
        (
            Question::ReviewPreference,
            report.counts.failed + report.counts.cancelled > 0,
        ),
        (
            Question::ExpectedSize,
            report.counts.tasks > 0
                && report
                    .cohorts
                    .keys()
                    .any(|key| key.contains("\"size\":\"unknown\"")),
        ),
    ];
    relevant.into_iter().find_map(|(question, needed)| {
        (needed && !interview.answers.contains_key(&question)).then_some(question)
    })
}
pub async fn answer(
    store: &mut Store,
    access: &Access,
    expected: Option<Revision>,
    question: Question,
    value: String,
    now: Timestamp,
) -> Result<Interview> {
    global_write(store, access)?;
    if value.trim().is_empty() || value.len() > 2048 || value.contains('\0') {
        return Err("invalid interview answer".into());
    }
    let id = name("interview", access);
    let previous: Option<Interview> = read(store, access, &id)?;
    if previous.as_ref().map(|v| v.revision) != expected {
        return Err("interview changed".into());
    }
    let mut interview = previous.unwrap_or(Interview {
        version: 1,
        revision: Revision::ZERO,
        answers: BTreeMap::new(),
    });
    interview.revision = expected.map_or(Ok(Revision::ZERO), |r| r.next().map_err(err))?;
    interview.answers.insert(question, value);
    commit(
        store,
        access,
        vec![Mutation::Put {
            record: row(access, id, interview.revision, &interview)?,
            expected,
        }],
        CommandId::new(),
        now,
    )
    .await?;
    Ok(interview)
}
pub async fn record_decision(
    store: &mut Store,
    access: &Access,
    scope: &vcp_domain::workspace::Scope,
    decision: &RoutingDecision,
    request_digest: &str,
    attempt: AttemptId,
    now: Timestamp,
) -> Result<()> {
    authorize(store, access, true)?;
    decision.validate().map_err(err)?;
    if scope.workspace != access.workspace
        || !access.allows_task(&scope.task)
        || decision.input.workspace != scope.workspace
        || decision.input.task != scope.task
        || request_digest.len() != 64
        || !request_digest.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("routing decision scope or request digest invalid".into());
    }
    let admitted: Attempt = store
        .state()
        .record(Collection::Attempt, attempt.as_str(), &access.workspace)
        .map_err(err)?
        .decode()
        .map_err(err)?;
    let selected = decision
        .selected
        .as_ref()
        .ok_or("cannot record an unselected routing attempt")?;
    if admitted.scope != *scope
        || admitted.root != decision.input.root
        || admitted.request_digest != request_digest
        || admitted.quote.price.model != selected.model
        || admitted.quote.price.provider != selected.endpoint
    {
        return Err("routing decision does not match admitted request".into());
    }
    let id = decision_name(&attempt);
    let value = serde_json::json!({"document_type":"vcp_routing_decision_v1","scope":scope,"task":scope.task,"root":decision.input.root,"decision":decision,"request_digest":request_digest,"attempt":attempt});
    if let Some(existing) = read::<serde_json::Value>(store, access, &id)? {
        return if existing == value {
            Ok(())
        } else {
            Err("routing attempt decision changed".into())
        };
    }
    let mut record = row(access, id, Revision::ZERO, &value)?;
    record
        .references
        .insert(key(Collection::Attempt, attempt.as_str()));
    record
        .references
        .insert(key(Collection::Task, scope.task.as_str()));
    commit(
        store,
        access,
        vec![Mutation::Put {
            record,
            expected: None,
        }],
        CommandId::new(),
        now,
    )
    .await
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EscalationAdmission {
    pub document_type: String,
    pub scope: vcp_domain::workspace::Scope,
    pub root: TaskId,
    pub attempt: AttemptId,
    pub admitted_at: Timestamp,
    pub plan: vcp_models::escalation::Plan,
    pub handoff: vcp_models::escalation::Handoff,
}
/// Root counters span support/child work; callers filter `scope.task` separately
/// when constructing a task's excluded model chain. IDs never define ordering.
pub fn admitted_escalations(
    store: &Store,
    access: &Access,
    scope: &vcp_domain::workspace::Scope,
) -> Result<Vec<EscalationAdmission>> {
    authorize(store, access, false)?;
    if scope.workspace != access.workspace || !access.allows_task(&scope.task) {
        return Err("escalation scope denied".into());
    }
    let task: Task = store
        .state()
        .record(Collection::Task, scope.task.as_str(), &access.workspace)
        .map_err(err)?
        .decode()
        .map_err(err)?;
    if task.scope != *scope {
        return Err("escalation session differs".into());
    }
    let mut records = Vec::new();
    for record in store.state().records.values().filter(|r| {
        r.collection == Collection::Projection
            && r.workspace == access.workspace
            && r.value["document_type"] == "vcp_escalation_admission_v1"
    }) {
        let admitted: EscalationAdmission = decode_record(record)?;
        if admitted.root != task.root {
            continue;
        }
        if admitted.scope.workspace != access.workspace || !access.allows_task(&admitted.scope.task)
        {
            return Err("root escalation history requires access to every included task".into());
        }
        records.push(admitted);
    }
    records.sort_by_key(|r| r.plan.after.total_attempts);
    if records
        .windows(2)
        .any(|pair| pair[0].plan.after.total_attempts == pair[1].plan.after.total_attempts)
    {
        return Err("duplicate root escalation sequence".into());
    }
    Ok(records)
}
pub async fn record_escalation(
    store: &mut Store,
    access: &Access,
    scope: &vcp_domain::workspace::Scope,
    plan: &vcp_models::escalation::Plan,
    handoff: &vcp_models::escalation::Handoff,
    attempt: AttemptId,
    now: Timestamp,
) -> Result<EscalationAdmission> {
    use vcp_models::escalation::{Class, Counters};
    authorize(store, access, true)?;
    if scope.workspace != access.workspace
        || !access.allows_task(&scope.task)
        || plan.revisions.scope != *scope
        || plan.revisions.authority != access.authority
    {
        return Err("escalation scope or authority mismatch".into());
    }
    let id = format!(
        "routing-escalation-{}",
        digest_bytes(attempt.as_str().as_bytes())
    );
    if let Some(existing) = read::<EscalationAdmission>(store, access, &id)? {
        if existing.scope != *scope
            || existing.attempt != attempt
            || existing.plan != *plan
            || existing.handoff != *handoff
        {
            return Err("escalation attempt reused with different evidence".into());
        }
        return Ok(existing);
    }
    let admitted: Attempt = store
        .state()
        .record(Collection::Attempt, attempt.as_str(), &access.workspace)
        .map_err(err)?
        .decode()
        .map_err(err)?;
    let previous: Attempt = store
        .state()
        .record(
            Collection::Attempt,
            plan.previous_attempt.as_str(),
            &access.workspace,
        )
        .map_err(err)?
        .decode()
        .map_err(err)?;
    let ledger: Ledger = store
        .state()
        .record(
            Collection::Ledger,
            admitted.root.as_str(),
            &access.workspace,
        )
        .map_err(err)?
        .decode()
        .map_err(err)?;
    if ledger.revision != plan.ledger_revision.next().map_err(err)?
        || ledger.unresolved != plan.unresolved
        || ledger.policy != admitted.admitted_policy
    {
        return Err("escalation ledger changed outside its exact admission reservation".into());
    }
    let task: Task = store
        .state()
        .record(Collection::Task, scope.task.as_str(), &access.workspace)
        .map_err(err)?
        .decode()
        .map_err(err)?;
    let workspace: Workspace = store
        .state()
        .record(
            Collection::Workspace,
            access.workspace.as_str(),
            &access.workspace,
        )
        .map_err(err)?
        .decode()
        .map_err(err)?;
    if task.state != TaskState::Running
        || task.steering != plan.revisions.steering
        || task.revision != plan.revisions.task_state
        || workspace.deletion != plan.revisions.deletion
        || workspace.binding.revision != plan.revisions.binding
    {
        return Err("escalation scheduling or source revision changed".into());
    }
    if (plan.trigger.class() == Class::QualitySwitch && plan.previous == plan.selected)
        || (plan.trigger.class() == Class::TransportRetry
            && (plan.previous != plan.selected
                || admitted.previous.as_ref() != Some(&plan.previous_attempt)))
    {
        return Err("escalation class does not match model/predecessor transition".into());
    }
    if admitted.scope != *scope
        || previous.scope != *scope
        || admitted.root != previous.root
        || admitted.phase != ReservationState::Created
        || attempt == plan.previous_attempt
        || admitted.request_digest != handoff.destination_request_sha256
        || admitted.quote.price.model != plan.selected.model
        || admitted.quote.price.provider != plan.selected.endpoint
        || admitted.quote.price.id != plan.selected_snapshot
        || previous.quote.price.model != plan.previous.model
        || previous.quote.price.provider != plan.previous.endpoint
        || handoff.previous_attempt != plan.previous_attempt
        || handoff.routing_decision != plan.routing_decision
        || now >= plan.policy.deadline
    {
        return Err("escalation does not match admitted request and predecessor".into());
    }
    let decision: serde_json::Value = read(store, access, &decision_name(&attempt))?
        .ok_or("escalation requires retained routing decision")?;
    if decision["decision"]["id"].as_str() != Some(plan.routing_decision.as_str())
        || decision["decision"]["input"]["policy"].as_str() != Some(plan.routing_policy.as_str())
    {
        return Err("escalation routing decision mismatch".into());
    }
    for digest in [
        &handoff.packet_sha256,
        &handoff.destination_manifest_sha256,
        &handoff.destination_request_sha256,
    ] {
        if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("invalid escalation handoff digest".into());
        }
    }
    let mut expected = plan.before.clone();
    expected.total_attempts = expected
        .total_attempts
        .checked_add(1)
        .ok_or("escalation count overflow")?;
    let counter = match plan.trigger.class() {
        Class::TransportRetry => &mut expected.transport_retries,
        Class::QualitySwitch => &mut expected.quality_switches,
        Class::Decomposition => &mut expected.decompositions,
    };
    *counter = counter
        .checked_add(1)
        .ok_or("escalation class count overflow")?;
    if expected != plan.after
        || plan.after.total_attempts > plan.policy.max_total_attempts
        || plan.after.transport_retries > plan.policy.max_transport_retries
        || plan.after.quality_switches > plan.policy.max_quality_switches
        || plan.after.decompositions > plan.policy.max_decompositions
    {
        return Err("escalation counters exceed selected bounds".into());
    }
    let root_attempts = store
        .state()
        .records
        .values()
        .filter(|r| r.workspace == access.workspace && r.collection == Collection::Attempt)
        .map(|r| r.decode::<Attempt>().map_err(err))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .filter(|a| a.root == admitted.root)
        .collect::<Vec<_>>();
    if root_attempts.len() != plan.after.total_attempts as usize {
        return Err("root attempt count changed before escalation publication".into());
    }
    let prior = admitted_escalations(store, access, scope)?;
    let mut prior_counters = prior
        .last()
        .map(|r| r.plan.after.clone())
        .unwrap_or_default();
    let non_transport: BTreeSet<_> = prior
        .iter()
        .filter(|r| r.plan.trigger.class() != Class::TransportRetry)
        .map(|r| &r.attempt)
        .collect();
    prior_counters.transport_retries = u32::try_from(
        root_attempts
            .iter()
            .filter(|a| a.id != attempt && a.previous.is_some() && !non_transport.contains(&a.id))
            .count(),
    )
    .map_err(|_| "root retry count overflow")?;
    let classes = |c: &Counters| (c.transport_retries, c.quality_switches, c.decompositions);
    if classes(&plan.before) != classes(&prior_counters)
        || prior
            .last()
            .is_some_and(|r| r.plan.after.total_attempts >= plan.after.total_attempts)
    {
        return Err("root escalation counter chain changed".into());
    }
    let mut references = BTreeSet::from([
        key(Collection::Attempt, attempt.as_str()),
        key(Collection::Attempt, plan.previous_attempt.as_str()),
        key(Collection::Task, scope.task.as_str()),
    ]);
    if plan.trigger.evidence.is_empty()
        || plan.trigger.evidence.len() > 64
        || handoff.original_artifacts.len() > 4096
    {
        return Err("escalation evidence bounds".into());
    }
    for artifact in plan
        .trigger
        .evidence
        .iter()
        .chain(&handoff.original_artifacts)
    {
        let descriptor: vcp_domain::artifact::ArtifactDescriptor = store
            .state()
            .record(Collection::Artifact, artifact.as_str(), &access.workspace)
            .map_err(err)?
            .decode()
            .map_err(err)?;
        if !access.allows_task(&descriptor.spec.scope.task)
            || descriptor.state == vcp_domain::artifact::CaptureState::Purged
        {
            return Err("escalation evidence inaccessible".into());
        }
        references.insert(key(Collection::Artifact, artifact.as_str()));
    }
    let result = EscalationAdmission {
        document_type: "vcp_escalation_admission_v1".into(),
        scope: scope.clone(),
        root: admitted.root,
        attempt,
        admitted_at: now,
        plan: plan.clone(),
        handoff: handoff.clone(),
    };
    let mut record = row(access, id, Revision::ZERO, &result)?;
    record.references = references;
    commit(
        store,
        access,
        vec![Mutation::Put {
            record,
            expected: None,
        }],
        CommandId::new(),
        now,
    )
    .await?;
    Ok(result)
}
