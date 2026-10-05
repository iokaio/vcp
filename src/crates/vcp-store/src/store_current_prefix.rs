// SPDX-License-Identifier: Apache-2.0
//! Explicit historical cuts. Full native replay is retained; only the resulting
//! current records and authenticated history roots survive reconstruction.
use super::*;
use crate::{backend::current_publication::Origin, store_history_reader::ReadPages};
#[cfg(test)]
#[path = "store_current_prefix_tests.rs"]
mod tests;

pub(crate) enum ReplayedCut {
    Legacy {
        current: crate::CurrentState,
        events: u64,
        digest: String,
    },
    Current(DurableOwner),
}
impl Opened {
    pub(crate) async fn verified_base(&self) -> Result<Option<crate::replay_base::ReplayBase>> {
        self.ensure_healthy()?;
        let base = crate::replay_base::ReplayBase::load(self.canonical_lock().root())?;
        let mut pages = ReadPages::from_locked(self.canonical_lock(), self.kind())?;
        let checked = self
            .owner
            .originals()
            .verify_base(&mut pages, base.as_ref())
            .await;
        let closed = pages.close().await;
        checked?;
        closed?;
        Ok(base)
    }
    pub(crate) async fn reconstruct(
        &self,
        watermark: vcp_domain::Watermark,
    ) -> Result<ReplayedCut> {
        self.ensure_healthy()?;
        if watermark > self.current().watermark {
            return Err(Error::Corruption("snapshot beyond canonical watermark"));
        }
        if watermark < self.owner.originals().base_watermark() {
            return Err(Error::Unavailable("history precedes retained replay base"));
        }
        let base = self.verified_base().await?;
        let mut pages = ReadPages::from_locked(self.canonical_lock(), self.kind())?;
        let mut reader = self.history().await?;
        let result = async {
            let origin = Origin::load(&mut pages, &self.origin_digest).await?;
            let mut state = base
                .as_ref()
                .map(|value| value.state.clone())
                .unwrap_or_default();
            while state.watermark < watermark.min(origin.watermark()) {
                let original = reader
                    .next_original()
                    .await?
                    .ok_or(Error::Corruption("snapshot prefix missing"))?;
                if reader.publication().is_some() {
                    return Err(Error::Corruption("legacy snapshot publication"));
                }
                origin
                    .verify_original(&mut pages, &original.commit, &original.bytes)
                    .await?;
                state = state.into_replayed(&original.commit)?;
            }
            if watermark < origin.watermark() {
                return Ok(ReplayedCut::Legacy {
                    current: crate::CurrentState::from_state(&state),
                    events: u64::try_from(state.events.len())
                        .map_err(|_| Error::Limit("history ordinal"))?,
                    digest: crate::legacy_state_stream::digest(&state)?,
                });
            }
            let mut owner = origin.admit(&mut pages, &state, base.as_ref()).await?;
            drop(state);
            drop(base);
            while owner.semantic().current().watermark < watermark {
                let original = reader
                    .next_original()
                    .await?
                    .ok_or(Error::Corruption("snapshot prefix missing"))?;
                let publication = reader
                    .publication()
                    .ok_or(Error::Corruption("snapshot publication missing"))?;
                owner = crate::backend::current_publication::replay::replay_payload(
                    &mut pages,
                    &owner,
                    &original.bytes,
                    publication,
                )
                .await?;
            }
            if watermark == self.current().watermark && reader.next_original().await?.is_some() {
                return Err(Error::Corruption("snapshot prefix extent"));
            }
            Ok(ReplayedCut::Current(owner))
        }
        .await;
        let history_closed = reader.close().await;
        let pages_closed = pages.close().await;
        let cut = result?;
        history_closed?;
        pages_closed?;
        Ok(cut)
    }
    pub(crate) async fn prefix_digest(&self, watermark: vcp_domain::Watermark) -> Result<String> {
        match self.reconstruct(watermark).await? {
            ReplayedCut::Legacy { digest, .. } => Ok(digest),
            ReplayedCut::Current(owner) => {
                let mut pages = ReadPages::from_locked(self.canonical_lock(), self.kind())?;
                let digest = owner
                    .semantic()
                    .catalog()
                    .legacy_digest(&mut pages, owner.semantic().current().into())
                    .await;
                let closed = pages.close().await;
                let digest = digest?;
                closed?;
                Ok(digest)
            }
        }
    }
}
