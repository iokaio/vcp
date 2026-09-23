// SPDX-License-Identifier: Apache-2.0
//! Governed memory inspection projections. These states convey no authority.
use crate::methods::{Counter, Id};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct InspectionState {
    pub memory_sequence: Counter,
    pub canonical_watermark: Counter,
    pub visibility: Visibility,
    pub applicable: bool,
    pub current: bool,
    /// Absent when retention removed the governed version payload.
    pub resolution: Option<Resolution>,
    #[cfg_attr(feature = "schema", schemars(length(max = 64)))]
    pub evidence: Vec<EvidenceState>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Retained,
    Pruned,
    Purged,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Resolution {
    pub outcome: Outcome,
    pub evidence_status: EvidenceStatus,
    #[cfg_attr(feature = "schema", schemars(length(max = 64)))]
    pub conflicts: Vec<Id>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Accepted,
    Disputed,
    Rejected,
    AwaitingReview,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
    Verified,
    Observed,
    Inferred,
    Unverified,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct EvidenceState {
    pub artifact: Id,
    pub availability: EvidenceAvailability,
    pub verification_current: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum EvidenceAvailability {
    Available,
    Unavailable,
    Missing,
    IntegrityFailure,
}
