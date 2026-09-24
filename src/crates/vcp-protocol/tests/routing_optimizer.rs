// SPDX-License-Identifier: Apache-2.0
use serde_json::{json, Value};
use vcp_protocol::routing_optimizer as wire;

fn request(edits: Value) -> Value {
    json!({"scope":{"workspace":"workspace","session":"session"},"expected_policy_revision":"9007199254740993","proposal":{"kind":"apply","report":"capture-command","edits":edits}})
}
fn valid(edits: Value) -> bool {
    serde_json::from_value::<wire::PreviewRequest>(request(edits))
        .is_ok_and(|v| v.validate().is_ok())
}

#[test]
fn exact_edits_are_bounded_unique_and_never_raw_configuration() {
    assert!(valid(
        json!([{"field":"output_tokens","value":"9007199254740993"},{"field":"pin","value":{"candidate":{"model":"model","endpoint":"qualified"},"fallback_candidates":[{"model":"other","endpoint":"qualified"}]}}])
    ));
    for edits in [
        json!([]),
        json!([{"field":"output_tokens","value":"0"}]),
        json!([{"field":"output_tokens","value":"1"},{"field":"output_tokens","value":"2"}]),
        json!([{"field":"allowed_models","value":["m","m"]}]),
        json!([{"field":"allowed_models","value":vec!["m";65]}]),
        json!([{"field":"allowed_endpoints","value":["https://secret.example"]}]),
        json!([{"field":"allowed_models","value":["line\nsecret"]}]),
        json!([{"field":"ordering","value":["total_cost","latency","quality","quality"]}]),
        json!([{"field":"minimum_samples","value":0}]),
        json!([{"field":"maximum_evidence_age_ms","value":"0"}]),
        json!([{"field":"quality_floor_bps","value":10001}]),
        json!([{"field":"escalation_max_total_attempts","value":65}]),
        json!([{"field":"retrieval_limits","value":{"results":64,"tokens":"16385","bytes":"65536"}}]),
        json!([{"field":"provider_key","value":"secret"}]),
        json!([{"field":"output_tokens","value":"1","raw_policy":{}}]),
        json!([{"field":"pin","value":{"candidate":{"model":"m","endpoint":"e"},"fallback_candidates":[{"model":"m","endpoint":"e"}]}}]),
    ] {
        assert!(!valid(edits.clone()), "accepted {edits}");
    }
    let mut value = request(json!([{"field":"allowed_models","value":[]}]));
    value["proposal"]["ceilings"] = json!({});
    assert!(serde_json::from_value::<wire::PreviewRequest>(value).is_err());
}

#[test]
fn capture_and_review_pin_distinct_revisions_and_strict_payloads() {
    let value = json!({"scope":{"workspace":"w","session":"s"},"mutation":{"command_id":"capture-command","expected_revision":"2","steering_revision":"0"},"expected_binding_revision":"3","window":{"from":"1","until":"9007199254740993"},"coverage":"session"});
    let capture: wire::ReportCapture = serde_json::from_value(value.clone()).unwrap();
    assert!(capture.validate().is_ok());
    assert_eq!(serde_json::to_value(&capture).unwrap(), value);
    let mut bad = value.clone();
    bad["mutation"]["steering_revision"] = json!("1");
    assert!(serde_json::from_value::<wire::ReportCapture>(bad)
        .unwrap()
        .validate()
        .is_err());
    let mut bad = value.clone();
    bad["window"]["from"] = json!("9007199254740994");
    assert!(serde_json::from_value::<wire::ReportCapture>(bad)
        .unwrap()
        .validate()
        .is_err());
    let mut bad = value;
    bad["actor"] = json!("other");
    assert!(serde_json::from_value::<wire::ReportCapture>(bad).is_err());
    let mut rollback = json!({"scope":{"workspace":"w","session":"s"},"expected_policy_revision":"4","proposal":{"kind":"rollback","target_revision":"3"}});
    assert!(
        serde_json::from_value::<wire::PreviewRequest>(rollback.clone())
            .unwrap()
            .validate()
            .is_ok()
    );
    rollback["proposal"]["target_revision"] = json!("5");
    assert!(serde_json::from_value::<wire::PreviewRequest>(rollback)
        .unwrap()
        .validate()
        .is_err());
}

#[test]
fn paging_and_preview_consumption_reject_unbounded_or_forged_inputs() {
    let mut read:wire::ReportRead=serde_json::from_value(json!({"scope":{"workspace":"w","session":"s"},"report":"command","section":"sources","limit":32,"cursor":null})).unwrap();
    assert!(read.validate().is_ok());
    read.limit = 33;
    assert!(read.validate().is_err());
    read.limit = 1;
    read.cursor = Some("x".repeat(4097));
    assert!(read.validate().is_err());
    let mut apply = json!({"scope":{"workspace":"w","session":"s"},"mutation":{"command_id":"apply-command","expected_revision":"2","steering_revision":"0"},"expected_binding_revision":"3","preview_id":"review","preview_sha256":"a".repeat(64)});
    assert!(serde_json::from_value::<wire::Apply>(apply.clone())
        .unwrap()
        .validate()
        .is_ok());
    assert!(serde_json::from_value::<wire::Rollback>(apply.clone())
        .unwrap()
        .validate()
        .is_ok());
    apply["preview_sha256"] = json!("A".repeat(64));
    assert!(serde_json::from_value::<wire::Apply>(apply.clone())
        .unwrap()
        .validate()
        .is_err());
    apply["policy"] = json!({});
    assert!(serde_json::from_value::<wire::Apply>(apply).is_err());
}
