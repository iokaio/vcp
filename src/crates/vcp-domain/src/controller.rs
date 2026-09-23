// SPDX-License-Identifier: Apache-2.0
//! Durable interactive ownership, separate from process lifetime and tool grants.
use crate::{ActorId, ControllerId, Error, OwnerEpoch, Result, Revision, SessionId, WorkspaceId};
use serde::{Deserialize, Serialize};

pub const DOCUMENT_TYPE: &str = "vcp_controller_lease_v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Holder {
    pub actor: ActorId,
    pub connection: ControllerId,
    pub process_owner: ControllerId,
    pub owner_epoch: OwnerEpoch,
}

/// No clock expiry is implied. A lost controlling connection requires immediate
/// admission fencing and owned pause/draining at the live host boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Acquired,
    Released,
    ConnectionLost,
    ProcessLost,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lease {
    pub document_type: String,
    pub schema_version: u32,
    pub id: String,
    pub workspace: WorkspaceId,
    pub session: SessionId,
    pub revision: Revision,
    /// Increments on acquisition, survives release and process restart.
    pub generation: Revision,
    /// None is released, not anonymous ownership or a reusable authority grant.
    pub holder: Option<Holder>,
    pub reason: Reason,
}

impl Lease {
    pub fn validate(&self) -> Result<()> {
        if self.document_type != DOCUMENT_TYPE || self.schema_version != 1 {
            return Err(Error::Invalid("controller lease schema"));
        }
        crate::TaskId::parse(self.id.clone())?;
        if self.holder.is_some() != matches!(self.reason, Reason::Acquired) {
            return Err(Error::Invalid("controller lease holder and reason"));
        }
        // Acquisition and release strictly alternate; there is no renewal or
        // direct transfer transition. Check reachability even on snapshot load,
        // where the preceding state may no longer be available for comparison.
        let revision = self
            .generation
            .get()
            .checked_sub(1)
            .and_then(|generation| generation.checked_mul(2))
            .and_then(|held_revision| {
                if self.holder.is_some() {
                    Some(held_revision)
                } else {
                    held_revision.checked_add(1)
                }
            });
        if revision != Some(self.revision.get()) {
            return Err(Error::Invalid("controller lease revision and generation"));
        }
        if self
            .holder
            .as_ref()
            .is_some_and(|holder| holder.owner_epoch == OwnerEpoch::ZERO)
        {
            return Err(Error::Invalid("controller lease process owner"));
        }
        Ok(())
    }

    pub fn validate_transition(&self, next: &Self) -> Result<()> {
        self.validate()?;
        next.validate()?;
        if self.id != next.id || self.workspace != next.workspace || self.session != next.session {
            return Err(Error::Scope);
        }
        if next.revision != self.revision.next()? {
            return Err(Error::Stale);
        }
        let valid = match (&self.holder, &next.holder, next.reason) {
            (None, Some(_), Reason::Acquired) => next.generation == self.generation.next()?,
            (Some(_), None, Reason::Released | Reason::ConnectionLost | Reason::ProcessLost) => {
                next.generation == self.generation
            }
            _ => false,
        };
        if !valid {
            return Err(Error::Transition);
        }
        Ok(())
    }
}
