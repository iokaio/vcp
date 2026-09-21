// SPDX-License-Identifier: Apache-2.0
//! Descriptive likelihood under a saved baseline; never a refit or policy signal.
use super::forecasts::{CohortReport, Episode, ForecastStatus, Report, State};
use serde::{Deserialize, Serialize};
use vcp_models::markov::Chain;

/// Load both immutable snapshots under current source authorization.
pub fn saved(
    store: &vcp_store::Store,
    access: &vcp_memory::access::Access,
    baseline: &str,
    current: &str,
) -> super::Result<Option<Comparison>> {
    let before = super::load_report(store, access, baseline)?;
    let after = super::load_report(store, access, current)?;
    let before = super::forecast_reports::load(store, access, &before)?;
    let after = super::forecast_reports::load(store, access, &after)?;
    Ok(match (before, after) {
        (Some(before), Some(after)) => Some(compare(&before, &after)),
        _ => None,
    })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Likelihood {
    pub episodes: u64,
    pub transitions: u64,
    pub total_log_likelihood: f64,
    pub mean_transition_log_likelihood: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CohortDrift {
    pub cohort: String,
    pub baseline: Option<Likelihood>,
    pub current: Option<Likelihood>,
    pub mean_log_likelihood_change: Option<f64>,
    pub unavailable: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Comparison {
    pub baseline: String,
    pub current: String,
    pub cohorts: Vec<CohortDrift>,
    pub unavailable: Vec<String>,
    pub caveats: Vec<String>,
}

fn terminal(state: State) -> bool {
    matches!(state, State::Completed | State::Failed | State::Cancelled)
}

fn score(model: &CohortReport, episodes: &[Episode]) -> Result<Likelihood, String> {
    let ForecastStatus::Forecast { probabilities, .. } = &model.status else {
        return Err("Baseline forecast abstained; no supported frozen probabilities.".into());
    };
    let chain = Chain::new(
        probabilities.clone(),
        model.alphabet.iter().copied().map(terminal).collect(),
    )
    .map_err(|e| e.to_string())?;
    let mut result = Likelihood {
        episodes: 0,
        transitions: 0,
        total_log_likelihood: 0.0,
        mean_transition_log_likelihood: 0.0,
    };
    for episode in episodes.iter().filter(|e| e.cohort == model.id) {
        let sequence = episode
            .visits
            .iter()
            .map(|v| {
                model
                    .alphabet
                    .iter()
                    .position(|s| *s == v.state)
                    .ok_or_else(|| {
                        "Current trace contains a state absent from the baseline.".to_owned()
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if sequence.len() < 2 {
            return Err("Trace has no transition denominator.".into());
        }
        let value = chain
            .log_likelihood(&sequence)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| {
                "Trace contains a zero-probability baseline transition; likelihood unavailable."
                    .to_owned()
            })?;
        result.episodes += 1;
        result.transitions += (sequence.len() - 1) as u64;
        result.total_log_likelihood += value;
    }
    if result.episodes < super::forecasts::MINIMUM_EPISODES || result.transitions == 0 {
        return Err("Too few complete matched episodes for descriptive drift.".into());
    }
    result.mean_transition_log_likelihood = result.total_log_likelihood / result.transitions as f64;
    if !result.mean_transition_log_likelihood.is_finite()
        || !result.total_log_likelihood.is_finite()
    {
        return Err("Non-finite likelihood; drift unavailable.".into());
    }
    Ok(result)
}

/// Inputs must already have passed saved-artifact access and retention validation.
pub fn compare(baseline: &Report, current: &Report) -> Comparison {
    let mut result = Comparison {
        baseline: baseline.id.clone(),
        current: current.id.clone(),
        cohorts: Vec::new(),
        unavailable: Vec::new(),
        caveats: vec![
            "Likelihood is conditional on the initial state under unchanged saved baseline probabilities; no refit occurs.".into(),
            "No calibrated drift threshold or causal regression claim. Context content is not a matched covariate; objective and context changes may explain differences.".into(),
            "Baseline self-likelihood is in-sample and optimistic. Means expose different transition denominators; they are not task quality scores.".into(),
        ],
    };
    if baseline.schema_version != current.schema_version
        || baseline.algorithm != current.algorithm
        || baseline.state_definition != current.state_definition
        || baseline.reward_definition != current.reward_definition
    {
        result
            .unavailable
            .push("Forecast definitions or algorithm revisions differ.".into());
    }
    if baseline.workspace != current.workspace
        || baseline.authority != current.authority
        || baseline.deletion != current.deletion
        || baseline.source_tasks != current.source_tasks
    {
        result
            .unavailable
            .push("Source workspace, access or deletion scope differs.".into());
    }
    let width = |r: &Report| {
        r.window
            .from
            .map(|from| r.window.until.get().saturating_sub(from.get()))
    };
    if width(baseline).is_none()
        || width(baseline) != width(current)
        || baseline.window.until > current.window.from.unwrap_or(current.window.until)
        || baseline.cutoff >= current.cutoff
    {
        result.unavailable.push(
            "Requires disjoint, equally sized explicit windows and increasing saved cutoffs."
                .into(),
        );
    }
    if baseline.excluded_pruned_action_records > 0
        || current.excluded_pruned_action_records > 0
        || baseline.excluded_pruned_tasks > 0
        || current.excluded_pruned_tasks > 0
    {
        result
            .unavailable
            .push("Pruned source history prevents matched drift.".into());
    }
    if baseline
        .episodes
        .iter()
        .any(|a| current.episodes.iter().any(|b| a.task == b.task))
    {
        result
            .unavailable
            .push("Windows share task episodes; independent drift comparison unavailable.".into());
    }
    if !baseline.excluded.is_empty() || !current.excluded.is_empty() {
        result.caveats.push(format!("Excluded episodes: baseline {}, current {}; estimates describe only complete eligible work.", baseline.excluded.len(), current.excluded.len()));
    }
    if baseline.cohorts.is_empty() || current.cohorts.is_empty() {
        result
            .unavailable
            .push("No complete eligible cohort.".into());
    }
    if !result.unavailable.is_empty() {
        return result;
    }
    for before in &baseline.cohorts {
        let mut row = CohortDrift {
            cohort: before.id.clone(),
            baseline: None,
            current: None,
            mean_log_likelihood_change: None,
            unavailable: Vec::new(),
        };
        match current.cohorts.iter().find(|after| after.id == before.id && after.identity == before.identity) {
            None => row.unavailable.push("Historical endpoint, role, policy, catalog or task-class cohort is absent or changed.".into()),
            Some(_) => {
                match score(before, &baseline.episodes) {
                    Ok(value) => row.baseline = Some(value),
                    Err(reason) => row.unavailable.push(reason),
                }
                match score(before, &current.episodes) {
                    Ok(value) => row.current = Some(value),
                    Err(reason) => row.unavailable.push(reason),
                }
                if let (Some(a), Some(b)) = (&row.baseline, &row.current) {
                    row.mean_log_likelihood_change = Some(b.mean_transition_log_likelihood - a.mean_transition_log_likelihood);
                }
            }
        }
        result.cohorts.push(row);
    }
    for after in &current.cohorts {
        if !baseline
            .cohorts
            .iter()
            .any(|before| before.id == after.id && before.identity == after.identity)
        {
            result.cohorts.push(CohortDrift {
                cohort: after.id.clone(),
                baseline: None,
                current: None,
                mean_log_likelihood_change: None,
                unavailable: vec!["Current cohort has no matching frozen baseline.".into()],
            });
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::{
        forecasts::{Cohort, Visit},
        HistoryWindow,
    };
    use super::*;
    use std::collections::BTreeSet;
    use vcp_domain::*;

    fn fixture() -> Report {
        let episodes = (0..20)
            .map(|_| Episode {
                task: TaskId::new(),
                cohort: "cohort".into(),
                visits: [State::Main, State::Retry, State::Completed]
                    .into_iter()
                    .enumerate()
                    .map(|(i, state)| Visit {
                        state,
                        watermark: Watermark::new(i as u64),
                        reward_micros: Some(0),
                    })
                    .collect(),
                known_cost_micros: 0,
                unknown_attempts: 0,
                liability_micros: 0,
                unknown_liabilities: 0,
                source_events: BTreeSet::new(),
                references: BTreeSet::new(),
            })
            .collect();
        let model = CohortReport {
            id: "cohort".into(),
            identity: Cohort {
                task_class: "fixture".into(),
                policy: "p".into(),
                catalog: "c".into(),
                authority_policy: PolicyRevision::ZERO,
                currency: "USD".into(),
                root_task: true,
                endpoints: vec![],
            },
            episodes: 20,
            alphabet: vec![State::Main, State::Retry, State::Completed],
            counts: vec![vec![0, 20, 0], vec![0, 20, 20], vec![0, 0, 0]],
            row_samples: vec![20, 40, 0],
            known_cost_micros: 0,
            unknown_attempts: 0,
            liability_micros: 0,
            unknown_liabilities: 0,
            dominant_loops: vec![],
            status: ForecastStatus::Forecast {
                probabilities: vec![
                    vec![0.0, 1.0, 0.0],
                    vec![0.0, 0.5, 0.5],
                    vec![0.0, 0.0, 1.0],
                ],
                transient_states: vec![State::Main, State::Retry],
                absorbing_states: vec![State::Completed],
                expected_visits: vec![],
                outcome_probabilities: vec![],
                expected_cost_micros: vec![],
            },
            uncertainty: vec![],
        };
        Report {
            schema_version: 1,
            algorithm: "a".into(),
            state_definition: "s".into(),
            reward_definition: "r".into(),
            id: "before".into(),
            workspace: WorkspaceId::new(),
            authority: AuthorityRevision::ZERO,
            deletion: DeletionEpoch::ZERO,
            cutoff: Watermark::new(100),
            window: HistoryWindow {
                from: Some(Timestamp::new(0)),
                until: Timestamp::new(100),
            },
            source_actions: "a".into(),
            source_transitions: "t".into(),
            excluded_pruned_action_records: 0,
            excluded_pruned_tasks: 0,
            source_tasks: None,
            episodes,
            excluded: vec![],
            cohorts: vec![model],
            source_events: BTreeSet::new(),
            references: BTreeSet::new(),
            minimum_episodes: 20,
            minimum_row_samples: 5,
            serving_qualified: false,
            limitations: vec![],
        }
    }
    fn later(before: &Report) -> Report {
        let mut after = before.clone();
        after.id = "after".into();
        after.cutoff = Watermark::new(200);
        after.window = HistoryWindow {
            from: Some(Timestamp::new(100)),
            until: Timestamp::new(200),
        };
        for episode in &mut after.episodes {
            episode.task = TaskId::new();
        }
        after
    }
    #[test]
    fn frozen_baseline_likelihood_has_exact_denominators_and_zero_support_abstains() {
        let before = fixture();
        let mut after = later(&before);
        for episode in &mut after.episodes {
            episode.visits.insert(
                2,
                Visit {
                    state: State::Retry,
                    watermark: Watermark::new(2),
                    reward_micros: Some(0),
                },
            );
        }
        let result = compare(&before, &after);
        assert!(result.unavailable.is_empty());
        let row = &result.cohorts[0];
        assert_eq!(row.baseline.as_ref().unwrap().transitions, 40);
        assert_eq!(row.current.as_ref().unwrap().transitions, 60);
        assert!(
            (row.current.as_ref().unwrap().total_log_likelihood - 40.0 * 0.5_f64.ln()).abs()
                < 1e-10
        );
        assert!((row.mean_log_likelihood_change.unwrap() - 0.5_f64.ln() / 6.0).abs() < 1e-10);
        after.episodes[0].visits.remove(1);
        after.episodes[0].visits.remove(1);
        let result = compare(&before, &after);
        assert!(result.cohorts[0].current.is_none());
        assert!(result.cohorts[0].unavailable[0].contains("zero-probability"));
        assert!(serde_json::to_string(&result).is_ok());
    }
    #[test]
    fn incompatible_windows_definitions_sources_and_sparse_traces_abstain() {
        let before = fixture();
        let mut after = later(&before);
        after.episodes.truncate(19);
        assert!(compare(&before, &after).cohorts[0].current.is_none());
        let mut after = later(&before);
        after.state_definition = "different".into();
        assert!(compare(&before, &after).cohorts.is_empty());
        let mut after = later(&before);
        after.window.from = None;
        assert!(compare(&before, &after).cohorts.is_empty());
        let mut after = later(&before);
        after.episodes[0].task = before.episodes[0].task.clone();
        assert!(compare(&before, &after)
            .unavailable
            .iter()
            .any(|r| r.contains("share task")));
        let mut after = later(&before);
        after.cohorts[0].identity.policy = "new-policy".into();
        assert!(compare(&before, &after)
            .cohorts
            .iter()
            .all(|c| c.current.is_none()));
        let mut after = later(&before);
        after.excluded_pruned_action_records = 1;
        assert!(compare(&before, &after).cohorts.is_empty());
    }
}
