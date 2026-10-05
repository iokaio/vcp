// SPDX-License-Identifier: Apache-2.0
//! Authoritative generation-charge receipts, separate from successful probes.
//! HTTP errors and missing output never establish zero cost.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use vcp_domain::accounting::Money;
use vcp_domain::Micros;

pub const MAX_RECEIPT_BYTES: usize = 64 * 1024;

pub fn valid_request_id(id: &str) -> bool {
    id.starts_with("gen-")
        && id.len() > 4
        && id.len() <= 256
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
}

/// Accept only generation identities, never generic correlation IDs. Conflicting
/// header/body identities are unavailable evidence, not a choice to guess.
pub fn failed_request_id(body: &[u8], header: Option<&str>) -> Result<Option<String>> {
    if body.len() > MAX_RECEIPT_BYTES {
        return Err(Error::Limit("failed response bytes"));
    }
    let header = header.filter(|id| valid_request_id(id));
    // A structured error body must be unambiguous even when a valid header is
    // available. Prose/empty gateway errors may instead use the header alone.
    let structured = matches!(
        body.iter()
            .copied()
            .find(|byte| !byte.is_ascii_whitespace()),
        Some(b'{' | b'[')
    );
    let value = if structured {
        Some(crate::decision::unique_json::parse(body)?)
    } else {
        None
    };
    let mut found = header.map(str::to_owned);
    if let Some(value) = value {
        for path in [
            "/id",
            "/request_id",
            "/error/metadata/request_id",
            "/error/metadata/generation_id",
        ] {
            if let Some(id) = value
                .pointer(path)
                .and_then(Value::as_str)
                .filter(|id| valid_request_id(id))
            {
                if found.as_deref().is_some_and(|old| old != id) {
                    return Err(Error::Protocol("conflicting generation identities"));
                }
                found = Some(id.to_owned());
            }
        }
    }
    Ok(found)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expected {
    pub request_id: String,
    pub model: String,
    pub endpoint: String,
    pub provider_name: Option<String>,
    pub model_revision: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ChargeReceipt {
    pub request_id: String,
    pub amount: Money,
    pub raw_sha256: String,
    pub normalization: &'static str,
}

/// A successful authenticated generation-metadata GET is authoritative for its
/// total charge, including rejected/cancelled generations. Attribution fields,
/// when present, must agree with the exact captured admission/catalog identity.
pub fn charge(raw: &[u8], expected: &Expected) -> Result<ChargeReceipt> {
    if raw.len() > MAX_RECEIPT_BYTES {
        return Err(Error::Limit("generation receipt bytes"));
    }
    if !valid_request_id(&expected.request_id) {
        return Err(Error::Protocol("generation identity"));
    }
    let value = crate::decision::unique_json::parse(raw)?;
    let data = value
        .get("data")
        .filter(|v| v.is_object())
        .ok_or(Error::Protocol("generation data missing"))?;
    if data["id"].as_str() != Some(expected.request_id.as_str()) || value.get("error").is_some() {
        return Err(Error::Protocol("generation receipt binding"));
    }
    let terminal = data["cancelled"].as_bool() == Some(true)
        || matches!(
            data["finish_reason"].as_str(),
            Some("stop" | "length" | "tool_calls" | "error" | "content_filter")
        );
    if !terminal {
        return Err(Error::Protocol("generation charge is not final"));
    }
    if let Some(currency) = data.get("currency") {
        if currency.as_str() != Some("USD") {
            return Err(Error::Protocol("generation currency"));
        }
    }
    for field in ["model", "model_permaslug"] {
        if let Some(model) = data.get(field).filter(|v| !v.is_null()) {
            let model = model
                .as_str()
                .ok_or(Error::Protocol("generation model type"))?;
            if model != expected.model && Some(model) != expected.model_revision.as_deref() {
                return Err(Error::Protocol("generation model differs from admission"));
            }
        }
    }
    if let Some(provider) = data.get("provider_name").filter(|v| !v.is_null()) {
        let provider = provider
            .as_str()
            .ok_or(Error::Protocol("generation provider type"))?;
        if provider != expected.endpoint && Some(provider) != expected.provider_name.as_deref() {
            return Err(Error::Protocol(
                "generation provider differs from admission",
            ));
        }
    }
    if let Some(endpoint) = data.get("endpoint").filter(|v| !v.is_null()) {
        if endpoint.as_str() != Some(expected.endpoint.as_str()) {
            return Err(Error::Protocol(
                "generation endpoint differs from admission",
            ));
        }
    }
    if let Some(attempts) = data.get("provider_responses").filter(|v| !v.is_null()) {
        let attempts = attempts
            .as_array()
            .filter(|v| v.len() <= 1)
            .ok_or(Error::Protocol("unexpected provider attempts"))?;
        for attempt in attempts {
            if !attempt.is_object() {
                return Err(Error::Protocol("generation attempted route type"));
            }
            for (field, first, second) in [
                (
                    "provider_name",
                    expected.endpoint.as_str(),
                    expected.provider_name.as_deref(),
                ),
                (
                    "model_permaslug",
                    expected.model.as_str(),
                    expected.model_revision.as_deref(),
                ),
            ] {
                if let Some(observed) = attempt.get(field).filter(|v| !v.is_null()) {
                    let observed = observed
                        .as_str()
                        .ok_or(Error::Protocol("generation attempted identity type"))?;
                    if observed != first && Some(observed) != second {
                        return Err(Error::Protocol("generation attempted route conflict"));
                    }
                }
            }
        }
    }
    let cost = data["total_cost"]
        .as_number()
        .ok_or(Error::Protocol("generation final charge missing"))?
        .to_string();
    let amount = Money {
        currency: "USD".to_owned().try_into().map_err(|_| Error::Decimal)?,
        micros: Micros::new(crate::catalog::usd_micros(&cost)?),
    };
    Ok(ChargeReceipt {
        request_id: expected.request_id.clone(),
        amount,
        raw_sha256: vcp_protocol::digest_bytes(raw),
        normalization: "generation-total-charge/1",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn expected() -> Expected {
        Expected {
            request_id: "gen-original".into(),
            model: "fixture/model".into(),
            endpoint: "fixture/us".into(),
            provider_name: Some("Fixture".into()),
            model_revision: Some("fixture/model-1".into()),
        }
    }
    #[test]
    fn rejected_zero_and_nonzero_are_authoritative_without_success_usage() {
        for (cost, micros) in [("0", 0), ("0.0000021", 3)] {
            let raw = format!(
                r#"{{"data":{{"id":"gen-original","model":"fixture/model-1","provider_name":"Fixture","cancelled":true,"total_cost":{cost},"provider_responses":[{{"status":429,"provider_name":"Fixture","model_permaslug":"fixture/model-1"}}]}}}}"#
            );
            assert_eq!(
                charge(raw.as_bytes(), &expected())
                    .unwrap()
                    .amount
                    .micros
                    .get(),
                micros
            );
        }
    }
    #[test]
    fn missing_conflicting_negative_and_duplicate_metadata_stays_unknown() {
        for raw in [
            r#"{"data":{"id":"gen-other","total_cost":0,"finish_reason":"error"}}"#,
            r#"{"data":{"id":"gen-original","finish_reason":"error"}}"#,
            r#"{"data":{"id":"gen-original","total_cost":-1,"finish_reason":"error"}}"#,
            r#"{"data":{"id":"gen-original","total_cost":0,"currency":"EUR","finish_reason":"error"}}"#,
            r#"{"data":{"id":"gen-original","total_cost":0,"model":"other/model","finish_reason":"error"}}"#,
            r#"{"data":{"id":"gen-original","total_cost":0,"provider_name":"Other","finish_reason":"error"}}"#,
            r#"{"data":{"id":"gen-original","total_cost":2,"total_cost":0,"finish_reason":"error"}}"#,
            r#"{"data":{"id":"gen-original","total_cost":0,"finish_reason":null,"cancelled":false}}"#,
            r#"{"data":{"id":"gen-original","total_cost":0,"finish_reason":"error","provider_responses":[123]}}"#,
            r#"{"data":{"id":"gen-original","total_cost":0,"finish_reason":"error","provider_responses":["bad"]}}"#,
        ] {
            assert!(charge(raw.as_bytes(), &expected()).is_err(), "{raw}");
        }
    }
    #[test]
    fn failed_identity_is_bounded_and_conflicts_reject() {
        assert_eq!(
            failed_request_id(br#"{"id":"gen-original"}"#, None).unwrap(),
            Some("gen-original".into())
        );
        assert_eq!(
            failed_request_id(b"unavailable", Some("gen-header")).unwrap(),
            Some("gen-header".into())
        );
        assert!(failed_request_id(br#"{"id":"gen-original"}"#, Some("gen-other")).is_err());
        assert!(failed_request_id(
            br#"{"id":"gen-original","id":"gen-header"}"#,
            Some("gen-header")
        )
        .is_err());
        assert!(failed_request_id(b"  { malformed JSON", Some("gen-header")).is_err());
        assert!(failed_request_id(b"\n [ malformed JSON", Some("gen-header")).is_err());
        assert_eq!(
            failed_request_id(br#"{"id":"request-correlation"}"#, None).unwrap(),
            None
        );
        assert!(!valid_request_id("gen-secret\n"));
    }
}
