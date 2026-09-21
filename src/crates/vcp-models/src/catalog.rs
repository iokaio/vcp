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
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub qualified_reasoning_efforts: BTreeSet<crate::reasoning::Effort>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tariff_normalization: Option<u32>,
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
        if !compatibility.responses_text_tools || !compatibility.provider_preferences_qualified {
            return Err(Error::Capability("dated compatibility record"));
        }
        Self::metadata(raw, observed_at, valid_until, compatibility)
    }
    fn metadata(
        raw: &[u8],
        observed_at: Timestamp,
        valid_until: Timestamp,
        compatibility: Compatibility,
    ) -> Result<Self> {
        if raw.len() > 4 * 1024 * 1024 {
            return Err(Error::Limit("catalog bytes"));
        }
        if compatibility.id.is_empty()
            || compatibility.endpoint.is_empty()
            || compatibility.model.is_empty()
            || compatibility.qualified_at > observed_at
            || compatibility.valid_until < valid_until
            || valid_until <= observed_at
            || (!compatibility.qualified_reasoning_efforts.is_empty()
                && !compatibility.required_parameters.contains("reasoning"))
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
        let mut tariffs = vec![prices];
        if let Some(overrides) = prices.get("overrides") {
            let overrides = overrides
                .as_array()
                .filter(|rows| rows.len() <= 128)
                .ok_or(Error::Capability("bounded pricing overrides"))?;
            for tier in overrides {
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
        // Admission does not predict the provider's tier or cache duration.
        // Preserve raw metadata and use the maximum listed rate for each class.
        let maximum = |keys: &[&str], mut bound: Rate| -> Result<Rate> {
            for tariff in &tariffs {
                for key in keys {
                    if let Some(value) = tariff.get(*key) {
                        let candidate =
                            rate(value.as_str().ok_or(Error::Capability("tariff price"))?)?;
                        if candidate.micros > bound.micros {
                            bound = candidate;
                        }
                    }
                }
            }
            Ok(bound)
        };
        let input = maximum(&["prompt"], rate(required("prompt")?)?)?;
        let output = maximum(&["completion"], rate(required("completion")?)?)?;
        let request = rate(&compatibility.request_price_limit)?;
        if maximum(&["request"], request.clone())?.micros > request.micros {
            return Err(Error::Capability("request price exceeds enforced ceiling"));
        }
        // Caching details may be absent. The conservative admission price is
        // at least the ordinary input price; missing observed totals stay unknown.
        let rates = BTreeMap::from([
            (ChargeCategory::Input, input.clone()),
            (ChargeCategory::Output, output),
            (
                ChargeCategory::CacheRead,
                maximum(&["input_cache_read"], input.clone())?,
            ),
            (
                ChargeCategory::CacheWrite,
                maximum(
                    &["input_cache_write", "input_cache_write_1h"],
                    input.clone(),
                )?,
            ),
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
        let legacy_input = rate(required("prompt")?)?;
        let legacy_cache = |key: &str| -> Result<Rate> {
            let legacy = prices
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(rate)
                .transpose()?
                .unwrap_or_else(|| legacy_input.clone());
            Ok(if legacy.micros < legacy_input.micros {
                legacy_input.clone()
            } else {
                legacy
            })
        };
        let tariff_upgrade = rates[&ChargeCategory::Input] != legacy_input
            || rates[&ChargeCategory::Output] != rate(required("completion")?)?
            || rates[&ChargeCategory::CacheRead] != legacy_cache("input_cache_read")?
            || rates[&ChargeCategory::CacheWrite] != legacy_cache("input_cache_write")?;
        let identity = if tariff_upgrade {
            vcp_protocol::canonical_bytes(&(
                "endpoint-tariff-maxima/2",
                observed_at,
                valid_until,
                &raw_sha256,
                &compatibility,
            ))?
        } else {
            // Preserve historical IDs only when their exact price interpretation
            // is unchanged. A corrected tariff cannot reuse an old price ID.
            vcp_protocol::canonical_bytes(&(observed_at, valid_until, &raw_sha256, &compatibility))?
        };
        let id = vcp_protocol::digest_bytes(&identity);
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
            tariff_normalization: tariff_upgrade.then_some(2),
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
    pub fn identity_digest(&self) -> Result<String> {
        let bytes = match self.tariff_normalization {
            None => vcp_protocol::canonical_bytes(&(
                self.observed_at,
                self.valid_until,
                &self.raw_sha256,
                &self.compatibility,
            ))?,
            Some(2) => vcp_protocol::canonical_bytes(&(
                "endpoint-tariff-maxima/2",
                self.observed_at,
                self.valid_until,
                &self.raw_sha256,
                &self.compatibility,
            ))?,
            _ => return Err(Error::Capability("unsupported tariff normalization")),
        };
        Ok(vcp_protocol::digest_bytes(&bytes))
    }
    /// An unqualified tokenizer estimate cannot bound monetary admission. Keep
    /// byte length for context fit, but reserve the full endpoint input capacity.
    pub fn reservation_input(&self, encoded_bytes: Units) -> Units {
        if self.compatibility.byte_ceiling_qualified {
            encoded_bytes
        } else {
            self.max_input
        }
    }
}

/// Qualification tooling may price a candidate without claiming its protocol or
/// provider-policy conformance. This type grants no production Snapshot.
#[cfg(feature = "qualification")]
#[derive(Clone, Debug, Serialize)]
pub struct CandidateMetadata {
    pub raw_sha256: String,
    pub context: Units,
    pub max_input: Units,
    pub max_output: Units,
    pub price: PriceSnapshot,
}
#[cfg(feature = "qualification")]
impl CandidateMetadata {
    pub fn from_endpoints(
        raw: &[u8],
        observed_at: Timestamp,
        valid_until: Timestamp,
        model: String,
        endpoint: String,
        request_price_limit: String,
        required_parameters: BTreeSet<String>,
    ) -> Result<Self> {
        let candidate = Snapshot::metadata(
            raw,
            observed_at,
            valid_until,
            Compatibility {
                id: "unqualified-conformance-candidate/1".into(),
                model,
                endpoint,
                qualified_at: observed_at,
                valid_until,
                responses_text_tools: false,
                byte_ceiling_qualified: false,
                provider_preferences_qualified: false,
                qualified_reasoning_efforts: BTreeSet::new(),
                deny_data_collection: true,
                require_zdr: false,
                request_price_limit,
                required_parameters,
            },
        )?;
        Ok(Self {
            raw_sha256: candidate.raw_sha256,
            context: candidate.context,
            max_input: candidate.max_input,
            max_output: candidate.max_output,
            price: candidate.price,
        })
    }
}
