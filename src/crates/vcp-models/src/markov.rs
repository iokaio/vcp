// SPDX-License-Identifier: Apache-2.0
//! Bounded local arithmetic. No history access, persistence or qualified routing
//! estimate is implied. Callers own alphabet legality, cohorts and attribution.
use serde::{Deserialize, Serialize};

pub const MAX_STATES: usize = 32;
pub const MAX_OBSERVATIONS: usize = 100_000;
const NORMALIZATION_TOLERANCE: f64 = 1e-12;
const PIVOT_MIN: f64 = 1e-12;
const RESIDUAL_TOLERANCE: f64 = 1e-9;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("Markov input exceeds the state or observation bound")]
    Limit,
    #[error("Markov input has inconsistent dimensions or state identity")]
    Shape,
    #[error("Markov probabilities, counts or rewards are invalid")]
    Invalid,
    #[error("Markov observations include a forbidden transition")]
    Forbidden,
    #[error("Markov row has insufficient observed samples")]
    Sparse,
    #[error("Markov model has a state with no observed path to absorption")]
    Nonabsorbing,
    #[error("Markov solve is singular, ill-conditioned or non-finite")]
    Numerical,
    #[error("Markov reward attribution is incomplete")]
    UnknownReward,
}
type Result<T> = std::result::Result<T, Error>;

fn dimension(n: usize) -> Result<()> {
    if n == 0 || n > MAX_STATES {
        return Err(Error::Limit);
    }
    Ok(())
}

/// Each slice is one complete contiguous segment; callers split at every gap,
/// task/attempt boundary and censoring break. Segments are never joined.
pub fn count(states: usize, segments: &[&[usize]]) -> Result<Vec<Vec<u64>>> {
    dimension(states)?;
    if segments.len() > MAX_OBSERVATIONS {
        return Err(Error::Limit);
    }
    let mut size = 0usize;
    let mut counts = vec![vec![0u64; states]; states];
    for segment in segments {
        size = size.checked_add(segment.len()).ok_or(Error::Limit)?;
        if size > MAX_OBSERVATIONS {
            return Err(Error::Limit);
        }
        if segment.iter().any(|&state| state >= states) {
            return Err(Error::Shape);
        }
        for pair in segment.windows(2) {
            counts[pair[0]][pair[1]] += 1; // bounded by MAX_OBSERVATIONS
        }
    }
    Ok(counts)
}

