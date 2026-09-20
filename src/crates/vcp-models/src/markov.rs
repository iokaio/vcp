// SPDX-License-Identifier: Apache-2.0
//! Bounded local arithmetic. No history access, persistence or qualified routing
//! estimate is implied. Callers own alphabet legality, cohorts and attribution.

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
