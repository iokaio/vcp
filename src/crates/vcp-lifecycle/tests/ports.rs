// SPDX-License-Identifier: Apache-2.0
// Behavioral fixtures adapted from Gemini CLI; see components/gemini-cli.md.
use serde_json::{json, Value};
use vcp_lifecycle::ports::{canonical, PreparedCalls};
fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/gemini/ports.json")).unwrap()
}
#[test]
fn canonical_arguments_and_rewrite_invalidate_confirmation() {
    let f = fixture();
    assert_eq!(
        canonical(&f["canonical"]["input"]).unwrap(),
        f["canonical"]["expected"]
    );
    assert!(canonical(&json!(9007199254740992u64)).is_err());
    assert!(canonical(&json!(1.1)).is_err());
    let mut calls = PreparedCalls::default();
    let old = calls.prepare("a", &f["rewrite"]["before"], &[]).unwrap();
    let current = calls.rewrite("a", &f["rewrite"]["after"]).unwrap();
    assert!(calls.admit("a", &old, true, true).is_err());
    assert!(calls.admit("a", &current, false, true).is_err());
    assert!(calls.admit("a", &current, true, false).is_err());
    calls.admit("a", &current, true, true).unwrap();
    assert!(calls.rewrite("a", &json!({})).is_err());
}
#[test]
fn out_of_order_results_keep_identity_and_actual_completion_order() {
    let f = fixture();
    let mut calls = PreparedCalls::default();
    for id in f["out_of_order"]["arrival"].as_array().unwrap() {
        let id = id.as_str().unwrap();
        let approval = calls.prepare(id, &json!({}), &[id.into()]).unwrap();
        calls.admit(id, &approval, true, true).unwrap();
    }
    for id in f["out_of_order"]["completion"].as_array().unwrap() {
        calls.receipt(id.as_str().unwrap(), "success").unwrap();
    }
    assert_eq!(
        calls
            .completed()
            .iter()
            .map(|row| row.0.clone())
            .collect::<Vec<_>>(),
        vec!["b", "a"]
    );
    assert!(calls.receipt("b", "duplicate").is_err());
}
#[test]
fn resources_remain_owned_until_cancellation_receipt_and_client_data_is_not_authority() {
    let f = fixture();
    let resources = vec![f["resource_conflict"]["resources"][0]
        .as_str()
        .unwrap()
        .into()];
    let mut calls = PreparedCalls::default();
    let a = calls
        .prepare("a", &json!({"isClientInitiated":true}), &resources)
        .unwrap();
    let b = calls.prepare("b", &json!({}), &resources).unwrap();
    assert!(calls.admit("a", &a, false, true).is_err());
    calls.admit("a", &a, true, true).unwrap();
    assert!(calls.admit("b", &b, true, true).is_err());
    calls.cancel("a").unwrap();
    assert!(calls.admit("b", &b, true, true).is_err());
    calls.receipt("a", "observed partial effect").unwrap();
    assert_eq!(calls.completed()[0].1, f["cancellation"]["vcp"]);
    calls.admit("b", &b, true, true).unwrap();
    let c = calls.prepare("c", &json!({}), &[]).unwrap();
    calls.cancel("c").unwrap();
    assert!(calls.admit("c", &c, true, true).is_err());
    assert!(calls.receipt("c", "late success").is_err());
}
