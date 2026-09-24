// SPDX-License-Identifier: Apache-2.0
//! Encrypted local publication, never proof of remote transfer or restore.
//! Public acceptance records intent; job reads report the independently observed
//! publication, checkpoint and source-release facts. No key or path is a wire input.
use crate::methods::{Counter, Id, Mutation, Scope};
use serde::{Deserialize, Serialize};

pub const CAPABILITY: &str = "backup/publisher/1";
pub const MAX_RESPONSE_BYTES: usize = 16 * 1024;

macro_rules! dto { ($name:ident,$wire:literal {$($field:ident:$ty:ty),* $(,)?})=>{
    #[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
    #[serde(deny_unknown_fields)]
    #[cfg_attr(feature="schema",derive(schemars::JsonSchema))]
    #[cfg_attr(feature="schema",schemars(rename=$wire))]
    pub struct $name { $(pub $field:$ty),* }
}; }
macro_rules! enumeration { ($name:ident,$wire:literal {$($variant:ident),*})=>{
    #[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
    #[serde(rename_all="snake_case")]
    #[cfg_attr(feature="schema",derive(schemars::JsonSchema))]
    #[cfg_attr(feature="schema",schemars(rename=$wire))]
    pub enum $name {$($variant),*}
}; }

dto!(StatusRequest,"BackupPublisherStatusRequest" {scope:Scope});
dto!(Create,"BackupPublisherCreate" {scope:Scope,mutation:Mutation,expected_binding_revision:Counter,capability:Id,expected_capability_generation:Counter});
dto!(Read,"BackupPublisherRead" {scope:Scope,operation:Id});
dto!(Retry,"BackupPublisherRetry" {scope:Scope,mutation:Mutation,expected_binding_revision:Counter,operation:Id,expected_operation_revision:Counter,expected_job_revision:Counter,capability:Id,expected_capability_generation:Counter});
dto!(Cancel,"BackupPublisherCancel" {scope:Scope,mutation:Mutation,expected_binding_revision:Counter,operation:Id,expected_operation_revision:Counter,expected_job_revision:Option<Counter>});

enumeration!(CapabilityReason,"BackupPublisherCapabilityReason" {NotLoaded,Unsupported,ConfigurationUnavailable});
enumeration!(Destination,"BackupPublisherDestination" {LocalEncryptedVault});
enumeration!(CloudTransfer,"BackupPublisherCloudTransfer" {Unknown});
enumeration!(RestoreVerification,"BackupPublisherRestoreVerification" {NotObserved});
enumeration!(Phase,"BackupPublisherPhase" {Queued,Preparing,Captured,ArchiveReady,CiphertextReady,Admitted,Published,Cancelled,Interrupted,ReconciliationRequired});
enumeration!(SourcePins,"BackupPublisherSourcePins" {NotObserved,Held,Released});
enumeration!(LocalPublication,"BackupPublisherLocalPublication" {NotObserved,InProgress,Published,Unknown});
enumeration!(Checkpoint,"BackupPublisherCheckpoint" {Pending,Matched,ReconciliationRequired,NotObserved});
enumeration!(Cleanup,"BackupPublisherCleanup" {Pending,Complete,ReconciliationRequired});
enumeration!(FailureReason,"BackupPublisherFailureReason" {SourceChanged,AuthorizationChanged,CapabilityUnavailable,StagingConflict,PublicationUnknown,CheckpointPending,CleanupPending,Interrupted});

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(rename = "BackupPublisherCapability"))]
pub enum Capability {
    Loaded {
        reference: Id,
        generation: Counter,
        configuration_revision: Counter,
    },
    Unavailable {
        reason: CapabilityReason,
    },
}
dto!(StatusView,"BackupPublisherStatusView" {scope:Scope,watermark:Counter,capability:Capability,busy:bool,active_operation:Option<Id>,destination:Destination,cloud_transfer:CloudTransfer,restore_verification:RestoreVerification});

// revision is the durable public intent/control revision, not workspace or job
// revision. A queued operation can have no native job yet and remain cancellable.
dto!(JobView,"BackupPublisherJobView" {scope:Scope,operation:Id,revision:Counter,job_revision:Option<Counter>,watermark:Counter,phase:Phase,cancel_requested:bool,snapshot_watermark:Option<Counter>,deletion_revision:Counter,authority_revision:Counter,source_pins:SourcePins,local_publication:LocalPublication,checkpoint:Checkpoint,cleanup:Cleanup,failure:Option<FailureReason>,cloud_transfer:CloudTransfer,restore_verification:RestoreVerification});

/// Matches the existing native backup operation-name constraint. This checks
/// canonical lowercase UUID spelling, not a particular UUID version or variant.
pub fn operation_id(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                byte == b'-'
            } else {
                byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
            }
        })
}
fn operation(value: &Id) -> Result<(), &'static str> {
    if operation_id(value.as_str()) {
        Ok(())
    } else {
        Err("backup operation must be a canonical UUID")
    }
}
fn mutation(value: &Mutation) -> Result<(), &'static str> {
    operation(&value.command_id)?;
    if value.steering_revision.as_str() != "0" {
        return Err("backup workspace mutation steering must be zero");
    }
    Ok(())
}
impl StatusRequest {
    pub fn validate(&self) -> Result<(), &'static str> {
        Ok(())
    }
}
impl Create {
    pub fn validate(&self) -> Result<(), &'static str> {
        mutation(&self.mutation)
    }
}
impl Read {
    pub fn validate(&self) -> Result<(), &'static str> {
        operation(&self.operation)
    }
}
impl Retry {
    pub fn validate(&self) -> Result<(), &'static str> {
        mutation(&self.mutation)?;
        operation(&self.operation)
    }
}
impl Cancel {
    pub fn validate(&self) -> Result<(), &'static str> {
        mutation(&self.mutation)?;
        operation(&self.operation)
    }
}
