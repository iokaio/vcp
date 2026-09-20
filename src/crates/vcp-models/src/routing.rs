// SPDX-License-Identifier: Apache-2.0
//! Versioned routing evidence and pure deterministic selection. Selection neither
//! sends requests nor reserves money; actual assembled requests use host admission.
use crate::{catalog::Snapshot, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{
    accounting::{ChargeCategory, Money, RequestRole, Usage},
    Micros, Revision, SteeringRevision, TaskId, Timestamp, Units, WorkspaceId,
};

pub const SCHEMA_VERSION: u32 = 1;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelEndpoint {
    pub model: String,
    /// Exact qualified endpoint tag; aliases do not inherit evidence.
    pub endpoint: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Frontier,
    High,
    Medium,
    Low,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    Low,
    Med,
    High,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Supported,
    Unsupported,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Live,
    Scripted,
    Research,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub source: String,
    pub sha256: String,
    pub observed_at: Timestamp,
    pub effective_at: Option<Timestamp>,
    pub limitations: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompatibilityObservation {
    pub id: String,
    pub compatibility: String,
    pub kind: EvidenceKind,
    pub state: State,
    pub observed_at: Timestamp,
    pub valid_until: Timestamp,
    pub provenance: Vec<Provenance>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleEvidence {
    pub id: String,
    pub role: RequestRole,
    pub task_class: String,
    pub kind: EvidenceKind,
    pub observed_at: Timestamp,
    pub valid_until: Timestamp,
    /// All evaluated tasks, including unsuccessful/abandoned work.
    pub samples: u32,
    /// Graded outcome quality, not model self-reported confidence.
    pub quality_bps: u16,
    pub latency_p50_ms: u64,
    pub latency_p95_ms: u64,
    pub usage_p50: Option<Usage>,
    pub usage_p95: Option<Usage>,
    pub provenance: Vec<Provenance>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupMembership {
    pub version: String,
    pub group: Group,
    pub roles: Vec<RoleEvidence>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub identity: ModelEndpoint,
    pub availability: State,
    /// Retained source reasons, including parse/research rejection diagnostics.
    pub reasons: Vec<String>,
    pub provenance: Vec<Provenance>,
    pub capabilities: BTreeMap<String, State>,
    pub snapshot: Option<Snapshot>,
    pub compatibility: Vec<CompatibilityObservation>,
    pub memberships: Vec<GroupMembership>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogRevision {
    pub schema_version: u32,
    pub id: String,
    pub parent: Option<String>,
    pub observed_at: Timestamp,
    pub effective_at: Option<Timestamp>,
    pub entries: Vec<Candidate>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preference {
    TotalCost,
    Latency,
    Quality,
    Capability,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    pub candidate: ModelEndpoint,
    /// Empty means strict pin. This finite set never broadens other permissions.
    pub fallback_candidates: BTreeSet<ModelEndpoint>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema_version: u32,
    pub id: String,
    pub parent: Option<String>,
    pub profile: Profile,
    /// Explicit finite allowlists. Empty means deny, never unrestricted.
    pub allowed_models: BTreeSet<String>,
    pub allowed_endpoints: BTreeSet<String>,
    pub allowed_groups: BTreeSet<Group>,
    pub quality_floor_bps: u16,
    pub minimum_samples: u32,
    pub maximum_evidence_age_ms: u64,
    pub deny_data_collection: bool,
    pub require_zdr: bool,
    /// All four criteria in explicit order; exact identity breaks final ties.
    pub ordering: Vec<Preference>,
    pub pin: Option<Pin>,
    pub broader_task_class: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CostEstimate {
    pub candidate: ModelEndpoint,
    /// Expected aggregate token/request usage, not a dispatch reservation.
    pub first_attempt: Usage,
    pub retries: Usage,
    pub handoff: Usage,
    /// None is unknown and excludes; explicit zero requires an assumption.
    pub support: Option<Money>,
    pub children: Option<Money>,
    pub verification: Option<Money>,
    pub assumptions: Vec<String>,
    pub evidence_refs: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutingInput {
    pub workspace: WorkspaceId,
    pub root: TaskId,
    pub task: TaskId,
    pub input_revision: Revision,
    pub steering: SteeringRevision,
    pub input_digest: String,
    pub catalog: String,
    pub policy: String,
    pub role: RequestRole,
    pub task_class: String,
    pub now: Timestamp,
    pub required_capabilities: BTreeSet<String>,
    /// Canonically observed failures excluded at this scheduling boundary.
    #[serde(default)]
    pub excluded: BTreeSet<ModelEndpoint>,
    /// Transport retry retains the exact prior endpoint; this grants no fallback.
    #[serde(default)]
    pub retry_pin: Option<ModelEndpoint>,
    pub input_tokens: Units,
    pub output_tokens: Units,
    /// Current ledger view supplied by the host; selection does not consume it.
    pub available: Money,
    pub protected_verification: Micros,
    pub estimates: Vec<CostEstimate>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Exclusion {
    RetryPinned,
    PreviouslyFailed,
    Unavailable,
    UnknownAvailability,
    CatalogNotEffective,
    ModelDenied,
    ProviderDenied,
    MissingSnapshot,
    StaleSnapshot,
    MissingLiveQualification,
    ContradictoryCompatibility,
    UnsupportedCapability,
    UnknownCapability,
    DataPolicy,
    ContextCapacity,
    MissingRoleEvidence,
    StaleRoleEvidence,
    InsufficientSamples,
    QualityFloor,
    ContradictoryMembership,
    GroupDenied,
    MissingCostEstimate,
    UnknownCost,
    InvalidCost,
    CurrencyMismatch,
    InsufficientVerificationReserve,
    Budget,
    PinRestricted,
    PinPreferred,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CostBreakdown {
    pub first_attempt: Micros,
    pub retries: Micros,
    pub handoff: Micros,
    pub support: Micros,
    pub children: Micros,
    pub verification: Micros,
    pub total: Money,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateDecision {
    pub identity: ModelEndpoint,
    pub exclusions: Vec<Exclusion>,
    pub source_reasons: Vec<String>,
    pub group: Option<Group>,
    pub quality_bps: Option<u16>,
    pub samples: Option<u32>,
    pub latency_p95_ms: Option<u64>,
    pub total_estimate: Option<CostBreakdown>,
    pub evidence_refs: Vec<String>,
    pub broader_cohort_used: Option<String>,
    pub assumptions: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutingDecision {
    pub schema_version: u32,
    pub id: String,
    pub input: RoutingInput,
    pub profile: Profile,
    pub ordering: Vec<Preference>,
    /// Eligible candidates ranked first; excluded rows follow by exact identity.
    pub candidates: Vec<CandidateDecision>,
    pub selected: Option<ModelEndpoint>,
    pub fallback_from: Option<ModelEndpoint>,
    /// Always absent: assemble and atomically admit the actual request separately.
    pub immediate_reservation: Option<Money>,
}

mod selection;
mod validation;
pub use selection::select;
