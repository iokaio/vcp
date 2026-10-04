// SPDX-License-Identifier: Apache-2.0
//! Canonical component of neutral-history/2. Not a complete archive: artifact
//! and selected-input closure must be bound by the outer inventory. Caller must
//! retain the native PinnedDurableSnapshot through capture and physical reads.
use crate::{
    contract::{Collection, State, MAX_STATE_BYTES},
    current_size::{CurrentSize, MAX_CURRENT_STATE_BYTES},
    durable_owner::{DurableOwner, Outcome},
    history_blob::{self, Blob, Reader, Writer},
    history_catalog::Catalog,
    history_index::Pages,
    original_commits::OriginalCommits,
    replay_base::ReplayBase,
    CurrentStateView, Error, Result,
};
use serde::{Deserialize, Serialize};
use vcp_domain::{Watermark, WorkspaceId};
use vcp_protocol::canonical_bytes;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Canonical {
    version: u32,
    workspace: WorkspaceId,
    watermark: Watermark,
    current: Blob,
    catalog: Catalog,
    originals: OriginalCommits,
    legacy_digest: String,
}
fn scope(current: CurrentStateView<'_>, workspace: &WorkspaceId) -> Result<()> {
    current.record(Collection::Workspace, workspace.as_str(), workspace)?;
    if current
        .records
        .values()
        .any(|row| &row.workspace != workspace)
    {
        return Err(Error::Access);
    }
    Ok(())
}
impl Canonical {
    pub(crate) fn workspace(&self) -> &WorkspaceId {
        &self.workspace
    }
    pub(crate) async fn copy_objects(
        &self,
        source: &mut impl Pages,
        destination: &mut impl Pages,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        self.current
            .copy_objects(source, destination, check)
            .await?;
        self.catalog
            .copy_objects(source, destination, check)
            .await?;
        self.originals
            .copy_objects(source, destination, check)
            .await
    }
    pub(crate) async fn capture(
        owner: &DurableOwner,
        source: &mut impl Pages,
        destination: &mut impl Pages,
        workspace: &WorkspaceId,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Self> {
        check()?;
        let current = owner.semantic().current();
        scope(current.into(), workspace)?;
        owner
            .semantic()
            .catalog()
            .verify_workspace(source, workspace, check)
            .await?;
        owner
            .originals()
            .copy_objects(source, destination, check)
            .await?;
        owner
            .semantic()
            .catalog()
            .copy_objects(source, destination, check)
            .await?;
        let legacy_digest = owner
            .semantic()
            .catalog()
            .legacy_digest(source, current.into())
            .await?;
        let current_blob = current_blob(destination, current.into(), check).await?;
        Ok(Self {
            version: 2,
            workspace: workspace.clone(),
            watermark: current.watermark,
            current: current_blob,
            catalog: owner.semantic().catalog().clone(),
            originals: owner.originals().clone(),
            legacy_digest,
        })
    }
    /// The source bytes have passed transport authentication only. Every
    /// retained transition is prepared again in its original order before the
    /// claimed final projection and every catalog index are accepted.
    pub(crate) async fn replay(
        &self,
        source: &mut impl Pages,
        destination: &mut impl Pages,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<DurableOwner> {
        check()?;
        if self.version != 2
            || self.watermark != self.catalog.watermark()
            || self.watermark != self.originals.watermark()
        {
            return Err(Error::Corruption("neutral canonical component cut"));
        }
        self.current.validate()?;
        if self.current.bytes > MAX_CURRENT_STATE_BYTES as u64 {
            return Err(Error::Limit("neutral current projection bytes"));
        }
        let base_bytes = if let Some(blob) = self.originals.base_blob() {
            Some(history_blob::read_bounded(source, blob, MAX_STATE_BYTES + 1024 * 1024).await?)
        } else {
            None
        };
        let base = base_bytes.as_deref().map(ReplayBase::decode).transpose()?;
        self.originals.verify_base(source, base.as_ref()).await?;
        let seed = base
            .as_ref()
            .map(|base| base.state.clone())
            .unwrap_or_default();
        let originals = OriginalCommits::from_validated_base(
            destination,
            base.as_ref().zip(base_bytes.as_deref()),
        )
        .await?;
        let catalog = Catalog::from_validated_state(destination, &seed).await?;
        let mut owner =
            DurableOwner::from_replayed(destination, &seed, base.as_ref(), catalog, originals)
                .await?;
        // The bounded legacy retention boundary is the only archival DTO here.
        // Release it before replay advances; later history stays in pages.
        drop(seed);
        drop(base);
        drop(base_bytes);
        let mut watermark = self.originals.base_watermark();
        while watermark < self.watermark {
            check()?;
            watermark = watermark.next()?;
            let original = self
                .originals
                .get(source, watermark)
                .await?
                .ok_or(Error::Corruption("neutral original commit missing"))?;
            let Outcome::Prepared(prepared) = owner
                .prepare(destination, &original.commit.transaction)
                .await?
            else {
                return Err(Error::Corruption("neutral replay duplicate transaction"));
            };
            if prepared.commit() != &original.commit {
                return Err(Error::Corruption("neutral replay receipt differs"));
            }
            owner = owner
                .advance(destination, &prepared, &original.bytes)
                .await?;
        }
        check()?;
        let current = owner.semantic().current();
        scope(current.into(), &self.workspace)?;
        owner
            .semantic()
            .catalog()
            .verify_workspace(destination, &self.workspace, check)
            .await?;
        if CurrentSize::measure(current.into())?.bytes() as u64 != self.current.bytes
            || current.projection_digest()? != self.current.sha256
        {
            return Err(Error::Corruption(
                "neutral replay current projection differs",
            ));
        }
        let mut reader = Reader::new(&self.current)?;
        loop {
            check()?;
            if reader.next(source).await?.is_none() {
                break;
            }
        }
        self.catalog
            .verify_replayed_catalog(source, owner.semantic().catalog(), destination, check)
            .await?;
        if owner
            .semantic()
            .catalog()
            .legacy_digest(destination, current.into())
            .await?
            != self.legacy_digest
        {
            return Err(Error::Corruption(
                "neutral replay legacy commitment differs",
            ));
        }
        check()?;
        Ok(owner)
    }
}

/// Row-wise canonical current projection encoding. No complete JSON value or
/// aggregate byte vector is assembled even within the bounded current view.
async fn current_blob(
    pages: &mut impl Pages,
    current: CurrentStateView<'_>,
    check: &dyn Fn() -> Result<()>,
) -> Result<Blob> {
    CurrentSize::measure(current)?.validate(current)?;
    let mut writer = Writer::new();
    writer.push(pages, b"{\"records\":{").await?;
    for (index, (key, value)) in current.records.iter().enumerate() {
        check()?;
        if index > 0 {
            writer.push(pages, b",").await?;
        }
        writer.push(pages, &canonical_bytes(key)?).await?;
        writer.push(pages, b":").await?;
        writer.push(pages, &canonical_bytes(value)?).await?;
    }
    writer
        .push(pages, b"},\"schema_version\":1,\"sequences\":{")
        .await?;
    for (index, (key, value)) in current.sequences.iter().enumerate() {
        check()?;
        if index > 0 {
            writer.push(pages, b",").await?;
        }
        writer.push(pages, &canonical_bytes(key)?).await?;
        writer.push(pages, b":").await?;
        writer.push(pages, &canonical_bytes(value)?).await?;
    }
    writer.push(pages, b"},\"watermark\":").await?;
    writer
        .push(pages, &canonical_bytes(&current.watermark)?)
        .await?;
    writer.push(pages, b"}").await?;
    check()?;
    writer.finish(pages).await
}

#[cfg(test)]
#[path = "portable_snapshot_canonical_tests.rs"]
mod tests;
