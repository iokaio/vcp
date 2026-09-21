// SPDX-License-Identifier: Apache-2.0
//! Source-bound attempt-visit cost rewards. This mapping is unqualified and is
//! never a routing estimate or permission to spend.
use super::{current_policy, current_registry, err, observations, HistoryWindow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{accounting::RequestRole, *};
use vcp_memory::access::Access;
use vcp_protocol::{canonical_bytes, digest_bytes};
use vcp_store::Store;

const ALGORITHM: &str = "exact-attempt-cohort-charge-mean/1";
const UNCERTAINTY: &str = "complete-cohort-or-abstain/1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cohort {
    pub role: RequestRole,
    pub model: String,
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
pub struct Cell {
    pub cohort: Cohort,
    pub currency: String,
    pub attempts: u64,
    pub exact_attempts: u64,
    pub unknown_attempts: u64,
    pub exact_charge_sum_micros: u64,
    pub exact_min_charge_micros: Option<u64>,
    pub exact_max_charge_micros: Option<u64>,
    pub observed_charged_attempts: u64,
    pub observed_charged_sum_micros: u64,
    pub observed_liability_attempts: u64,
    pub observed_liability_sum_micros: u64,
    /// Upward-rounded exact mean. None means the cohort has an unknown
    /// remainder; it must never be interpreted as zero.
    pub point_estimate_micros: Option<u64>,
    pub unknown_remainder: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Abstention {
    NoAttempts,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Status {
    Mapped { cells: Vec<Cell> },
    Abstained { reason: Abstention },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
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
    pub left_censored_attempts: u64,
    pub right_censored_attempts: u64,
    pub excluded_pruned_records: u64,
    pub algorithm: String,
    pub state_definition: String,
    pub reward_definition: String,
    pub uncertainty_method: String,
    pub features: Vec<String>,
    pub cohort: String,
    pub policy: Option<String>,
    pub catalog: Option<String>,
    pub attempts: u64,
    pub exact_attempts: u64,
    pub unknown_attempts: u64,
    pub status: Status,
    pub qualification: Option<String>,
    pub serving_qualified: bool,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    role: u8,
    model: String,
    endpoint: String,
    authority_policy: PolicyRevision,
    task_class: Option<String>,
    root_task: bool,
    retry_depth: Option<u16>,
    decomposition_depth: Option<u16>,
    prior_attempts: Option<u16>,
    prior_failed_checks: Option<u16>,
    currency: String,
}

struct Builder {
    cohort: Cohort,
    currency: String,
    attempts: u64,
    exact_attempts: u64,
    exact_charge_sum_micros: u64,
    exact_min_charge_micros: Option<u64>,
    exact_max_charge_micros: Option<u64>,
    observed_charged_attempts: u64,
    observed_charged_sum_micros: u64,
    observed_liability_attempts: u64,
    observed_liability_sum_micros: u64,
}

/// Map each attempt once to an exact augmented cohort visit and its terminal
/// charge reward. A point estimate exists only when every visit in the cohort
/// has a complete exact terminal charge.
pub fn map(store: &Store, access: &Access, window: HistoryWindow) -> Result<Artifact> {
    let source = observations::observe(store, access, window)?;
    let source_digest = source
        .id
        .strip_prefix("action-evidence-")
        .filter(|value| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        .ok_or("invalid action evidence identity")?
        .to_owned();
    let policy = current_policy(store, access)?.map(|value| value.value.id);
    let catalog = current_registry(store, access)?.map(|value| value.value.catalog.id);
    let attempt_gaps = source.attempts.iter().try_fold(0u64, |sum, trace| {
        sum.checked_add(trace.gaps.len() as u64)
            .ok_or("reward gap count overflow")
    })?;
    let source_gaps = add(source.gaps.len() as u64, attempt_gaps)?;
    let left_censored_attempts = source
        .attempts
        .iter()
        .filter(|trace| trace.left_censored)
        .count() as u64;
    let right_censored_attempts = source
        .attempts
        .iter()
        .filter(|trace| trace.right_censored)
        .count() as u64;
    let mut groups = BTreeMap::<Key, Builder>::new();
    let mut exact_attempts = 0u64;
    for trace in &source.attempts {
        let cohort = Cohort {
            role: trace.cohort.role,
            model: trace.cohort.model.clone(),
            endpoint: trace.cohort.endpoint.clone(),
            authority_policy: trace.cohort.authority_policy,
            task_class: trace.cohort.task_class.clone(),
            root_task: trace.cohort.root_task,
            retry_depth: trace.cohort.retry_depth,
            decomposition_depth: trace.cohort.decomposition_depth,
            prior_attempts: trace.cohort.prior_attempts,
            prior_failed_checks: trace.cohort.prior_failed_checks,
        };
        let key = Key {
            role: role_order(cohort.role),
            model: cohort.model.clone(),
            endpoint: cohort.endpoint.clone(),
            authority_policy: cohort.authority_policy,
            task_class: cohort.task_class.clone(),
            root_task: cohort.root_task,
            retry_depth: cohort.retry_depth,
            decomposition_depth: cohort.decomposition_depth,
            prior_attempts: cohort.prior_attempts,
            prior_failed_checks: cohort.prior_failed_checks,
            currency: trace.charge.currency.clone(),
        };
        let group = groups.entry(key).or_insert_with(|| Builder {
            cohort,
            currency: trace.charge.currency.clone(),
            attempts: 0,
            exact_attempts: 0,
            exact_charge_sum_micros: 0,
            exact_min_charge_micros: None,
            exact_max_charge_micros: None,
            observed_charged_attempts: 0,
            observed_charged_sum_micros: 0,
            observed_liability_attempts: 0,
            observed_liability_sum_micros: 0,
        });
        group.attempts = add(group.attempts, 1)?;
        if let Some(value) = trace.charge.charged_micros {
            group.observed_charged_attempts = add(group.observed_charged_attempts, 1)?;
            group.observed_charged_sum_micros = add(group.observed_charged_sum_micros, value)?;
        }
        if let Some(value) = trace.charge.liability_micros {
            group.observed_liability_attempts = add(group.observed_liability_attempts, 1)?;
            group.observed_liability_sum_micros = add(group.observed_liability_sum_micros, value)?;
        }
        match trace.charge.final_charge_micros {
            Some(value) if trace.charge.complete && !trace.charge.unknown_remainder => {
                group.exact_attempts = add(group.exact_attempts, 1)?;
                group.exact_charge_sum_micros = add(group.exact_charge_sum_micros, value)?;
                group.exact_min_charge_micros = Some(
                    group
                        .exact_min_charge_micros
                        .map_or(value, |current| current.min(value)),
                );
                group.exact_max_charge_micros = Some(
                    group
                        .exact_max_charge_micros
                        .map_or(value, |current| current.max(value)),
                );
                exact_attempts = add(exact_attempts, 1)?;
            }
            None => {}
            Some(_) => return Err("invalid terminal charge attribution".into()),
        }
    }
    let mut cells = Vec::with_capacity(groups.len());
    for (_, group) in groups {
        let unknown_attempts = group
            .attempts
            .checked_sub(group.exact_attempts)
            .ok_or("reward sample count underflow")?;
        let point_estimate_micros = if unknown_attempts == 0 && group.attempts > 0 {
            let quotient = group.exact_charge_sum_micros / group.attempts;
            let remainder = group.exact_charge_sum_micros % group.attempts;
            Some(add(quotient, u64::from(remainder != 0))?)
        } else {
            None
        };
        cells.push(Cell {
            cohort: group.cohort,
            currency: group.currency,
            attempts: group.attempts,
            exact_attempts: group.exact_attempts,
            unknown_attempts,
            exact_charge_sum_micros: group.exact_charge_sum_micros,
            exact_min_charge_micros: group.exact_min_charge_micros,
            exact_max_charge_micros: group.exact_max_charge_micros,
            observed_charged_attempts: group.observed_charged_attempts,
            observed_charged_sum_micros: group.observed_charged_sum_micros,
            observed_liability_attempts: group.observed_liability_attempts,
            observed_liability_sum_micros: group.observed_liability_sum_micros,
            point_estimate_micros,
            unknown_remainder: unknown_attempts > 0,
        });
    }
    let attempts = source.attempts.len() as u64;
    let unknown_attempts = attempts
        .checked_sub(exact_attempts)
        .ok_or("reward attempt count underflow")?;
    let status = if attempts == 0 {
        Status::Abstained {
            reason: Abstention::NoAttempts,
        }
    } else {
        Status::Mapped { cells }
    };
    let mut artifact = Artifact {
        schema_version: 1,
        id: String::new(),
        workspace: source.workspace,
        authority: source.authority,
        deletion: source.deletion,
        source_evidence: source.id,
        source_digest,
        source_cutoff: source.cutoff,
        source_window: source.window,
        source_tasks: source.source_tasks,
        source_alphabet: source.alphabet,
        source_gaps,
        left_censored_attempts,
        right_censored_attempts,
        excluded_pruned_records: source.excluded_pruned_records,
        algorithm: ALGORITHM.into(),
        state_definition: "attempt-visit-exact-cohort/1".into(),
        reward_definition: "terminal-attempt-charge-micros/1".into(),
        uncertainty_method: UNCERTAINTY.into(),
        features: vec![
            "role".into(),
            "model".into(),
            "endpoint".into(),
            "authority_policy".into(),
            "task_class".into(),
            "root_task".into(),
            "retry_depth".into(),
            "decomposition_depth".into(),
            "prior_attempts".into(),
            "prior_failed_checks".into(),
        ],
        cohort: "authorized-exact-attempt-scope/1".into(),
        policy,
        catalog,
        attempts,
        exact_attempts,
        unknown_attempts,
        status,
        qualification: None,
        serving_qualified: false,
        limitations: vec![
            "Exact attempt-visit cost rewards only; completion value and utility are not inferred.".into(),
            "A cohort with any unknown terminal charge has no point estimate; observed charged and liability totals remain separate.".into(),
            "Roles, root/child identity, currencies and augmented counters remain separate to prevent component rollup double counting.".into(),
            "The artifact is rebuilt from retained authorized evidence and is not qualified for routing or forecasting.".into(),
        ],
    };
    let encoded = canonical_bytes(&artifact).map_err(err)?;
    if encoded.len() > 512 * 1024 {
        return Err("reward artifact exceeds 512 KiB".into());
    }
    artifact.id = format!("markov-reward-map-{}", digest_bytes(&encoded));
    Ok(artifact)
}

fn add(left: u64, right: u64) -> Result<u64> {
    left.checked_add(right)
        .ok_or_else(|| "reward arithmetic overflow".into())
}

fn role_order(role: RequestRole) -> u8 {
    match role {
        RequestRole::Main => 0,
        RequestRole::Helper => 1,
        RequestRole::Compaction => 2,
        RequestRole::Reviewer => 3,
        RequestRole::Child => 4,
        RequestRole::Optimizer => 5,
        RequestRole::Memory => 6,
        RequestRole::Verification => 7,
    }
}
