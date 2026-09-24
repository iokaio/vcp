// SPDX-License-Identifier: Apache-2.0
//! Read-only observed routing configuration; rows never grant dispatch authority.
use crate::methods::{Counter, EvidenceReference, Id, Scope};
use serde::{Deserialize, Serialize};
pub const CAPABILITY: &str = "routing/status/1";
pub const MAX_PAGE_BYTES: usize = 65536;
macro_rules! dto { ($name:ident,$wire:literal { $($(#[$a:meta])* $field:ident:$ty:ty),* $(,)? })=>{
    #[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)] #[serde(deny_unknown_fields)]
    #[cfg_attr(feature="schema",derive(schemars::JsonSchema))] #[cfg_attr(feature="schema",schemars(rename=$wire))]
    pub struct $name { $($(#[$a])* pub $field:$ty),* }
}; }
macro_rules! enumeration { ($name:ident,$wire:literal {$($variant:ident),*})=>{
    #[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)] #[serde(rename_all="snake_case")]
    #[cfg_attr(feature="schema",derive(schemars::JsonSchema))] #[cfg_attr(feature="schema",schemars(rename=$wire))]
    pub enum $name {$($variant),*}
}; }
enumeration!(Section,"RoutingStatusSection" {PolicyEntries,Catalog});
enumeration!(Profile,"RoutingStatusProfile" {Low,Med,High});
enumeration!(Group,"RoutingStatusGroup" {Frontier,High,Medium,Low});
enumeration!(Preference,"RoutingStatusPreference" {TotalCost,Latency,Quality,Capability});
enumeration!(Effort,"RoutingStatusEffort" {Minimal,Low,Medium,High});
enumeration!(Availability,"RoutingStatusAvailability" {Supported,Unsupported,Unknown});
enumeration!(Representation,"RoutingStatusRepresentation" {Persisted,Effective});
enumeration!(EffectiveReason,"RoutingStatusEffectiveReason" {NotConfigured,HostUnconfigured});
enumeration!(RegistryReason,"RoutingStatusRegistryReason" {NotConfigured,Restricted,SourceUnavailable});
enumeration!(SourceAvailability,"RoutingStatusSourceAvailability" {RetainedMetadataOnly});
enumeration!(ReportCapture,"RoutingStatusReportCapture" {ExplicitMutationRequired});
enumeration!(Preferences,"RoutingStatusPreferences" {WorkspaceAuthorityRequired});
enumeration!(Preview,"RoutingStatusPreview" {HostCeilingsRequired,SeparateAuthorizedOperation});
enumeration!(RemoteAdvice,"RoutingStatusRemoteAdvice" {Disabled});
dto!(Request,"RoutingStatusRequest" {scope:Scope,task:Id,section:Section,#[cfg_attr(feature="schema",schemars(range(min=1,max=32)))] limit:u32,#[cfg_attr(feature="schema",schemars(length(min=1,max=4096)))] cursor:Option<String>});
dto!(Text,"RoutingStatusText" {#[cfg_attr(feature="schema",schemars(length(max=512)))] text:String,truncated:bool});
dto!(Identity,"RoutingStatusIdentity" {model:String,endpoint:String});
dto!(RetrievalLimits,"RoutingStatusRetrievalLimits" {results:u32,tokens:Counter,bytes:Counter});
dto!(EscalationLimits,"RoutingStatusEscalationLimits" {max_transport_retries:Option<u32>,max_quality_switches:Option<u32>,max_total_attempts:Option<u32>,minimum_repeated_failures:Option<u32>});
dto!(PolicySummary,"RoutingStatusPolicySummary" {id:String,parent_id:Option<String>,profile:Profile,quality_floor_bps:u16,minimum_samples:u32,maximum_evidence_age_ms:Counter,deny_data_collection:bool,require_zdr:bool,ordering:Vec<Preference>,pin:Option<Identity>,input_tokens:Option<Counter>,output_tokens:Option<Counter>,reasoning_effort:Option<Effort>,retrieval_limits:Option<RetrievalLimits>,escalation_limits:Option<EscalationLimits>,broader_task_class:Option<Text>,allowed_models_count:Counter,allowed_endpoints_count:Counter,allowed_groups_count:Counter,pin_fallback_count:Counter});
dto!(PublishedPolicy,"RoutingStatusPublishedPolicy" {revision:Counter,parent_revision:Option<Counter>,actor:Id,authority_revision:Counter,published_at_ms:Counter,policy:PolicySummary});
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(rename = "RoutingStatusEffective"))]
pub enum Effective {
    Observed {
        ceilings_sha256: String,
        policy: PolicySummary,
    },
    Unavailable {
        reason: EffectiveReason,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(rename = "RoutingStatusRegistry"))]
pub enum Registry {
    Observed {
        revision: Counter,
        catalog_id: String,
        observed_at_ms: Counter,
        effective_at_ms: Option<Counter>,
        source: EvidenceReference,
        source_task: Id,
        source_availability: SourceAvailability,
    },
    Unavailable {
        reason: RegistryReason,
    },
}
dto!(Optimizer,"RoutingStatusOptimizer" {report_capture:ReportCapture,preferences:Preferences,preview:Preview,remote_advice:RemoteAdvice});
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(rename = "RoutingStatusEntry"))]
pub enum Entry {
    AllowedModel { value: String },
    AllowedEndpoint { value: String },
    AllowedGroup { value: Group },
    PinFallback { model: String, endpoint: String },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(rename = "RoutingStatusRow"))]
pub enum Row {
    PolicyEntry {
        representation: Representation,
        entry: Entry,
    },
    Candidate {
        model: String,
        endpoint: String,
        availability: Availability,
        groups: Vec<Group>,
        compatibility_count: Counter,
        role_evidence_count: Counter,
        capability_count: Counter,
        provenance_count: Counter,
        reasons: Vec<Text>,
        reasons_truncated: bool,
    },
}
dto!(Page,"RoutingStatusPage" {scope:Scope,task:Id,watermark:Counter,observed_at_ms:Counter,authority_revision:Counter,deletion_revision:Counter,binding_revision:Counter,persisted:Option<PublishedPolicy>,effective:Effective,registry:Registry,optimizer:Optimizer,section:Section,rows:Vec<Row>,next_cursor:Option<String>,complete:bool});
pub fn validate_request(value: &Request) -> Result<(), &'static str> {
    if !(1..=32).contains(&value.limit)
        || value
            .cursor
            .as_ref()
            .is_some_and(|value| value.is_empty() || value.len() > 4096)
    {
        Err("routing status bounds")
    } else {
        Ok(())
    }
}
