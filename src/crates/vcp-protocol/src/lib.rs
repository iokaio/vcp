// SPDX-License-Identifier: Apache-2.0
//! Internal domain envelopes and the separately negotiated public wire contract.
pub mod command;
pub mod errors;
pub mod event;
pub mod handshake;
pub mod jsonrpc;
pub mod memory;
pub mod memory_query;
pub mod methods;
pub mod persisted_json;
pub mod redaction;
pub mod subscription;
pub mod version;

use serde::Serialize;
use sha2::{Digest, Sha256};

pub fn digest_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Hash an already bounded reader without retaining its contents in memory.
pub fn digest_reader(mut reader: impl std::io::Read) -> std::io::Result<(String, u64)> {
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 64 * 1024];
    let mut length = 0u64;
    loop {
        let count = reader.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        hash.update(&bytes[..count]);
        length = length
            .checked_add(count as u64)
            .ok_or_else(|| std::io::Error::other("digest byte count overflow"))?;
    }
    Ok((format!("{:x}", hash.finalize()), length))
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
