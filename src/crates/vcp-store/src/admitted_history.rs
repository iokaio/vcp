// SPDX-License-Identifier: Apache-2.0
//! Jointly admitted current/catalog pair after complete semantic replay.
//! A cut is data evidence, never a canonical ownership or execution capability.
use crate::{
    contract::State, current_size::CurrentSize, history_catalog::Catalog, history_index::Pages,
    CurrentState, Result,
};

pub(crate) struct AdmittedCut {
    current: CurrentState,
    catalog: Catalog,
    identity: String,
    size: CurrentSize,
    // Derived only from validated rows; never decoded as persisted authority.
    last_event_watermark: vcp_domain::Watermark,
}

#[cfg(test)]
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
        let size = CurrentSize::measure((&current).into())?;
        size.validate((&current).into())?;
        let identity = vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&(
            "vcp-admitted-history-cut/1",
            &catalog,
            current.projection_digest()?,
        ))?);
        Ok(Self {
            current,
            catalog,
            identity,
            size,
            last_event_watermark: replayed
                .events
                .last()
                .map_or(vcp_domain::Watermark::ZERO, |event| event.watermark),
        })
    }
    pub(crate) fn size(&self) -> CurrentSize {
        self.size
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
    pub(crate) fn last_event_watermark(&self) -> vcp_domain::Watermark {
        self.last_event_watermark
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
            size: prepared.size(),
            last_event_watermark: prepared
                .proposed()
                .events
                .last()
                .map_or(self.last_event_watermark, |event| event.watermark),
        })
    }
}
