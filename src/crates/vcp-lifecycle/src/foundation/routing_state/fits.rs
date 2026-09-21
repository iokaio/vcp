// SPDX-License-Identifier: Apache-2.0
//! Rebuildable, unqualified fitted artifacts over retained task-state evidence.
use super::{current_policy, current_registry, err, transitions, HistoryWindow, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use vcp_domain::{task::TaskState, *};
use vcp_memory::access::Access;
use vcp_models::markov::{Chain, Error as MarkovError};
use vcp_protocol::{canonical_bytes, digest_bytes};
use vcp_store::Store;

const ALGORITHM: &str = "first-order-observed-support/1";
const UNCERTAINTY: &str = "raw-row-minimum-samples/1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Abstention {
    NoTransitions,
    MissingTerminal,
    NoTransient,
    Sparse,
    Nonabsorbing,
    InvalidEvidence,
    Numerical,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum FitStatus {
    Fitted { probabilities: Vec<Vec<f64>> },
    Abstained { reason: Abstention },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    pub left_censored_traces: u64,
    pub right_censored_traces: u64,
    pub excluded_pruned_tasks: u64,
    pub algorithm: String,
    pub prior_basis_points: u16,
    pub minimum_samples: u64,
    pub uncertainty_method: String,
    pub features: Vec<String>,
    pub cohort: String,
    pub task_class: Option<String>,
    pub endpoint: Option<String>,
    pub policy: Option<String>,
    pub catalog: Option<String>,
    pub alphabet: Vec<TaskState>,
    pub absorbing: Vec<bool>,
    pub counts: Vec<Vec<u64>>,
    pub row_samples: Vec<u64>,
    pub status: FitStatus,
    pub qualification: Option<String>,
    pub serving_qualified: bool,
    pub limitations: Vec<String>,
}

/// Fit one bounded first-order candidate from the current authorized retained
/// view. The result is never stored and is never permission to serve a value.
pub fn fit(
    store: &Store,
    access: &Access,
    window: HistoryWindow,
    prior_basis_points: u16,
    minimum_samples: u64,
) -> Result<Artifact> {
    if prior_basis_points > 10_000 || minimum_samples == 0 {
        return Err("invalid fit prior or minimum sample gate".into());
    }
    let source = transitions::observe(store, access, window)?;
    let source_digest = source
        .id
        .strip_prefix("transition-evidence-")
        .filter(|value| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        .ok_or("invalid transition evidence identity")?
        .to_owned();
    let policy = current_policy(store, access)?.map(|value| value.value.id);
    let catalog = current_registry(store, access)?.map(|value| value.value.catalog.id);
    let source_gaps = source.traces.iter().try_fold(0u64, |sum, trace| {
        sum.checked_add(trace.gaps.len() as u64)
            .ok_or("fit gap count overflow")
    })?;
    let left_censored_traces = source
        .traces
        .iter()
        .filter(|trace| trace.left_censored)
        .count() as u64;
    let right_censored_traces = source
        .traces
        .iter()
        .filter(|trace| trace.right_censored)
        .count() as u64;
    let order = [
        TaskState::Pending,
        TaskState::Running,
        TaskState::WaitingForInput,
        TaskState::Blocked,
        TaskState::Paused,
        TaskState::Completed,
        TaskState::Failed,
        TaskState::Cancelled,
    ];
    let alphabet = order
        .into_iter()
        .filter(|state| {
            source
                .transitions
                .iter()
                .any(|edge| edge.from == *state || edge.to == *state)
        })
        .collect::<Vec<_>>();
    let mut counts = vec![vec![0u64; alphabet.len()]; alphabet.len()];
    for edge in &source.transitions {
        let Some(from) = alphabet.iter().position(|state| *state == edge.from) else {
            continue;
        };
        let Some(to) = alphabet.iter().position(|state| *state == edge.to) else {
            continue;
        };
        counts[from][to] = edge.count;
    }
    let absorbing = alphabet
        .iter()
        .map(|state| state.terminal())
        .collect::<Vec<_>>();
    let row_samples = counts
        .iter()
        .map(|row| {
            row.iter().try_fold(0u64, |sum, value| {
                sum.checked_add(*value).ok_or("fit count overflow")
            })
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let status = if alphabet.is_empty() {
        FitStatus::Abstained {
            reason: Abstention::NoTransitions,
        }
    } else if !absorbing.iter().any(|value| *value) {
        FitStatus::Abstained {
            reason: Abstention::MissingTerminal,
        }
    } else if absorbing.iter().all(|value| *value) {
        FitStatus::Abstained {
            reason: Abstention::NoTransient,
        }
    } else {
        let legal = alphabet
            .iter()
            .map(|from| alphabet.iter().map(|to| legal(*from, *to)).collect())
            .collect::<Vec<Vec<bool>>>();
        match Chain::fit(
            &counts,
            &legal,
            absorbing.clone(),
            f64::from(prior_basis_points) / 10_000.0,
            minimum_samples,
        ) {
            Ok(chain) => FitStatus::Fitted {
                probabilities: chain.probabilities().to_vec(),
            },
            Err(error) => FitStatus::Abstained {
                reason: map_error(error),
            },
        }
    };
    let mut artifact = Artifact {
        schema_version: 1,
        id: String::new(),
        workspace: source.workspace.clone(),
        authority: source.authority,
        deletion: source.deletion,
        source_evidence: source.id,
        source_digest,
        source_cutoff: source.cutoff,
        source_window: source.window,
        source_tasks: source.source_tasks,
        source_alphabet: source.alphabet,
        source_gaps,
        left_censored_traces,
        right_censored_traces,
        excluded_pruned_tasks: source.excluded_pruned_tasks,
        algorithm: ALGORITHM.into(),
        prior_basis_points,
        minimum_samples,
        uncertainty_method: UNCERTAINTY.into(),
        features: vec!["task_state".into()],
        cohort: "authorized-task-state-scope/1".into(),
        task_class: None,
        endpoint: None,
        policy,
        catalog,
        alphabet,
        absorbing,
        counts,
        row_samples,
        status,
        qualification: None,
        serving_qualified: false,
        limitations: vec![
            "Observed first-order task-state candidate only; not calibrated or qualified for routing.".into(),
            "Alphabet contains only states present on retained connected edges; absent outcomes remain unavailable.".into(),
            "The fit is rebuilt from its source evidence and must be discarded after authority, deletion, scope or source identity changes.".into(),
        ],
    };
    let encoded = canonical_bytes(&artifact).map_err(err)?;
    if encoded.len() > 512 * 1024 {
        return Err("fit artifact exceeds 512 KiB".into());
    }
    artifact.id = format!("markov-fit-{}", digest_bytes(&encoded));
    Ok(artifact)
}

fn map_error(error: MarkovError) -> Abstention {
    match error {
        MarkovError::Sparse => Abstention::Sparse,
        MarkovError::Nonabsorbing => Abstention::Nonabsorbing,
        MarkovError::Numerical => Abstention::Numerical,
        MarkovError::Limit
        | MarkovError::Shape
        | MarkovError::Invalid
        | MarkovError::Forbidden
        | MarkovError::UnknownReward => Abstention::InvalidEvidence,
    }
}

fn legal(from: TaskState, to: TaskState) -> bool {
    if from.terminal() {
        return from == to;
    }
    if from == to || to == TaskState::Pending {
        return false;
    }
    if to == TaskState::Completed {
        return from == TaskState::Running;
    }
    true
}
