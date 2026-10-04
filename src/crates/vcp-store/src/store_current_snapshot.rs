// SPDX-License-Identifier: Apache-2.0
//! A real native generation/artifact pin, paired with owner-admitted roots.
//! There is no constructor from a decoded descriptor or same-watermark DTO.
use super::*;
use crate::store_history_reader::ReadPages;
use crate::{history_index::Pages, portable_snapshot::complete::Archive, snapshot_inputs::Inputs};

pub(crate) struct PinnedDurableSnapshot {
    owner: Arc<DurableOwner>,
    pages: ReadPages,
    spool: Spool,
    forbidden: Vec<PathBuf>,
    _artifacts: Vec<ArtifactPin>,
    _root_pin: File,
}
impl StagedCurrent<'_> {
    pub(crate) async fn snapshot(&self) -> Result<PinnedDurableSnapshot> {
        if self.source.poisoned {
            return Err(Error::Unavailable("canonical state uncertain"));
        }
        let root_pin = snapshot_pin::acquire(&self.source.root)?;
        let mut artifacts = Vec::new();
        for record in self
            .owner
            .semantic()
            .current()
            .records
            .values()
            .filter(|record| record.collection == Collection::Artifact)
        {
            let descriptor: ArtifactDescriptor = record.decode()?;
            if descriptor.state != vcp_domain::artifact::CaptureState::Purged {
                artifacts.push(self.source.spool.pin(&descriptor.spec.id)?);
            }
        }
        let pages = ReadPages::from_locked(self.source.canonical_lock(), self.source.kind)?;
        Ok(PinnedDurableSnapshot {
            owner: Arc::clone(&self.owner),
            pages,
            spool: self.source.spool.clone(),
            forbidden: self.source.forbidden_roots.clone(),
            _artifacts: artifacts,
            _root_pin: root_pin,
        })
    }
}
impl PinnedDurableSnapshot {
    /// Capture only an actual held native owner, never independently supplied
    /// history/current descriptors that happen to name the same watermark.
    pub(crate) fn from_opened(
        source: &crate::backend::current_publication::open::Opened,
    ) -> Result<Self> {
        source.ensure_healthy()?;
        let lock = source.canonical_lock();
        let root_pin = snapshot_pin::acquire(lock.root())?;
        let mut artifacts = Vec::new();
        for record in source
            .current()
            .records
            .values()
            .filter(|record| record.collection == Collection::Artifact)
        {
            let descriptor: ArtifactDescriptor = record.decode()?;
            if descriptor.state != vcp_domain::artifact::CaptureState::Purged {
                artifacts.push(source.spool.pin(&descriptor.spec.id)?);
            }
        }
        Ok(Self {
            owner: Arc::clone(&source.owner),
            pages: ReadPages::from_locked(lock, source.kind())?,
            spool: source.spool.clone(),
            forbidden: lock.forbidden().to_vec(),
            _artifacts: artifacts,
            _root_pin: root_pin,
        })
    }
    pub(crate) fn current(&self) -> crate::CurrentStateView<'_> {
        self.owner.semantic().current().into()
    }
    pub(crate) async fn archive_state(&mut self) -> Result<State> {
        self.owner.archive_state(&mut self.pages).await
    }
    pub(crate) async fn logical_digest(&mut self) -> Result<String> {
        self.owner
            .semantic()
            .catalog()
            .legacy_digest(&mut self.pages, self.owner.semantic().current().into())
            .await
    }
    pub(crate) async fn capture(
        &mut self,
        destination: &mut impl Pages,
        workspace: &WorkspaceId,
        inputs: &Inputs,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Archive> {
        Archive::capture(
            &self.owner,
            &mut self.pages,
            destination,
            workspace,
            &self.spool,
            &self.forbidden,
            inputs,
            check,
        )
        .await
    }
    /// Finish the reader before releasing its native generation/artifact pins.
    pub(crate) async fn close(self) -> Result<()> {
        let Self {
            pages,
            owner,
            spool,
            forbidden,
            _artifacts,
            _root_pin,
        } = self;
        let result = pages.close().await;
        drop(owner);
        drop(spool);
        drop(forbidden);
        drop(_artifacts);
        drop(_root_pin);
        result
    }
}

#[cfg(test)]
#[path = "store_current_snapshot_tests.rs"]
mod tests;
