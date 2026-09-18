// SPDX-License-Identifier: Apache-2.0
//! Private versioned envelopes. Public transports and SDK bindings remain deferred.
pub mod command;
pub mod event;
pub mod subscription;
pub mod version;

use serde::Serialize;
use sha2::{Digest, Sha256};

pub fn digest_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Digest v1: UTF-8 JSON, recursive lexicographic object keys, array order retained.
/// Numeric domain counters already serialize as canonical decimal strings.
pub fn canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    fn canonical(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(map) => {
                let ordered: std::collections::BTreeMap<_, _> = map.into_iter().collect();
                let mut map = serde_json::Map::new();
                for (key, value) in ordered {
                    map.insert(key, canonical(value));
                }
                serde_json::Value::Object(map)
            }
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.into_iter().map(canonical).collect())
            }
            value => value,
        }
    }
    serde_json::to_vec(&canonical(serde_json::to_value(value)?))
}
