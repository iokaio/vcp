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

#[test]
fn host_marker_constants_match_the_shared_marker_file() {
    // src/skills/markers.json also drives the builtin fixture author.
    let shared: serde_json::Value =
        serde_json::from_str(include_str!("../../../skills/markers.json")).unwrap();
    let list = |key: &str| -> Vec<String> { serde_json::from_value(shared[key].clone()).unwrap() };
    assert_eq!(
        list("root_markers"),
        vcp_lifecycle::foundation::skills::ROOT_MARKERS
    );
    assert_eq!(
        list("root_pattern_extensions"),
        vcp_lifecycle::foundation::skills::ROOT_PATTERN_EXTENSIONS
    );
}
