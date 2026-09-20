// SPDX-License-Identifier: Apache-2.0
// Intended integration under tests/mcp.rs after draft promotion.
use vcp_extensions::mcp::schema::{Error, Limits, Schema};

#[test]
fn open_objects_cannot_admit_non_profile_numbers_as_synthetic_maps() {
    let schema = Schema::compile(br#"{"type":"object"}"#, Limits::default()).unwrap();
    // Also run with serde_json/arbitrary_precision enabled: serde then presents
    // these tokens to custom visitors as synthetic maps, not visit_f64 calls.
    for number in [
        "1.5",
        "1.0",
        "1e0",
        "1e9999",
        "18446744073709551616",
        "-9223372036854775809",
    ] {
        for input in [number.to_owned(), format!("{{\"unknown\":[{number}]}}")] {
            assert_eq!(schema.arguments(input.as_bytes()).unwrap_err(), Error::Json);
        }
    }
    let exact = br#"{"a":-9223372036854775808,"b":18446744073709551615,"c":9007199254740993}"#;
    assert_eq!(schema.arguments(exact).unwrap().canonical_bytes(), exact);
}

#[test]
fn serde_private_keys_are_rejected_at_every_depth_before_value_conversion() {
    let schema = Schema::compile(br#"{"type":"object"}"#, Limits::default()).unwrap();
    for input in [
        r#"{"$serde_json::private::Number":"1.5"}"#,
        r#"{"v":{"$serde_json::private::Number":"1.5"}}"#,
        r#"{"v":[{"\u0024serde_json::private::Number":"1.5"}]}"#,
        r#"{"$serde_json::private::RawValue":"null"}"#,
        r#"{"v":{"\u0024serde_json::private::RawValue":"1.5"}}"#,
    ] {
        assert_eq!(schema.arguments(input.as_bytes()).unwrap_err(), Error::Json);
    }
    assert!(Schema::compile(
        br#"{"type":"object","properties":{"$serde_json::private::Number":{"type":"string"}}}"#,
        Limits::default(),
    )
    .is_err());
}

#[test]
fn default_additional_properties_and_nested_predicates() {
    let schema = Schema::compile(br#"{"type":"object","properties":{"label":{"type":"string","minLength":1,"maxLength":1},"rows":{"type":"array","items":{"type":"integer"},"maxItems":2}},"required":["label"]}"#, Limits::default()).unwrap();
    assert!(schema
        .arguments("{\"label\":\"💚\",\"unknown\":{\"v\":[null,true]}}".as_bytes())
        .is_ok());
    for invalid in [
        r#"{}"#,
        r#"{"label":"ab"}"#,
        r#"{"label":"x","rows":[1,2,3]}"#,
        r#"{"label":"x","rows":[1.0]}"#,
        r#"{"label":"x","rows":[18446744073709551616]}"#,
    ] {
        assert!(schema.arguments(invalid.as_bytes()).is_err());
    }
}
#[test]
fn unsupported_schema_and_duplicate_keys_fail_admission() {
    for unsupported in [
        r#"{"type":"object","additionalProperties":{"type":"string"}}"#,
        r#"{"type":"object","properties":{"v":{"type":"string","pattern":".*"}}}"#,
        r#"{"type":"object","properties":{"v":{"type":"number","enum":[0.1]}}}"#,
        r#"{"type":"object","type":"string"}"#,
        r#"{"type":"object","$ref":"https://example.invalid/schema"}"#,
    ] {
        assert!(Schema::compile(unsupported.as_bytes(), Limits::default()).is_err());
    }
}
#[test]
fn strict_object_canonical_order_bounds_and_safe_diagnostics() {
    let a = Schema::compile(br#"{"type":"object","properties":{"v":{"type":"string","enum":["private-sentinel"]}},"additionalProperties":false}"#, Limits::default()).unwrap();
    let b = Schema::compile(br#"{"additionalProperties":false,"properties":{"v":{"enum":["private-sentinel"],"type":"string"}},"type":"object"}"#, Limits::default()).unwrap();
    assert_eq!(a.digest(), b.digest());
    assert!(a.arguments(br#"{"unexpected":true}"#).is_err());
    assert!(a
        .arguments(br#"{"v":"private-sentinel","v":"private-sentinel"}"#)
        .is_err());
    let checked = a.arguments(br#"{"v":"private-sentinel"}"#).unwrap();
    assert!(!format!("{a:?} {checked:?}").contains("private-sentinel"));
    let tiny = Schema::compile(
        br#"{"type":"object"}"#,
        Limits {
            bytes: 32,
            ..Limits::default()
        },
    )
    .unwrap();
    assert_eq!(
        tiny.arguments(br#"{"long":"abcdefghijklmnopqrstuvwxyz"}"#)
            .unwrap_err(),
        Error::Bounds
    );
}
