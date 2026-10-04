// SPDX-License-Identifier: Apache-2.0
//! Internal domain envelopes and the separately negotiated public wire contract.
pub mod backup_publisher;
pub mod command;
pub mod errors;
pub mod event;
pub mod handshake;
pub mod history;
pub mod jsonrpc;
pub mod memory;
pub mod memory_governance;
pub mod memory_history;
pub mod memory_query;
pub mod memory_retention;
pub mod methods;
pub mod persisted_json;
pub mod policy_inspection;
pub mod redaction;
pub mod routing_inspection;
pub mod routing_optimizer;
pub mod subscription;
pub mod version;

use serde::Serialize;
use sha2::{Digest, Sha256};

pub fn digest_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Incremental digest of caller-defined bytes. This does not canonicalize JSON;
/// callers preserve the same framing and canonical field order as digest_bytes.
#[derive(Default)]
pub struct DigestWriter {
    hash: Sha256,
    length: u64,
}
impl DigestWriter {
    pub fn finish(self) -> (String, u64) {
        (format!("{:x}", self.hash.finalize()), self.length)
    }
}
impl std::io::Write for DigestWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let length = self
            .length
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| std::io::Error::other("digest byte count overflow"))?;
        self.hash.update(bytes);
        self.length = length;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
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

pub mod editor;
