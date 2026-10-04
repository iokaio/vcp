// SPDX-License-Identifier: Apache-2.0
//! Explicit execution limits. Unbounded is neither zero nor an unknown amount.
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limit<T> {
    Finite(T),
    Unbounded,
}

impl<T> Limit<T> {
    pub fn finite(&self) -> Option<&T> {
        match self {
            Self::Finite(value) => Some(value),
            Self::Unbounded => None,
        }
    }

    pub fn is_unbounded(&self) -> bool {
        matches!(self, Self::Unbounded)
    }

    pub fn map<U>(self, convert: impl FnOnce(T) -> U) -> Limit<U> {
        match self {
            Self::Finite(value) => Limit::Finite(convert(value)),
            Self::Unbounded => Limit::Unbounded,
        }
    }
}

impl<T: PartialOrd> Limit<T> {
    pub fn exceeds(&self, value: &T) -> bool {
        self.finite().is_some_and(|limit| value > limit)
    }
}

impl<T, E> Limit<Result<T, E>> {
    pub fn transpose(self) -> Result<Limit<T>, E> {
        match self {
            Self::Finite(value) => value.map(Limit::Finite),
            Self::Unbounded => Ok(Limit::Unbounded),
        }
    }
}

impl<T> From<T> for Limit<T> {
    fn from(value: T) -> Self {
        Self::Finite(value)
    }
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Versioned<T> {
    Finite {
        #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 1)))]
        version: u32,
        value: T,
    },
    Unbounded {
        #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 1)))]
        version: u32,
    },
}

#[derive(Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
enum Input<T> {
    Versioned(Versioned<T>),
    Legacy(T),
}

impl<T: Serialize> Serialize for Limit<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Finite(value) => Versioned::Finite { version: 1, value },
            Self::Unbounded => Versioned::Unbounded { version: 1 },
        }
        .serialize(serializer)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Limit<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match Input::deserialize(deserializer)? {
            Input::Legacy(value) => Ok(Self::Finite(value)),
            Input::Versioned(Versioned::Finite { version: 1, value }) => Ok(Self::Finite(value)),
            Input::Versioned(Versioned::Unbounded { version: 1 }) => Ok(Self::Unbounded),
            Input::Versioned(_) => Err(serde::de::Error::custom(
                "unsupported execution limit version",
            )),
        }
    }
}

#[cfg(feature = "schema")]
impl<T: schemars::JsonSchema> schemars::JsonSchema for Limit<T> {
    fn schema_name() -> String {
        format!("Limit_{}", T::schema_name())
    }

    fn json_schema(generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        Input::<T>::json_schema(generator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_str, json, to_value};

    #[test]
    fn explicit_unbounded_is_distinct_from_zero_and_legacy_values() {
        assert_eq!(from_str::<Limit<u32>>("30").unwrap(), Limit::Finite(30));
        assert_eq!(
            to_value(Limit::<u32>::Unbounded).unwrap(),
            json!({"version":1,"kind":"unbounded"})
        );
        assert_eq!(
            to_value(Limit::Finite(0u32)).unwrap(),
            json!({"version":1,"kind":"finite","value":0})
        );
        assert!(!Limit::<u32>::Unbounded.exceeds(&u32::MAX));
        assert!(Limit::Finite(0).exceeds(&1));
    }

    #[test]
    fn rejects_unknown_version_ambiguous_fields_and_missing_values() {
        for raw in [
            r#"{"version":2,"kind":"unbounded"}"#,
            r#"{"version":1,"kind":"unbounded","value":0}"#,
            r#"{"version":1,"kind":"finite"}"#,
            "null",
        ] {
            assert!(from_str::<Limit<u32>>(raw).is_err(), "{raw}");
        }
    }
}