/// Validated transient/absorbing chain. Fields are private so solve/likelihood
/// cannot accidentally bypass validation. This is not a portable fitted artifact.
#[derive(Clone, Debug)]
pub struct Chain {
    probabilities: Vec<Vec<f64>>,
    absorbing: Vec<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Analysis {
    /// Stable original state indexes for matrix rows/columns below.
    pub transient_states: Vec<usize>,
    pub absorbing_states: Vec<usize>,
    /// Transient x transient; includes the initial transient visit.
    pub expected_visits: Vec<Vec<f64>>,
    /// Transient x absorbing; terminal starts are trivially already absorbed.
    pub outcome_probabilities: Vec<Vec<f64>>,
    /// One nonnegative reward per transient visit, summed until absorption.
    pub expected_rewards: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderComparison {
    pub heldout_predictions: u64,
    pub first_parameters: u64,
    pub second_parameters: u64,
    pub first_log_likelihood: f64,
    pub second_log_likelihood: f64,
    pub first_penalized_score: f64,
    pub second_penalized_score: f64,
    pub selected_order: u8,
    pub first_order_two_step_max_abs_error: f64,
}

impl Chain {
    pub fn new(probabilities: Vec<Vec<f64>>, absorbing: Vec<bool>) -> Result<Self> {
        let n = probabilities.len();
        dimension(n)?;
        if absorbing.len() != n || probabilities.iter().any(|row| row.len() != n) {
            return Err(Error::Shape);
        }
        for (i, row) in probabilities.iter().enumerate() {
            if row.iter().any(|p| !p.is_finite() || *p < 0.0 || *p > 1.0)
                || (row.iter().sum::<f64>() - 1.0).abs() > NORMALIZATION_TOLERANCE
            {
                return Err(Error::Invalid);
            }
            if absorbing[i]
                && row
                    .iter()
                    .enumerate()
                    .any(|(j, &p)| p != if i == j { 1.0 } else { 0.0 })
            {
                return Err(Error::Invalid);
            }
        }
        // Every state must reach a declared terminal through positive edges.
        // A closed transient class is nonabsorbing even if other states succeed.
        let mut reachable = absorbing.clone();
        for _ in 0..n {
            for i in 0..n {
                reachable[i] |= probabilities[i]
                    .iter()
                    .enumerate()
                    .any(|(j, &p)| p > 0.0 && reachable[j]);
            }
        }
        if reachable.iter().any(|reached| !reached) {
            return Err(Error::Nonabsorbing);
        }
        Ok(Self {
            probabilities,
            absorbing,
        })
    }

    /// Smooth only observed legal edges. An unobserved legal edge gets no mass:
    /// a prior must never manufacture a path to success. Terminal self-loops are
    /// structural, not observations. Sparse-row gates use raw sample counts.
    pub fn fit(
        counts: &[Vec<u64>],
        legal: &[Vec<bool>],
        absorbing: Vec<bool>,
        prior: f64,
        minimum_samples: u64,
    ) -> Result<Self> {
        let n = counts.len();
        dimension(n)?;
        if legal.len() != n
            || absorbing.len() != n
            || counts.iter().any(|row| row.len() != n)
            || legal.iter().any(|row| row.len() != n)
        {
            return Err(Error::Shape);
        }
        if !prior.is_finite() || prior < 0.0 || minimum_samples == 0 {
            return Err(Error::Invalid);
        }
        let mut probabilities = vec![vec![0.0; n]; n];
        for i in 0..n {
            let mut samples = 0u64;
            for j in 0..n {
                samples = samples.checked_add(counts[i][j]).ok_or(Error::Invalid)?;
                if counts[i][j] > 0 && (!legal[i][j] || (absorbing[i] && i != j)) {
                    return Err(Error::Forbidden);
                }
            }
            if absorbing[i] {
                probabilities[i][i] = 1.0;
                continue;
            }
            if samples < minimum_samples {
                return Err(Error::Sparse);
            }
            for j in 0..n {
                if counts[i][j] > 0 {
                    probabilities[i][j] = counts[i][j] as f64 + prior;
                }
            }
            let total = probabilities[i].iter().sum::<f64>();
            if !total.is_finite() || total <= 0.0 {
                return Err(Error::Numerical);
            }
            for p in &mut probabilities[i] {
                *p /= total;
            }
        }
        Self::new(probabilities, absorbing)
    }

    pub fn probabilities(&self) -> &[Vec<f64>] {
        &self.probabilities
    }

    /// Conditional on the first state. None means a zero-probability sequence,
    /// not a floating-point underflow. No initial-state distribution is fitted.
    pub fn log_likelihood(&self, sequence: &[usize]) -> Result<Option<f64>> {
        if sequence.len() > MAX_OBSERVATIONS {
            return Err(Error::Limit);
        }
        if sequence
            .iter()
            .any(|&state| state >= self.probabilities.len())
        {
            return Err(Error::Shape);
        }
        let mut log = 0.0;
        for pair in sequence.windows(2) {
            let probability = self.probabilities[pair[0]][pair[1]];
            if probability == 0.0 {
                return Ok(None);
            }
            log += probability.ln();
        }
        if !log.is_finite() {
            return Err(Error::Numerical);
        }
        Ok(Some(log))
    }

    /// Rewards use caller-declared units (no currency mixing/conversion). Every
    /// state must be known and terminals must have zero reward. Unknown liability
    /// cannot silently become free. This routine never supplies admission bounds.
    pub fn analyze(&self, rewards: &[Option<f64>]) -> Result<Analysis> {
        if rewards.len() != self.absorbing.len() {
            return Err(Error::Shape);
        }
        let rewards = rewards
            .iter()
            .copied()
            .collect::<Option<Vec<_>>>()
            .ok_or(Error::UnknownReward)?;
        for (i, &reward) in rewards.iter().enumerate() {
            if !reward.is_finite() || reward < 0.0 || (self.absorbing[i] && reward != 0.0) {
                return Err(Error::Invalid);
            }
        }
        let transient_states: Vec<_> = (0..self.absorbing.len())
            .filter(|&i| !self.absorbing[i])
            .collect();
        let absorbing_states: Vec<_> = (0..self.absorbing.len())
            .filter(|&i| self.absorbing[i])
            .collect();
        let system: Vec<Vec<f64>> = transient_states
            .iter()
            .map(|&i| {
                transient_states
                    .iter()
                    .map(|&j| (if i == j { 1.0 } else { 0.0 }) - self.probabilities[i][j])
                    .collect()
            })
            .collect();
        let expected_visits = inverse(&system)?;
        let mut outcome_probabilities = Vec::new();
        let mut expected_rewards = Vec::new();
        for visits in &expected_visits {
            let outcomes: Vec<f64> = absorbing_states
                .iter()
                .map(|&terminal| {
                    visits
                        .iter()
                        .zip(&transient_states)
                        .map(|(visits, &state)| visits * self.probabilities[state][terminal])
                        .sum()
                })
                .collect();
            if outcomes
                .iter()
                .any(|p| !p.is_finite() || *p < 0.0 || *p > 1.0 + RESIDUAL_TOLERANCE)
                || (outcomes.iter().sum::<f64>() - 1.0).abs() > RESIDUAL_TOLERANCE
            {
                return Err(Error::Numerical);
            }
            let reward: f64 = visits
                .iter()
                .zip(&transient_states)
                .map(|(visits, &state)| visits * rewards[state])
                .sum();
            if !reward.is_finite() || reward < 0.0 {
                return Err(Error::Numerical);
            }
            outcome_probabilities.push(outcomes);
            expected_rewards.push(reward);
        }
        Ok(Analysis {
            transient_states,
            absorbing_states,
            expected_visits,
            outcome_probabilities,
            expected_rewards,
        })
    }
}

/// Compare first- and second-order conditional fits on the same held-out
/// predictions. Priors apply only to outcomes observed in training. Missing
/// context/support or undersampled rows abstain through `Sparse`.
pub fn compare_orders(
    states: usize,
    training: &[&[usize]],
    heldout: &[&[usize]],
    prior: f64,
    minimum_samples: u64,
) -> Result<OrderComparison> {
    dimension(states)?;
    if !prior.is_finite() || prior < 0.0 || minimum_samples == 0 {
        return Err(Error::Invalid);
    }
    let training_observations = validate_segments(states, training)?;
    let heldout_observations = validate_segments(states, heldout)?;
    if training_observations
        .checked_add(heldout_observations)
        .ok_or(Error::Limit)?
        > MAX_OBSERVATIONS
    {
        return Err(Error::Limit);
    }
    let mut first = vec![vec![0u64; states]; states];
    let mut second = std::collections::BTreeMap::<(usize, usize), Vec<u64>>::new();
    for segment in training {
        for pair in segment.windows(2) {
            first[pair[0]][pair[1]] = first[pair[0]][pair[1]].checked_add(1).ok_or(Error::Limit)?;
        }
        for triple in segment.windows(3) {
            let row = second
                .entry((triple[0], triple[1]))
                .or_insert_with(|| vec![0; states]);
            row[triple[2]] = row[triple[2]].checked_add(1).ok_or(Error::Limit)?;
        }
    }
    let first_probabilities = probabilities(&first, prior)?;
    let mut second_probabilities = std::collections::BTreeMap::new();
    for (context, row) in &second {
        second_probabilities.insert(
            *context,
            probabilities(std::slice::from_ref(row), prior)?.remove(0),
        );
    }
    let first_parameters = parameters(first.iter())?;
    let second_parameters = parameters(second.values())?;
    let mut first_log = 0.0;
    let mut second_log = 0.0;
    let mut predictions = 0u64;
    let mut two_step = vec![vec![0u64; states]; states];
    for segment in heldout {
        for triple in segment.windows(3) {
            let first_samples = sum(&first[triple[1]])?;
            let second_row = second.get(&(triple[0], triple[1])).ok_or(Error::Sparse)?;
            if first_samples < minimum_samples || sum(second_row)? < minimum_samples {
                return Err(Error::Sparse);
            }
            let first_probability = first_probabilities[triple[1]][triple[2]];
            let second_probability = second_probabilities[&(triple[0], triple[1])][triple[2]];
            if first_probability == 0.0 || second_probability == 0.0 {
                return Err(Error::Sparse);
            }
            first_log += first_probability.ln();
            second_log += second_probability.ln();
            predictions = predictions.checked_add(1).ok_or(Error::Limit)?;
            two_step[triple[0]][triple[2]] = two_step[triple[0]][triple[2]]
                .checked_add(1)
                .ok_or(Error::Limit)?;
        }
    }
    if predictions == 0 || !first_log.is_finite() || !second_log.is_finite() {
        return Err(Error::Sparse);
    }
    let training_predictions = second.values().try_fold(0u64, |total, row| {
        total.checked_add(sum(row)?).ok_or(Error::Limit)
    })?;
    if training_predictions == 0 {
        return Err(Error::Sparse);
    }
    let penalty_scale = (training_predictions as f64).ln() * 0.5;
    let first_score = first_log - penalty_scale * first_parameters as f64;
    let second_score = second_log - penalty_scale * second_parameters as f64;
    let mut max_error = 0.0f64;
    for start in 0..states {
        let observed = sum(&two_step[start])?;
        if observed == 0 {
            continue;
        }
        if sum(&first[start])? < minimum_samples {
            return Err(Error::Sparse);
        }
        for middle in 0..states {
            if first_probabilities[start][middle] > 0.0 && sum(&first[middle])? < minimum_samples {
                return Err(Error::Sparse);
            }
        }
        for end in 0..states {
            let predicted = (0..states)
                .map(|middle| first_probabilities[start][middle] * first_probabilities[middle][end])
                .sum::<f64>();
            let actual = two_step[start][end] as f64 / observed as f64;
            max_error = max_error.max((predicted - actual).abs());
        }
    }
    if !first_score.is_finite() || !second_score.is_finite() || !max_error.is_finite() {
        return Err(Error::Numerical);
    }
    Ok(OrderComparison {
        heldout_predictions: predictions,
        first_parameters,
        second_parameters,
        first_log_likelihood: first_log,
        second_log_likelihood: second_log,
        first_penalized_score: first_score,
        second_penalized_score: second_score,
        selected_order: if second_score > first_score { 2 } else { 1 },
        first_order_two_step_max_abs_error: max_error,
    })
}

fn validate_segments(states: usize, segments: &[&[usize]]) -> Result<usize> {
    if segments.len() > MAX_OBSERVATIONS {
        return Err(Error::Limit);
    }
    let mut total = 0usize;
    for segment in segments {
        total = total.checked_add(segment.len()).ok_or(Error::Limit)?;
        if total > MAX_OBSERVATIONS || segment.iter().any(|state| *state >= states) {
            return Err(if total > MAX_OBSERVATIONS {
                Error::Limit
            } else {
                Error::Shape
            });
        }
    }
    Ok(total)
}

fn probabilities(rows: &[Vec<u64>], prior: f64) -> Result<Vec<Vec<f64>>> {
    let mut probabilities = Vec::with_capacity(rows.len());
    for row in rows {
        let total = row
            .iter()
            .filter(|count| **count > 0)
            .map(|count| *count as f64 + prior)
            .sum::<f64>();
        if !total.is_finite() {
            return Err(Error::Numerical);
        }
        let row = row
            .iter()
            .map(|count| {
                if *count == 0 || total == 0.0 {
                    0.0
                } else {
                    (*count as f64 + prior) / total
                }
            })
            .collect::<Vec<_>>();
        if row.iter().any(|probability| !probability.is_finite()) {
            return Err(Error::Numerical);
        }
        probabilities.push(row);
    }
    Ok(probabilities)
}

fn parameters<'a>(mut rows: impl Iterator<Item = &'a Vec<u64>>) -> Result<u64> {
    rows.try_fold(0u64, |total, row| {
        let supported = row.iter().filter(|count| **count > 0).count() as u64;
        total
            .checked_add(supported.saturating_sub(1))
            .ok_or(Error::Limit)
    })
}

fn sum(row: &[u64]) -> Result<u64> {
    row.iter().try_fold(0u64, |total, value| {
        total.checked_add(*value).ok_or(Error::Limit)
    })
}

// Partial-pivot Gauss-Jordan elimination, O(MAX_STATES^3), followed by an
// independent A*N=I residual check. No unbounded iterative convergence loop.
fn inverse(system: &[Vec<f64>]) -> Result<Vec<Vec<f64>>> {
    let n = system.len();
    let mut work = system.to_vec();
    let mut result = vec![vec![0.0; n]; n];
    for (i, row) in result.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    for column in 0..n {
        let mut pivot = column;
        for row in column + 1..n {
            if work[row][column].abs() > work[pivot][column].abs() {
                pivot = row;
            }
        }
        let divisor = work[pivot][column];
        if !divisor.is_finite() || divisor.abs() <= PIVOT_MIN {
            return Err(Error::Numerical);
        }
        work.swap(column, pivot);
        result.swap(column, pivot);
        for j in 0..n {
            work[column][j] /= divisor;
            result[column][j] /= divisor;
        }
        for row in 0..n {
            if row == column {
                continue;
            }
            let factor = work[row][column];
            for j in 0..n {
                work[row][j] -= factor * work[column][j];
                result[row][j] -= factor * result[column][j];
                if !work[row][j].is_finite() || !result[row][j].is_finite() {
                    return Err(Error::Numerical);
                }
            }
        }
    }
    if result.iter().flatten().any(|x| !x.is_finite() || *x < 0.0) {
        return Err(Error::Numerical);
    }
    for i in 0..n {
        for j in 0..n {
            let mut value = 0.0;
            let mut scale = 1.0;
            for k in 0..n {
                let term = system[i][k] * result[k][j];
                value += term;
                scale += term.abs();
            }
            let residual = (value - if i == j { 1.0 } else { 0.0 }).abs();
            if !residual.is_finite() || !scale.is_finite() || residual > RESIDUAL_TOLERANCE * scale
            {
                return Err(Error::Numerical);
            }
        }
    }
    Ok(result)
}
