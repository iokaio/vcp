// SPDX-License-Identifier: Apache-2.0
//! Bounded reads of original commits under an already validated canonical owner.
use super::*;
use vcp_domain::Watermark;
#[path = "backend_history_current.rs"]
mod current;
pub(crate) use current::CurrentCommitReader;

enum Source {
    Sqlite(SqliteConnection),
    Files {
        file: File,
        offset: u64,
        chain: String,
    },
}
pub(crate) struct CommitReader<'a> {
    source: Source,
    validated: &'a State,
    at: Watermark,
}
impl Backend {
    pub(crate) async fn history<'a>(
        &self,
        root: &Path,
        validated: &'a State,
        base: Watermark,
    ) -> Result<CommitReader<'a>> {
        let source = match self {
            Self::Sqlite(_) => {
                // The Store owner remains held. This handle never writes or
                // reconstructs another owner, and only loads one bounded row.
                let path = root.join("canonical.sqlite");
                crate::artifact::reject_link(&path)?;
                let options = SqliteConnectOptions::new()
                    .filename(path)
                    .read_only(true)
                    .create_if_missing(false)
                    .busy_timeout(Duration::from_millis(100));
                Source::Sqlite(
                    SqliteConnection::connect_with(&options)
                        .await
                        .map_err(sql_error)?,
                )
            }
            Self::Files(journal) => {
                // try_clone shares the file position on Windows. A distinct
                // handle must be bound back to the pinned writer's identity.
                let path = journal.root.join("canonical.frames");
                crate::artifact::reject_link(&path)?;
                let file = File::open(path)?;
                if crate::vault_publish::native_identity(&file)?
                    != crate::vault_publish::native_identity(&journal.file)?
                {
                    return Err(Error::Corruption("retained journal identity changed"));
                }
                Source::Files {
                    file,
                    offset: 0,
                    chain: journal.initial_chain.clone(),
                }
            }
        };
        Ok(CommitReader {
            source,
            validated,
            at: base,
        })
    }
}
impl CommitReader<'_> {
    pub(crate) async fn next(&mut self) -> Result<Option<Commit>> {
        Ok(self.next_original().await?.map(|original| original.commit))
    }
    pub(crate) async fn next_original(
        &mut self,
    ) -> Result<Option<crate::original_commits::OriginalCommit>> {
        if self.at == self.validated.watermark {
            return Ok(None);
        }
        let expected = self.at.next()?;
        let (commit, payload) = match &mut self.source {
            Source::Sqlite(db) => {
                let row = sqlx::query("SELECT id,CASE WHEN length(payload) BETWEEN 1 AND ? THEN payload ELSE NULL END AS payload,digest FROM commits WHERE watermark=?")
                    .bind(MAX_COMMIT_BYTES as i64)
                    .bind(
                        i64::try_from(expected.get())
                            .map_err(|_| Error::Limit("SQLite watermark"))?,
                    )
                    .fetch_optional(&mut *db)
                    .await?
                    .ok_or(Error::Corruption("retained commit missing"))?;
                let payload = row
                    .try_get::<Option<Vec<u8>>, _>("payload")?
                    .ok_or(Error::Corruption("retained commit length"))?;
                if payload.len() > MAX_COMMIT_BYTES
                    || digest_bytes(&payload) != row.try_get::<String, _>("digest")?
                {
                    return Err(Error::Corruption("retained commit checksum"));
                }
                let commit: Commit = serde_json::from_slice(&payload)?;
                if commit.transaction.id.as_str() != row.try_get::<String, _>("id")? {
                    return Err(Error::Corruption("retained commit identity"));
                }
                (commit, payload)
            }
            Source::Files {
                file,
                offset,
                chain,
            } => {
                let mut header = [0; HEADER];
                read_at(file, &mut header, *offset)?;
                let len = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
                let inverse = u32::from_le_bytes(header[12..16].try_into().unwrap());
                if &header[..8] != MAGIC
                    || len == 0
                    || len > MAX_COMMIT_BYTES
                    || inverse != !(len as u32)
                    || header[16..] != *chain.as_bytes()
                {
                    return Err(Error::Corruption("retained journal header or chain"));
                }
                let mut payload = vec![0; len];
                read_at(file, &mut payload, *offset + HEADER as u64)?;
                let mut trailer = [0; TRAILER];
                read_at(file, &mut trailer, *offset + HEADER as u64 + len as u64)?;
                let hash = digest_bytes(&[header.as_slice(), payload.as_slice()].concat());
                if trailer[..64] != *hash.as_bytes() || &trailer[64..] != COMMITTED {
                    return Err(Error::Corruption("retained journal bytes"));
                }
                let commit: Commit = serde_json::from_slice(&payload)?;
                *offset += (HEADER + len + TRAILER) as u64;
                *chain = hash;
                (commit, payload)
            }
        };
        if commit.version != FORMAT_VERSION
            || commit.receipt.watermark != expected
            || self.validated.transactions.get(&commit.transaction.id) != Some(&commit.receipt)
            || digest_bytes(&canonical_bytes(&commit.transaction)?) != commit.receipt.digest
        {
            return Err(Error::Corruption(
                "retained commit differs from validated owner",
            ));
        }
        self.at = expected;
        Ok(Some(crate::original_commits::OriginalCommit {
            bytes: payload,
            commit,
        }))
    }
    pub(crate) async fn close(self) -> Result<()> {
        match self.source {
            Source::Sqlite(db) => db.close().await.map_err(sql_error),
            Source::Files { .. } => Ok(()),
        }
    }
}
fn read_at(file: &File, mut bytes: &mut [u8], mut offset: u64) -> Result<()> {
    while !bytes.is_empty() {
        #[cfg(windows)]
        let read = std::os::windows::fs::FileExt::seek_read(file, bytes, offset)?;
        #[cfg(unix)]
        let read = std::os::unix::fs::FileExt::read_at(file, bytes, offset)?;
        if read == 0 {
            return Err(Error::Corruption("retained journal truncated"));
        }
        offset += read as u64;
        bytes = &mut bytes[read..];
    }
    Ok(())
}
