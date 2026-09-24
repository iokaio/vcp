// SPDX-License-Identifier: Apache-2.0
//! Strict JSON data parser; no Gemini environment substitution or includes.
use super::{Error, Result};
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};

struct Bounded(usize);
impl<'de> DeserializeSeed<'de> for Bounded {
    type Value = serde_json::Value;
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> std::result::Result<Self::Value, D::Error> {
        if self.0 > 16 {
            return Err(serde::de::Error::custom("source depth"));
        }
        deserializer.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Bounded {
    type Value = serde_json::Value;
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("bounded JSON data")
    }
    fn visit_bool<E: serde::de::Error>(self, v: bool) -> std::result::Result<Self::Value, E> {
        Ok(v.into())
    }
    fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<Self::Value, E> {
        Ok(v.into())
    }
    fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<Self::Value, E> {
        Ok(v.into())
    }
    fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<Self::Value, E> {
        serde_json::Number::from_f64(v)
            .map(serde_json::Value::Number)
            .ok_or_else(|| E::custom("number"))
    }
    fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<Self::Value, E> {
        Ok(v.into())
    }
    fn visit_string<E: serde::de::Error>(self, v: String) -> std::result::Result<Self::Value, E> {
        Ok(v.into())
    }
    fn visit_none<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
        Ok(serde_json::Value::Null)
    }
    fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
        Ok(serde_json::Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(
        self,
        mut seq: A,
    ) -> std::result::Result<Self::Value, A::Error> {
        let mut out = Vec::new();
        while let Some(value) = seq.next_element_seed(Bounded(self.0 + 1))? {
            if out.len() >= 4096 {
                return Err(serde::de::Error::custom("array bound"));
            }
            out.push(value);
        }
        Ok(serde_json::Value::Array(out))
    }
    fn visit_map<A: MapAccess<'de>>(
        self,
        mut map: A,
    ) -> std::result::Result<Self::Value, A::Error> {
        let mut out = serde_json::Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if out.len() >= 4096 || out.contains_key(&key) {
                return Err(serde::de::Error::custom("duplicate key or object bound"));
            }
            out.insert(key, map.next_value_seed(Bounded(self.0 + 1))?);
        }
        Ok(serde_json::Value::Object(out))
    }
}
pub(crate) fn parse(bytes: &[u8]) -> Result<serde_json::Value> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = Bounded(0)
        .deserialize(&mut deserializer)
        .map_err(|_| Error::Invalid("JSON syntax or bounds"))?;
    deserializer
        .end()
        .map_err(|_| Error::Invalid("trailing JSON"))?;
    Ok(value)
}
