// SPDX-License-Identifier: Apache-2.0
//! Runs the frozen builtin fixture contracts (the qualification example) as a
//! test, so changed host markers or descriptors cannot silently stale them.
#[allow(dead_code)]
#[path = "../examples/builtin_skill_qualification.rs"]
mod qualification;

#[test]
fn frozen_builtin_fixtures_pass_native_contracts() {
    let report = qualification::evaluate().unwrap();
    let failed: Vec<_> = report["attempts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|attempt| attempt["pass"] != true)
        .map(|attempt| {
            (
                attempt["id"].clone(),
                attempt.get("assertions").cloned(),
                attempt.get("error").cloned(),
            )
        })
        .collect();
    assert!(report["pass"] == true && failed.is_empty(), "{failed:#?}");
}
