// SPDX-License-Identifier: Apache-2.0
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{
    accounting::*,
    artifact::ArtifactDescriptor,
    effect::Effect,
    task::{Task, Turn},
    verification::Verification,
    workspace::{Session, Workspace},
    *,
};
use vcp_protocol::{
    canonical_bytes,
    command::{Approval, CommandReceipt, CommandResult},
    digest_bytes,
    event::{EventEnvelope, EventInput},
};
#[path = "ingestion_contract.rs"]
mod ingestion_contract;

pub const FORMAT_VERSION: u32 = 1;
pub const MAX_TRANSACTION_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_COMMIT_BYTES: usize = MAX_TRANSACTION_BYTES * 2 + 65_536;
pub const MAX_RECORD_BYTES: usize = 1024 * 1024;
pub const MAX_RECORDS: usize = 100_000;
/// This first schema uses a bounded in-memory canonical view. Reaching capacity
/// rejects admission explicitly; it never truncates or silently prunes history.
pub const MAX_STATE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Collection {
    Workspace,
    Session,
    Task,
    Turn,
    Effect,
    Artifact,
    Verification,
    Approval,
    Ledger,
    Reservation,
    Attempt,
    Settlement,
    Claim,
    IndexIntent,
    Generation,
    Tombstone,
    Projection,
    SnapshotPin,
    Access,
    LocalResources,
}
impl Collection {
    pub fn name(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::Session => "session",
            Self::Task => "task",
            Self::Turn => "turn",
            Self::Effect => "effect",
            Self::Artifact => "artifact",
            Self::Verification => "verification",
            Self::Approval => "approval",
            Self::Ledger => "ledger",
            Self::Reservation => "reservation",
            Self::Attempt => "attempt",
            Self::Settlement => "settlement",
            Self::Claim => "claim",
            Self::IndexIntent => "index_intent",
            Self::Generation => "generation",
            Self::Tombstone => "tombstone",
            Self::Projection => "projection",
            Self::SnapshotPin => "snapshot_pin",
            Self::Access => "access",
            Self::LocalResources => "local_resources",
        }
    }
}
pub fn key(collection: Collection, id: &str) -> String {
    format!("{}:{id}", collection.name())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub collection: Collection,
    pub id: String,
    pub workspace: WorkspaceId,
    pub revision: Revision,
    pub value: serde_json::Value,
    /// Explicit references supplement the mandatory relationships derived from typed data.
    pub references: BTreeSet<String>,
}
impl Record {
    fn memory_kind(&self) -> Result<Option<&str>> {
        let Some(kind) = self.value["document_type"].as_str() else {
            return Ok(None);
        };
        let expected = match kind {
            "vcp_memory_proposal_v1" | "vcp_memory_version_v1" => Collection::Claim,
            "vcp_memory_head_v1" | "vcp_memory_sequence_v1" | "vcp_memory_result_v1" => {
                Collection::Projection
            }
            "vcp_memory_index_intent_v1" => Collection::IndexIntent,
            _ if kind.starts_with("vcp_memory_") => return Err(Error::Incompatible),
            _ => return Ok(None),
        };
        if self.collection != expected {
            return Err(Error::Corruption("memory document collection"));
        }
        Ok(Some(kind))
    }
    fn immutable_memory(&self) -> Result<bool> {
        Ok(matches!(
            self.memory_kind()?,
            Some("vcp_memory_proposal_v1" | "vcp_memory_version_v1" | "vcp_memory_result_v1")
        ))
    }
    pub fn typed<T: Serialize>(
        collection: Collection,
        id: impl Into<String>,
        workspace: WorkspaceId,
        revision: Revision,
        value: &T,
    ) -> Result<Self> {
        let record = Self {
            collection,
            id: id.into(),
            workspace,
            revision,
            value: serde_json::to_value(value)?,
            references: BTreeSet::new(),
        };
        record.validate_shape()?;
        Ok(record)
    }
    pub fn key(&self) -> String {
        key(self.collection, &self.id)
    }
    pub fn decode<T: serde::de::DeserializeOwned>(&self) -> Result<T> {
        Ok(serde_json::from_value(self.value.clone())?)
    }
    pub fn validate_shape(&self) -> Result<()> {
        TaskId::parse(self.id.clone())?;
        if canonical_bytes(self)?.len() > MAX_RECORD_BYTES {
            return Err(Error::Limit("canonical record"));
        }
        if ingestion_contract::kind(self)?.is_some() {
            return ingestion_contract::shape(self);
        }
        let scope = |workspace: &WorkspaceId, id: &str, revision: Revision| -> Result<()> {
            if workspace != &self.workspace || id != self.id || revision != self.revision {
                return Err(Error::Corruption("record identity or revision"));
            }
            Ok(())
        };
        if let Some(kind) = self.memory_kind()? {
            use vcp_domain::memory::*;
            if self.value["schema_version"].as_u64() != Some(1) {
                return Err(Error::Incompatible);
            }
            match kind {
                "vcp_memory_proposal_v1" => {
                    let value: ProposalRecord = self.decode()?;
                    value.validate()?;
                    scope(&value.scope.workspace, value.id.as_str(), value.revision)?;
                    if value.proposal.id != value.id || value.proposal.scope != value.scope {
                        return Err(Error::Corruption("memory proposal identity"));
                    }
                }
                "vcp_memory_version_v1" => {
                    let value: Version = self.decode()?;
                    value.validate()?;
                    scope(&value.scope.workspace, value.id.as_str(), value.revision)?;
                    if value.proposal.scope != value.scope
                        || !matches!(
                            value.resolution.outcome,
                            Outcome::Accepted | Outcome::Disputed
                        )
                    {
                        return Err(Error::Corruption("memory version resolution or scope"));
                    }
                }
                "vcp_memory_head_v1" => {
                    let value: Head = self.decode()?;
                    value.validate()?;
                    scope(&value.scope.workspace, value.id.as_str(), value.revision)?;
                    let unique: BTreeSet<_> = value.disputed.iter().collect();
                    if unique.len() != value.disputed.len()
                        || value.current.as_ref().is_some_and(|id| unique.contains(id))
                    {
                        return Err(Error::Corruption("memory head versions"));
                    }
                }
                "vcp_memory_sequence_v1" => {
                    let value: MemoryHead = self.decode()?;
                    value.validate()?;
                    scope(&value.workspace, value.id.as_str(), value.revision)?;
                    if value.id != value.workspace {
                        return Err(Error::Corruption("memory sequence identity"));
                    }
                }
                "vcp_memory_index_intent_v1" => {
                    let value: IndexIntent = self.decode()?;
                    value.validate()?;
                    scope(&value.scope.workspace, value.id.as_str(), value.revision)?;
                }
                "vcp_memory_result_v1" => {
                    let value: ProposalResult = self.decode()?;
                    value.validate()?;
                    scope(&value.scope.workspace, value.id.as_str(), value.revision)?;
                }
                _ => unreachable!(),
            }
            return Ok(());
        }
        match self.collection {
            Collection::Workspace => {
                let value: Workspace = self.decode()?;
                value.validate()?;
                scope(&value.id, value.id.as_str(), value.revision)?;
            }
            Collection::Session => {
                let value: Session = self.decode()?;
                if value.fork_through.is_some() && value.fork_origin.is_none() {
                    return Err(Error::Corruption("fork boundary requires session ancestry"));
                }
                scope(&value.workspace, value.id.as_str(), value.revision)?;
            }
            Collection::Task => {
                let value: Task = self.decode()?;
                value.validate()?;
                scope(
                    &value.scope.workspace,
                    value.scope.task.as_str(),
                    value.revision,
                )?;
            }
            Collection::Turn => {
                let value: Turn = self.decode()?;
                scope(&value.scope.workspace, value.id.as_str(), value.revision)?;
            }
            Collection::Effect => {
                let value: Effect = self.decode()?;
                scope(&value.scope.workspace, value.id.as_str(), value.revision)?;
            }
            Collection::Artifact => {
                let value: ArtifactDescriptor = self.decode()?;
                value.validate()?;
                scope(
                    &value.spec.scope.workspace,
                    value.spec.id.as_str(),
                    self.revision,
                )?;
            }
            Collection::Verification => {
                let value: Verification = self.decode()?;
                value.fingerprint.validate()?;
                scope(&value.scope.workspace, value.id.as_str(), self.revision)?;
            }
            Collection::Approval => {
                let value: Approval = self.decode()?;
                scope(&value.scope.workspace, value.id.as_str(), value.revision)?;
            }
            Collection::Ledger => {
                let value: Ledger = self.decode()?;
                value.validate()?;
                scope(
                    &value.scope.workspace,
                    value.scope.task.as_str(),
                    value.revision,
                )?;
            }
            Collection::Reservation => {
                let value: Reservation = self.decode()?;
                value.validate()?;
                scope(&value.scope.workspace, value.id.as_str(), value.revision)?;
            }
            Collection::Attempt => {
                let value: Attempt = self.decode()?;
                value.validate()?;
                scope(&value.scope.workspace, value.id.as_str(), value.revision)?;
            }
            Collection::Settlement => {
                let value: Settlement = self.decode()?;
                if value.schema_version != 1
                    || value.normalization_version != 1
                    || value.id != value.observation.id
                    || value.scope != value.observation.scope
                    || value.attempt != value.observation.attempt
                {
                    return Err(Error::Corruption("settlement identity"));
                }
                scope(&value.scope.workspace, value.id.as_str(), self.revision)?;
            }
            Collection::LocalResources => {
                let value: LocalResources = self.decode()?;
                if value.schema_version != 1 || value.source.trim().is_empty() {
                    return Err(Error::Corruption("local resource observation"));
                }
                scope(&value.scope.workspace, value.id.as_str(), self.revision)?;
            }
            Collection::Access if self.value["document_type"] == "vcp_authority_v1" => {
                use vcp_domain::policy::*;
                let value: AuthorityDocument = self.decode()?;
                if value.schema_version != 1 {
                    return Err(Error::Incompatible);
                }
                scope(&value.workspace, value.id.as_str(), value.revision)?;
                match &value.data {
                    AuthorityData::Policy { policy } => {
                        if policy.workspace != value.workspace
                            || value.id.as_str() != value.workspace.as_str()
                            || policy.revision.get() != value.revision.get()
                        {
                            return Err(Error::Corruption("policy identity"));
                        }
                        vcp_policy::validate_policy(policy)
                            .map_err(|_| Error::Corruption("policy shape"))?;
                    }
                    AuthorityData::Grant { grant } => {
                        if grant.scope.workspace() != &value.workspace
                            || value.id.as_str() != grant.id.as_str()
                            || grant.revision != value.revision
                            || grant.id.as_str() == value.workspace.as_str()
                        {
                            return Err(Error::Corruption("grant identity"));
                        }
                        vcp_policy::validate_grant(grant)
                            .map_err(|_| Error::Corruption("grant shape"))?;
                    }
                }
            }
            _ => {
                if !self.value.is_object() {
                    return Err(Error::Corruption(
                        "versioned collection document must be an object",
                    ));
                }
                if self.value.get("schema_version").and_then(|v| v.as_u64()) != Some(1) {
                    return Err(Error::Incompatible);
                }
            }
        }
        Ok(())
    }
    pub fn required_references(&self) -> Result<BTreeSet<String>> {
        let mut refs = self.references.clone();
        if self.collection != Collection::Workspace {
            refs.insert(key(Collection::Workspace, self.workspace.as_str()));
        }
        if ingestion_contract::kind(self)?.is_some() {
            refs.extend(ingestion_contract::references(self)?);
            return Ok(refs);
        }
        if let Some(kind) = self.memory_kind()? {
            use vcp_domain::memory::*;
            if let Some(scope) = self.task_scope()? {
                refs.insert(key(Collection::Task, scope.task.as_str()));
            }
            match kind {
                "vcp_memory_proposal_v1" => {
                    let value: ProposalRecord = self.decode()?;
                    value.validate()?;
                    // Rejected allegations are history, not canonical references.
                    // Only evidence actually validated by governance is authoritative.
                    refs.extend(
                        value
                            .resolution
                            .validated_evidence
                            .iter()
                            .map(|id| key(Collection::Artifact, id.as_str())),
                    );
                }
                "vcp_memory_version_v1" => {
                    let value: Version = self.decode()?;
                    value.validate()?;
                    refs.insert(key(Collection::Claim, value.proposal.id.as_str()));
                    refs.extend(
                        value
                            .resolution
                            .validated_evidence
                            .iter()
                            .map(|id| key(Collection::Artifact, id.as_str())),
                    );
                    refs.extend(
                        value
                            .proposal
                            .predecessor
                            .iter()
                            .chain(value.resolution.conflicts.iter())
                            .map(|id| key(Collection::Claim, id.as_str())),
                    );
                }
                "vcp_memory_head_v1" => {
                    let value: Head = self.decode()?;
                    value.validate()?;
                    refs.extend(
                        value
                            .current
                            .iter()
                            .chain(value.disputed.iter())
                            .map(|id| key(Collection::Claim, id.as_str())),
                    );
                }
                "vcp_memory_index_intent_v1" => {
                    let value: IndexIntent = self.decode()?;
                    value.validate()?;
                    refs.extend(
                        value
                            .versions
                            .iter()
                            .chain(value.supersedes.iter())
                            .map(|id| key(Collection::Claim, id.as_str())),
                    );
                }
                "vcp_memory_result_v1" => {
                    let value: ProposalResult = self.decode()?;
                    value.validate()?;
                    refs.insert(key(Collection::Claim, value.proposal.as_str()));
                    refs.extend(
                        value
                            .version
                            .iter()
                            .map(|id| key(Collection::Claim, id.as_str())),
                    );
                    refs.extend(
                        value
                            .intent
                            .iter()
                            .map(|id| key(Collection::IndexIntent, id.as_str())),
                    );
                }
                _ => (),
            }
            return Ok(refs);
        }
        match self.collection {
            Collection::Access if self.value["document_type"] == "vcp_authority_v1" => {
                use vcp_domain::policy::*;
                let value: AuthorityDocument = self.decode()?;
                if let AuthorityData::Grant { grant } = value.data {
                    if let Some(approval) = &grant.approval {
                        refs.insert(key(Collection::Approval, approval.as_str()));
                    }
                    match grant.scope {
                        GrantScope::Task { scope } => {
                            refs.insert(key(Collection::Task, scope.task.as_str()));
                        }
                        GrantScope::Session { session, .. } => {
                            refs.insert(key(Collection::Session, session.as_str()));
                        }
                        GrantScope::Workspace { .. } => (),
                    }
                }
            }
            Collection::Session => {
                let value: Session = self.decode()?;
                if let Some(origin) = value.fork_origin {
                    refs.insert(key(Collection::Session, origin.as_str()));
                }
                if let Some(turn) = value.fork_through {
                    refs.insert(key(Collection::Turn, turn.as_str()));
                }
            }
            Collection::Task => {
                let value: Task = self.decode()?;
                refs.insert(key(Collection::Session, value.scope.session.as_str()));
                if let Some(parent) = value.parent {
                    refs.insert(key(Collection::Task, parent.as_str()));
                    refs.insert(key(Collection::Task, value.root.as_str()));
                }
                if let Some(origin) = value.fork_origin {
                    refs.insert(key(Collection::Task, origin.as_str()));
                }
            }
            Collection::Turn => {
                let value: Turn = self.decode()?;
                refs.insert(key(Collection::Task, value.scope.task.as_str()));
                refs.insert(key(Collection::Artifact, value.trigger.as_str()));
            }
            Collection::Effect => {
                let value: Effect = self.decode()?;
                refs.insert(key(Collection::Task, value.scope.task.as_str()));
                for id in value.observed_changes {
                    refs.insert(key(Collection::Artifact, id.as_str()));
                }
            }
            Collection::Artifact => {
                let value: ArtifactDescriptor = self.decode()?;
                refs.insert(key(Collection::Task, value.spec.scope.task.as_str()));
            }
            Collection::Verification => {
                let value: Verification = self.decode()?;
                refs.insert(key(Collection::Task, value.scope.task.as_str()));
                for id in value
                    .outputs
                    .into_iter()
                    .chain(value.checks.into_iter().map(|c| c.output))
                {
                    refs.insert(key(Collection::Artifact, id.as_str()));
                }
                for id in value.unresolved_effects {
                    refs.insert(key(Collection::Effect, id.as_str()));
                }
            }
            Collection::Approval => {
                let value: Approval = self.decode()?;
                refs.insert(key(Collection::Effect, value.effect.as_str()));
                refs.insert(key(Collection::Task, value.scope.task.as_str()));
            }
            Collection::Ledger => {
                let value: Ledger = self.decode()?;
                refs.insert(key(Collection::Task, value.scope.task.as_str()));
                for id in value.allocations.keys() {
                    refs.insert(key(Collection::Task, id.as_str()));
                }
            }
            Collection::Reservation => {
                let value: Reservation = self.decode()?;
                refs.insert(key(Collection::Task, value.scope.task.as_str()));
                refs.insert(key(Collection::Ledger, value.root.as_str()));
                refs.insert(key(Collection::Attempt, value.attempt.as_str()));
            }
            Collection::Attempt => {
                let value: Attempt = self.decode()?;
                refs.insert(key(Collection::Task, value.scope.task.as_str()));
                refs.insert(key(Collection::Ledger, value.root.as_str()));
                refs.insert(key(Collection::Reservation, value.reservation.as_str()));
                refs.insert(key(Collection::Artifact, value.request.as_str()));
                if let Some(previous) = value.previous {
                    refs.insert(key(Collection::Attempt, previous.as_str()));
                }
            }
            Collection::Settlement => {
                let value: Settlement = self.decode()?;
                refs.insert(key(Collection::Task, value.scope.task.as_str()));
                refs.insert(key(Collection::Attempt, value.attempt.as_str()));
                refs.insert(key(Collection::Artifact, value.observation.raw.as_str()));
            }
            Collection::LocalResources => {
                let value: LocalResources = self.decode()?;
                refs.insert(key(Collection::Task, value.scope.task.as_str()));
            }
            _ => {}
        }
        Ok(refs)
    }
    fn task_scope(&self) -> Result<Option<vcp_domain::workspace::Scope>> {
        if ingestion_contract::kind(self)?.is_some() {
            return ingestion_contract::scope(self).map(Some);
        }
        if let Some(kind) = self.memory_kind()? {
            use vcp_domain::memory::*;
            return Ok(match kind {
                "vcp_memory_proposal_v1" => Some(self.decode::<ProposalRecord>()?.scope),
                "vcp_memory_version_v1" => Some(self.decode::<Version>()?.scope),
                "vcp_memory_head_v1" => Some(self.decode::<Head>()?.scope),
                "vcp_memory_index_intent_v1" => Some(self.decode::<IndexIntent>()?.scope),
                "vcp_memory_result_v1" => Some(self.decode::<ProposalResult>()?.scope),
                _ => None,
            });
        }
        Ok(match self.collection {
            Collection::Access if self.value["document_type"] == "vcp_authority_v1" => {
                use vcp_domain::policy::*;
                match self.decode::<AuthorityDocument>()?.data {
                    AuthorityData::Grant {
                        grant:
                            Grant {
                                scope: GrantScope::Task { scope },
                                ..
                            },
                    } => Some(scope),
                    _ => None,
                }
            }
            Collection::Task => Some(self.decode::<Task>()?.scope),
            Collection::Turn => Some(self.decode::<Turn>()?.scope),
            Collection::Effect => Some(self.decode::<Effect>()?.scope),
            Collection::Artifact => Some(self.decode::<ArtifactDescriptor>()?.spec.scope),
            Collection::Verification => Some(self.decode::<Verification>()?.scope),
            Collection::Approval => Some(self.decode::<Approval>()?.scope),
            Collection::Ledger => Some(self.decode::<Ledger>()?.scope),
            Collection::Reservation => Some(self.decode::<Reservation>()?.scope),
            Collection::Attempt => Some(self.decode::<Attempt>()?.scope),
            Collection::Settlement => Some(self.decode::<Settlement>()?.scope),
            Collection::LocalResources => Some(self.decode::<LocalResources>()?.scope),
            _ => None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Mutation {
    Put {
        expected: Option<Revision>,
        record: Record,
    },
    /// Only disposable projections may be physically removed through this path.
    DropProjection { id: String, expected: Revision },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptInput {
    pub command: CommandId,
    pub workspace: WorkspaceId,
    pub session: SessionId,
    pub digest: String,
    pub result: CommandResult,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transaction {
    pub id: TransactionId,
    pub expected_watermark: Watermark,
    pub mutations: Vec<Mutation>,
    pub events: Vec<EventInput>,
    pub command: Option<ReceiptInput>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub transaction: TransactionId,
    pub digest: String,
    pub watermark: Watermark,
    pub command: Option<CommandReceipt>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commit {
    pub version: u32,
    pub transaction: Transaction,
    pub receipt: Receipt,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub watermark: Watermark,
    pub records: BTreeMap<String, Record>,
    pub events: Vec<EventEnvelope>,
    pub commands: BTreeMap<String, CommandReceipt>,
    pub transactions: BTreeMap<TransactionId, Receipt>,
    pub sequences: BTreeMap<SessionId, SessionSeq>,
}
pub fn command_key(workspace: &WorkspaceId, command: &CommandId) -> String {
    format!("{workspace}:{command}")
}
impl State {
    fn memory_version(
        &self,
        id: &ClaimVersionId,
        workspace: &WorkspaceId,
    ) -> Result<vcp_domain::memory::Version> {
        let record = self.record(Collection::Claim, id.as_str(), workspace)?;
        if record.memory_kind()? != Some("vcp_memory_version_v1") {
            return Err(Error::Corruption("memory version reference type"));
        }
        record.decode()
    }
    fn validate_memory(&self, record: &Record) -> Result<()> {
        use vcp_domain::memory::*;
        match record.memory_kind()? {
            Some("vcp_memory_version_v1") => {
                let value: Version = record.decode()?;
                let proposal = self.record(
                    Collection::Claim,
                    value.proposal.id.as_str(),
                    &record.workspace,
                )?;
                if proposal.memory_kind()? != Some("vcp_memory_proposal_v1") {
                    return Err(Error::Corruption("memory proposal reference type"));
                }
                let proposal: ProposalRecord = proposal.decode()?;
                if proposal.proposal != value.proposal || proposal.resolution != value.resolution {
                    return Err(Error::Corruption("memory version differs from proposal"));
                }
                if let Some(id) = &value.proposal.predecessor {
                    let predecessor = self.memory_version(id, &record.workspace)?;
                    if predecessor.proposal.claim != value.proposal.claim
                        || predecessor.memory_seq >= value.memory_seq
                    {
                        return Err(Error::Corruption("memory predecessor lineage"));
                    }
                }
                for id in &value.resolution.conflicts {
                    self.memory_version(id, &record.workspace)?;
                }
                if value.canonical_watermark > self.watermark {
                    return Err(Error::Corruption("memory version watermark"));
                }
            }
            Some("vcp_memory_head_v1") => {
                let value: Head = record.decode()?;
                for id in value.current.iter().chain(value.disputed.iter()) {
                    let version = self.memory_version(id, &record.workspace)?;
                    if version.proposal.claim != value.id {
                        return Err(Error::Corruption("memory head claim"));
                    }
                }
            }
            Some("vcp_memory_index_intent_v1") => {
                let value: IndexIntent = record.decode()?;
                for id in value.versions.iter().chain(value.supersedes.iter()) {
                    self.memory_version(id, &record.workspace)?;
                }
                if value.canonical_watermark > self.watermark {
                    return Err(Error::Corruption("memory index watermark"));
                }
            }
            Some("vcp_memory_result_v1") => {
                let value: ProposalResult = record.decode()?;
                let proposal = self.record(
                    Collection::Claim,
                    value.proposal.as_str(),
                    &record.workspace,
                )?;
                if proposal.memory_kind()? != Some("vcp_memory_proposal_v1") {
                    return Err(Error::Corruption("memory result proposal type"));
                }
                let proposal: ProposalRecord = proposal.decode()?;
                if proposal.proposal.command != value.id
                    || proposal.scope != value.scope
                    || proposal.payload_digest != value.payload_digest
                    || proposal.resolution != value.resolution
                {
                    return Err(Error::Corruption("memory result differs from proposal"));
                }
                if let Some(id) = &value.version {
                    let version = self.memory_version(id, &record.workspace)?;
                    if version.proposal.id != value.proposal {
                        return Err(Error::Corruption("memory result version"));
                    }
                }
                if let Some(id) = &value.intent {
                    let intent =
                        self.record(Collection::IndexIntent, id.as_str(), &record.workspace)?;
                    if intent.memory_kind()? != Some("vcp_memory_index_intent_v1") {
                        return Err(Error::Corruption("memory result index type"));
                    }
                }
            }
            _ => (),
        }
        Ok(())
    }
    pub fn record(
        &self,
        collection: Collection,
        id: &str,
        workspace: &WorkspaceId,
    ) -> Result<&Record> {
        let record = self
            .records
            .get(&key(collection, id))
            .ok_or(Error::Conflict("record not found"))?;
        if &record.workspace != workspace {
            return Err(Error::Access);
        }
        Ok(record)
    }
    pub fn command(
        &self,
        workspace: &WorkspaceId,
        id: &CommandId,
        digest: &str,
    ) -> Result<Option<CommandReceipt>> {
        match self.commands.get(&command_key(workspace, id)) {
            Some(receipt) if receipt.digest == digest => Ok(Some(receipt.clone())),
            Some(_) => Err(Error::Conflict("command ID reused with different meaning")),
            None => Ok(None),
        }
    }
    pub fn validate(&self) -> Result<()> {
        if self.records.len() > MAX_RECORDS {
            return Err(Error::Limit("canonical record count"));
        }
        if canonical_bytes(self)?.len() > MAX_STATE_BYTES {
            return Err(Error::Limit(
                "canonical view bytes; explicit migration required",
            ));
        }
        for (key, record) in &self.records {
            if &record.key() != key {
                return Err(Error::Corruption("canonical key"));
            }
            record.validate_shape()?;
            self.validate_memory(record)?;
            if record.collection == Collection::Access
                && record.value["document_type"] == "vcp_authority_v1"
            {
                use vcp_domain::policy::*;
                let document: AuthorityDocument = record.decode()?;
                if let AuthorityData::Grant { grant } = document.data {
                    if let Some(id) = &grant.approval {
                        let approval: Approval = self
                            .record(Collection::Approval, id.as_str(), &record.workspace)?
                            .decode()?;
                        if approval.state != vcp_protocol::command::ApprovalState::Allowed
                            || grant.actor != approval.actor
                            || grant.policy != approval.policy
                            || grant.scope
                                != (GrantScope::Task {
                                    scope: approval.scope.clone(),
                                })
                            || grant.target
                                != (GrantTarget::Exact {
                                    digest: approval.operation_digest.clone(),
                                })
                            || grant.expires_at != approval.expires_at
                            || Some(grant.authority) != approval.authority
                            || Some(grant.binding) != approval.binding
                        {
                            return Err(Error::Corruption("grant differs from recorded approval"));
                        }
                    }
                }
            }
            for reference in record.required_references()? {
                let target = self
                    .records
                    .get(&reference)
                    .ok_or(Error::Corruption("missing canonical reference"))?;
                if target.workspace != record.workspace {
                    return Err(Error::Access);
                }
                if let (Some(source), Some(target)) = (record.task_scope()?, target.task_scope()?) {
                    // Fork and parent links are explicit task relationships. Data
                    // belonging to another task cannot be reused as this task's
                    // turn input, verification, approval, or effect observation.
                    if record.memory_kind()?.is_none()
                        && ingestion_contract::kind(record)?.is_none()
                        && record.collection != Collection::Task
                        && record.collection != Collection::Ledger
                        && reference.split(':').next() != Some("ledger")
                        && source != target
                    {
                        return Err(Error::Access);
                    }
                }
            }
            if let Some(scope) = record.task_scope()? {
                let task: Task = self
                    .record(Collection::Task, scope.task.as_str(), &scope.workspace)?
                    .decode()?;
                if task.scope != scope {
                    return Err(Error::Access);
                }
            }
            if record.collection == Collection::Session {
                let session: Session = record.decode()?;
                if let Some(boundary) = session.fork_through {
                    let turn: Turn = self
                        .record(Collection::Turn, boundary.as_str(), &record.workspace)?
                        .decode()?;
                    if Some(&turn.scope.session) != session.fork_origin.as_ref()
                        || turn.state != vcp_domain::task::TurnState::Completed
                    {
                        return Err(Error::Corruption(
                            "session fork boundary differs from ancestry",
                        ));
                    }
                }
            }
            if record.collection == Collection::Task {
                let task: Task = record.decode()?;
                let mut seen = BTreeSet::from([task.scope.task.clone()]);
                let mut parent = task.parent.clone();
                while let Some(id) = parent {
                    if !seen.insert(id.clone()) {
                        return Err(Error::Corruption("task ancestry cycle"));
                    }
                    let ancestor: Task = self
                        .record(Collection::Task, id.as_str(), &task.scope.workspace)?
                        .decode()?;
                    if ancestor.root != task.root || ancestor.scope.session != task.scope.session {
                        return Err(Error::Corruption("task root or session"));
                    }
                    parent = ancestor.parent;
                }
                let session: Session = self
                    .record(
                        Collection::Session,
                        task.scope.session.as_str(),
                        &task.scope.workspace,
                    )?
                    .decode()?;
                if session.workspace != task.scope.workspace {
                    return Err(Error::Access);
                }
            }
        }
        let mut event_ids = BTreeSet::new();
        let mut sequences = BTreeMap::<SessionId, SessionSeq>::new();
        for event in &self.events {
            if event.version != 1
                || event.watermark > self.watermark
                || !event_ids.insert(event.event.id.clone())
            {
                return Err(Error::Corruption("event identity"));
            }
            self.record(
                Collection::Session,
                event.event.session.as_str(),
                &event.event.workspace,
            )?;
            if let Some(task) = &event.event.task {
                let task: Task = self
                    .record(Collection::Task, task.as_str(), &event.event.workspace)?
                    .decode()?;
                if task.scope.session != event.event.session {
                    return Err(Error::Access);
                }
            }
            for artifact in &event.event.artifacts {
                let artifact: ArtifactDescriptor = self
                    .record(
                        Collection::Artifact,
                        artifact.as_str(),
                        &event.event.workspace,
                    )?
                    .decode()?;
                if artifact.spec.scope.session != event.event.session
                    || event
                        .event
                        .task
                        .as_ref()
                        .is_some_and(|t| t != &artifact.spec.scope.task)
                {
                    return Err(Error::Access);
                }
            }
            let expected = sequences
                .get(&event.event.session)
                .copied()
                .unwrap_or_default()
                .next()?;
            if event.sequence != expected {
                return Err(Error::Corruption("event sequence gap or duplicate"));
            }
            sequences.insert(event.event.session.clone(), expected);
        }
        if sequences != self.sequences {
            return Err(Error::Corruption("session watermark"));
        }
        crate::accounting_contract::validate(self)?;
        ingestion_contract::validate(self)?;
        Ok(())
    }
    pub fn prepare(&self, transaction: &Transaction) -> Result<(Self, Commit)> {
        let bytes = canonical_bytes(transaction)?;
        if bytes.len() > MAX_TRANSACTION_BYTES {
            return Err(Error::Limit("transaction bytes"));
        }
        let digest = digest_bytes(&bytes);
        if let Some(receipt) = self.transactions.get(&transaction.id) {
            if receipt.digest != digest {
                return Err(Error::Conflict("transaction ID reused"));
            }
            return Ok((
                self.clone(),
                Commit {
                    version: FORMAT_VERSION,
                    transaction: transaction.clone(),
                    receipt: receipt.clone(),
                },
            ));
        }
        if transaction.expected_watermark != self.watermark {
            return Err(Error::Conflict("stale canonical watermark"));
        }
        let watermark = self.watermark.next()?;
        let mut result = self.clone();
        result.watermark = watermark;
        let mut touched = BTreeSet::new();
        for mutation in &transaction.mutations {
            match mutation {
                Mutation::Put { expected, record } => {
                    let key = record.key();
                    if !touched.insert(key.clone()) {
                        return Err(Error::Conflict("duplicate mutation"));
                    }
                    match (self.records.get(&key), expected) {
                        (None, None) if record.revision == Revision::ZERO => {
                            ingestion_contract::insert(record)?;
                        }
                        (Some(previous), Some(expected))
                            if previous.revision == *expected
                                && record.revision == expected.next()?
                                && previous.workspace == record.workspace =>
                        {
                            crate::accounting_contract::transition(previous, record)?;
                            ingestion_contract::transition(previous, record)?;
                            if previous.immutable_memory()? {
                                return Err(Error::Conflict("immutable memory evidence"));
                            }
                            if previous.memory_kind()? != record.memory_kind()? {
                                return Err(Error::Conflict("memory document type changed"));
                            }
                            if previous.memory_kind()? == Some("vcp_memory_sequence_v1") {
                                let before: vcp_domain::memory::MemoryHead = previous.decode()?;
                                let after: vcp_domain::memory::MemoryHead = record.decode()?;
                                if after.sequence <= before.sequence {
                                    return Err(Error::Conflict("memory sequence must advance"));
                                }
                            }
                            if previous.memory_kind()? == Some("vcp_memory_index_intent_v1") {
                                let mut before: vcp_domain::memory::IndexIntent =
                                    previous.decode()?;
                                let after: vcp_domain::memory::IndexIntent = record.decode()?;
                                before.revision = after.revision;
                                before.status = after.status;
                                if before != after {
                                    return Err(Error::Conflict(
                                        "memory index intent identity changed",
                                    ));
                                }
                            }
                            if matches!(
                                record.collection,
                                Collection::Verification
                                    | Collection::Settlement
                                    | Collection::LocalResources
                            ) {
                                return Err(Error::Conflict("immutable evidence record"));
                            }
                            if record.collection == Collection::Artifact {
                                let prior: ArtifactDescriptor = previous.decode()?;
                                let next: ArtifactDescriptor = record.decode()?;
                                if prior.state != vcp_domain::artifact::CaptureState::Pending
                                    || next.length < prior.length
                                    || prior.spec.id != next.spec.id
                                {
                                    return Err(Error::Conflict(
                                        "immutable or regressing artifact",
                                    ));
                                }
                            }
                        }
                        _ => return Err(Error::Conflict("entity revision or identity")),
                    }
                    result.records.insert(key, record.clone());
                }
                Mutation::DropProjection { id, expected } => {
                    let key = key(Collection::Projection, id);
                    if self
                        .records
                        .get(&key)
                        .map(Record::immutable_memory)
                        .transpose()?
                        .unwrap_or(false)
                    {
                        return Err(Error::Conflict("immutable memory evidence"));
                    }
                    if !touched.insert(key.clone())
                        || self
                            .records
                            .get(&key)
                            .is_none_or(|r| r.revision != *expected)
                    {
                        return Err(Error::Conflict("projection revision"));
                    }
                    result.records.remove(&key);
                }
            }
        }
        let mut first = SessionSeq::ZERO;
        let mut last = SessionSeq::ZERO;
        for event in &transaction.events {
            let sequence = result
                .sequences
                .get(&event.session)
                .copied()
                .unwrap_or_default()
                .next()?;
            result.sequences.insert(event.session.clone(), sequence);
            if transaction
                .command
                .as_ref()
                .is_some_and(|c| c.session == event.session)
            {
                if first == SessionSeq::ZERO {
                    first = sequence;
                }
                last = sequence;
            }
            result.events.push(EventEnvelope {
                version: 1,
                sequence,
                watermark,
                event: event.clone(),
            });
        }
        let command = if let Some(input) = &transaction.command {
            result.record(
                Collection::Session,
                input.session.as_str(),
                &input.workspace,
            )?;
            if input.digest.len() != 64
                || !input
                    .digest
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            {
                return Err(Error::Corruption("command digest"));
            }
            if transaction.events.iter().any(|e| {
                e.workspace != input.workspace
                    || e.session != input.session
                    || e.correlation != input.command
            }) {
                return Err(Error::Access);
            }
            let key = command_key(&input.workspace, &input.command);
            if result.commands.contains_key(&key) {
                return Err(Error::Conflict("command already committed"));
            }
            let receipt = CommandReceipt {
                version: 1,
                command: input.command.clone(),
                workspace: input.workspace.clone(),
                digest: input.digest.clone(),
                transaction: transaction.id.clone(),
                watermark,
                first_event: first,
                last_event: last,
                result: input.result.clone(),
            };
            result.commands.insert(key, receipt.clone());
            Some(receipt)
        } else {
            None
        };
        let receipt = Receipt {
            transaction: transaction.id.clone(),
            digest,
            watermark,
            command,
        };
        result
            .transactions
            .insert(transaction.id.clone(), receipt.clone());
        result.validate()?;
        crate::accounting_contract::admission(self, &result, transaction)?;
        Ok((
            result,
            Commit {
                version: FORMAT_VERSION,
                transaction: transaction.clone(),
                receipt,
            },
        ))
    }
    pub fn replay(&mut self, commit: &Commit) -> Result<()> {
        if commit.version != FORMAT_VERSION {
            return Err(Error::Incompatible);
        }
        if self.transactions.contains_key(&commit.transaction.id) {
            return Err(Error::Corruption("duplicate persisted transaction"));
        }
        let (next, expected) = self.prepare(&commit.transaction)?;
        if expected != *commit {
            return Err(Error::Corruption("durable receipt mismatch"));
        }
        *self = next;
        Ok(())
    }
}

#[allow(async_fn_in_trait)]
pub trait CanonicalStore {
    fn state(&self) -> &State;
    async fn transact(&mut self, transaction: Transaction) -> Result<Receipt>;
}
