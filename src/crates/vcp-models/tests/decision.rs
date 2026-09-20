// SPDX-License-Identifier: Apache-2.0
//! Public synthetic protocol fixtures; no transport or live qualification claim.
use serde_json::{json, Value};
use std::collections::BTreeMap;
use vcp_domain::{workspace::Scope, *};
use vcp_models::decision::*;

fn fixture() -> (Request, Policy) {
    let state = json!({"evidence":"Review failed twice; current check reports unresolved error."});
    let binding = Binding {
        scope: Scope {
            workspace: WorkspaceId::parse("workspace").unwrap(),
            session: SessionId::parse("session").unwrap(),
            task: TaskId::parse("task").unwrap(),
        },
        root: TaskId::parse("root").unwrap(),
        step: Revision::new(3),
        steering: SteeringRevision::new(2),
        authority: AuthorityRevision::new(4),
        deletion: DeletionEpoch::new(1),
        policy: "a".repeat(64),
        catalog: "b".repeat(64),
        input: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&state).unwrap()),
        evidence: BTreeMap::from([("evidence-1".into(), "c".repeat(64))]),
    };
    let request = Request {
        version: VERSION,
        binding,
        purpose: Purpose::Escalation,
        question_revision: "d".repeat(64),
        state,
        questions: BTreeMap::from([
            (
                "repeat".into(),
                Question::Boolean {
                    instructions: "Does the evidence describe repeated failure?".into(),
                    yes: "More than one failed check is observed".into(),
                    no: "No repeated failure is observed".into(),
                },
            ),
            (
                "action".into(),
                Question::Choice {
                    instructions: "Choose the useful next action among the permitted options"
                        .into(),
                    options: BTreeMap::from([
                        ("review".into(), "Seek independent review".into()),
                        ("stop".into(), "Stop for missing information".into()),
                    ]),
                },
            ),
            (
                "risk".into(),
                Question::Score {
                    instructions: "Rate the risk using the supplied levels".into(),
                    levels: vec![
                        "No unresolved error".into(),
                        "Local unresolved error".into(),
                        "Broad unresolved failure".into(),
                    ],
                },
            ),
        ]),
        deadline: Timestamp::new(2000),
    };
    let evaluator = QualifiedEvaluator {
        model: "typesafe/jev-1.13".into(),
        provider: "typesafe".into(),
        served_model: "typesafe/jev-1.13-20260917".into(),
        served_provider: "TypeSafe".into(),
        operation: Operation::JevDecisions,
        purpose: Purpose::Escalation,
        mode: Mode::Shadow,
        evidence_digest: "e".repeat(64),
        configuration_digest: "f".repeat(64),
        valid_until: Timestamp::new(3000),
        require_distributions: true,
        require_confidence: true,
        deny_data_collection: true,
        require_zdr: true,
        prompt_price_per_million: "0.042".into(),
        output_price_per_million: "0".into(),
        request_price: "0.001".into(),
    };
    (
        request,
        Policy {
            mode: Mode::Shadow,
            evaluator: Some(evaluator),
            attempt_limit: 2,
            attempts_used: 0,
        },
    )
}
fn response() -> Value {
    json!({"id":"synthetic-request","model":"typesafe/jev-1.13-20260917","provider":"TypeSafe",
        "usage":{"input_tokens":300,"output_tokens":50,"cost":0.0000126},
        "answers":{"repeat":{"type":"noul","noul":0.8},"action":{"type":"choice","choice":"review","probabilities":{"review":0.7,"stop":0.3},"confidence":0.4},
        "risk":{"type":"score","score":1.25,"probabilities":{"0":0.0,"1":0.75,"2":0.25},"confidence":0.5,"legend":{"0":"No unresolved error","1":"Local unresolved error","2":"Broad unresolved failure"}}}})
}
fn outcome(request: &Request, policy: &Policy, value: Value) -> Outcome {
    let prepared = prepare(request, policy, Timestamp::new(1000))
        .unwrap()
        .unwrap();
    decode(
        &prepared,
        &serde_json::to_vec(&value).unwrap(),
        &request.binding,
        Timestamp::new(1001),
    )
}
#[test]
fn disabled_and_rules_have_no_remote_preparation_even_with_no_qualification() {
    let (request, _) = fixture();
    for mode in [Mode::Disabled, Mode::Deterministic] {
        let policy = Policy {
            mode,
            ..Policy::default()
        };
        assert!(prepare(&request, &policy, Timestamp::new(1000))
            .unwrap()
            .is_none());
        assert!(matches!(baseline(mode), Outcome::Baseline { .. }));
    }
    assert_eq!(Policy::default().mode, Mode::Disabled);
    assert!(Usage::default().unknown_liability);
}
#[test]
fn native_wire_keeps_questions_controls_and_probabilities_separate() {
    let (request, policy) = fixture();
    let prepared = prepare(&request, &policy, Timestamp::new(1000))
        .unwrap()
        .unwrap();
    assert_eq!(
        prepared.endpoint(),
        "https://openrouter.ai/api/alpha/decisions"
    );
    assert_eq!(prepared.body()["provider"]["only"], json!(["typesafe"]));
    assert_eq!(prepared.body()["provider"]["allow_fallbacks"], false);
    assert_eq!(prepared.body()["provider"]["data_collection"], "deny");
    assert_eq!(prepared.body()["provider"]["zdr"], true);
    assert!(prepared.body().get("messages").is_none());
    assert!(prepared.body().get("tools").is_none());
    assert_eq!(prepared.body()["questions"]["repeat"]["type"], "noul");
    let Outcome::Advice {
        answers,
        usage,
        mode,
        ..
    } = outcome(&request, &policy, response())
    else {
        panic!("valid native advice")
    };
    assert_eq!(mode, Mode::Shadow);
    assert_eq!(
        answers["repeat"],
        Answer::Boolean {
            yes_probability: 0.8
        }
    );
    let Answer::Choice {
        probabilities,
        confidence,
        ..
    } = &answers["action"]
    else {
        panic!("choice")
    };
    assert_eq!(probabilities.as_ref().unwrap()["review"], 0.7);
    assert_eq!(*confidence, Some(0.4));
    assert_eq!(usage.observed_cost, Some(Micros::new(13)));
    assert!(!usage.unknown_liability);
}
#[test]
fn every_malformed_answer_abstains_without_losing_observed_usage() {
    let (request, policy) = fixture();
    let mut cases = Vec::new();
    let mut bad = response();
    bad["answers"].as_object_mut().unwrap().remove("repeat");
    cases.push(bad);
    let mut bad = response();
    bad["answers"]["injected"] = json!({"type":"noul","noul":1});
    cases.push(bad);
    let mut bad = response();
    bad["answers"]["repeat"]["noul"] = json!(true);
    cases.push(bad);
    let mut bad = response();
    bad["answers"]["repeat"]["noul"] = json!(1.1);
    cases.push(bad);
    let mut bad = response();
    bad["answers"]["repeat"]["confidence"] = json!(0.9);
    cases.push(bad);
    let mut bad = response();
    bad["answers"]["action"]["choice"] = json!("run_shell");
    cases.push(bad);
    let mut bad = response();
    bad["answers"]["action"]["probabilities"]["review"] = json!(0.6);
    cases.push(bad);
    let mut bad = response();
    bad["answers"]["action"]["choice"] = json!("stop");
    cases.push(bad);
    let mut bad = response();
    bad["answers"]["risk"]["score"] = json!(2);
    cases.push(bad);
    let mut bad = response();
    bad["answers"]["risk"]["legend"]["1"] = json!("New rubric");
    cases.push(bad);
    let mut bad = response();
    bad["answers"]["action"]
        .as_object_mut()
        .unwrap()
        .remove("probabilities");
    cases.push(bad);
    for bad in cases {
        let Outcome::Abstain { usage, .. } = outcome(&request, &policy, bad) else {
            panic!("malformed batch was accepted")
        };
        assert_eq!(usage.observed_cost, Some(Micros::new(13)));
    }
}
#[test]
fn missing_native_fields_and_unknown_charge_are_not_fabricated() {
    let (request, mut policy) = fixture();
    let evaluator = policy.evaluator.as_mut().unwrap();
    evaluator.require_distributions = false;
    evaluator.require_confidence = false;
    let mut value = response();
    for id in ["action", "risk"] {
        let answer = value["answers"][id].as_object_mut().unwrap();
        answer.remove("probabilities");
        answer.remove("confidence");
    }
    value["usage"].as_object_mut().unwrap().remove("cost");
    let Outcome::Advice { answers, usage, .. } = outcome(&request, &policy, value) else {
        panic!("explicit reduced capability")
    };
    assert!(usage.unknown_liability);
    assert_eq!(usage.observed_cost, None);
    assert!(matches!(
        answers["action"],
        Answer::Choice {
            probabilities: None,
            confidence: None,
            ..
        }
    ));
}
#[test]
fn stale_late_advice_retains_charge_and_changes_never_reuse_input() {
    let (mut request, policy) = fixture();
    let prepared = prepare(&request, &policy, Timestamp::new(1000))
        .unwrap()
        .unwrap();
    let bytes = serde_json::to_vec(&response()).unwrap();
    let mut changed = request.binding.clone();
    changed.deletion = changed.deletion.next().unwrap();
    for (binding, now) in [(&changed, 1001), (&request.binding, 2000)] {
        let Outcome::Abstain { reason, usage } =
            decode(&prepared, &bytes, binding, Timestamp::new(now))
        else {
            panic!("stale accepted")
        };
        assert_eq!(reason, "stale_advice");
        assert_eq!(usage.observed_cost, Some(Micros::new(13)));
    }
    request.state["evidence"] = json!("injected alternative");
    assert!(prepare(&request, &policy, Timestamp::new(1000)).is_err());
}
#[test]
fn duplicate_keys_nonfinite_and_byte_bounds_fail_closed() {
    let (request, policy) = fixture();
    let prepared = prepare(&request, &policy, Timestamp::new(1000))
        .unwrap()
        .unwrap();
    for bytes in [
        b"{\"answers\":{},\"answers\":{}}".to_vec(),
        b"{\"noul\":NaN}".to_vec(),
        b"{\"noul\":1e999}".to_vec(),
        vec![b' '; MAX_BYTES + 1],
    ] {
        let Outcome::Abstain { usage, .. } =
            decode(&prepared, &bytes, &request.binding, Timestamp::new(1001))
        else {
            panic!("invalid JSON accepted")
        };
        assert!(usage.unknown_liability);
    }
    let mut policy = policy;
    policy.attempts_used = policy.attempt_limit;
    assert!(prepare(&request, &policy, Timestamp::new(1000)).is_err());
}
#[test]
fn conventional_answers_are_discrete_closed_and_cannot_claim_native_certainty() {
    let (request, mut policy) = fixture();
    let evaluator = policy.evaluator.as_mut().unwrap();
    evaluator.operation = Operation::ConventionalChat;
    evaluator.model = "synthetic/comparator".into();
    evaluator.served_model = "synthetic/comparator".into();
    evaluator.provider = "synthetic".into();
    evaluator.served_provider = "Synthetic".into();
    assert!(prepare(&request, &policy, Timestamp::new(1000)).is_err());
    let evaluator = policy.evaluator.as_mut().unwrap();
    evaluator.require_distributions = false;
    evaluator.require_confidence = false;
    let prepared = prepare(&request, &policy, Timestamp::new(1000))
        .unwrap()
        .unwrap();
    assert_eq!(
        prepared.endpoint(),
        "https://openrouter.ai/api/v1/chat/completions"
    );
    assert_eq!(
        prepared.body()["response_format"]["json_schema"]["strict"],
        true
    );
    assert!(prepared.body().get("tools").is_none());
    let mut value = json!({"model":"synthetic/comparator","provider":"Synthetic","choices":[{"index":0,"finish_reason":"stop","message":{"role":"assistant","content":"{\"repeat\":true,\"action\":\"review\",\"risk\":null}"}}],"usage":{"prompt_tokens":20,"completion_tokens":10}});
    let Outcome::Advice { answers, usage, .. } = outcome(&request, &policy, value.clone()) else {
        panic!("discrete advice")
    };
    assert_eq!(answers["repeat"], Answer::DiscreteBoolean { value: true });
    assert_eq!(answers["risk"], Answer::Abstain);
    assert!(usage.unknown_liability);
    value["choices"][0]["finish_reason"] = json!("length");
    assert!(matches!(
        outcome(&request, &policy, value),
        Outcome::Abstain { .. }
    ));
}
