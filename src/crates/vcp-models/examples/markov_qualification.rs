// SPDX-License-Identifier: Apache-2.0
//! Frozen synthetic four-arm rejection record. Never calls a provider or enables policy.
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeSet, time::Instant};
use vcp_models::{escalation, stall};
use vcp_protocol::digest_bytes;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: String,
    revision: String,
    purpose: String,
    synthetic_only: bool,
    states: usize,
    minimum_row_samples: u64,
    minimum_repeated_failures: u32,
    criteria: Criteria,
    splits: Value,
    uncertainty: String,
    rollback: Vec<String>,
    cases: Vec<Case>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Criteria {
    minimum_held_out_projects: usize,
    minimum_held_out_cases: usize,
    minimum_precision: f64,
    minimum_recall: f64,
    minimum_coverage: f64,
    maximum_serious_misses: usize,
    maximum_local_evaluation_micros: u128,
    require_live_outcome_trials: bool,
    require_calibrated_intervals: bool,
    require_known_liabilities: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    project: String,
    partition: String,
    training_from: u64,
    training_until: u64,
    observed_at: u64,
    outcome_at: u64,
    training: String,
    window: String,
    before: String,
    after: String,
    failed_verifications: u32,
    stall_label: bool,
    serious: bool,
    reconciled_outcome: String,
    known_cost_micros: Option<u64>,
    label_source: String,
}
fn symbols(text: &str, states: usize) -> Result<Vec<usize>, String> {
    if text.len() > stall::MAX_OBSERVATIONS {
        return Err("oversized fixture".into());
    }
    text.bytes()
        .map(|b| match b.checked_sub(b'0') {
            Some(n) if (n as usize) < states => Ok(n as usize),
            _ => Err("invalid synthetic symbol".into()),
        })
        .collect()
}
fn validate(m: &Manifest) -> Result<(), String> {
    if m.schema != "p6-markov-four-arm-manifest/1"
        || !m.synthetic_only
        || m.purpose != "repeated_strategy_suspicion"
        || m.cases.len() > 256
        || m.cases.is_empty()
        || m.states != 2
        || m.minimum_row_samples != 8
        || m.minimum_repeated_failures != 3
    {
        return Err("invalid frozen comparison contract".into());
    }
    let mut ids = BTreeSet::new();
    let mut projects = BTreeSet::new();
    for case in &m.cases {
        if !ids.insert(&case.id)
            || !projects.insert(&case.project)
            || case.training_from >= case.training_until
            || case.training_until >= case.observed_at
            || case.observed_at >= case.outcome_at
            || !["tuning", "calibration", "held_out"].contains(&case.partition.as_str())
            || !["completed", "failed"].contains(&case.reconciled_outcome.as_str())
            || case.label_source.is_empty()
            || case.window.len() > stall::MAX_WINDOW
        {
            return Err("split, project, label or temporal leakage".into());
        }
        symbols(&case.training, m.states)?;
        symbols(&case.window, m.states)?;
    }
    for pair in [["tuning", "calibration"], ["calibration", "held_out"]] {
        let end = m
            .cases
            .iter()
            .filter(|c| c.partition == pair[0])
            .map(|c| c.outcome_at)
            .max()
            .ok_or("missing split")?;
        let begin = m
            .cases
            .iter()
            .filter(|c| c.partition == pair[1])
            .map(|c| c.training_from)
            .min()
            .ok_or("missing split")?;
        if end >= begin {
            return Err("time splits overlap".into());
        }
    }
    let c = &m.criteria;
    if c.minimum_held_out_cases < 40
        || c.minimum_held_out_projects < 20
        || c.minimum_precision < 0.95
        || c.minimum_precision > 1.0
        || c.minimum_recall < 0.95
        || c.minimum_recall > 1.0
        || c.minimum_coverage < 0.95
        || c.minimum_coverage > 1.0
        || c.maximum_serious_misses != 0
        || c.maximum_local_evaluation_micros == 0
        || c.maximum_local_evaluation_micros > 5000
        || !c.require_live_outcome_trials
        || !c.require_calibrated_intervals
        || !c.require_known_liabilities
    {
        return Err("qualification gates weakened".into());
    }
    Ok(())
}
#[derive(Default)]
struct Metrics {
    total: usize,
    observed: usize,
    tp: usize,
    fp: usize,
    fn_: usize,
    tn: usize,
    positive_cases: usize,
    serious_misses: usize,
    unknown: usize,
    max_micros: u128,
}
impl Metrics {
    fn add(&mut self, c: &Case, prediction: Option<bool>, elapsed: u128) {
        self.total += 1;
        if c.stall_label {
            self.positive_cases += 1;
        }
        self.max_micros = self.max_micros.max(elapsed);
        if c.known_cost_micros.is_none() {
            self.unknown += 1;
        }
        if let Some(p) = prediction {
            self.observed += 1;
            match (p, c.stall_label) {
                (true, true) => self.tp += 1,
                (true, false) => self.fp += 1,
                (false, true) => self.fn_ += 1,
                (false, false) => self.tn += 1,
            }
        }
        // Abstentions do not erase serious undetected stalls.
        if c.stall_label && c.serious && prediction != Some(true) {
            self.serious_misses += 1;
        }
    }
    fn value(&self) -> Value {
        json!({"cases":self.total,"covered":self.observed,"abstained":self.total-self.observed,
            "true_positive":self.tp,"false_positive":self.fp,"false_negative":self.fn_,"true_negative":self.tn,
            "serious_misses_including_abstention":self.serious_misses,"unknown_costs":self.unknown,
            "precision":proportion(self.tp,self.tp+self.fp),"recall":proportion(self.tp,self.positive_cases),
            "coverage":proportion(self.observed,self.total),"maximum_evaluation_wall_micros":self.max_micros,
            "uncertainty":"Wilson95% descriptive bounds on synthetic fixtures; correlated fixture families are not population evidence"})
    }
    fn rejected(&self, c: &Criteria, projects: usize) -> Vec<&'static str> {
        let mut why = vec![
            "synthetic_only_no_live_outcome_trials",
            "no_calibrated_forecast_intervals",
            "no_authorized_live_cap",
            "unqualified_source_and_config",
        ];
        if self.total < c.minimum_held_out_cases || projects < c.minimum_held_out_projects {
            why.push("insufficient_held_out_projects_or_cases");
        }
        if lower(self.tp, self.tp + self.fp) < c.minimum_precision {
            why.push("precision_floor");
        }
        if lower(self.tp, self.positive_cases) < c.minimum_recall {
            why.push("recall_floor");
        }
        if lower(self.observed, self.total) < c.minimum_coverage {
            why.push("coverage_floor");
        }
        if self.serious_misses > c.maximum_serious_misses {
            why.push("serious_miss");
        }
        if self.unknown > 0 {
            why.push("unknown_liability");
        }
        if self.max_micros > c.maximum_local_evaluation_micros {
            why.push("local_overhead");
        }
        why
    }
}
fn bounds(success: usize, total: usize) -> Option<(f64, f64)> {
    if total == 0 {
        return None;
    }
    let n = total as f64;
    let p = success as f64 / n;
    let z = 1.959963984540054;
    let denominator = 1.0 + z * z / n;
    let center = (p + z * z / (2.0 * n)) / denominator;
    let radius = z * ((p * (1.0 - p) + z * z / (4.0 * n)) / n).sqrt() / denominator;
    Some(((center - radius).max(0.0), (center + radius).min(1.0)))
}
fn lower(s: usize, n: usize) -> f64 {
    bounds(s, n).map(|b| b.0).unwrap_or(0.0)
}
fn proportion(s: usize, n: usize) -> Value {
    match bounds(s, n) {
        Some((lo, hi)) => {
            json!({"numerator":s,"denominator":n,"point":s as f64/n as f64,"lower95":lo,"upper95":hi})
        }
        None => json!({"numerator":s,"denominator":n,"unavailable":"empty denominator"}),
    }
}

