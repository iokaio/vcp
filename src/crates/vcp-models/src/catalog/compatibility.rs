// SPDX-License-Identifier: Apache-2.0
//! Reusable adapter evidence, distinct from live model/provider qualification.
//!
//! The bounded contract covers our Responses text/tool codec and enforced
//! provider request fields. Fresh complete endpoint metadata must independently
//! establish model identity, exact endpoint, parameter support and tariffs.
//! No model-quality, tokenizer, served-provider or reasoning evidence is implied.
use super::{Compatibility, Snapshot};
use crate::{Error, Result};
use std::collections::BTreeSet;
use vcp_domain::Timestamp;

pub const REVIEWED_AT: Timestamp = Timestamp::new(1_790_899_200_000);
pub const VALID_UNTIL: Timestamp = Timestamp::new(1_798_675_200_000);
pub const METADATA_TTL_MS: u64 = 12 * 60 * 60 * 1000;
const PREFIX: &str = "openrouter-responses-adapter-contract/1/";

/// The checked-in adapter version is the evidence, not the connection prompt.
pub fn evidence_id() -> String {
    let source = [
        include_bytes!("../request.rs").as_slice(),
        include_bytes!("../stream.rs").as_slice(),
        include_bytes!("../catalog.rs").as_slice(),
        include_bytes!("estimates.rs").as_slice(),
    ]
    .concat();
    format!("{PREFIX}{}", vcp_protocol::digest_bytes(&source))
}

pub fn required_parameters() -> BTreeSet<String> {
    BTreeSet::from(["tools".into(), "tool_choice".into(), "max_tokens".into()])
}

