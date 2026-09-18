// SPDX-License-Identifier: Apache-2.0
use crate::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trust {
    Untrusted,
    Trusted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub host: HostId,
    /// Validated and canonicalized by the host adapter, never used as durable identity.
    pub root: String,
    pub repository: String,
    pub worktree: String,
    pub revision: Revision,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub binding: Binding,
    pub trust: Trust,
    pub revision: Revision,
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
}
impl Workspace {
    pub fn validate(&self) -> Result<()> {
        if self.binding.root.is_empty()
            || self.binding.root.len() > 32768
            || self.binding.root.contains('\0')
            || self.binding.repository.is_empty()
            || self.binding.worktree.is_empty()
        {
            return Err(Error::Invalid("workspace binding"));
        }
        Ok(())
    }
    pub fn rebind(&self, expected: Revision, mut binding: Binding) -> Result<Self> {
        if self.revision != expected {
            return Err(Error::Stale);
        }
        binding.revision = self.binding.revision.next()?;
        let mut next = self.clone();
        next.binding = binding;
        next.revision = self.revision.next()?;
        // Execution authority cannot transfer implicitly to a new location/host.
        next.authority = self.authority.next()?;
        next.trust = Trust::Untrusted;
        next.validate()?;
        Ok(next)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub id: SessionId,
    pub workspace: WorkspaceId,
    pub revision: Revision,
    pub configuration: Revision,
    pub fork_origin: Option<SessionId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub workspace: WorkspaceId,
    pub session: SessionId,
    pub task: TaskId,
}
