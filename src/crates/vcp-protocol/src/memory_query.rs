// SPDX-License-Identifier: Apache-2.0
//! Tagged retained search sources. Requires negotiated `memory/query-sources/1`.
//! A repository artifact is never represented as a fabricated governed claim.
use crate::{
    memory::{EvidenceStatus, Outcome},
    methods::{Counter, Id, Scope},
};
use serde::{Deserialize, Serialize};

pub const CAPABILITY: &str = "memory/query-sources/1";
pub const MAX_QUERY_BYTES: usize = 4096;
pub const MAX_RESULTS: u32 = 64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Source {
    Artifact { artifact: Id },
    Claim { claim: Id, version: Id },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Finding {
    #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 512)))]
    pub record_id: String,
    pub source: Source,
    pub root: Id,
    #[cfg_attr(feature = "schema", schemars(length(min = 64, max = 64)))]
    pub source_sha256: String,
    pub start: Counter,
    pub end: Counter,
    pub outcome: Outcome,
    pub evidence_status: EvidenceStatus,
    #[cfg_attr(feature = "schema", schemars(length(max = 64)))]
    pub evidence: Vec<Id>,
    #[cfg_attr(feature = "schema", schemars(length(max = 16384)))]
    pub content: String,
    pub trimmed: bool,
    /// One-based final shared retrieval order; never a fabricated source ID.
    #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 64)))]
    pub rank: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub scope: Scope,
    pub task: Id,
    pub generation: Option<Id>,
    pub generation_watermark: Option<Counter>,
    pub canonical_watermark: Counter,
    /// Indexed memory sequence, not the session event sequence.
    pub sequence: Counter,
    #[cfg_attr(feature = "schema", schemars(length(max = 64)))]
    pub findings: Vec<Finding>,
    pub rebuild_required: bool,
    /// Fixed internal degradation codes, allowlisted by the public adapter.
    #[cfg_attr(feature = "schema", schemars(length(max = 32)))]
    pub degraded: Vec<String>,
    /// Shared retrieval exhausted a candidate, result, byte or time bound.
    pub truncated: bool,
    /// All selected-source/materialization checks passed without known deficits.
    /// This is not a claim that lexical retrieval found every relevant source.
    pub complete: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn artifact_and_claim_identities_are_disjoint_and_strict() {
        let artifact: Source =
            serde_json::from_str(r#"{"kind":"artifact","artifact":"captured-file"}"#).unwrap();
        assert!(matches!(artifact, Source::Artifact { .. }));
        let claim: Source =
            serde_json::from_str(r#"{"kind":"claim","claim":"claim","version":"version"}"#)
                .unwrap();
        assert!(matches!(claim, Source::Claim { .. }));
        for invalid in [
            r#"{"kind":"artifact","artifact":"a","claim":"invented"}"#,
            r#"{"kind":"claim","claim":"c"}"#,
            r#"{"kind":"claim","claim":"c","version":"v","artifact":"a"}"#,
        ] {
            assert!(serde_json::from_str::<Source>(invalid).is_err());
        }
    }
}
