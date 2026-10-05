// SPDX-License-Identifier: Apache-2.0
//! Reservation estimates are distinct from actual observed charges.
use super::{Currency, Money};
use crate::{Micros, Units};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A conservative reservation estimate, including explicitly unpriced terms.
/// `known_component` is a portion of the estimate, not a lower bound of spend.
/// An unknown term is one missing category valuation in a quote. Aggregation
/// sums those terms across reservations; it does not count reservations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EstimatedMicros {
    Known(Micros),
    Unknown(UnknownEstimate),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownEstimate {
    known_component: Micros,
    unknown_components: Units,
}

impl EstimatedMicros {
    pub const ZERO: Self = Self::Known(Micros::ZERO);
    pub fn unknown(known_component: Micros, unknown_components: Units) -> crate::Result<Self> {
        if unknown_components == Units::ZERO {
            return Err(crate::Error::Invalid(
                "unknown estimate needs unpriced terms",
            ));
        }
        Ok(Self::Unknown(UnknownEstimate {
            known_component,
            unknown_components,
        }))
    }
    /// Returns a total only when every term is priced.
    pub fn known(self) -> Option<Micros> {
        match self {
            Self::Known(value) => Some(value),
            Self::Unknown(_) => None,
        }
    }
    pub fn known_component(self) -> Micros {
        match self {
            Self::Known(value) => value,
            Self::Unknown(value) => value.known_component,
        }
    }
    pub fn unknown_components(self) -> Units {
        match self {
            Self::Known(_) => Units::ZERO,
            Self::Unknown(value) => value.unknown_components,
        }
    }
    /// An unpriced estimate is never a known zero liability.
    pub fn is_zero(self) -> bool {
        self == Self::ZERO
    }
    pub fn checked_add(self, other: Self) -> crate::Result<Self> {
        let known = Micros::new(
            self.known_component()
                .get()
                .checked_add(other.known_component().get())
                .ok_or(crate::Error::Overflow)?,
        );
        let unknown = Units::new(
            self.unknown_components()
                .get()
                .checked_add(other.unknown_components().get())
                .ok_or(crate::Error::Overflow)?,
        );
        if unknown == Units::ZERO {
            Ok(Self::Known(known))
        } else {
            Self::unknown(known, unknown)
        }
    }
    /// Observed charges reduce the retained numeric estimate but cannot price
    /// its missing terms. Only a known final settlement/release resolves them.
    pub fn remaining_after(self, charged: Micros) -> Self {
        let remaining = Micros::new(self.known_component().get().saturating_sub(charged.get()));
        match self {
            Self::Known(_) => Self::Known(remaining),
            Self::Unknown(value) => Self::Unknown(UnknownEstimate {
                known_component: remaining,
                ..value
            }),
        }
    }
}
impl Default for EstimatedMicros {
    fn default() -> Self {
        Self::ZERO
    }
}
impl std::fmt::Display for EstimatedMicros {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Known(value) => write!(formatter, "{}", value.get()),
            Self::Unknown(value) => write!(
                formatter,
                "unknown (known estimate component {}, unpriced terms {})",
                value.known_component.get(),
                value.unknown_components.get()
            ),
        }
    }
}
impl From<Micros> for EstimatedMicros {
    fn from(value: Micros) -> Self {
        Self::Known(value)
    }
}
// Equality with an observed amount is meaningful only for a fully priced
// estimate. In particular, an unpriced zero component never equals zero.
impl PartialEq<Micros> for EstimatedMicros {
    fn eq(&self, other: &Micros) -> bool {
        self.known() == Some(*other)
    }
}
impl PartialEq<EstimatedMicros> for Micros {
    fn eq(&self, other: &EstimatedMicros) -> bool {
        other == self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstimatedMoney {
    pub currency: Currency,
    pub micros: EstimatedMicros,
}
impl From<Money> for EstimatedMoney {
    fn from(value: Money) -> Self {
        Self {
            currency: value.currency,
            micros: value.micros.into(),
        }
    }
}
impl PartialEq<Money> for EstimatedMoney {
    fn eq(&self, other: &Money) -> bool {
        self.currency == other.currency && self.micros == other.micros
    }
}
impl PartialEq<EstimatedMoney> for Money {
    fn eq(&self, other: &EstimatedMoney) -> bool {
        other == self
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Versioned {
    Unknown {
        version: u32,
        known_component: Micros,
        unknown_components: Units,
    },
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Input {
    Known(Micros),
    Unknown(Versioned),
}
impl Serialize for EstimatedMicros {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            // Preserve original finite quote, reservation and ledger bytes.
            Self::Known(value) => value.serialize(serializer),
            Self::Unknown(value) => Versioned::Unknown {
                version: 1,
                known_component: value.known_component,
                unknown_components: value.unknown_components,
            }
            .serialize(serializer),
        }
    }
}
impl<'de> Deserialize<'de> for EstimatedMicros {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match Input::deserialize(deserializer)? {
            Input::Known(value) => Ok(Self::Known(value)),
            Input::Unknown(Versioned::Unknown {
                version: 1,
                known_component,
                unknown_components,
            }) => {
                Self::unknown(known_component, unknown_components).map_err(serde::de::Error::custom)
            }
            Input::Unknown(_) => Err(serde::de::Error::custom("unsupported estimate version")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_str, json, to_vec};
    #[test]
    fn legacy_known_money_retains_exact_bytes_and_unknown_is_explicit() {
        let original = br#"{"currency":"USD","micros":"425"}"#;
        let known: EstimatedMoney = serde_json::from_slice(original).unwrap();
        assert_eq!(known.micros.known(), Some(Micros::new(425)));
        assert_eq!(to_vec(&known).unwrap(), original);
        let unknown = EstimatedMicros::unknown(Micros::ZERO, Units::new(2)).unwrap();
        assert_eq!(unknown.known(), None);
        assert!(!unknown.is_zero());
        assert_eq!(
            serde_json::to_value(unknown).unwrap(),
            json!({"kind":"unknown","version":1,"known_component":"0","unknown_components":"2"})
        );
        assert_eq!(
            serde_json::from_slice::<EstimatedMicros>(&to_vec(&unknown).unwrap()).unwrap(),
            unknown
        );
    }
    #[test]
    fn aggregation_and_charges_preserve_unknown_terms_and_check_overflow() {
        let one = EstimatedMicros::unknown(Micros::new(20), Units::new(2)).unwrap();
        let two = EstimatedMicros::unknown(Micros::new(30), Units::new(1)).unwrap();
        let sum = one
            .checked_add(two)
            .unwrap()
            .checked_add(Micros::new(5).into())
            .unwrap();
        assert_eq!(sum.known_component(), Micros::new(55));
        assert_eq!(sum.unknown_components(), Units::new(3));
        let remainder = sum.remaining_after(Micros::new(100));
        assert_eq!(remainder.known_component(), Micros::ZERO);
        assert_eq!(remainder.known(), None);
        assert_eq!(remainder.unknown_components(), Units::new(3));
        assert!(EstimatedMicros::Known(Micros::new(u64::MAX))
            .checked_add(Micros::new(1).into())
            .is_err());
        assert!(EstimatedMicros::unknown(Micros::ZERO, Units::new(u64::MAX))
            .unwrap()
            .checked_add(one)
            .is_err());
        assert_eq!(
            EstimatedMicros::Known(Micros::new(20)).remaining_after(Micros::new(30)),
            EstimatedMicros::ZERO
        );
    }
    #[test]
    fn unknown_decoder_rejects_ambiguous_or_noncanonical_values() {
        for raw in [
            r#"{"kind":"unknown","version":2,"known_component":"1","unknown_components":"1"}"#,
            r#"{"kind":"unknown","version":1,"known_component":"1","unknown_components":"0"}"#,
            r#"{"kind":"unknown","version":1,"known_component":"1","unknown_components":"1","total":"1"}"#,
            r#"{"kind":"unknown","version":1,"known_component":"01","unknown_components":"1"}"#,
            r#"{"kind":"unknown","version":1,"known_component":"1"}"#,
            "null",
            "0",
            r#""01""#,
        ] {
            assert!(from_str::<EstimatedMicros>(raw).is_err(), "{raw}");
        }
        assert!(EstimatedMicros::unknown(Micros::ZERO, Units::ZERO).is_err());
    }
}
