// SPDX-License-Identifier: Apache-2.0
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{accounting::*, Micros, Timestamp, Units};

/// Exact nonnegative decimal conversion, including bounded scientific notation.
/// Returns millionths rounded upwards, never a floating-point money operation.
pub fn usd_micros(value: &str) -> Result<u64> {
    let (mantissa, exponent) = match value.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i32>().map_err(|_| Error::Decimal)?),
        None => (value, 0),
    };
    if value.len() > 64 || !(-30..=30).contains(&exponent) {
        return Err(Error::Decimal);
    }
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if whole.is_empty()
        || whole.len() + fraction.len() > 30
        || !whole
            .bytes()
            .chain(fraction.bytes())
            .all(|b| b.is_ascii_digit())
        || (mantissa.contains('.') && fraction.is_empty())
    {
        return Err(Error::Decimal);
    }
    let digits: u128 = format!("{whole}{fraction}")
        .parse()
        .map_err(|_| Error::Decimal)?;
    let power = 6 + exponent - fraction.len() as i32;
    let result = if power >= 0 {
        digits
            .checked_mul(10u128.checked_pow(power as u32).ok_or(Error::Decimal)?)
            .ok_or(Error::Decimal)?
    } else {
        let divisor = 10u128.checked_pow((-power) as u32).ok_or(Error::Decimal)?;
        digits / divisor + u128::from(digits % divisor != 0)
    };
    u64::try_from(result).map_err(|_| Error::Decimal)
}
/// Rates preserve up to 12 fractional USD digits through a million-token unit.
/// Finer precision is rounded upward at that unit, so admission stays conservative.
fn rate(value: &str) -> Result<Rate> {
    let micros_per_million = usd_micros(&format!("{}e6", value)).or_else(|_| {
        let (m, e) = value.split_once(['e', 'E']).ok_or(Error::Decimal)?;
        let e = e.parse::<i32>().map_err(|_| Error::Decimal)?;
        usd_micros(&format!("{m}e{}", e.checked_add(6).ok_or(Error::Decimal)?))
    })?;
    Ok(Rate {
        micros: Micros::new(micros_per_million),
        per_units: Units::new(1_000_000),
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Compatibility {
    pub id: String,
    pub model: String,
    /// Exact endpoint tag from the endpoint catalog, not the display name.
    pub endpoint: String,
    pub qualified_at: Timestamp,
    pub valid_until: Timestamp,
    pub responses_text_tools: bool,
    pub byte_ceiling_qualified: bool,
    pub provider_preferences_qualified: bool,
    pub deny_data_collection: bool,
    pub require_zdr: bool,
    /// Explicit USD/request ceiling enforced by provider.max_price.request.
    /// Missing catalog request pricing is bounded by this value, never free by omission.
    pub request_price_limit: String,
    pub required_parameters: BTreeSet<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub observed_at: Timestamp,
    pub valid_until: Timestamp,
    pub raw_sha256: String,
    pub compatibility: Compatibility,
    pub context: Units,
    pub max_input: Units,
    pub max_output: Units,
    pub price: PriceSnapshot,
}
impl Snapshot {
    pub fn from_endpoints(
        raw: &[u8],
        observed_at: Timestamp,
        valid_until: Timestamp,
        compatibility: Compatibility,
    ) -> Result<Self> {
        if raw.len() > 4 * 1024 * 1024 {
            return Err(Error::Limit("catalog bytes"));
        }
        if compatibility.id.is_empty()
            || !compatibility.responses_text_tools
            || !compatibility.byte_ceiling_qualified
            || !compatibility.provider_preferences_qualified
            || compatibility.endpoint.is_empty()
            || compatibility.model.is_empty()
            || compatibility.qualified_at > observed_at
            || compatibility.valid_until < valid_until
            || valid_until <= observed_at
        {
            return Err(Error::Capability("dated compatibility record"));
        }
        let value: serde_json::Value = serde_json::from_slice(raw)?;
        let data = &value["data"];
        if data["id"].as_str() != Some(&compatibility.model) {
            return Err(Error::Capability("catalog model identity"));
        }
        let endpoints = data["endpoints"]
            .as_array()
            .ok_or(Error::Capability("endpoint catalog"))?;
        let matches: Vec<_> = endpoints
            .iter()
            .filter(|e| e["tag"].as_str() == Some(&compatibility.endpoint))
            .collect();
        if matches.len() != 1 {
            return Err(Error::Capability("unambiguous exact endpoint"));
        }
        // A base provider slug may also select its regional/variant endpoints.
        // This codec qualifies one endpoint, so reject a known expanded pool.
        if !compatibility.endpoint.contains('/')
            && endpoints.iter().any(|e| {
                e["tag"].as_str().is_some_and(|tag| {
                    tag.strip_prefix(&compatibility.endpoint)
                        .is_some_and(|suffix| suffix.starts_with('/'))
                })
            })
        {
            return Err(Error::Capability(
                "provider slug expands to multiple endpoints",
            ));
        }
        let endpoint = matches[0];
        if endpoint["status"].as_i64() != Some(0) {
            return Err(Error::Capability("endpoint unavailable"));
        }
        let supported = endpoint["supported_parameters"]
            .as_array()
            .ok_or(Error::Capability("parameter catalog"))?;
        if compatibility
            .required_parameters
            .iter()
            .any(|p| !supported.iter().any(|v| v.as_str() == Some(p)))
            || !supported.iter().any(|v| v.as_str() == Some("tools"))
        {
            return Err(Error::Capability("required parameters"));
        }
        let positive = |key: &str| {
            endpoint[key]
                .as_u64()
                .filter(|v| *v > 0)
                .ok_or(Error::Capability("context/output bounds"))
        };
        let context = positive("context_length")?;
        let max_input = if endpoint["max_prompt_tokens"].is_null() {
            context
        } else {
            positive("max_prompt_tokens")?.min(context)
        };
        let max_output = positive("max_completion_tokens")?.min(context);
        let prices = endpoint["pricing"]
            .as_object()
            .ok_or(Error::Capability("pricing"))?;
        let required = |key: &str| {
            prices
                .get(key)
                .and_then(|p| p.as_str())
                .ok_or(Error::Capability("missing price"))
        };
        let input = rate(required("prompt")?)?;
        let output = rate(required("completion")?)?;
        let request = rate(&compatibility.request_price_limit)?;
        if let Some(value) = prices.get("request") {
            let catalog_rate = rate(value.as_str().ok_or(Error::Capability("request price"))?)?;
            if catalog_rate.micros > request.micros {
                return Err(Error::Capability("request price exceeds enforced ceiling"));
            }
        }
        // Caching details may be absent. The conservative admission price is
        // at least the ordinary input price; missing observed totals stay unknown.
        let cache = |key: &str| -> Result<Rate> {
            let candidate = match prices.get(key).and_then(|p| p.as_str()) {
                Some(v) => rate(v)?,
                None => input.clone(),
            };
            Ok(if candidate.micros < input.micros {
                input.clone()
            } else {
                candidate
            })
        };
        let rates = BTreeMap::from([
            (ChargeCategory::Input, input.clone()),
            (ChargeCategory::Output, output),
            (ChargeCategory::CacheRead, cache("input_cache_read")?),
            (ChargeCategory::CacheWrite, cache("input_cache_write")?),
            (ChargeCategory::Request, request),
            (
                ChargeCategory::ProviderTool,
                Rate {
                    micros: Micros::ZERO,
                    per_units: Units::new(1),
                },
            ),
        ]);
        let raw_sha256 = vcp_protocol::digest_bytes(raw);
        let id = vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&(
            observed_at,
            valid_until,
            &raw_sha256,
            &compatibility,
        ))?);
        let price = PriceSnapshot {
            id: id.clone(),
            provider: compatibility.endpoint.clone(),
            model: compatibility.model.clone(),
            currency: "USD".to_owned().try_into().map_err(|_| Error::Decimal)?,
            capability: vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&compatibility)?),
            valid_until,
            rates,
        };
        Ok(Self {
            id,
            observed_at,
            valid_until,
            raw_sha256,
            compatibility,
            context: Units::new(context),
            max_input: Units::new(max_input),
            max_output: Units::new(max_output),
            price,
        })
    }
    pub fn current(&self, now: Timestamp) -> Result<()> {
        if now < self.observed_at
            || now >= self.valid_until
            || now >= self.compatibility.valid_until
        {
            Err(Error::Stale)
        } else {
            Ok(())
        }
    }
}
