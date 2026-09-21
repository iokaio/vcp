// SPDX-License-Identifier: Apache-2.0
//! Native Decisions charge metadata, not operation or answer qualification.
use crate::{catalog, Error, Result};
use vcp_domain::{accounting::Rate, Units};

#[derive(Clone, Debug)]
pub struct NativeChargeBound {
    pub raw_sha256: String,
    pub model: String,
    pub provider: String,
    pub input: Units,
    pub input_rate: Rate,
    pub cache_read_rate: Rate,
    pub cache_write_rate: Rate,
    pub request_rate: Rate,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn catalog() -> serde_json::Value {
        serde_json::json!({"data":{"id":"typesafe/jev-1.13", "architecture":{"modality":"text->decisions"},
            "endpoints":[{"tag":"typesafe", "status":0, "context_length":32000,
            "pricing":{"prompt":"0.000000042", "completion":"0", "discount":0}}]}})
    }
    fn parse(value: &serde_json::Value) -> Result<NativeChargeBound> {
        NativeChargeBound::from_endpoints(
            &serde_json::to_vec(value).unwrap(),
            "typesafe/jev-1.13",
            "typesafe",
            "0.001",
        )
    }
    #[test]
    fn native_bound_preserves_precision_and_rejects_unbounded_categories() {
        let bound = parse(&catalog()).unwrap();
        assert_eq!(bound.input.get(), 64000);
        assert_eq!(bound.input_rate.micros.get(), 42000);
        assert_eq!(bound.input_rate.per_units.get(), 1000000);
        assert_eq!(bound.cache_read_rate, bound.input_rate);
        for change in 0..8 {
            let mut value = catalog();
            let endpoint = &mut value["data"]["endpoints"][0];
            match change {
                0 => endpoint["pricing"]["completion"] = "0.0000000000001".into(),
                1 => {
                    endpoint["pricing"]
                        .as_object_mut()
                        .unwrap()
                        .remove("completion");
                }
                2 => endpoint["pricing"]["overrides"] = serde_json::json!([]),
                3 => endpoint["pricing"]["web_search"] = "1".into(),
                4 => endpoint["context_length"] = 0.into(),
                5 => endpoint["status"] = 1.into(),
                6 => endpoint["pricing"]["request"] = "0.002".into(),
                _ => endpoint["pricing"]["prompt"] = (-1).into(),
            }
            assert!(parse(&value).is_err(), "mutation {change}");
        }
        let mut value = catalog();
        let duplicate = value["data"]["endpoints"][0].clone();
        value["data"]["endpoints"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
        assert!(parse(&value).is_err());
        assert!(
            NativeChargeBound::from_endpoints(br#"{"data":{},"data":{}}"#, "x", "y", "0").is_err()
        );
    }
}

impl NativeChargeBound {
    /// Uses the whole advertised context for every input/cache category. Native
    /// requests have no output cap: only an explicitly zero completion tariff
    /// is eligible. Unrecognized charge categories/tier formats fail closed.
    /// Provider max-price enforcement still needs separate operation evidence.
    pub fn from_endpoints(
        raw: &[u8],
        model: &str,
        provider: &str,
        request_cap: &str,
    ) -> Result<Self> {
        if raw.len() > super::MAX_BYTES {
            return Err(Error::Limit("native catalog bytes"));
        }
        let value = super::unique_json::parse(raw)?;
        let data = &value["data"];
        if data["id"].as_str() != Some(model)
            || model != "typesafe/jev-1.13"
            || provider != "typesafe"
            || data["architecture"]["modality"].as_str() != Some("text->decisions")
        {
            return Err(Error::Capability("native catalog model"));
        }
        let endpoints = data["endpoints"]
            .as_array()
            .ok_or(Error::Capability("native endpoints"))?;
        let matching: Vec<_> = endpoints
            .iter()
            .filter(|e| e["tag"].as_str() == Some(provider))
            .collect();
        if matching.len() != 1
            || endpoints.iter().any(|e| {
                e["tag"].as_str().is_some_and(|tag| {
                    tag.strip_prefix(provider)
                        .is_some_and(|s| s.starts_with('/'))
                })
            })
        {
            return Err(Error::Capability("native exact endpoint"));
        }
        let endpoint = matching[0];
        if endpoint["status"].as_i64() != Some(0) {
            return Err(Error::Capability("native endpoint unavailable"));
        }
        let context = endpoint["context_length"]
            .as_u64()
            .filter(|v| *v > 0)
            .ok_or(Error::Capability("native context bound"))?;
        // TypeSafe's Jev 1.13 has simultaneous 32k state+longest-question and
        // 64k state+ALL-questions limits. Catalog context is not total billing.
        // https://docs.typesafe.ai/models, observed September 21, 2026.
        if context != 32000 {
            return Err(Error::Capability("native context contract drift"));
        }
        let input = 64000;
        let prices = endpoint["pricing"]
            .as_object()
            .ok_or(Error::Capability("native pricing"))?;
        if prices.keys().any(|key| {
            !matches!(
                key.as_str(),
                "prompt"
                    | "completion"
                    | "request"
                    | "discount"
                    | "input_cache_read"
                    | "input_cache_write"
                    | "input_cache_write_1h"
            )
        }) {
            return Err(Error::Capability(
                "unqualified native charge category or tier",
            ));
        }
        let price = |key: &str| -> Result<Rate> {
            catalog::rate(
                prices
                    .get(key)
                    .and_then(|v| v.as_str())
                    .ok_or(Error::Capability("native explicit price"))?,
            )
        };
        if price("completion")?.micros.get() != 0 {
            return Err(Error::Capability("native output charge unbounded"));
        }
        let input_rate = price("prompt")?;
        let cache = |keys: &[&str]| -> Result<Rate> {
            let mut bound = input_rate.clone();
            for key in keys {
                if prices.contains_key(*key) {
                    let rate = price(key)?;
                    if rate.micros > bound.micros {
                        bound = rate;
                    }
                }
            }
            Ok(bound)
        };
        let request_rate = catalog::rate(request_cap)?;
        if prices.contains_key("request") && price("request")?.micros > request_rate.micros {
            return Err(Error::Capability("native request cap"));
        }
        Ok(Self {
            raw_sha256: vcp_protocol::digest_bytes(raw),
            model: model.into(),
            provider: provider.into(),
            input: Units::new(input),
            cache_read_rate: cache(&["input_cache_read"])?,
            cache_write_rate: cache(&["input_cache_write", "input_cache_write_1h"])?,
            input_rate,
            request_rate,
        })
    }
}
