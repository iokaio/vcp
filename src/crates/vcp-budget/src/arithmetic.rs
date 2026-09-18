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
    for (category, units) in bounds.disjoint()? {
        // Every supported category needs an explicit rate, including explicit
        // zero. Absence can never be interpreted as a free provider feature.
        amounts.push(rate(
            price.rates.get(&category).ok_or(Error::Quote)?,
            units,
        )?);
    }
    let amount = Money {
        currency: price.currency.clone(),
        micros: sum(amounts)?,
    };
    Ok(CostQuote {
        normalization_version: 1,
        price,
        bounds,
        amount,
        method: "ceil_disjoint_bounds_v1".into(),
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