fn evaluate(m: &Manifest, manifest_digest: &str) -> Result<Value, String> {
    validate(m)?;
    let mut cases = Vec::new();
    let mut arms = Vec::new();
    for arm in ["rules_only", "local_statistics"] {
        let mut held = Metrics::default();
        let mut all = Metrics::default();
        for case in &m.cases {
            // Every project fits only its private earlier prefix. No pooled shipping fit.
            let training = symbols(&case.training, m.states)?;
            let window = symbols(&case.window, m.states)?;
            let fit_start = Instant::now();
            let model = if arm == "local_statistics" {
                Some(stall::fit(m.states, &[training], m.minimum_row_samples)?)
            } else {
                None
            };
            let fit_nanos = fit_start.elapsed().as_nanos();
            let start = Instant::now();
            let (prediction, evidence) = if let Some(model) = model {
                let signal = model.evaluate(&window)?;
                let prediction = (signal.status == stall::SignalStatus::Observed)
                    .then_some(signal.repeated_strategy_suspected);
                (
                    prediction,
                    serde_json::to_value(signal).map_err(|e| e.to_string())?,
                )
            } else {
                let signal = escalation::repeated_strategy_signal(
                    &digest_bytes(case.before.as_bytes()),
                    &digest_bytes(case.after.as_bytes()),
                    case.failed_verifications,
                    m.minimum_repeated_failures,
                )
                .map_err(|e| e.to_string())?;
                (
                    Some(signal),
                    json!({"equal_progress_fingerprint":case.before==case.after,"failed_verifications":case.failed_verifications}),
                )
            };
            let elapsed = start.elapsed();
            all.add(case, prediction, elapsed.as_micros());
            if case.partition == "held_out" {
                held.add(case, prediction, elapsed.as_micros());
            }
            cases.push(json!({"arm":arm,"case":case.id,"project":case.project,"partition":case.partition,"prediction":prediction,"label":case.stall_label,"evidence":evidence,"fit_wall_nanos":fit_nanos,"evaluation_wall_nanos":elapsed.as_nanos(),"synthetic_outcome":case.reconciled_outcome,"synthetic_known_cost_micros":case.known_cost_micros}));
        }
        arms.push(json!({"arm":arm,"status":"ran_synthetic","supported_purpose":m.purpose,"held_out":held.value(),"all_splits":all.value(),"qualification":{"enabled":false,"status":"rejected","reasons":held.rejected(&m.criteria,held.total)}}));
    }
    for arm in ["actual_jev_openrouter", "conventional_openrouter"] {
        arms.push(json!({"arm":arm,"status":"not_run","requests":0,"known_spend_micros":0,"reason":"No authorized live cap and outcome trial evidence. No substitute endpoint or fixture result is attributed to this arm.","qualification":{"enabled":false,"status":"not_run"}}));
    }
    Ok(
        json!({"schema":"p6-markov-four-arm-result/1","manifest_revision":m.revision,"manifest_sha256":manifest_digest,"algorithm":stall::ALGORITHM,"purpose":m.purpose,"splits":m.splits,"uncertainty":m.uncertainty,"rollback_triggers":m.rollback,"arms":arms,"cases":cases,
        "unsupported_purposes":["routing","review","policy_proposal"],
        "probability_scores":{"status":"unavailable","reason":"Transition probability and entropy suspicion are not stall probabilities; no Brier or calibration score is computed."},
        "forecast_validation":{"status":"not_run","reason":"M3 arithmetic fixtures are contract tests; independent forecast outcome and interval coverage datasets are absent."},
        "task_benefit":{"status":"not_run","reason":"Synthetic oracle labels/costs are not completed live task cost, quality, intervention or latency measurements."},
        "overhead":{"cpu_time":"unavailable; only local wall timing measured","peak_memory":"unavailable; no process memory sampler","inspection":"not_run; covered by separate M3 contract tests"},
        "hidden_state":{"status":"not_promoted","reason":"Identical observed symbols and fingerprints have opposite synthetic labels. Hidden causes are not identifiable from this input; no evidence supports HMM deployment."},
        "serving":{"enabled":false,"qualified_cost_estimate":"not_installed","baseline":"unchanged","live_provider_requests":0}}),
    )
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let manifest = args
        .next()
        .ok_or("expected manifest path and output path")?;
    let output = args.next().ok_or("expected output path")?;
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let bytes = std::fs::read(manifest)?;
    let m: Manifest =
        serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes))?;
    let value = evaluate(&m, &digest_bytes(&bytes))?;
    std::fs::write(output, serde_json::to_vec_pretty(&value)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Manifest {
        serde_json::from_slice(include_bytes!("../../../evals/markov/manifest.json")).unwrap()
    }
    #[test]
    fn rejection_preserves_sparse_serious_unknown_and_calibration_gates() {
        let m = fixture();
        validate(&m).unwrap();
        let mut metrics = Metrics::default();
        let case = m
            .cases
            .iter()
            .find(|c| c.partition == "held_out" && c.serious)
            .unwrap();
        metrics.add(case, None, 1);
        let reasons = metrics.rejected(&m.criteria, 1);
        for required in [
            "insufficient_held_out_projects_or_cases",
            "serious_miss",
            "unknown_liability",
            "no_calibrated_forecast_intervals",
        ] {
            assert!(reasons.contains(&required));
        }
        let result = evaluate(&m, "frozen").unwrap();
        assert_eq!(result["arms"].as_array().unwrap().len(), 4);
        assert_eq!(result["serving"]["enabled"], false);
        assert_eq!(result["arms"][2]["status"], "not_run");
    }
    #[test]
    fn split_leakage_and_weakened_gates_are_rejected() {
        let mut m = fixture();
        m.cases[4].project = m.cases[0].project.clone();
        assert!(validate(&m).is_err());
        let mut m = fixture();
        m.cases[4].training_until = 100;
        assert!(validate(&m).is_err());
        let mut m = fixture();
        m.criteria.require_live_outcome_trials = false;
        assert!(validate(&m).is_err());
        let mut m = fixture();
        m.criteria.minimum_precision = 0.5;
        assert!(validate(&m).is_err());
        let mut m = fixture();
        m.criteria.maximum_local_evaluation_micros = 5001;
        assert!(validate(&m).is_err());
        let mut m = fixture();
        m.cases[4].training_from = 110;
        assert!(validate(&m).is_err());
        assert_eq!(bounds(0, 0), None);
        let (lo, hi) = bounds(5, 10).unwrap();
        assert!((lo - 0.2365930905).abs() < 1e-9);
        assert!((hi - 0.7634069095).abs() < 1e-9);
    }
    #[test]
    fn indistinguishable_observations_do_not_identify_the_hidden_cause() {
        let m = fixture();
        let left = &m.cases[8];
        let right = &m.cases[9];
        assert_eq!(left.training, right.training);
        assert_eq!(left.window, right.window);
        assert_eq!((&left.before, &left.after), (&right.before, &right.after));
        assert_ne!(left.stall_label, right.stall_label);
        let model = stall::fit(
            m.states,
            &[symbols(&left.training, m.states).unwrap()],
            m.minimum_row_samples,
        )
        .unwrap();
        assert_eq!(
            model
                .evaluate(&symbols(&left.window, m.states).unwrap())
                .unwrap(),
            model
                .evaluate(&symbols(&right.window, m.states).unwrap())
                .unwrap()
        );
    }
}
