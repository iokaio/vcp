// SPDX-License-Identifier: Apache-2.0
use super::*;

fn fixture(unpriced: bool) -> (tempfile::TempDir, tempfile::TempDir, Profile, Vec<u8>) {
    let workspace = tempfile::tempdir().unwrap();
    let configuration = tempfile::tempdir().unwrap();
    let observed = now();
    let expires = Timestamp::new(observed.get() + 60_000);
    let compatibility = serde_json::from_value(serde_json::json!({
        "id":"fixture", "model":"fixture/model", "endpoint":"fixture/provider",
        "qualified_at":observed, "valid_until":expires, "responses_text_tools":true,
        "byte_ceiling_qualified":true, "provider_preferences_qualified":true,
        "deny_data_collection":true, "require_zdr":true, "request_price_limit":"0",
        "required_parameters":["tools","max_tokens"]
    }))
    .unwrap();
    let pricing = if unpriced {
        serde_json::json!({"prompt":"0.000001"})
    } else {
        serde_json::json!({"prompt":"0.000001","completion":"0.000002","request":"0"})
    };
    let raw = serde_json::to_vec(&serde_json::json!({"data":{"id":"fixture/model","endpoints":[{
        "tag":"fixture/provider", "status":0, "context_length":10000, "max_prompt_tokens":9000,
        "max_completion_tokens":8000, "supported_parameters":["tools","max_tokens"], "pricing":pricing
    }]}})).unwrap();
    let provider = if unpriced {
        Snapshot::from_endpoints_unbounded(&raw, observed, expires, compatibility)
    } else {
        Snapshot::from_endpoints(&raw, observed, expires, compatibility)
    }
    .unwrap();
    let catalog = configuration.path().join("catalog.json");
    std::fs::write(&catalog, &raw).unwrap();
    let profile = serde_json::from_value(serde_json::json!({
        "version":1, "workspace":workspace.path().canonicalize().unwrap(), "trust_workspace":true,
        "maximum_autonomy":"autonomous", "automatic_effects":[], "budget_usd":"1.00",
        "provider":provider, "catalog":catalog, "affected_paths":["file.txt"],
        "max_requests":7, "deadline_seconds":60, "processes":[], "checks":[]
    }))
    .unwrap();
    (workspace, configuration, profile, raw)
}

#[test]
fn actual_profile_preparation_accepts_v3_unpriced_and_preserves_legacy_capture() {
    for unpriced in [false, true] {
        let (_workspace, _configuration, profile, raw) = fixture(unpriced);
        let original = serde_json::to_value(&profile).unwrap();
        let captured = profile.provider.clone();
        let prepared = profile.prepare(Autonomy::Autonomous).unwrap();
        assert_eq!(prepared.raw_catalog, raw);
        assert_eq!(prepared.profile.provider.tariff_normalization, Some(3));
        assert_eq!(prepared.profile.provider.raw_sha256, captured.raw_sha256);
        assert_eq!(
            prepared.profile.provider.compatibility,
            captured.compatibility
        );
        assert_eq!(prepared.profile.provider.max_output, captured.max_output);
        assert_eq!(prepared.profile.max_requests, 7);
        assert!(prepared.profile.deadline_seconds.is_unbounded());
        assert_eq!(original["deadline_seconds"]["value"], 60);
        if unpriced {
            assert!(!prepared
                .profile
                .provider
                .price
                .rates
                .contains_key(&vcp_domain::accounting::ChargeCategory::Output));
            assert!(prepared
                .profile
                .provider
                .for_execution(
                    &raw,
                    vcp_domain::Limit::Finite(vcp_domain::Micros::new(1_000_000))
                )
                .is_err());
        } else {
            assert_ne!(prepared.profile.provider.id, captured.id);
            assert_eq!(captured.rebuild_captured(&raw).unwrap(), captured);
        }
    }
}

#[test]
fn actual_profile_preparation_rejects_tampered_identity_and_malformed_legacy_prices() {
    for unpriced in [false, true] {
        let (_workspace, _configuration, mut profile, _) = fixture(unpriced);
        profile.provider.max_output = Units::new(7999);
        assert!(profile.prepare(Autonomy::Autonomous).is_err());
    }
    let (_workspace, _configuration, profile, raw) = fixture(false);
    let mut malformed: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    malformed["data"]["endpoints"][0]["pricing"]["completion"] = serde_json::json!("malformed");
    std::fs::write(&profile.catalog, serde_json::to_vec(&malformed).unwrap()).unwrap();
    assert!(profile.prepare(Autonomy::Autonomous).is_err());
}
