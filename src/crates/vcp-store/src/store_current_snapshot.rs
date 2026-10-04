// SPDX-License-Identifier: Apache-2.0
//! A real native generation/artifact pin, paired with owner-admitted roots.
//! There is no constructor from a decoded descriptor or same-watermark DTO.
use super::*;
use crate::{history_index::Pages, portable_snapshot::complete::Archive, snapshot_inputs::Inputs};
use sqlx::{sqlite::SqliteConnectOptions, SqliteConnection};

pub(crate) struct PinnedDurableSnapshot {
    owner: Arc<DurableOwner>,
    pages: ReadPages,
    spool: Spool,
    forbidden: Vec<PathBuf>,
    _artifacts: Vec<ArtifactPin>,
    _root_pin: File,
}
enum ReadPages {
    Files(Directory),
    Sqlite {
        db: SqliteConnection,
        _file: File,
        _root: Directory,
    },
}
impl Pages for ReadPages {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        match self {
            Self::Files(directory) => io::Files::new(directory).read(digest, limit).await,
            Self::Sqlite { db, .. } => io::Sqlite::new(db).read(digest, limit).await,
        }
    }
    async fn write(&mut self, _: &str, _: &[u8]) -> Result<()> {
        Err(Error::Access)
    }
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
        let pages = match self.source.kind {
            BackendKind::Files => {
                ReadPages::Files(Directory::canonical_child(self.source, "history-pages")?)
            }
            BackendKind::Sqlite => {
                let root = Directory::canonical_root(self.source)?;
                let path = root.path.join("canonical.sqlite");
                reject_link(&path)?;
                let mut options = OpenOptions::new();
                options.read(true);
                #[cfg(windows)]
                {
                    use std::os::windows::fs::OpenOptionsExt;
                    options.custom_flags(0x0020_0000).share_mode(1 | 2);
                }
                let file = options.open(&path)?;
                if !crate::private_paths::allowed_handle(&file, false)? {
                    return Err(Error::Access);
                }
                let db = SqliteConnection::connect_with(
                    &SqliteConnectOptions::new()
                        .filename(path)
                        .read_only(true)
                        .create_if_missing(false)
                        .busy_timeout(std::time::Duration::from_millis(100)),
                )
                .await?;
                ReadPages::Sqlite {
                    db,
                    _file: file,
                    _root: root,
                }
            }
        };
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
    pub(crate) fn current(&self) -> crate::CurrentStateView<'_> {
        self.owner.semantic().current().into()
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
        let result = match pages {
            ReadPages::Files(directory) => {
                drop(directory);
                Ok(())
            }
            ReadPages::Sqlite { db, _file, _root } => {
                let result = db.close().await.map_err(Error::from);
                drop(_file);
                drop(_root);
                result
            }
        };
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
