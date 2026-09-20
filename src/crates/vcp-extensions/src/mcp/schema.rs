// SPDX-License-Identifier: Apache-2.0
//! Restricted admission profile mcp-schema/1, not general JSON Schema compliance.
use serde::{
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
    Deserializer,
};
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub bytes: usize,
    pub depth: usize,
    pub nodes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            bytes: 128 * 1024,
            depth: 16,
            nodes: 4096,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("MCP JSON exceeds configured bounds")]
    Bounds,
    #[error("MCP JSON is invalid or contains duplicate keys")]
    Json,
    #[error("MCP schema uses unsupported or invalid semantics")]
    Schema,
    #[error("MCP arguments do not conform to the admitted schema")]
    Arguments,
}
#[derive(Clone)]
pub struct Schema {
    root: Node,
    digest: String,
    limits: Limits,
    canonical: Vec<u8>,
}
impl fmt::Debug for Schema {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Schema")
            .field("digest", &self.digest)
            .finish_non_exhaustive()
    }
}
#[derive(Clone)]
pub struct CheckedArguments {
    canonical: Vec<u8>,
    digest: String,
    schema_digest: String,
}
impl fmt::Debug for CheckedArguments {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CheckedArguments")
            .field("bytes", &self.canonical.len())
            .field("digest", &self.digest)
            .finish_non_exhaustive()
    }
}
impl CheckedArguments {
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn schema_digest(&self) -> &str {
        &self.schema_digest
    }
}
#[derive(Clone)]
struct Node {
    kind: Kind,
    choices: Option<Vec<Value>>,
    constant: Option<Value>,
}
#[derive(Clone)]
enum Kind {
    Object {
        properties: BTreeMap<String, Node>,
        required: BTreeSet<String>,
        additional: bool,
    },
    Array {
        items: Box<Node>,
        min: u64,
        max: u64,
    },
    String {
        min: u64,
        max: u64,
    },
    Integer,
    Number,
    Boolean,
    Null,
}
impl Schema {
    pub fn compile(bytes: &[u8], limits: Limits) -> Result<Self, Error> {
        let value = bounded_json(bytes, limits)?;
        let root = compile(&value)?;
        if !matches!(root.kind, Kind::Object { .. }) {
            return Err(Error::Schema);
        }
        let canonical = vcp_protocol::canonical_bytes(&value).map_err(|_| Error::Schema)?;
        Ok(Self {
            root,
            digest: vcp_protocol::digest_bytes(&canonical),
            limits,
            canonical,
        })
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }
    pub fn arguments(&self, bytes: &[u8]) -> Result<CheckedArguments, Error> {
        let value = bounded_json(bytes, self.limits)?;
        if !self.root.accepts(&value) {
            return Err(Error::Arguments);
        }
        let canonical = vcp_protocol::canonical_bytes(&value).map_err(|_| Error::Arguments)?;
        if canonical.len() > self.limits.bytes {
            return Err(Error::Bounds);
        }
        Ok(CheckedArguments {
            digest: vcp_protocol::digest_bytes(&canonical),
            canonical,
            schema_digest: self.digest.clone(),
        })
    }
}
fn compile(value: &Value) -> Result<Node, Error> {
    let object = value.as_object().ok_or(Error::Schema)?;
    let ty = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or(Error::Schema)?;
    let specific: &[&str] = match ty {
        "object" => &["properties", "required", "additionalProperties"],
        "array" => &["items", "minItems", "maxItems"],
        "string" => &["minLength", "maxLength"],
        "integer" | "number" | "boolean" | "null" => &[],
        _ => return Err(Error::Schema),
    };
    for (key, value) in object {
        if key == "$schema" {
            if value.as_str() != Some("https://json-schema.org/draft/2020-12/schema") {
                return Err(Error::Schema);
            }
        } else if matches!(key.as_str(), "title" | "description") {
            if !value.is_string() {
                return Err(Error::Schema);
            }
        } else if !matches!(key.as_str(), "type" | "enum" | "const")
            && !specific.contains(&key.as_str())
        {
            return Err(Error::Schema);
        }
    }
    let choices = object
        .get("enum")
        .map(|value| {
            let values = value.as_array().ok_or(Error::Schema)?;
            if values.is_empty() || values.len() > 128 || values.iter().any(|v| !scalar_literal(v))
            {
                return Err(Error::Schema);
            }
            for (i, value) in values.iter().enumerate() {
                if values[..i].contains(value) {
                    return Err(Error::Schema);
                }
            }
            Ok(values.clone())
        })
        .transpose()?;
    let constant = object
        .get("const")
        .map(|value| {
            if scalar_literal(value) {
                Ok(value.clone())
            } else {
                Err(Error::Schema)
            }
        })
        .transpose()?;
    let kind = match ty {
        "object" => {
            let mut properties = BTreeMap::new();
            if let Some(values) = object.get("properties") {
                for (key, value) in values.as_object().ok_or(Error::Schema)? {
                    properties.insert(key.clone(), compile(value)?);
                }
            }
            let mut required = BTreeSet::new();
            if let Some(values) = object.get("required") {
                for value in values.as_array().ok_or(Error::Schema)? {
                    let name = value.as_str().ok_or(Error::Schema)?;
                    if !required.insert(name.to_owned()) {
                        return Err(Error::Schema);
                    }
                }
            }
            let additional = object
                .get("additionalProperties")
                .map(|v| v.as_bool().ok_or(Error::Schema))
                .transpose()?
                .unwrap_or(true);
            Kind::Object {
                properties,
                required,
                additional,
            }
        }
        "array" => {
            let (min, max) = bounds(object, "minItems", "maxItems")?;
            Kind::Array {
                items: Box::new(compile(object.get("items").ok_or(Error::Schema)?)?),
                min,
                max,
            }
        }
        "string" => {
            let (min, max) = bounds(object, "minLength", "maxLength")?;
            Kind::String { min, max }
        }
        "integer" => Kind::Integer,
        "number" => Kind::Number,
        "boolean" => Kind::Boolean,
        "null" => Kind::Null,
        _ => return Err(Error::Schema),
    };
    Ok(Node {
        kind,
        choices,
        constant,
    })
}
fn scalar_literal(v: &Value) -> bool {
    matches!(v, Value::Null | Value::Bool(_) | Value::String(_))
}
fn bounds(object: &Map<String, Value>, min: &str, max: &str) -> Result<(u64, u64), Error> {
    let min = object
        .get(min)
        .map(|v| v.as_u64().ok_or(Error::Schema))
        .transpose()?
        .unwrap_or(0);
    let max = object
        .get(max)
        .map(|v| v.as_u64().ok_or(Error::Schema))
        .transpose()?
        .unwrap_or(u64::MAX);
    if min > max {
        Err(Error::Schema)
    } else {
        Ok((min, max))
    }
}
impl Node {
    fn accepts(&self, value: &Value) -> bool {
        if self
            .choices
            .as_ref()
            .is_some_and(|choices| !choices.contains(value))
            || self
                .constant
                .as_ref()
                .is_some_and(|constant| constant != value)
        {
            return false;
        }
        match &self.kind {
            Kind::Object {
                properties,
                required,
                additional,
            } => value.as_object().is_some_and(|values| {
                required.iter().all(|name| values.contains_key(name))
                    && values.iter().all(|(name, value)| {
                        properties
                            .get(name)
                            .map_or(*additional, |node| node.accepts(value))
                    })
            }),
            Kind::Array { items, min, max } => value.as_array().is_some_and(|values| {
                let len = values.len() as u64;
                len >= *min && len <= *max && values.iter().all(|value| items.accepts(value))
            }),
            Kind::String { min, max } => value.as_str().is_some_and(|value| {
                let len = value.chars().count() as u64;
                len >= *min && len <= *max
            }),
            Kind::Integer => value.is_i64() || value.is_u64(),
            Kind::Number => value.is_number(),
            Kind::Boolean => value.is_boolean(),
            Kind::Null => value.is_null(),
        }
    }
}
// Reject duplicate keys before Value can silently select the last occurrence.
struct Budget {
    remaining: usize,
    exceeded: bool,
}
struct Json<'a> {
    budget: &'a mut Budget,
    depth: usize,
    limits: Limits,
}
impl<'de> DeserializeSeed<'de> for Json<'_> {
    type Value = Value;
    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        if self.depth > self.limits.depth || self.budget.remaining == 0 {
            self.budget.exceeded = true;
            return Err(de::Error::custom("JSON admission bound"));
        }
        self.budget.remaining -= 1;
        deserializer.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Json<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded unique-key JSON")
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(v.into())
    }
    // Do not round decimal/exponent or out-of-range integer text into
    // canonical dispatch bytes. The profile admits exact i64/u64 only.
    fn visit_f64<E: de::Error>(self, _v: f64) -> Result<Value, E> {
        Err(E::custom("numeric representation outside profile"))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }
    fn visit_string<E: de::Error>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }
    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut input: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = input.next_element_seed(Json {
            budget: &mut *self.budget,
            depth: self.depth + 1,
            limits: self.limits,
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut input: A) -> Result<Value, A::Error> {
        let mut values = Map::new();
        while let Some(key) = input.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(de::Error::custom("duplicate key"));
            }
            let value = input.next_value_seed(Json {
                budget: &mut *self.budget,
                depth: self.depth + 1,
                limits: self.limits,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}
pub(crate) fn bounded_json(bytes: &[u8], limits: Limits) -> Result<Value, Error> {
    if limits.bytes == 0
        || limits.bytes > 1024 * 1024
        || limits.depth == 0
        || limits.depth > 32
        || limits.nodes == 0
        || limits.nodes > 65536
        || bytes.len() > limits.bytes
    {
        return Err(Error::Bounds);
    }
    // Bounds apply before each child value is deserialized or retained; the byte
    // cap also bounds any one key/string allocation. Serde's recursion guard stays on.
    let mut budget = Budget {
        remaining: limits.nodes,
        exceeded: false,
    };
    let mut parser = serde_json::Deserializer::from_slice(bytes);
    let value = Json {
        budget: &mut budget,
        depth: 0,
        limits,
    }
    .deserialize(&mut parser)
    .map_err(|_| {
        if budget.exceeded {
            Error::Bounds
        } else {
            Error::Json
        }
    })?;
    parser.end().map_err(|_| Error::Json)?;
    Ok(value)
}

#[cfg(test)]
mod parser_tests {
    use super::*;
    #[test]
    fn budgets_stop_before_deserializing_excess_values() {
        // Malformed excess payload proves the child parser was never entered:
        // otherwise these would return Json instead of Bounds.
        assert_eq!(
            bounded_json(
                br#"[0,{malformed"#,
                Limits {
                    nodes: 2,
                    ..Limits::default()
                }
            ),
            Err(Error::Bounds)
        );
        assert_eq!(
            bounded_json(
                br#"[[{malformed"#,
                Limits {
                    depth: 1,
                    ..Limits::default()
                }
            ),
            Err(Error::Bounds)
        );
        assert!(bounded_json(
            br#"[0]"#,
            Limits {
                nodes: 2,
                depth: 1,
                ..Limits::default()
            }
        )
        .is_ok());
        assert_eq!(
            bounded_json(br#"{}{}"#, Limits::default()),
            Err(Error::Json)
        );
    }
}
