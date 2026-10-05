// SPDX-License-Identifier: Apache-2.0
//! Observed tariffs for explicitly unbounded requests; absence is not zero.
use super::*;

pub(super) fn rates(value: &serde_json::Value) -> Result<BTreeMap<ChargeCategory, Rate>> {
    let empty = serde_json::Map::new();
    let prices = if value.is_null() {
        &empty
    } else {
        value
            .as_object()
            .ok_or(Error::Capability("pricing object"))?
    };
    let mut tariffs = vec![prices];
    if let Some(overrides) = prices.get("overrides") {
        for tier in overrides
            .as_array()
            .filter(|rows| rows.len() <= 128)
            .ok_or(Error::Capability("bounded pricing overrides"))?
        {
            let tier = tier
                .as_object()
                .ok_or(Error::Capability("pricing override"))?;
            if tier
                .get("min_prompt_tokens")
                .and_then(serde_json::Value::as_u64)
                .is_none()
                || tier.keys().any(|key| {
                    !matches!(
                        key.as_str(),
                        "min_prompt_tokens"
                            | "prompt"
                            | "completion"
                            | "request"
                            | "input_cache_read"
                            | "input_cache_write"
                            | "input_cache_write_1h"
                    )
                })
            {
                return Err(Error::Capability("unsupported pricing override"));
            }
            tariffs.push(tier);
        }
    }
    let maximum = |keys: &[&str], mut bound: Option<Rate>| -> Result<Option<Rate>> {
        for tariff in &tariffs {
            for key in keys {
                if let Some(value) = tariff.get(*key).filter(|value| !value.is_null()) {
                    let candidate = rate(value.as_str().ok_or(Error::Capability("tariff price"))?)?;
                    if bound
                        .as_ref()
                        .is_none_or(|old| candidate.micros > old.micros)
                    {
                        bound = Some(candidate);
                    }
                }
            }
        }
        Ok(bound)
    };
    let base = |key: &str| -> Result<Option<Rate>> {
        prices
            .get(key)
            .filter(|value| !value.is_null())
            .map(|value| rate(value.as_str().ok_or(Error::Capability("tariff price"))?))
            .transpose()
    };
    let input = base("prompt")?;
    let output = base("completion")?;
    let request = base("request")?;
    // Parse every supplied rate even when the base tariff remains unknown.
    let observed_input = maximum(&["prompt"], input.clone())?;
    let observed_output = maximum(&["completion"], output.clone())?;
    let observed_request = maximum(&["request"], request.clone())?;
    let cache_read = maximum(&["input_cache_read"], observed_input.clone())?;
    let cache_write = maximum(
        &["input_cache_write", "input_cache_write_1h"],
        observed_input.clone(),
    )?;
    let mut rates = BTreeMap::new();
    if input.is_some() {
        if let Some(value) = observed_input {
            rates.insert(ChargeCategory::Input, value);
        }
        if let Some(value) = cache_read {
            rates.insert(ChargeCategory::CacheRead, value);
        }
        if let Some(value) = cache_write {
            rates.insert(ChargeCategory::CacheWrite, value);
        }
    }
    if output.is_some() {
        if let Some(value) = observed_output {
            rates.insert(ChargeCategory::Output, value);
        }
    }
    if request.is_some() {
        if let Some(value) = observed_request {
            rates.insert(ChargeCategory::Request, value);
        }
    }
    // The qualified codec exposes only local functions; no billable provider tools.
    rates.insert(
        ChargeCategory::ProviderTool,
        Rate {
            micros: Micros::ZERO,
            per_units: Units::new(1),
        },
    );
    Ok(rates)
}
