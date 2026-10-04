// SPDX-License-Identifier: Apache-2.0
//! Test-only qualification of a jointly admitted current/catalog pair.
//! A cut is data evidence, never a canonical ownership or execution capability.
use crate::{
    contract::State, current_size::CurrentSize, history_catalog::Catalog, history_index::Pages,
    CurrentState, Result,
};

pub(crate) struct AdmittedCut {
    current: CurrentState,
    catalog: Catalog,
    identity: String,
}

#[path = "admitted_history_tests.rs"]
mod tests;
impl AdmittedCut {
    /// The caller must have completed mandatory semantic replay of every
    /// retained transition. Final-state validity alone cannot establish that
    /// historical fact. Keep the source ownership lock/generation pin alive
    /// throughout admission and all subsequent physical reads of this cut.
    pub(crate) async fn from_replayed(
        pages: &mut impl Pages,
        replayed: &State,
        catalog: Catalog,
    ) -> Result<Self> {
        replayed.validate()?;
        catalog.verify_replayed_state(pages, replayed).await?;
        let current = CurrentState::from_state(replayed);
        CurrentSize::measure((&current).into())?.validate((&current).into())?;
        let identity = vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&(
            "vcp-admitted-history-cut/1",
            &catalog,
            current.projection_digest()?,
        ))?);
        Ok(Self {
            current,
            catalog,
            identity,
        })
    }
    pub(crate) fn current(&self) -> &CurrentState {
        &self.current
    }
    pub(crate) fn catalog(&self) -> &Catalog {
        &self.catalog
    }
    pub(crate) fn identity(&self) -> &str {
        &self.identity
    }

    /// Stage the exact certificate's append without changing this source cut.
    /// The returned cut is still tentative: only the canonical backend's
    /// durable publication boundary may install it as the owner's live cut.
    pub(crate) async fn advance(
        &self,
        pages: &mut impl Pages,
        prepared: &crate::contract::current_transition::PreparedCurrent,
    ) -> Result<Self> {
        let catalog = self.catalog.append_current(pages, self, prepared).await?;
        let current = prepared.proposed().current.clone();
        let identity = vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&(
            "vcp-admitted-history-cut/1",
            &catalog,
            current.projection_digest()?,
        ))?);
        Ok(Self {
            current,
            catalog,
            identity,
        })
    }
}
