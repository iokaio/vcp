// SPDX-License-Identifier: Apache-2.0
//! The migration boundary is a commitment to a fully replayed legacy prefix,
//! never a checkpoint that authorizes skipping that prefix on later opens.
use super::*;
use crate::{history_catalog::Catalog, history_index::Pages, original_commits::OriginalCommits};
use vcp_domain::Watermark;

const MAX_ORIGIN_BYTES: usize = 16 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Origin {
    version: u32,
    watermark: Watermark,
    legacy_sha256: String,
    identity: String,
    catalog: Catalog,
    originals: OriginalCommits,
}
impl Origin {
    /// The reader borrows the same replayed owner and checks every original
    /// native body against that owner's receipts. All writes remain tentative;
    /// the format marker is published by the caller only after full readiness.
    pub(crate) async fn stage(
        pages: &mut impl Pages,
        state: &State,
        base: Option<&crate::replay_base::ReplayBase>,
        reader: &mut super::super::history::CommitReader<'_>,
    ) -> Result<(Self, String, DurableOwner)> {
        let base_bytes = base.map(canonical_bytes).transpose()?;
        let mut originals =
            OriginalCommits::from_validated_base(pages, base.zip(base_bytes.as_deref())).await?;
        while let Some(original) = reader.next_original().await? {
            originals = originals
                .append_verified(pages, &original.bytes, &original.commit)
                .await?;
        }
        let catalog = Catalog::from_validated_state(pages, state).await?;
        let owner =
            DurableOwner::from_replayed(pages, state, base, catalog.clone(), originals.clone())
                .await?;
        let origin = Self {
            version: 3,
            watermark: state.watermark,
            legacy_sha256: crate::legacy_state_stream::digest(state)?,
            identity: owner.identity().to_owned(),
            catalog,
            originals,
        };
        let bytes = canonical_bytes(&origin)?;
        if bytes.len() > MAX_ORIGIN_BYTES {
            return Err(Error::Limit("history origin descriptor"));
        }
        let digest = digest_bytes(&bytes);
        pages.write(&digest, &bytes).await?;
        Ok((origin, digest, owner))
    }
    pub(crate) async fn load(pages: &mut impl Pages, digest: &str) -> Result<Self> {
        let bytes = pages.read(digest, MAX_ORIGIN_BYTES).await?;
        if bytes.len() > MAX_ORIGIN_BYTES || digest_bytes(&bytes) != digest {
            return Err(Error::Corruption("history origin digest"));
        }
        let value: Self = serde_json::from_slice(&bytes)?;
        if value.version != 3 || canonical_bytes(&value)? != bytes {
            return Err(Error::Corruption("history origin format"));
        }
        Ok(value)
    }
    pub(crate) fn watermark(&self) -> Watermark {
        self.watermark
    }
    pub(crate) async fn verify_original(
        &self,
        pages: &mut impl Pages,
        commit: &Commit,
        payload: &[u8],
    ) -> Result<()> {
        let original = self
            .originals
            .get(pages, commit.receipt.watermark)
            .await?
            .ok_or(Error::Corruption("history origin original missing"))?;
        if original.bytes != payload || original.commit != *commit {
            return Err(Error::Corruption("history origin native payload differs"));
        }
        Ok(())
    }
    /// Must follow full native-prefix replay and verify_original for each row.
    pub(crate) async fn admit(
        &self,
        pages: &mut impl Pages,
        state: &State,
        base: Option<&crate::replay_base::ReplayBase>,
    ) -> Result<DurableOwner> {
        if state.watermark != self.watermark
            || crate::legacy_state_stream::digest(state)? != self.legacy_sha256
        {
            return Err(Error::Corruption("history origin legacy state differs"));
        }
        let owner = DurableOwner::from_replayed(
            pages,
            state,
            base,
            self.catalog.clone(),
            self.originals.clone(),
        )
        .await?;
        if owner.identity() != self.identity {
            return Err(Error::Corruption("history origin owner differs"));
        }
        Ok(owner)
    }
}
