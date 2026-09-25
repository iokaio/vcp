// SPDX-License-Identifier: Apache-2.0
//! Predeclared P10-03 diagnostic grading; no task-outcome or human-time claims.
use super::*;

#[test]
fn observer_diagnostic_matched_campaign() {
    // Labels are declared independently of the analyzer. The two environment /
    // productive cases intentionally count as repetition, never a stall.
    let cases: &[(&str, &[u8], Option<&str>, usize)] = &[
        ("constant", &[1, 1, 1], None, 1),
        ("alternating", &[1, 2, 1, 2, 1, 2], None, 1),
        ("incomplete", &[1, 1], None, 0),
        ("changing_diagnostics", &[1, 2, 3, 4], None, 0),
        ("success_reset", &[1, 1, 1, 1, 1], Some("success"), 0),
        ("unavailable", &[1, 1, 1, 1, 1], Some("unknown"), 0),
        ("source_edit", &[1, 1, 1, 1, 1], Some("edit"), 0),
        ("correction", &[1, 1, 1, 1, 1], Some("steering"), 0),
        ("missing_evidence", &[1, 1, 1], Some("gap"), 0),
        ("pruned_evidence", &[1, 1, 1], Some("pruned"), 0),
        ("environment_repetition", &[3, 3, 3], None, 1),
        ("productive_repetition", &[4, 4, 4], None, 1),
    ];
    let mut rows = Vec::new();
    for &(name, pattern, mutation, expected) in cases {
        let mut source = tests::evidence(pattern);
        match mutation {
            Some("success") => {
                source.verifications[2].checks[0].result = CheckResult::Passed;
                source.verifications[2].checks[0].failure_signature = None;
            }
            Some("unknown") => source.verifications[2].checks[0].failure_signature = None,
            Some("edit") => source.verifications[2].input_fingerprint = digest_bytes(b"edit"),
            Some("steering") => source.verifications[2].steering = SteeringRevision::new(1),
            Some("gap") => source.gaps.push(observations::Gap {
                task: source.verifications[0].task.clone(),
                event: EventId::new(),
                reason: observations::GapReason::MissingFacts,
            }),
            Some("pruned") => source.excluded_pruned_records = 1,
            None => {}
            Some(_) => panic!("unknown fixture mutation"),
        }
        let before = source.clone();
        let start = std::time::Instant::now();
        let proposals = analyze(&source).unwrap();
        let elapsed = start.elapsed().as_micros();
        assert_eq!(proposals.len(), expected, "{name}");
        assert_eq!(source, before, "observer must not mutate its evidence");
        let references: usize = proposals.iter().map(|p| p.verifications.len()).sum();
        rows.push(serde_json::json!({
            "fixture": name, "expected_notices": expected,
            "disabled": { "observer_evaluations": 0, "notices": 0,
                "individual_record_inspections_for_grouped_evidence": references },
            "enabled": { "observer_evaluations": 1, "notices": proposals.len(),
                "grouped_evidence_views": proposals.len(), "local_elapsed_micros": elapsed,
                "proposal_bytes": canonical_bytes(&proposals).unwrap().len() },
            "main_requests": 0, "helper_requests": 0,
            "main_billable_micros": 0, "helper_billable_micros": 0,
            "controller_interventions": 0, "task_outcome_benefit": "unmeasured"
        }));
    }
    println!(
        "P10_OBSERVER_DIAGNOSTIC_CAMPAIGN={}",
        serde_json::json!({
            "schema_version": 1, "scope": "pure diagnostic fixtures; native lifecycle graded separately",
            "default_enabled": false, "rows": rows
        })
    );
}
