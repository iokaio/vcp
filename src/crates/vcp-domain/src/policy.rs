// SPDX-License-Identifier: Apache-2.0
//! Authority data contains no executable capability. Trusted adapters prepare
//! operations and the broker rechecks current facts before any dispatch.
use crate::{workspace::Scope, *};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectClass {
    Read,
    Write,
    Execute,
    Network,
    Install,
    Publish,
    Opaque,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Isolation {
    PathContainment,
    JobTree,
    ProcessCount,
    FilteredEnvironment,
    WorkspaceFilesystem,
    NoNetwork,
    Pty,
    Timeout,
    OutputLimit,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Autonomy {
    Plan,
    Ask,
    #[default]
    Workspace,
    Autonomous,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleOrigin {
    Host,
    User,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub root: RootId,
    /// Canonical forward-slash relative name; empty denotes the whole root.
    pub path: String,
    pub write: bool,
    /// Hash of the complete native identity/version observation, including absence.
    pub version: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Invocation {
    Local,
    Process {
        executable: String,
        executable_identity: String,
        arguments: Vec<String>,
        directory: String,
        environment_digest: String,
        /// Shell syntax is opaque; it never receives a read-only classification.
        shell: bool,
    },
    Remote {
        server_identity: String,
        endpoint: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub scope: Scope,
    pub actor: ActorId,
    pub host: HostId,
    pub binding: Revision,
    pub authority: AuthorityRevision,
    pub steering: SteeringRevision,
    pub policy: PolicyRevision,
    pub tool: String,
    pub schema: String,
    /// Canonical JSON object, independently validated by the registered tool.
    pub arguments: String,
    pub invocation: Invocation,
    pub resources: Vec<Resource>,
    pub effects: BTreeSet<EffectClass>,
    pub required_isolation: BTreeSet<Isolation>,
    pub timeout_ms: Units,
    pub output_bytes: ByteCount,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Denial {
    pub id: String,
    pub origin: RuleOrigin,
    pub reason: String,
    /// Empty means any effect/tool/root/path respectively.
    pub effects: BTreeSet<EffectClass>,
    pub tool: Option<String>,
    pub roots: BTreeSet<RootId>,
    pub paths: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GrantScope {
    Workspace {
        workspace: WorkspaceId,
    },
    Session {
        workspace: WorkspaceId,
        session: SessionId,
    },
    Task {
        scope: Scope,
    },
}
impl GrantScope {
    pub fn contains(&self, scope: &Scope) -> bool {
        match self {
            Self::Workspace { workspace } => workspace == &scope.workspace,
            Self::Session { workspace, session } => {
                workspace == &scope.workspace && session == &scope.session
            }
            Self::Task { scope: granted } => granted == scope,
        }
    }
    pub fn workspace(&self) -> &WorkspaceId {
        match self {
            Self::Workspace { workspace } | Self::Session { workspace, .. } => workspace,
            Self::Task { scope } => &scope.workspace,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GrantTarget {
    Exact {
        digest: String,
    },
    /// Trusted configuration only. Every field is exact except the explicit
    /// resource prefixes. No regex, shell-prefix or model risk label grants.
    Configured {
        tool: String,
        schema: String,
        arguments_digest: String,
        invocation: Invocation,
        effects: BTreeSet<EffectClass>,
        roots: BTreeSet<RootId>,
        paths: Vec<String>,
        isolation: BTreeSet<Isolation>,
        timeout_ms: Units,
        output_bytes: ByteCount,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    pub id: GrantId,
    pub actor: ActorId,
    pub scope: GrantScope,
    pub host: HostId,
    pub binding: Revision,
    pub authority: AuthorityRevision,
    pub policy: PolicyRevision,
    pub expires_at: Timestamp,
    pub target: GrantTarget,
    pub origin: RuleOrigin,
    pub reason: String,
    pub revoked: bool,
    pub revision: Revision,
    pub approval: Option<ApprovalId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub workspace: WorkspaceId,
    pub revision: PolicyRevision,
    pub mode: Autonomy,
    pub denials: Vec<Denial>,
    pub workspace_roots: BTreeSet<RootId>,
    /// Autonomous mode uses this explicit ceiling; empty grants nothing.
    pub automatic_effects: BTreeSet<EffectClass>,
    pub timeout_ceiling_ms: Units,
    pub output_ceiling_bytes: ByteCount,
}

/// Versioned documents in the canonical access collection. They are durable
/// policy inputs, never portable OS authority or a broker dispatch permit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityDocument {
    pub document_type: AuthorityFormat,
    pub schema_version: u32,
    pub id: AuthorityId,
    pub workspace: WorkspaceId,
    pub revision: Revision,
    pub data: AuthorityData,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthorityFormat {
    #[serde(rename = "vcp_authority_v1")]
    VcpAuthorityV1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuthorityData {
    Policy { policy: Policy },
    Grant { grant: Grant },
}
