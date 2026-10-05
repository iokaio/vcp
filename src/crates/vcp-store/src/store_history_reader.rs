// SPDX-License-Identifier: Apache-2.0
//! A pinned physical generation reader. Roots remain supplied by the admitted
//! owner; this capability alone grants no semantic trust or write authority.
use crate::{
    artifact::reject_link,
    canonical_lock::CanonicalLock,
    history_index::{io, Pages},
    private_paths::Directory,
    BackendKind, Error, Result,
};
use sqlx::{sqlite::SqliteConnectOptions, Connection, SqliteConnection};
use std::{
    fs::{File, OpenOptions},
    path::PathBuf,
};

pub(crate) enum ReadPages {
    Files(Directory),
    Sqlite {
        db: Option<SqliteConnection>,
        path: PathBuf,
        _file: File,
        _root: Directory,
    },
}
impl ReadPages {
    /// Pin identity synchronously. SQLite's read-only connection is opened on
    /// first history access, so a current-only snapshot never opens another DB.
    pub(crate) fn from_locked(owner: &CanonicalLock, kind: BackendKind) -> Result<Self> {
        match kind {
            BackendKind::Files => {
                reject_link(&owner.root().join("history-pages"))?;
                Ok(Self::Files(Directory::locked_child(
                    owner,
                    "history-pages",
                )?))
            }
            BackendKind::Sqlite => {
                let root = Directory::locked_root(owner)?;
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
                Ok(Self::Sqlite {
                    db: None,
                    path,
                    _file: file,
                    _root: root,
                })
            }
        }
    }
    pub(crate) async fn close(self) -> Result<()> {
        match self {
            Self::Files(directory) => {
                drop(directory);
                Ok(())
            }
            Self::Sqlite {
                db,
                path,
                _file,
                _root,
            } => {
                let result = match db {
                    Some(db) => db.close().await.map_err(Error::from),
                    None => Ok(()),
                };
                drop((path, _file, _root));
                result
            }
        }
    }
    #[cfg(test)]
    pub(crate) fn connected(&self) -> bool {
        matches!(self, Self::Sqlite { db: Some(_), .. })
    }
}
impl Pages for ReadPages {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        match self {
            Self::Files(directory) => io::Files::new(directory).read(digest, limit).await,
            Self::Sqlite { db, path, .. } => {
                if db.is_none() {
                    let connection = SqliteConnection::connect_with(
                        &SqliteConnectOptions::new()
                            .filename(&*path)
                            .read_only(true)
                            .create_if_missing(false)
                            .busy_timeout(std::time::Duration::from_millis(100)),
                    )
                    .await?;
                    *db = Some(connection);
                }
                let connection = db
                    .as_mut()
                    .ok_or(Error::Unavailable("history reader connection"))?;
                io::Sqlite::new(connection).read(digest, limit).await
            }
        }
    }
    async fn write(&mut self, _: &str, _: &[u8]) -> Result<()> {
        Err(Error::Access)
    }
}
