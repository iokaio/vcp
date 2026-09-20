// SPDX-License-Identifier: Apache-2.0
use serde::{Deserialize, Serialize};
use vcp_domain::{
    artifact::*, effect::*, ids::*, revision::*, task::*, verification::*, workspace::*,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandEnvelope {
    pub version: u32,
    pub id: CommandId,
    pub workspace: WorkspaceId,
    pub session: SessionId,
    pub task: Option<TaskId>,
    pub caller: ActorId,
    pub controller: ControllerId,
    pub owner_epoch: OwnerEpoch,
    pub expected: Revision,
    pub steering: SteeringRevision,
    pub payload: Command,
}
impl CommandEnvelope {
    pub fn parse_jsonl(bytes: &[u8]) -> Result<Self, crate::version::Error> {
        if bytes.len() > crate::version::MAX_COMMAND_BYTES {
            return Err(crate::version::Error::Limit);
        }
        let command: Self = serde_json::from_slice(bytes)?;
        command.validate_version()?;
        Ok(command)
    }
    pub fn validate_version(&self) -> Result<(), crate::version::Error> {
        if self.version != crate::version::VERSION {
            return Err(crate::version::Error::Version(self.version));
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String, serde_json::Error> {
        Ok(crate::digest_bytes(&crate::canonical_bytes(self)?))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Initialize {
        binding: Binding,
    },
    CreateSession {
        id: SessionId,
        fork_through: Option<TurnId>,
    },
    CreateTask {
        root: TaskId,
        parent: Option<TaskId>,
        fork_origin: Option<TaskId>,
        objective: Objective,
        fingerprint: Fingerprint,
        editing: bool,
        required_checks: Vec<String>,
    },
    Transition {
        next: TaskState,
        reason: String,
        verification: Option<VerificationId>,
    },
    Steer {
        objective: Objective,
    },
    ObserveFingerprint {
        fingerprint: Fingerprint,
    },
    StartTurn {
        id: TurnId,
        trigger: ArtifactId,
    },
    AdvanceTurn {
        id: TurnId,
        next: TurnState,
        reason: String,
    },
    ProposeEffect {
        id: ToolRunId,
        operation_digest: String,
    },
    AdvanceEffect {
        id: ToolRunId,
        next: EffectState,
        reason: String,
        execution: Option<ExecutionId>,
        exit_code: Option<i32>,
        observed_changes: Vec<ArtifactId>,
    },
    RecordVerification {
        verification: Verification,
    },
    AttachArtifact {
        descriptor: ArtifactDescriptor,
    },
    Rebind {
        binding: Binding,
    },
    SetWorkspaceTrust {
        trust: Trust,
    },
    SetPolicy {
        policy: vcp_domain::policy::Policy,
    },
    SetGrant {
        grant: vcp_domain::policy::Grant,
    },
    Ask {
        approval: Approval,
    },
    Decide {
        id: ApprovalId,
        operation_digest: String,
        effect_revision: Revision,
        allow: bool,
    },
    /// A read/inspection request never changes the objective or resumes work.
    Inspect,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalState {
    Pending,
    Allowed,
    Denied,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Approval {
    pub id: ApprovalId,
    pub scope: Scope,
    pub effect: ToolRunId,
    pub effect_revision: Revision,
    pub steering: SteeringRevision,
    pub operation_digest: String,
    pub actor: ActorId,
    pub policy: PolicyRevision,
    pub expires_at: Timestamp,
    pub state: ApprovalState,
    pub revision: Revision,
    /// Absent in legacy records; those remain inspectable but require a fresh
    /// question before they can grant authority to a new owner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controller: Option<ControllerId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_epoch: Option<OwnerEpoch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authority: Option<AuthorityRevision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<Revision>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandReceipt {
    pub version: u32,
    pub command: CommandId,
    pub workspace: WorkspaceId,
    pub digest: String,
    pub transaction: TransactionId,
    pub watermark: Watermark,
    pub first_event: SessionSeq,
    pub last_event: SessionSeq,
    pub result: CommandResult,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum CommandResult {
    Accepted {
        revision: Revision,
    },
    Inspection {
        task: Option<Task>,
    },
    InspectionRedacted {
        task: Option<TaskId>,
        deletion: DeletionEpoch,
        original_payload_digest: String,
    },
}
impl CommandReceipt {
    pub fn jsonl(&self) -> Result<Vec<u8>, serde_json::Error> {
        let mut bytes = serde_json::to_vec(self)?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}
