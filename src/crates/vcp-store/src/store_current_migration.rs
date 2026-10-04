// SPDX-License-Identifier: Apache-2.0
//! Stage the native migration using the existing Store's real ownership and
//! path capabilities. No format marker changes until live API retirement.
use super::*;
use crate::{
    backend::current_publication::{self, Origin},
    durable_owner::DurableOwner,
    history_index::io,
    private_paths::Directory,
};
use sqlx::Connection;

/// The mutable borrow prevents commits, close, rewrite or cleanup through the
/// source while staging/validation is in progress. Dropping it leaves the old
/// format fully usable; immutable staged objects confer no execution authority.
pub(crate) struct StagedCurrent<'a> {
    source: &'a mut Store,
    origin: Origin,
    origin_digest: String,
    owner: Arc<DurableOwner>,
    directory: Option<Directory>,
    _root_pin: File,
}
impl Store {
    pub(crate) async fn stage_current(&mut self) -> Result<StagedCurrent<'_>> {
        if self.poisoned {
            return Err(Error::Unavailable("canonical state uncertain"));
        }
        let root_pin = snapshot_pin::acquire(&self.root)?;
        let directory = if self.kind == BackendKind::Files {
            Some(Directory::canonical_child(self, "history-pages")?)
        } else {
            None
        };
        let base = crate::replay_base::ReplayBase::load(&self.root)?;
        if base
            .as_ref()
            .map(|base| &base.state)
            .unwrap_or(&State::default())
            != &self.base
        {
            return Err(Error::Corruption("migration replay base changed"));
        }
        let mut reader = self
            .backend
            .history(&self.root, &self.state, self.base.watermark)
            .await?;
        let (origin, origin_digest, owner) = match &mut self.backend {
            Backend::Files(_) => {
                let directory = directory
                    .as_ref()
                    .ok_or(Error::Corruption("migration page directory"))?;
                Origin::stage(
                    &mut io::Files::new(directory),
                    &self.state,
                    base.as_ref(),
                    &mut reader,
                )
                .await?
            }
            Backend::Sqlite(db) => {
                // Pages, auxiliary schema and the origin descriptor are staged
                // atomically. No live canonical row or format marker changes.
                let mut tx = db.begin_with("BEGIN IMMEDIATE").await?;
                current_publication::initialize_sqlite(&mut tx).await?;
                let staged = Origin::stage(
                    &mut io::Sqlite::new(&mut tx),
                    &self.state,
                    base.as_ref(),
                    &mut reader,
                )
                .await?;
                tx.commit().await?;
                staged
            }
        };
        reader.close().await?;
        let mut staged = StagedCurrent {
            source: self,
            origin,
            origin_digest,
            owner: Arc::new(owner),
            directory,
            _root_pin: root_pin,
        };
        staged.verify().await?;
        Ok(staged)
    }
}
impl StagedCurrent<'_> {
    pub(crate) fn owner(&self) -> &DurableOwner {
        &self.owner
    }
    pub(crate) fn origin_digest(&self) -> &str {
        &self.origin_digest
    }
    pub(crate) async fn verify(&mut self) -> Result<()> {
        if self.source.poisoned {
            return Err(Error::Unavailable(
                "reopen after interrupted migration verification",
            ));
        }
        // Full replay temporarily moves the actual journal writer's cursor.
        // An error or cancelled future cannot release a still-usable writer at
        // an interior offset. Reopen is required unless replay finishes fully.
        self.source.poisoned = true;
        let result = self.verify_inner().await;
        if result.is_ok() {
            self.source.poisoned = false;
        }
        result
    }
    async fn verify_inner(&mut self) -> Result<()> {
        let base = crate::replay_base::ReplayBase::load(&self.source.root)?;
        let mut diagnostics = crate::StoreDiagnostics::new(self.source.kind);
        let replayed = match &mut self.source.backend {
            Backend::Files(journal) => {
                let directory = self
                    .directory
                    .as_ref()
                    .ok_or(Error::Corruption("migration page directory"))?;
                let mut pages = io::Files::new(directory);
                let loaded = Origin::load(&mut pages, &self.origin_digest).await?;
                if loaded.watermark() != self.origin.watermark() {
                    return Err(Error::Corruption("migration origin cut changed"));
                }
                journal
                    .replay_current_all(&mut pages, &loaded, base.as_ref(), &mut diagnostics)
                    .await?
            }
            Backend::Sqlite(db) => {
                let loaded = Origin::load(&mut io::Sqlite::new(db), &self.origin_digest).await?;
                if loaded.watermark() != self.origin.watermark() {
                    return Err(Error::Corruption("migration origin cut changed"));
                }
                current_publication::replay_sqlite_all(db, &loaded, base.as_ref(), &mut diagnostics)
                    .await?
            }
        };
        if replayed.identity() != self.owner.identity() {
            return Err(Error::Corruption("migration replay owner changed"));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "store_current_migration_tests.rs"]
mod tests;
