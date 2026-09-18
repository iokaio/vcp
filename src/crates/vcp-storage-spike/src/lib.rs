// SPDX-License-Identifier: Apache-2.0
//! Bounded P0 storage/portability experiment. Not a supported persistence format.
pub mod search;
pub mod store;
pub mod vault;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub fn reject(message: &str) -> Box<dyn std::error::Error + Send + Sync> {
    io::Error::other(message).into()
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub const LIMIT: usize = 32 * 1024 * 1024;
pub fn object_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub id: String,
    pub kind: String,
    pub workspace: String,
    pub payload: serde_json::Value,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct View {
    pub format: u32,
    pub sequence: u64,
    pub command: String,
    pub deletion_epoch: u64,
    pub generation: u64,
    pub records: Vec<Record>,
    pub artifacts: BTreeMap<String, u64>,
    pub vectors: BTreeMap<String, Vec<f32>>,
}
impl View {
    pub fn validate(&self) -> Result<()> {
        let ids: std::collections::BTreeSet<_> = self.records.iter().map(|r| &r.id).collect();
        if self.format != 1
            || self.sequence == 0
            || self.command.is_empty()
            || self.command.len() > 256
            || ids.len() != self.records.len()
            || self.generation > self.sequence
            || self.records.len() > 100_000
            || self.artifacts.len() > 10_000
            || self
                .records
                .iter()
                .any(|r| r.id.is_empty() || r.workspace.is_empty() || r.kind.is_empty())
            || self
                .artifacts
                .iter()
                .any(|(id, size)| !object_id(id) || *size > LIMIT as u64)
            || self.vectors.iter().any(|(id, v)| {
                !ids.contains(id) || v.len() != 3 || v.iter().any(|x| !x.is_finite())
            })
        {
            return Err(reject("invalid neutral view"));
        }
        Ok(())
    }
}

/// Public synthetic corpus, including historical dispute and unsettled liability.
pub fn fixture(sequence: u64, count: usize) -> (View, BTreeMap<String, Vec<u8>>) {
    let mut records = vec![
        Record {
            id: "claim-old".into(),
            kind: "claim".into(),
            workspace: "synthetic-A".into(),
            payload: serde_json::json!({"text":"VCP_P0_PLAINTEXT_CANARY old claim", "superseded_by":"claim-new", "disputed":true}),
        },
        Record {
            id: "claim-new".into(),
            kind: "claim".into(),
            workspace: "synthetic-A".into(),
            payload: serde_json::json!({"text":"new claim", "evidence":"event-0"}),
        },
        Record {
            id: "reservation-1".into(),
            kind: "reservation".into(),
            workspace: "synthetic-A".into(),
            payload: serde_json::json!({"reserved_microusd":12500,"settled_microusd":null,"effect":"unknown"}),
        },
        Record {
            id: "tombstone-1".into(),
            kind: "deletion".into(),
            workspace: "synthetic-B".into(),
            payload: serde_json::json!({"id":"removed-claim","epoch":1}),
        },
    ];
    for n in 0..count {
        records.push(Record {
            id: format!("event-{n}"),
            kind: "event".into(),
            workspace: if n % 2 == 0 {
                "synthetic-A"
            } else {
                "synthetic-B"
            }
            .into(),
            payload: serde_json::json!({"ordinal":n,"text":format!("public fixture event {n}" )}),
        });
    }
    let bytes = b"VCP_P0_PLAINTEXT_CANARY artifact\r\n".repeat(4096);
    let id = digest(&bytes);
    let objects = BTreeMap::from([(id.clone(), bytes)]);
    let view = View {
        format: 1,
        sequence,
        command: format!("commit-{sequence}"),
        deletion_epoch: 1,
        generation: 1,
        records,
        artifacts: objects
            .iter()
            .map(|(id, b)| (id.clone(), b.len() as u64))
            .collect(),
        vectors: BTreeMap::from([
            ("claim-old".into(), vec![1.0, 0.0, 0.0]),
            ("claim-new".into(), vec![0.0, 1.0, 0.0]),
        ]),
    };
    (view, objects)
}
