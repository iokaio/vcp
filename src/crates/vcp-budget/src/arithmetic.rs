// SPDX-License-Identifier: Apache-2.0
use crate::{Error, Result};
use vcp_domain::{accounting::*, revision::*};
pub fn add(a: Micros, b: Micros) -> Result<Micros> {
    a.get()
        .checked_add(b.get())
        .map(Micros::new)
        .ok_or(Error::Overflow)
}
pub fn sum(values: impl IntoIterator<Item = Micros>) -> Result<Micros> {
    values.into_iter().try_fold(Micros::ZERO, add)
}
pub fn estimated_sum(values: impl IntoIterator<Item = EstimatedMicros>) -> Result<EstimatedMicros> {
    values
        .into_iter()
        .try_fold(EstimatedMicros::ZERO, |one, two| {
            one.checked_add(two).map_err(Error::from)
        })
}
pub fn known(value: EstimatedMicros) -> Result<Micros> {
    value.known().ok_or(Error::Exhausted(
        "unpriced estimate under a finite constraint",
    ))
}
pub fn rate(rate: &Rate, units: Units) -> Result<Micros> {
    if rate.per_units.get() == 0 {
        return Err(Error::Quote);
    }
    let product = (rate.micros.get() as u128) * (units.get() as u128);
    let divisor = rate.per_units.get() as u128;
    let rounded = product / divisor + u128::from(product % divisor != 0);
    Ok(Micros::new(
        u64::try_from(rounded).map_err(|_| Error::Overflow)?,
    ))
}
pub fn quote(price: PriceSnapshot, bounds: Usage, now: Timestamp) -> Result<CostQuote> {
    if !valid_hash(&price.id)
        || !valid_hash(&price.capability)
        || price.provider.trim().is_empty()
        || price.model.trim().is_empty()
        || price.valid_until <= now
    {
        return Err(Error::Quote);
    }
    let mut amounts = Vec::new();
    let mut missing = 0;
    for (category, units) in bounds.disjoint()? {
        // Absence is an unpriced valuation term, never a free provider feature.
        if let Some(quoted) = price.rates.get(&category) {
            amounts.push(rate(quoted, units)?);
        } else {
            missing += 1;
        }
    }
    let known_component = sum(amounts)?;
    let amount = EstimatedMoney {
        currency: price.currency.clone(),
        micros: if missing == 0 {
            known_component.into()
        } else {
            EstimatedMicros::unknown(known_component, Units::new(missing))?
        },
    };
    Ok(CostQuote {
        normalization_version: if missing == 0 { 1 } else { 2 },
        price,
        bounds,
        amount,
        method: if missing == 0 {
            "ceil_disjoint_bounds_v1"
        } else {
            "ceil_disjoint_bounds_unknown_v2"
        }
        .into(),
    })
}
pub fn validate_quote(value: &CostQuote, now: Timestamp) -> Result<()> {
    if quote(value.price.clone(), value.bounds.clone(), now)? != *value {
        return Err(Error::Quote);
    }
    Ok(())
}
pub fn day(now: Timestamp, offset_minutes: i16) -> Result<i64> {
    if !(-1439..=1439).contains(&offset_minutes) {
        return Err(Error::Quote);
    }
    let millis = i128::from(now.get()) + i128::from(offset_minutes) * 60_000;
    i64::try_from(millis.div_euclid(86_400_000)).map_err(|_| Error::Overflow)
}
