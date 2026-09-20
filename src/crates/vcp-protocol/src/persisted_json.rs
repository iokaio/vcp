// SPDX-License-Identifier: Apache-2.0
//! Decode persisted JSON values without interpreting literal serde-private keys.
//!
//! This is a persistence codec, not an external-content admission policy. Object
//! keys remain ordinary keys; only lexical scalar tokens are parsed as scalars.
//! Serialization and canonical v1 are unchanged. Numbers retain the behavior of
//! the selected serde_json feature graph; no numeric normalization is performed.
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::Deserializer;
use serde_json::{value::RawValue, Map, Value};
use std::fmt;

// The old serde_json reader has a 128-level recursion limit, including its typed
// enclosing fields. A per-value limit of 128 therefore retains every value that
// reader could decode. Store admission remains stricter: 1 MiB per record and
// 8 MiB per transaction, including event data. This ceiling does not lower either.
const MAX_DEPTH: usize = 128;
const MAX_BYTES: usize = 16 * 1024 * 1024;

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
            if values.contains_key(&key) {
                return Err(de::Error::custom("duplicate JSON key"));
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
pub fn parse(raw: &[u8]) -> Result<Value, serde_json::Error> {
    if raw.len() > MAX_BYTES {
        return Err(de::Error::custom("JSON byte limit"));
    }
    let raw: &RawValue = serde_json::from_slice(raw)?;
    let mut remaining = raw.get().len();
    Node {
        remaining: &mut remaining,
        depth: 0,
    }
    .parse(raw)
}

pub fn deserialize<'de, D: serde::Deserializer<'de>>(decoder: D) -> Result<Value, D::Error> {
    // RawValue also supports serde_json's owned and borrowed Value deserializers.
    // Internally tagged Serde enums instead buffer through ContentDeserializer;
    // containing enums must retain their own raw discriminator boundary.
    let raw: Box<RawValue> = serde::Deserialize::deserialize(decoder)?;
    parse(raw.get().as_bytes()).map_err(de::Error::custom)
}
