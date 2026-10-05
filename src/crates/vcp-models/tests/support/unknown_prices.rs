// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_domain::{accounting::ChargeCategory, Limit, Micros};

#[test]
fn unbounded_price_interpretation_preserves_original_metadata_and_hard_preferences() {
    let raw = serde_json::to_vec(&catalog()).unwrap();
    let legacy = snapshot();
    let encoded = |snapshot: &Snapshot| {
        let env = envelope(
            snapshot,
            Units::new(512),
            Units::new(128),
            Timestamp::new(10),
        )
        .unwrap();
        encode(&[], &env, &tools(), snapshot).unwrap()
    };
    let legacy_bytes = encoded(&legacy);
    let effective = legacy.for_execution(&raw, Limit::Unbounded).unwrap();
    assert_eq!(effective.tariff_normalization, Some(3));
    assert_ne!(effective.id, legacy.id);
    assert_eq!(effective.raw_sha256, legacy.raw_sha256);
    assert_eq!(effective.compatibility, legacy.compatibility);
    assert_eq!(effective.rebuild_captured(&raw).unwrap(), effective);
    let mut expected: Value = serde_json::from_slice(&legacy_bytes).unwrap();
    expected["provider"]
        .as_object_mut()
        .unwrap()
        .remove("max_price");
    assert_eq!(
        serde_json::from_slice::<Value>(&encoded(&effective)).unwrap(),
        expected
    );
    assert_eq!(
        encoded(
            &effective
                .for_execution(&raw, Limit::Finite(Micros::new(100)))
                .unwrap()
        ),
        legacy_bytes
    );
    let mut invalid = effective.clone();
    invalid.max_output = Units::new(999999);
    assert!(invalid.for_execution(&raw, Limit::Unbounded).is_err());
}

#[test]
fn missing_prices_are_not_zero_and_malformed_or_missing_capability_still_denies() {
    let mut value = catalog();
    value["data"]["endpoints"][0]["pricing"] = json!({"prompt":"0.000001","request":"0.01"});
    let parse = |value: &Value| {
        Snapshot::from_endpoints_unbounded(
            &serde_json::to_vec(value).unwrap(),
            Timestamp::new(10),
            Timestamp::new(1000),
            compat(),
        )
    };
    let snapshot = parse(&value).unwrap();
    assert!(!snapshot.price.rates.contains_key(&ChargeCategory::Output));
    assert!(
        snapshot.price.rates[&ChargeCategory::Request].micros
            > super::snapshot().price.rates[&ChargeCategory::Request].micros
    );
    assert!(snapshot
        .for_execution(
            &serde_json::to_vec(&value).unwrap(),
            Limit::Finite(Micros::new(100))
        )
        .is_err());
    for field in [
        "status",
        "supported_parameters",
        "context_length",
        "max_completion_tokens",
    ] {
        let mut invalid = value.clone();
        invalid["data"]["endpoints"][0][field] = Value::Null;
        assert!(parse(&invalid).is_err(), "{field}");
    }
    value["data"]["endpoints"][0]["pricing"]["completion"] = json!("malformed");
    assert!(parse(&value).is_err());
    value["data"]["endpoints"][0]["pricing"] = Value::Null;
    let snapshot = parse(&value).unwrap();
    assert_eq!(
        snapshot.price.rates.len(),
        1,
        "only supported local-tool class is known free"
    );
    assert!(snapshot
        .price
        .rates
        .contains_key(&ChargeCategory::ProviderTool));
}
