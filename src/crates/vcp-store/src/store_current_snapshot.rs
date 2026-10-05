// SPDX-License-Identifier: Apache-2.0
//! A real native generation/artifact pin, paired with owner-admitted roots.
//! There is no constructor from a decoded descriptor or same-watermark DTO.
use super::*;
use crate::store_history_reader::ReadPages;
use crate::{history_index::Pages, portable_snapshot::complete::Archive, snapshot_inputs::Inputs};

pub(crate) struct PinnedDurableSnapshot {
    owner: Arc<DurableOwner>,
    legacy: Option<(crate::CurrentState, u64, String)>,
    pages: tokio::sync::Mutex<ReadPages>,
    spool: Spool,
    spool_directory: crate::private_paths::Directory,
    forbidden: Vec<PathBuf>,
    _artifacts: Vec<ArtifactPin>,
    _root_pin: File,
}
#[cfg(test)]
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
            legacy: None,
            pages: tokio::sync::Mutex::new(pages),
            spool: self.source.spool.clone(),
            spool_directory: crate::private_paths::Directory::locked_root(
                self.source.canonical_lock(),
            )?
            .existing_child("spool")?,
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
        Self::from_parts(source, Arc::clone(&source.owner), None)
    }
    pub(crate) async fn at(
        source: &crate::backend::current_publication::open::Opened,
        watermark: Watermark,
    ) -> Result<Self> {
        use crate::backend::current_publication::open::prefix::ReplayedCut;
        match source.reconstruct(watermark).await? {
            ReplayedCut::Current(owner) => Self::from_parts(source, Arc::new(owner), None),
            ReplayedCut::Legacy {
                current,
                events,
                digest,
            } => Self::from_parts(
                source,
                Arc::clone(&source.owner),
                Some((current, events, digest)),
            ),
        }
    }
    fn from_parts(
        source: &crate::backend::current_publication::open::Opened,
        owner: Arc<DurableOwner>,
        legacy: Option<(crate::CurrentState, u64, String)>,
    ) -> Result<Self> {
        source.ensure_healthy()?;
        let lock = source.canonical_lock();
        let root_pin = snapshot_pin::acquire(lock.root())?;
        let mut artifacts = Vec::new();
        let current = legacy
            .as_ref()
            .map(|(current, _, _)| current)
            .unwrap_or_else(|| owner.semantic().current());
        for record in current
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
            owner,
            legacy,
            pages: tokio::sync::Mutex::new(ReadPages::from_locked(lock, source.kind())?),
            spool: source.spool.clone(),
            spool_directory: crate::private_paths::Directory::locked_root(lock)?
                .existing_child("spool")?,
            forbidden: lock.forbidden().to_vec(),
            _artifacts: artifacts,
            _root_pin: root_pin,
        })
    }
    pub(crate) fn current(&self) -> crate::CurrentStateView<'_> {
        self.legacy
            .as_ref()
            .map(|(current, _, _)| current)
            .unwrap_or_else(|| self.owner.semantic().current())
            .into()
    }
    pub(crate) async fn history_event_count(&self) -> Result<u64> {
        Ok(self
            .legacy
            .as_ref()
            .map(|(_, events, _)| *events)
            .unwrap_or_else(|| self.owner.semantic().catalog().event_count()))
    }
    pub(crate) async fn history_event_at(&self, ordinal: u64) -> Result<Option<EventEnvelope>> {
        if ordinal >= self.history_event_count().await? {
            return Ok(None);
        }
        self.owner
            .semantic()
            .catalog()
            .event_at(&mut *self.pages.lock().await, ordinal)
            .await
    }
    pub(crate) async fn history_event(&self, id: &EventId) -> Result<Option<EventEnvelope>> {
        Ok(self
            .owner
            .semantic()
            .catalog()
            .event(&mut *self.pages.lock().await, id)
            .await?
            .filter(|row| row.watermark <= self.current().watermark))
    }
    pub(crate) async fn command_receipt_by_id(
        &self,
        workspace: &WorkspaceId,
        command: &CommandId,
    ) -> Result<Option<CommandReceipt>> {
        Ok(self
            .owner
            .semantic()
            .catalog()
            .command_unchecked_meaning(&mut *self.pages.lock().await, workspace, command)
            .await?
            .filter(|row| row.watermark <= self.current().watermark))
    }
    pub(crate) async fn history_events(
        &self,
        after: Option<u64>,
        limit: usize,
    ) -> Result<Vec<EventEnvelope>> {
        if limit == 0 || limit > 4096 {
            return Err(Error::Limit("history page"));
        }
        let first = after
            .map_or(Some(0), |at| at.checked_add(1))
            .ok_or(Error::Limit("history ordinal"))?;
        let end = self.history_event_count().await?;
        if first > end {
            return Err(Error::Conflict("history ordinal ahead of owner"));
        }
        if first == end {
            return Ok(Vec::new());
        }
        let limit = limit.min(usize::try_from(end - first).unwrap_or(usize::MAX));
        self.owner
            .semantic()
            .catalog()
            .event_page(&mut *self.pages.lock().await, after, limit)
            .await
    }
    pub(crate) async fn archive_state(&self) -> Result<State> {
        let mut pages = self.pages.lock().await;
        if let Some((current, events, digest)) = &self.legacy {
            let bytes = self
                .owner
                .semantic()
                .catalog()
                .legacy_prefix_bytes(&mut *pages, current.into(), *events)
                .await?;
            if digest_bytes(&bytes) != *digest {
                return Err(Error::Corruption("historical archival prefix differs"));
            }
            let state: State = serde_json::from_slice(&bytes)?;
            state.validate()?;
            Ok(state)
        } else {
            self.owner.archive_state(&mut *pages).await
        }
    }
    pub(crate) async fn verify_workspace(&self, workspace: &WorkspaceId) -> Result<()> {
        if self
            .current()
            .records
            .values()
            .any(|row| &row.workspace != workspace)
        {
            return Err(Error::Access);
        }
        let end = self.history_event_count().await?;
        let mut at = 0;
        while at < end {
            let rows = self.history_events(at.checked_sub(1), 64).await?;
            if rows.is_empty() {
                return Err(Error::Corruption("snapshot event extent"));
            }
            for row in rows {
                if &row.event.workspace != workspace {
                    return Err(Error::Access);
                }
                at += 1;
            }
        }
        let mut pages = self.pages.lock().await;
        let mut after = None;
        loop {
            let rows = self
                .owner
                .semantic()
                .catalog()
                .command_page(&mut *pages, after.as_deref(), 64)
                .await?;
            if rows.is_empty() {
                break;
            }
            for (key, row) in rows {
                if row.watermark <= self.current().watermark && &row.workspace != workspace {
                    return Err(Error::Access);
                }
                after = Some(key);
            }
        }
        Ok(())
    }
    pub(crate) async fn logical_digest(&self) -> Result<String> {
        if let Some((_, _, digest)) = &self.legacy {
            return Ok(digest.clone());
        }
        self.owner
            .semantic()
            .catalog()
            .legacy_digest(
                &mut *self.pages.lock().await,
                self.owner.semantic().current().into(),
            )
            .await
    }
    pub(crate) async fn capture(
        &mut self,
        destination: &mut impl Pages,
        workspace: &WorkspaceId,
        inputs: &Inputs,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Archive> {
        if self.legacy.is_some() {
            return Err(Error::Unavailable(
                "streaming archive predates layout origin",
            ));
        }
        Archive::capture_admitted(
            &self.owner,
            self.pages.get_mut(),
            destination,
            workspace,
            &self.spool,
            &self.spool_directory,
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
            legacy,
            spool,
            spool_directory,
            forbidden,
            _artifacts,
            _root_pin,
        } = self;
        let result = pages.into_inner().close().await;
        drop(owner);
        drop(legacy);
        drop(spool);
        drop(spool_directory);
        drop(forbidden);
        drop(_artifacts);
        drop(_root_pin);
        result
    }
}

#[cfg(test)]
#[path = "store_current_snapshot_tests.rs"]
mod tests;
