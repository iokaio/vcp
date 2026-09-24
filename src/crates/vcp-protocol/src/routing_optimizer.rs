// SPDX-License-Identifier: Apache-2.0
//! Explicit local optimizer evidence and exact, ephemeral policy review.
//! Neither a report nor a preview grants authority to dispatch an operation.
use crate::{
    methods::{Counter, Id, Mutation, Scope},
    routing_inspection::{
        Effort, Group, Identity, PolicySummary, Preference, Profile, RetrievalLimits, Text,
    },
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const CAPABILITY: &str = "routing/optimizer/1";
pub const MAX_PAGE_BYTES: usize = 65_536;
pub const MAX_PREVIEW_BYTES: usize = 65_536;
pub const MAX_EDITS: usize = 17;
pub const MAX_ENTRIES: usize = 64;

macro_rules! dto { ($name:ident,$wire:literal {$($(#[$a:meta])* $field:ident:$ty:ty),* $(,)?})=>{
    #[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)] #[serde(deny_unknown_fields)]
    #[cfg_attr(feature="schema",derive(schemars::JsonSchema))]
    #[cfg_attr(feature="schema",schemars(rename=$wire))]
    pub struct $name { $($(#[$a])* pub $field:$ty),* }
}; }
macro_rules! enumeration { ($name:ident,$wire:literal {$($variant:ident),*})=>{
    #[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)] #[serde(rename_all="snake_case")]
    #[cfg_attr(feature="schema",derive(schemars::JsonSchema))]
    #[cfg_attr(feature="schema",schemars(rename=$wire))]
    pub enum $name {$($variant),*}
}; }

enumeration!(Coverage,"RoutingOptimizerCoverage" {Session,Workspace});
enumeration!(ReportSection,"RoutingOptimizerReportSection" {Summary,Cohorts,Uncertainty,Sources,Forecast});
enumeration!(Operation,"RoutingOptimizerOperation" {Apply,Rollback});
enumeration!(Assessment,"RoutingOptimizerAssessment" {NotDispatchAuthority});
dto!(Window,"RoutingOptimizerWindow" {from:Option<Counter>,until:Counter});
dto!(ReportCapture,"RoutingOptimizerReportCapture" {scope:Scope,mutation:Mutation,expected_binding_revision:Counter,window:Window,coverage:Coverage});
dto!(ReportRead,"RoutingOptimizerReportRead" {scope:Scope,report:Id,section:ReportSection,#[cfg_attr(feature="schema",schemars(range(min=1,max=32)))] limit:u32,#[cfg_attr(feature="schema",schemars(length(min=1,max=4096)))] cursor:Option<String>});
dto!(PreviewRequest,"RoutingOptimizerPreviewRequest" {scope:Scope,expected_policy_revision:Counter,proposal:Proposal});
dto!(Apply,"RoutingOptimizerApply" {scope:Scope,mutation:Mutation,expected_binding_revision:Counter,preview_id:Id,#[cfg_attr(feature="schema",schemars(length(min=64,max=64)))] preview_sha256:String});
dto!(Rollback,"RoutingOptimizerRollback" {scope:Scope,mutation:Mutation,expected_binding_revision:Counter,preview_id:Id,#[cfg_attr(feature="schema",schemars(length(min=64,max=64)))] preview_sha256:String});

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(rename = "RoutingOptimizerProposal"))]
pub enum Proposal {
    Apply {
        report: Id,
        #[cfg_attr(feature = "schema", schemars(length(min = 1, max = 17)))]
        edits: Vec<Edit>,
    },
    Rollback {
        target_revision: Counter,
    },
}
dto!(Pin,"RoutingOptimizerPin" {candidate:Identity,#[cfg_attr(feature="schema",schemars(length(max=64)))] fallback_candidates:Vec<Identity>});
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "field",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(rename = "RoutingOptimizerEdit"))]
pub enum Edit {
    RetrievalLimits(Option<RetrievalLimits>),
    InputTokens(Option<Counter>),
    EscalationMaxTransportRetries(Option<u32>),
    EscalationMaxQualitySwitches(Option<u32>),
    EscalationMaxTotalAttempts(Option<u32>),
    EscalationMinimumRepeatedFailures(Option<u32>),
    ReasoningEffort(Option<Effort>),
    OutputTokens(Option<Counter>),
    Profile(Profile),
    Ordering(#[cfg_attr(feature = "schema", schemars(length(min = 4, max = 4)))] Vec<Preference>),
    QualityFloorBps(#[cfg_attr(feature = "schema", schemars(range(max = 10000)))] u16),
    MinimumSamples(#[cfg_attr(feature = "schema", schemars(range(min = 1)))] u32),
    MaximumEvidenceAgeMs(Counter),
    AllowedModels(#[cfg_attr(feature = "schema", schemars(length(max = 64)))] Vec<String>),
    AllowedEndpoints(#[cfg_attr(feature = "schema", schemars(length(max = 64)))] Vec<String>),
    AllowedGroups(#[cfg_attr(feature = "schema", schemars(length(max = 4)))] Vec<Group>),
    Pin(Option<Pin>),
}
enumeration!(Field,"RoutingOptimizerField" {RetrievalLimits,InputTokens,EscalationMaxTransportRetries,EscalationMaxQualitySwitches,EscalationMaxTotalAttempts,EscalationMinimumRepeatedFailures,ReasoningEffort,OutputTokens,Profile,Ordering,QualityFloorBps,MinimumSamples,MaximumEvidenceAgeMs,AllowedModels,AllowedEndpoints,AllowedGroups,Pin,DenyDataCollection,RequireZdr,BroaderTaskClass});
impl Edit {
    pub fn field(&self) -> Field {
        match self {
            Self::RetrievalLimits(_) => Field::RetrievalLimits,
            Self::InputTokens(_) => Field::InputTokens,
            Self::EscalationMaxTransportRetries(_) => Field::EscalationMaxTransportRetries,
            Self::EscalationMaxQualitySwitches(_) => Field::EscalationMaxQualitySwitches,
            Self::EscalationMaxTotalAttempts(_) => Field::EscalationMaxTotalAttempts,
            Self::EscalationMinimumRepeatedFailures(_) => Field::EscalationMinimumRepeatedFailures,
            Self::ReasoningEffort(_) => Field::ReasoningEffort,
            Self::OutputTokens(_) => Field::OutputTokens,
            Self::Profile(_) => Field::Profile,
            Self::Ordering(_) => Field::Ordering,
            Self::QualityFloorBps(_) => Field::QualityFloorBps,
            Self::MinimumSamples(_) => Field::MinimumSamples,
            Self::MaximumEvidenceAgeMs(_) => Field::MaximumEvidenceAgeMs,
            Self::AllowedModels(_) => Field::AllowedModels,
            Self::AllowedEndpoints(_) => Field::AllowedEndpoints,
            Self::AllowedGroups(_) => Field::AllowedGroups,
            Self::Pin(_) => Field::Pin,
        }
    }
}

// Whole reviewed policies are bounded, never silently reduced to allowlist counts.
dto!(ReviewedPolicy,"RoutingOptimizerReviewedPolicy" {summary:PolicySummary,#[cfg_attr(feature="schema",schemars(length(max=64)))] allowed_models:Vec<String>,#[cfg_attr(feature="schema",schemars(length(max=64)))] allowed_endpoints:Vec<String>,#[cfg_attr(feature="schema",schemars(length(max=4)))] allowed_groups:Vec<Group>,#[cfg_attr(feature="schema",schemars(length(max=64)))] pin_fallback:Vec<Identity>});
dto!(PreviewView,"RoutingOptimizerPreviewView" {scope:Scope,preview_id:Id,preview_sha256:String,operation:Operation,report:Option<Id>,base_policy_revision:Counter,target_policy_revision:Option<Counter>,authority_revision:Counter,deletion_revision:Counter,binding_revision:Counter,ceilings_sha256:String,#[cfg_attr(feature="schema",schemars(range(min=1,max=60000)))] expires_in_ms:u32,prior:ReviewedPolicy,persisted:ReviewedPolicy,effective:ReviewedPolicy,#[cfg_attr(feature="schema",schemars(length(max=17)))] selected:Vec<Edit>,#[cfg_attr(feature="schema",schemars(length(max=20)))] changed:Vec<Field>,#[cfg_attr(feature="schema",schemars(length(max=20)))] clamped:Vec<Field>,#[cfg_attr(feature="schema",schemars(length(max=32)))] uncertainty:Vec<Text>,uncertainty_truncated:bool,assessment:Assessment});

dto!(Money,"RoutingOptimizerMoney" {#[cfg_attr(feature="schema",schemars(length(min=1,max=16)))] currency:String,micros:Counter});
dto!(Counts,"RoutingOptimizerCounts" {tasks:Counter,completed:Counter,failed:Counter,cancelled:Counter,unfinished:Counter,abandoned:Option<Counter>,attempts:Counter,retries:Counter,child_tasks:Counter,supporting_attempts:Counter,#[cfg_attr(feature="schema",schemars(length(max=32)))] known_spend:Vec<Money>,uncertain_attempts:Counter,#[cfg_attr(feature="schema",schemars(length(max=32)))] reserved_liability:Vec<Money>,pruned_tasks:Counter});
dto!(Observed,"RoutingOptimizerObserved" {pruned_events:Counter,pruned_attempts:Counter,objective_change_events:Counter,verification_checks_passed:Counter,verification_checks_failed:Counter,verification_checks_not_run:Counter,pruned_verifications:Counter,latency_samples:Counter,submission_to_final_usage_p50_ms:Option<Counter>,submission_to_final_usage_p95_ms:Option<Counter>,attempts_without_complete_latency:Counter});
dto!(Cohort,"RoutingOptimizerCohort" {provider:Text,model:Text,catalog:Text,routing_policy:Text,authority_policy:Counter,task_class:Text,size:Text,role:Text,count:Counter});
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(rename = "RoutingOptimizerReportRow"))]
pub enum ReportRow {
    Summary {
        counts: Counts,
        observed: Observed,
    },
    Cohort {
        value: Cohort,
    },
    Uncertainty {
        text: Text,
    },
    SourceTask {
        task: Id,
    },
    SourceEvent {
        event: Id,
    },
    Forecast {
        artifact: Id,
        sha256: String,
        source_manifest: Id,
        source_task_count: Counter,
    },
}
dto!(ReportPage,"RoutingOptimizerReportPage" {scope:Scope,report:Id,coverage:Coverage,window:Window,cutoff:Counter,watermark:Counter,authority_revision:Counter,deletion_revision:Counter,binding_revision:Counter,cohort_denominator:Text,section:ReportSection,#[cfg_attr(feature="schema",schemars(length(max=32)))] rows:Vec<ReportRow>,#[cfg_attr(feature="schema",schemars(length(min=1,max=4096)))] next_cursor:Option<String>,complete:bool});

fn number(value: &Counter) -> u64 {
    value.as_str().parse().unwrap_or(u64::MAX)
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.contains("://")
        && !value.chars().any(char::is_control)
}
fn identities(values: &[Identity]) -> bool {
    values.len() <= MAX_ENTRIES
        && values
            .iter()
            .all(|v| identifier(&v.model) && identifier(&v.endpoint))
        && values
            .iter()
            .map(|v| (&v.model, &v.endpoint))
            .collect::<BTreeSet<_>>()
            .len()
            == values.len()
}
fn strings(values: &[String]) -> bool {
    values.len() <= MAX_ENTRIES
        && values.iter().all(|v| identifier(v))
        && values.iter().collect::<BTreeSet<_>>().len() == values.len()
}
fn unique<T: PartialEq>(values: &[T]) -> bool {
    values
        .iter()
        .enumerate()
        .all(|(i, v)| !values[..i].contains(v))
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn mutation(value: &Mutation) -> Result<(), &'static str> {
    if number(&value.steering_revision) != 0 {
        Err("optimizer steering must be zero")
    } else {
        Ok(())
    }
}
pub fn validate_capture(value: &ReportCapture) -> Result<(), &'static str> {
    mutation(&value.mutation)?;
    if value
        .window
        .from
        .as_ref()
        .is_some_and(|from| number(from) >= number(&value.window.until))
    {
        return Err("optimizer window bounds");
    }
    Ok(())
}
pub fn validate_read(value: &ReportRead) -> Result<(), &'static str> {
    if !(1..=32).contains(&value.limit)
        || value
            .cursor
            .as_ref()
            .is_some_and(|v| v.is_empty() || v.len() > 4096 || !v.is_ascii())
    {
        Err("optimizer page bounds")
    } else {
        Ok(())
    }
}
pub fn validate_edits(values: &[Edit]) -> Result<(), &'static str> {
    if values.is_empty()
        || values.len() > MAX_EDITS
        || !unique(&values.iter().map(Edit::field).collect::<Vec<_>>())
    {
        return Err("optimizer edit fields");
    }
    for value in values {
        let valid = match value {
            Edit::RetrievalLimits(Some(v)) => {
                (1..=64).contains(&v.results)
                    && (2..=16384).contains(&number(&v.tokens))
                    && (2..=65536).contains(&number(&v.bytes))
            }
            Edit::InputTokens(Some(v))
            | Edit::OutputTokens(Some(v))
            | Edit::MaximumEvidenceAgeMs(v) => number(v) > 0,
            Edit::EscalationMaxTransportRetries(Some(v)) => *v <= 4,
            Edit::EscalationMaxQualitySwitches(Some(v)) => *v <= 8,
            Edit::EscalationMaxTotalAttempts(Some(v))
            | Edit::EscalationMinimumRepeatedFailures(Some(v)) => (1..=64).contains(v),
            Edit::Ordering(v) => v.len() == 4 && unique(v),
            Edit::QualityFloorBps(v) => *v <= 10000,
            Edit::MinimumSamples(v) => *v > 0,
            Edit::AllowedModels(v) | Edit::AllowedEndpoints(v) => strings(v),
            Edit::AllowedGroups(v) => v.len() <= 4 && unique(v),
            Edit::Pin(Some(v)) => {
                identifier(&v.candidate.model)
                    && identifier(&v.candidate.endpoint)
                    && identities(&v.fallback_candidates)
                    && !v.fallback_candidates.contains(&v.candidate)
            }
            _ => true,
        };
        if !valid {
            return Err("optimizer edit bounds");
        }
    }
    if serde_json::to_vec(values)
        .map_err(|_| "optimizer encoding")?
        .len()
        > 32768
    {
        return Err("optimizer edits byte bound");
    }
    Ok(())
}
pub fn validate_preview(value: &PreviewRequest) -> Result<(), &'static str> {
    match &value.proposal {
        Proposal::Apply { edits, .. } => validate_edits(edits),
        Proposal::Rollback { target_revision } => {
            if number(target_revision) >= number(&value.expected_policy_revision) {
                Err("optimizer rollback target")
            } else {
                Ok(())
            }
        }
    }
}
pub fn validate_apply(value: &Apply) -> Result<(), &'static str> {
    mutation(&value.mutation)?;
    if hash(&value.preview_sha256) {
        Ok(())
    } else {
        Err("optimizer preview digest")
    }
}
pub fn validate_rollback(value: &Rollback) -> Result<(), &'static str> {
    mutation(&value.mutation)?;
    if hash(&value.preview_sha256) {
        Ok(())
    } else {
        Err("optimizer preview digest")
    }
}

impl ReportCapture {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_capture(self)
    }
}
impl ReportRead {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_read(self)
    }
}
impl PreviewRequest {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_preview(self)
    }
}
impl Apply {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_apply(self)
    }
}
impl Rollback {
    pub fn validate(&self) -> Result<(), &'static str> {
        validate_rollback(self)
    }
}