fn exact(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-._/".contains(&c))
        && value
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn contract(model: String, endpoint: String) -> Compatibility {
    Compatibility {
        id: evidence_id(),
        model,
        endpoint,
        qualified_at: REVIEWED_AT,
        valid_until: VALID_UNTIL,
        // These flags describe empirical evidence. Adapter admission must not
        // falsely turn one account connection into per-model qualification.
        responses_text_tools: false,
        provider_preferences_qualified: false,
        byte_ceiling_qualified: false,
        qualified_reasoning_efforts: BTreeSet::new(),
        deny_data_collection: true,
        require_zdr: false,
        request_price_limit: "0.001".into(),
        required_parameters: required_parameters(),
    }
}

pub fn admitted(value: &Compatibility) -> bool {
    if value.id.starts_with(PREFIX) {
        exact(&value.model)
            && exact(&value.endpoint)
            && *value == contract(value.model.clone(), value.endpoint.clone())
    } else {
        value.responses_text_tools && value.provider_preferences_qualified
    }
}

pub fn metadata_window(
    value: &Compatibility,
    observed: Timestamp,
    expires: Timestamp,
) -> Result<()> {
    if value.id.starts_with(PREFIX)
        && (!admitted(value)
            || observed < REVIEWED_AT
            || expires > VALID_UNTIL
            || expires <= observed
            || expires.get().saturating_sub(observed.get()) > METADATA_TTL_MS)
    {
        return Err(Error::Capability(
            "adapter contract or fresh metadata window",
        ));
    }
    Ok(())
}

/// Fresh metadata can reject a compatible adapter candidate, never expand its
/// capabilities. Unknown, ambiguous or unsupported endpoints remain unavailable.
pub fn snapshots(raw: &[u8], observed: Timestamp) -> Result<Vec<Snapshot>> {
    if raw.len() > 4 * 1024 * 1024 {
        return Err(Error::Limit("catalog bytes"));
    }
    let data: serde_json::Value = serde_json::from_slice(raw)?;
    let model = data["data"]["id"]
        .as_str()
        .filter(|v| exact(v))
        .ok_or(Error::Capability("catalog model identity"))?;
    let endpoints = data["data"]["endpoints"]
        .as_array()
        .filter(|rows| rows.len() <= 1024)
        .ok_or(Error::Capability("bounded endpoint catalog"))?;
    let expires = Timestamp::new(
        observed
            .get()
            .saturating_add(METADATA_TTL_MS)
            .min(VALID_UNTIL.get()),
    );
    metadata_window(
        &contract(model.into(), "validation".into()),
        observed,
        expires,
    )?;
    let tags: BTreeSet<_> = endpoints
        .iter()
        .filter_map(|row| row["tag"].as_str())
        .filter(|tag| exact(tag))
        .collect();
    let result: Vec<_> = tags
        .into_iter()
        .filter_map(|endpoint| {
            Snapshot::from_endpoints_unbounded(
                raw,
                observed,
                expires,
                contract(model.into(), endpoint.into()),
            )
            .ok()
        })
        .collect();
    if result.is_empty() {
        return Err(Error::Capability("no compatible fresh exact endpoint"));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn catalog() -> serde_json::Value {
        json!({"data":{"id":"fixture/model","endpoints":[{"tag":"fixture/region","status":0,"context_length":32000,"max_prompt_tokens":24000,"max_completion_tokens":8000,"supported_parameters":["tools","tool_choice","max_tokens"],"pricing":{"prompt":"0.000001","completion":"0.000002","request":"0"}}]}})
    }
    #[test]
    fn reusable_adapter_evidence_requires_fresh_independent_metadata() {
        let raw = serde_json::to_vec(&catalog()).unwrap();
        let first = snapshots(&raw, REVIEWED_AT).unwrap().remove(0);
        let later = snapshots(
            &raw,
            Timestamp::new(REVIEWED_AT.get() + 2 * METADATA_TTL_MS),
        )
        .unwrap()
        .remove(0);
        assert_eq!(first.compatibility, later.compatibility);
        assert_ne!(first.id, later.id);
        assert!(!later.compatibility.responses_text_tools);
        assert!(!later.compatibility.provider_preferences_qualified);
        assert!(!later.compatibility.byte_ceiling_qualified);
        assert!(first.current(first.valid_until).is_err());
        assert!(snapshots(&raw, VALID_UNTIL).is_err());
        let mut changed = catalog();
        changed["data"]["endpoints"][0]["pricing"]["prompt"] = json!("0.000009");
        let changed = snapshots(&serde_json::to_vec(&changed).unwrap(), REVIEWED_AT)
            .unwrap()
            .remove(0);
        assert_ne!(first.price, changed.price);
        assert_eq!(first.compatibility, changed.compatibility);
    }
    #[test]
    fn unsupported_ambiguous_and_forged_capabilities_fail_closed() {
        for field in ["supported_parameters", "status"] {
            let mut value = catalog();
            value["data"]["endpoints"][0][field] = serde_json::Value::Null;
            assert!(snapshots(&serde_json::to_vec(&value).unwrap(), REVIEWED_AT).is_err());
        }
        let mut unpriced = catalog();
        unpriced["data"]["endpoints"][0]["pricing"] = serde_json::Value::Null;
        let unpriced_snapshots =
            snapshots(&serde_json::to_vec(&unpriced).unwrap(), REVIEWED_AT).unwrap();
        assert_eq!(unpriced_snapshots[0].tariff_normalization, Some(3));
        assert!(!unpriced_snapshots[0]
            .price
            .rates
            .contains_key(&vcp_domain::accounting::ChargeCategory::Input));
        let mut value = catalog();
        let duplicate = value["data"]["endpoints"][0].clone();
        value["data"]["endpoints"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
        assert!(snapshots(&serde_json::to_vec(&value).unwrap(), REVIEWED_AT).is_err());
        let mut evidence = contract("fixture/model".into(), "fixture/region".into());
        evidence.byte_ceiling_qualified = true;
        assert!(!admitted(&evidence));
        evidence = contract("fixture/model".into(), "fixture/region".into());
        evidence.id.push('x');
        assert!(!admitted(&evidence));
        evidence = contract("fixture/model".into(), "fixture/region".into());
        assert!(metadata_window(
            &evidence,
            REVIEWED_AT,
            Timestamp::new(REVIEWED_AT.get() + METADATA_TTL_MS + 1)
        )
        .is_err());
    }
}
