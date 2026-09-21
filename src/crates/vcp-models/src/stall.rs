// SPDX-License-Identifier: Apache-2.0
//! Frozen local transition statistics. A suspicion is an uncalibrated heuristic,
//! never a stall probability, action recommendation or serving qualification.
use serde::{Deserialize, Serialize};

pub const ALGORITHM: &str = "second-order-laplace-entropy/1";
pub const MAX_STATES: usize = 32;
pub const MAX_OBSERVATIONS: usize = 4096;
pub const MAX_WINDOW: usize = 256;
pub const LAPLACE_PRIOR: u64 = 1;
pub const ENTROPY_THRESHOLD: f64 = 0.5;
pub const TRANSITION_THRESHOLD: f64 = 0.75;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    schema_version: u32,
    algorithm: String,
    states: usize,
    minimum_samples: u64,
    training_observations: usize,
    counts: Vec<Vec<u64>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalStatus {
    InsufficientHistory,
    Sparse,
    Observed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Signal {
    pub status: SignalStatus,
    /// The preceding two states of the final observed transition, not a claim
    /// about the cause or next action.
    pub context: Option<[usize; 2]>,
    pub row_counts: Vec<u64>,
    pub row_support: u64,
    pub normalized_entropy: Option<f64>,
    pub observed_transition_probability: Option<f64>,
    pub repeated_strategy_suspected: bool,
}

/// Fit only on caller-supplied training segments. No transitions cross segment
/// boundaries. Alphabet meaning, causal continuity and held-out separation are
/// responsibilities of the caller.
pub fn fit(states: usize, segments: &[Vec<usize>], minimum_samples: u64) -> Result<Model, String> {
    if !(1..=MAX_STATES).contains(&states)
        || !(1..=MAX_OBSERVATIONS as u64).contains(&minimum_samples)
        || segments.len() > MAX_OBSERVATIONS
    {
        return Err("local model exceeds state, segment or sample bounds".into());
    }
    let mut model = Model {
        schema_version: 1,
        algorithm: ALGORITHM.into(),
        states,
        minimum_samples,
        training_observations: 0,
        counts: vec![vec![0; states]; states * states],
    };
    for segment in segments {
        model.training_observations = model
            .training_observations
            .checked_add(segment.len())
            .ok_or("local training observation overflow")?;
        if model.training_observations > MAX_OBSERVATIONS || segment.iter().any(|&s| s >= states) {
            return Err("invalid or oversized local training segment".into());
        }
        for triple in segment.windows(3) {
            model.counts[triple[0] * states + triple[1]][triple[2]] += 1;
        }
    }
    model.validate()?;
    Ok(model)
}

impl Model {
    pub fn states(&self) -> usize {
        self.states
    }
    pub fn minimum_samples(&self) -> u64 {
        self.minimum_samples
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || self.algorithm != ALGORITHM
            || !(1..=MAX_STATES).contains(&self.states)
            || !(1..=MAX_OBSERVATIONS as u64).contains(&self.minimum_samples)
            || self.training_observations > MAX_OBSERVATIONS
            || self.counts.len() != self.states * self.states
            || self.counts.iter().any(|row| row.len() != self.states)
        {
            return Err("invalid frozen local model identity or dimensions".into());
        }
        let mut total = 0u64;
        for count in self.counts.iter().flatten() {
            total = total
                .checked_add(*count)
                .ok_or("local model count overflow")?;
        }
        if total > self.training_observations.saturating_sub(2) as u64 {
            return Err("local model counts exceed training observations".into());
        }
        Ok(())
    }

    /// Read only the frozen row for the final observed triple. This method never
    /// fits or updates counts, including when evaluating replayed observations.
    pub fn evaluate(&self, window: &[usize]) -> Result<Signal, String> {
        self.validate()?;
        if window.len() > MAX_WINDOW || window.iter().any(|&s| s >= self.states) {
            return Err("invalid or oversized local evaluation window".into());
        }
        let mut signal = Signal {
            status: SignalStatus::InsufficientHistory,
            context: None,
            row_counts: vec![],
            row_support: 0,
            normalized_entropy: None,
            observed_transition_probability: None,
            repeated_strategy_suspected: false,
        };
        if window.len() < 3 {
            return Ok(signal);
        }
        let triple = &window[window.len() - 3..];
        let row = &self.counts[triple[0] * self.states + triple[1]];
        signal.context = Some([triple[0], triple[1]]);
        signal.row_counts = row.clone();
        signal.row_support = row.iter().sum(); // validate bounds the total to 4094.
        signal.status = SignalStatus::Sparse;
        if signal.row_support < self.minimum_samples {
            return Ok(signal);
        }
        let denominator = (signal.row_support + LAPLACE_PRIOR * self.states as u64) as f64;
        let entropy: f64 = row
            .iter()
            .map(|&count| {
                let p = (count + LAPLACE_PRIOR) as f64 / denominator;
                -p * p.ln()
            })
            .sum();
        // A one-symbol alphabet has no uncertainty; it still does not establish
        // stalled work and must not be interpreted as calibrated confidence.
        let normalized = if self.states == 1 {
            0.0
        } else {
            entropy / (self.states as f64).ln()
        };
        let probability = (row[triple[2]] + LAPLACE_PRIOR) as f64 / denominator;
        if !normalized.is_finite()
            || !probability.is_finite()
            || !(-1e-12..=1.0 + 1e-12).contains(&normalized)
            || !(0.0..=1.0).contains(&probability)
        {
            return Err("nonfinite or invalid local transition statistic".into());
        }
        signal.status = SignalStatus::Observed;
        signal.normalized_entropy = Some(normalized.clamp(0.0, 1.0));
        signal.observed_transition_probability = Some(probability);
        signal.repeated_strategy_suspected =
            normalized <= ENTROPY_THRESHOLD && probability >= TRANSITION_THRESHOLD;
        Ok(signal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_low_entropy_is_only_an_observed_transition_heuristic() {
        let model = fit(2, &[vec![0; 32]], 8).unwrap();
        let before = serde_json::to_vec(&model).unwrap();
        let signal = model.evaluate(&[0, 0, 0]).unwrap();
        assert_eq!(signal.status, SignalStatus::Observed);
        assert_eq!(signal.row_counts, vec![30, 0]);
        assert_eq!(signal.row_support, 30);
        assert!(signal.normalized_entropy.unwrap() < ENTROPY_THRESHOLD);
        assert!(signal.repeated_strategy_suspected);
        assert!(
            !model
                .evaluate(&[0, 0, 1])
                .unwrap()
                .repeated_strategy_suspected
        );
        assert_eq!(model.evaluate(&[0, 0, 0]).unwrap(), signal);
        assert_eq!(serde_json::to_vec(&model).unwrap(), before);
        let replay: Model = serde_json::from_slice(&before).unwrap();
        assert_eq!(replay.evaluate(&[0, 0, 0]).unwrap(), signal);
    }

    #[test]
    fn gaps_sparse_rows_and_unpredictable_rows_do_not_claim_suspicion() {
        let model = fit(2, &[vec![0, 0], vec![0, 0]], 1).unwrap();
        assert_eq!(
            model.evaluate(&[0, 0]).unwrap().status,
            SignalStatus::InsufficientHistory
        );
        let sparse = model.evaluate(&[0, 0, 0]).unwrap();
        assert_eq!(sparse.status, SignalStatus::Sparse);
        assert_eq!(sparse.row_support, 0);
        assert!(sparse.normalized_entropy.is_none());
        assert!(!sparse.repeated_strategy_suspected);
        let segments: Vec<_> = (0..20).map(|i| vec![0, 0, i % 2]).collect();
        let uniform = fit(2, &segments, 10).unwrap().evaluate(&[0, 0, 0]).unwrap();
        assert_eq!(uniform.row_counts, vec![10, 10]);
        assert!((uniform.normalized_entropy.unwrap() - 1.0).abs() < 1e-12);
        assert!(!uniform.repeated_strategy_suspected);
    }

    #[test]
    fn validates_serialized_models_and_every_input_bound() {
        assert!(fit(0, &[], 1).is_err());
        assert!(fit(33, &[], 1).is_err());
        assert!(fit(2, &[], 0).is_err());
        assert!(fit(2, &[vec![2]], 1).is_err());
        assert!(fit(2, &[vec![0; MAX_OBSERVATIONS + 1]], 1).is_err());
        assert!(fit(1, &vec![vec![]; MAX_OBSERVATIONS + 1], 1).is_err());
        let model = fit(1, &[vec![0; MAX_OBSERVATIONS]], 1).unwrap();
        assert_eq!(
            model.evaluate(&[0, 0, 0]).unwrap().normalized_entropy,
            Some(0.0)
        );
        assert!(model.evaluate(&vec![0; MAX_WINDOW + 1]).is_err());
        assert!(model.evaluate(&[1]).is_err());
        for mutation in 0..5 {
            let mut value = serde_json::to_value(&model).unwrap();
            match mutation {
                0 => value["counts"][0][0] = serde_json::json!(u64::MAX),
                1 => value["states"] = serde_json::json!(33),
                2 => value["algorithm"] = serde_json::json!("future"),
                3 => value["minimum_samples"] = serde_json::json!(0),
                _ => value["counts"] = serde_json::json!([]),
            }
            let invalid: Model = serde_json::from_value(value).unwrap();
            assert!(invalid.evaluate(&[0, 0, 0]).is_err());
        }
    }
}
