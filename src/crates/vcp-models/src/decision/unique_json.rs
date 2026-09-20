// SPDX-License-Identifier: Apache-2.0
//! Bounded duplicate-aware JSON, preserving numeric tokens without MCP normalization.
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::Deserializer;
use serde_json::{value::RawValue, Map, Value};
use std::fmt;

const MAX_DEPTH: usize = 32;
const MAX_NODES: usize = 4096;
const PRIVATE_NUMBER: &str = "$serde_json::private::Number";
const PRIVATE_RAW: &str = "$serde_json::private::RawValue";

struct Node<'a> {
    remaining: &'a mut usize,
    depth: usize,
}
impl Node<'_> {
    fn parse(self, raw: &RawValue) -> Result<Value, serde_json::Error> {
        if self.depth > MAX_DEPTH || *self.remaining == 0 {
            return Err(de::Error::custom("JSON structural limit"));
        }
        *self.remaining -= 1;
        let mut decoder = serde_json::Deserializer::from_str(raw.get());
        let value = match raw.get().as_bytes().first() {
            Some(b'{') => decoder.deserialize_map(self)?,
            Some(b'[') => decoder.deserialize_seq(self)?,
            // RawValue already validated JSON syntax. Dispatch scalar text directly
            // to Value so real numbers cannot be confused with private serde maps.
            _ => return serde_json::from_str(raw.get()),
        };
        decoder.end()?;
        Ok(value)
    }
}
impl<'de> DeserializeSeed<'de> for Node<'_> {
    type Value = Value;
    fn deserialize<D: Deserializer<'de>>(self, decoder: D) -> Result<Value, D::Error> {
        let raw: &RawValue = serde::Deserialize::deserialize(decoder)?;
        self.parse(raw).map_err(de::Error::custom)
    }
}
impl<'de> Visitor<'de> for Node<'_> {
    type Value = Value;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("bounded JSON without duplicate keys")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut values = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if values.contains_key(&key) || matches!(key.as_str(), PRIVATE_NUMBER | PRIVATE_RAW) {
                return Err(de::Error::custom("duplicate or reserved JSON key"));
            }
            let value = map.next_value_seed(Node {
                remaining: self.remaining,
                depth: self.depth + 1,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = seq.next_element_seed(Node {
            remaining: self.remaining,
            depth: self.depth + 1,
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }
}
pub(super) fn parse(raw: &[u8]) -> Result<Value, serde_json::Error> {
    if raw.len() > super::MAX_BYTES {
        return Err(de::Error::custom("JSON byte limit"));
    }
    let raw: &RawValue = serde_json::from_slice(raw)?;
    let mut remaining = MAX_NODES;
    Node {
        remaining: &mut remaining,
        depth: 0,
    }
    .parse(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decimals_remain_numbers_and_exact_cost_text() {
        let value = parse(br#"{"confidence":0.4,"score":1.25,"cost":0.0000120000000000000001,"big":18446744073709551616,"exponent":3e-1}"#).unwrap();
        assert_eq!(value["confidence"].as_f64(), Some(0.4));
        assert_eq!(value["score"].as_f64(), Some(1.25));
        assert_eq!(value["cost"].to_string(), "0.0000120000000000000001");
        assert_eq!(
            crate::catalog::usd_micros(&value["cost"].to_string()).unwrap(),
            13
        );
        assert_eq!(value["big"].to_string(), "18446744073709551616");
        assert_eq!(value["exponent"].to_string(), "3e-1");
    }
    #[test]
    fn real_private_objects_cannot_impersonate_numbers_or_raw_values() {
        for raw in [
            r#"{"$serde_json::private::Number":"0.4"}"#,
            r#"{"nested":{"$serde_json::private::RawValue":"0.4"}}"#,
            r#"{"\u0024serde_json::private::Number":"0.4"}"#,
        ] {
            assert!(parse(raw.as_bytes()).is_err());
        }
    }
    #[test]
    fn decoded_duplicates_rejected_at_every_depth() {
        for raw in [
            r#"{"a":1,"\u0061":2}"#,
            r#"[{"x":0.1,"x":0.2}]"#,
            r#"{"outer":{"a":1,"a":2}}"#,
        ] {
            assert!(parse(raw.as_bytes()).is_err());
        }
    }
    #[test]
    fn bounds_and_trailing_data_fail_closed() {
        assert!(parse(&vec![b' '; super::super::MAX_BYTES + 1]).is_err());
        assert!(parse(
            format!(
                "{}0{}",
                "[".repeat(MAX_DEPTH + 1),
                "]".repeat(MAX_DEPTH + 1)
            )
            .as_bytes()
        )
        .is_err());
        assert!(parse(format!("[{}]", vec!["0"; MAX_NODES].join(",")).as_bytes()).is_err());
        assert!(parse(b"0 1").is_err());
        assert!(parse(b"1e").is_err());
        assert!(parse(b"NaN").is_err());
        assert!(parse(b"1e999").unwrap().as_f64().is_none());
    }
}
