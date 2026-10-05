// SPDX-License-Identifier: Apache-2.0
//! Explicit owner pools. Membership is permission, never measured quality.
use crate::{routing::ModelEndpoint, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use vcp_domain::{
    accounting::{
        ChargeCategory, EstimatedMicros, EstimatedMoney, MonetaryLimit, Money, RequestRole,
    },
    Micros, Units,
};

/// Identical representative request for every route; this is not admission's
/// conservative full-context monetary bound. Take the largest input/cache rate.
pub fn reference_cost(
    snapshot: &crate::catalog::Snapshot,
    input: Units,
    output: Units,
) -> std::result::Result<Money, String> {
    let estimate = reference_estimate(snapshot, input, output)?;
    Ok(Money {
        currency: estimate.currency,
        micros: estimate
            .micros
            .known()
            .ok_or("reference price category unavailable")?,
    })
}

pub fn reference_estimate(
    snapshot: &crate::catalog::Snapshot,
    input: Units,
    output: Units,
) -> std::result::Result<EstimatedMoney, String> {
    let charge = |category, units: Units| -> std::result::Result<EstimatedMicros, String> {
        let Some(rate) = snapshot.price.rates.get(&category) else {
            return EstimatedMicros::unknown(Micros::ZERO, Units::new(1))
                .map_err(|e| e.to_string());
        };
        if rate.per_units == Units::ZERO {
            return Err("reference price denominator invalid".into());
        }
        let numerator = u128::from(rate.micros.get()) * u128::from(units.get());
        let denominator = u128::from(rate.per_units.get());
        u64::try_from(numerator.div_ceil(denominator))
            .map(|value| Micros::new(value).into())
            .map_err(|_| "reference price overflow".into())
    };
    let input = [
        ChargeCategory::Input,
        ChargeCategory::CacheRead,
        ChargeCategory::CacheWrite,
    ]
    .into_iter()
    .map(|category| charge(category, input))
    .collect::<std::result::Result<Vec<_>, _>>()?;
    let known = input
        .iter()
        .map(|value| value.known_component())
        .max()
        .ok_or("reference input price unavailable")?;
    let unpriced = input
        .iter()
        .map(|value| value.unknown_components().get())
        .sum::<u64>();
    let input = if unpriced == 0 {
        known.into()
    } else {
        EstimatedMicros::unknown(known, Units::new(unpriced)).map_err(|e| e.to_string())?
    };
    let total = input
        .checked_add(charge(ChargeCategory::Output, output)?)
        .and_then(|value| {
            value.checked_add(
                charge(ChargeCategory::Request, Units::new(1))
                    .map_err(|_| vcp_domain::Error::Invalid("reference request price"))?,
            )
        })
        .map_err(|e| e.to_string())?;
    Ok(EstimatedMoney {
        currency: snapshot.price.currency.clone(),
        micros: total,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceSet {
    pub id: String,
    pub label: String,
    pub members: Vec<ModelEndpoint>,
    pub reference_request_cost: EstimatedMoney,
    #[serde(serialize_with = "serialize_ceiling")]
    pub max_reference_request_cost: MonetaryLimit,
}
fn serialize_ceiling<S: serde::Serializer>(
    value: &MonetaryLimit,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    if let Some(micros) = value.micros.finite() {
        Money {
            currency: value.currency.clone(),
            micros: *micros,
        }
        .serialize(serializer)
    } else {
        value.serialize(serializer)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleSets {
    pub role: RequestRole,
    pub sets: Vec<ChoiceSet>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub roles: Vec<RoleSets>,
    pub reference_input_tokens: Units,
    pub reference_output_tokens: Units,
}
impl Policy {
    pub fn sets(&self, role: RequestRole) -> &[ChoiceSet] {
        self.roles
            .iter()
            .find(|entry| entry.role == role)
            .map(|entry| entry.sets.as_slice())
            .unwrap_or(&[])
    }
    pub fn validate(&self) -> Result<()> {
        if self.roles.is_empty()
            || self.roles.len() > 8
            || self.reference_input_tokens.get() == 0
            || self.reference_input_tokens.get() > 10_000_000
            || self.reference_output_tokens.get() == 0
            || self.reference_output_tokens.get() > 1_000_000
        {
            return Err(Error::Protocol("rotation role or reference bounds"));
        }
        for (index, role) in self.roles.iter().enumerate() {
            if self.roles[..index]
                .iter()
                .any(|other| other.role == role.role)
                || role.sets.is_empty()
                || role.sets.len() > 3
            {
                return Err(Error::Protocol("rotation roles or choice sets"));
            }
            let mut members = BTreeSet::new();
            let mut ids = BTreeSet::new();
            for set in &role.sets {
                if set.id.is_empty()
                    || set.id.len() > 128
                    || !set
                        .id
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
                    || !ids.insert(&set.id)
                    || set.label.is_empty()
                    || set.label.len() > 256
                    || set.label.chars().any(char::is_control)
                    || set.members.is_empty()
                    || set.members.len() > 32
                    || set.reference_request_cost.currency
                        != set.max_reference_request_cost.currency
                    || (!set.max_reference_request_cost.micros.is_unbounded()
                        && set
                            .reference_request_cost
                            .micros
                            .known()
                            .is_none_or(|cost| {
                                set.max_reference_request_cost.micros.exceeds(&cost)
                            }))
                {
                    return Err(Error::Protocol(
                        "rotation choice set bounds or price ceiling",
                    ));
                }
                for member in &set.members {
                    member.validate()?;
                    if !members.insert(member) {
                        return Err(Error::Protocol("rotation duplicate member"));
                    }
                }
            }
        }
        Ok(())
    }
}

/// Pure two-level ordering: models get equal turns regardless of endpoint count.
/// Availability and mandatory permission gates are supplied by the host first.
pub fn next(
    eligible: &[ModelEndpoint],
    model_position: u64,
    endpoint_position: u64,
) -> Option<ModelEndpoint> {
    let models: Vec<_> = eligible
        .iter()
        .map(|member| &member.model)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let model = models.get((model_position % models.len().max(1) as u64) as usize)?;
    let endpoints: Vec<_> = eligible
        .iter()
        .filter(|member| &member.model == *model)
        .collect();
    endpoints
        .get((endpoint_position % endpoints.len().max(1) as u64) as usize)
        .map(|value| (*value).clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn endpoint(model: &str, endpoint: &str) -> ModelEndpoint {
        ModelEndpoint {
            model: model.into(),
            endpoint: endpoint.into(),
        }
    }
    fn policy() -> Policy {
        let money = |amount| Money {
            currency: "USD".to_owned().try_into().unwrap(),
            micros: Micros::new(amount),
        };
        Policy {
            reference_input_tokens: Units::new(8000),
            reference_output_tokens: Units::new(1024),
            roles: vec![RoleSets {
                role: RequestRole::Main,
                sets: vec![ChoiceSet {
                    id: "normal".into(),
                    label: "Normal".into(),
                    members: vec![endpoint("fixture/a", "region/a")],
                    reference_request_cost: money(10).into(),
                    max_reference_request_cost: money(20).into(),
                }],
            }],
        }
    }
    #[test]
    fn owner_policy_rejects_duplicate_permissions_and_unbounded_choices() {
        let base = policy();
        base.validate().unwrap();
        assert!(base.sets(RequestRole::Reviewer).is_empty());
        let mut repeated = base.clone();
        repeated.roles.push(base.roles[0].clone());
        assert!(repeated.validate().is_err());
        let mut duplicated_route = base.clone();
        let mut reserve = duplicated_route.roles[0].sets[0].clone();
        reserve.id = "reserve".into();
        duplicated_route.roles[0].sets.push(reserve);
        assert!(duplicated_route.validate().is_err());
        let mut too_many = base.clone();
        for index in 1..4 {
            let mut set = base.roles[0].sets[0].clone();
            set.id = format!("set-{index}");
            set.members[0].model = format!("fixture/{index}");
            too_many.roles[0].sets.push(set);
        }
        assert!(too_many.validate().is_err());
        let mut invalid_price = base.clone();
        invalid_price.roles[0].sets[0]
            .max_reference_request_cost
            .micros = Micros::new(9).into();
        assert!(invalid_price.validate().is_err());
    }
    #[test]
    fn finite_rotation_bytes_remain_legacy_and_unknown_reference_requires_unbounded() {
        let mut policy = policy();
        let legacy = serde_json::to_value(&policy).unwrap();
        assert_eq!(
            legacy["roles"][0]["sets"][0]["max_reference_request_cost"]["micros"],
            "20"
        );
        assert_eq!(serde_json::from_value::<Policy>(legacy).unwrap(), policy);
        policy.roles[0].sets[0].reference_request_cost.micros =
            EstimatedMicros::unknown(Micros::new(10), Units::new(1)).unwrap();
        assert!(policy.validate().is_err());
        policy.roles[0].sets[0].max_reference_request_cost.micros = vcp_domain::Limit::Unbounded;
        policy.validate().unwrap();
        let encoded = serde_json::to_value(&policy).unwrap();
        assert_eq!(
            encoded["roles"][0]["sets"][0]["max_reference_request_cost"]["micros"],
            serde_json::json!({"kind":"unbounded","version":1})
        );
        assert_eq!(serde_json::from_value::<Policy>(encoded).unwrap(), policy);
    }
    #[test]
    fn extra_endpoints_do_not_weight_model_rotation() {
        let routes = vec![
            endpoint("a", "one"),
            endpoint("a", "two"),
            endpoint("a", "three"),
            endpoint("b", "one"),
        ];
        let models: Vec<_> = (0..6)
            .map(|position| next(&routes, position, position / 2).unwrap().model)
            .collect();
        assert_eq!(models, ["a", "b", "a", "b", "a", "b"]);
        assert_eq!(next(&routes, 2, 1).unwrap().endpoint, "two");
        assert_eq!(next(&[], 0, 0), None);
    }
}
