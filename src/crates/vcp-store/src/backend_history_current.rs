// SPDX-License-Identifier: Apache-2.0
//! Exact native commit reads bound to the jointly admitted original-byte root.
//! This preserves explicit prefix/snapshot/conversion corruption checks after
//! receipt maps leave resident State; it never reconstructs a second Store.
use super::*;
use crate::{
    canonical_lock::CanonicalLock,
    durable_owner::DurableOwner,
    journal_frame::{self, ReadFrame},
    store_history_reader::ReadPages,
};

pub(crate) struct CurrentCommitReader<'a> {
    source: Source,
    owner: &'a DurableOwner,
    pages: ReadPages,
    at: Watermark,
    final_chain: Option<String>,
    failed: bool,
}
impl Backend {
    /// Caller supplies its actual jointly constructed backend/owner/lock. The
    /// borrow excludes mutation/close until this bounded cursor is released.
    pub(crate) async fn history_current<'a>(
        &self,
        lock: &CanonicalLock,
        owner: &'a DurableOwner,
    ) -> Result<CurrentCommitReader<'a>> {
        let (source, kind, final_chain) = match self {
            Self::Sqlite(_) => {
                let path = lock.root().join("canonical.sqlite");
                crate::artifact::reject_link(&path)?;
                let db = SqliteConnection::connect_with(
                    &SqliteConnectOptions::new()
                        .filename(path)
                        .read_only(true)
                        .create_if_missing(false)
                        .busy_timeout(Duration::from_millis(100)),
                )
                .await?;
                (Source::Sqlite(db), BackendKind::Sqlite, None)
            }
            Self::Files(journal) => {
                let path = lock.root().join("canonical.frames");
                crate::artifact::reject_link(&path)?;
                let file = File::open(path)?;
                if crate::vault_publish::native_identity(&file)?
                    != crate::vault_publish::native_identity(&journal.file)?
                {
                    return Err(Error::Corruption("retained journal identity changed"));
                }
                (
                    Source::Files {
                        file,
                        offset: 0,
                        chain: journal.initial_chain.clone(),
                    },
                    BackendKind::Files,
                    Some(journal.chain.clone()),
                )
            }
        };
        Ok(CurrentCommitReader {
            source,
            owner,
            pages: ReadPages::from_locked(lock, kind)?,
            at: owner.originals().base_watermark(),
            final_chain,
            failed: false,
        })
    }
}
impl CurrentCommitReader<'_> {
    pub(crate) async fn next_original(
        &mut self,
    ) -> Result<Option<crate::original_commits::OriginalCommit>> {
        if self.failed {
            return Err(Error::Unavailable("retained history reader failed"));
        }
        self.failed = true;
        if self.at == self.owner.originals().watermark() {
            if let Source::Files { chain, .. } = &self.source {
                if self.final_chain.as_ref() != Some(chain) {
                    return Err(Error::Corruption("retained journal head changed"));
                }
            }
            self.failed = false;
            return Ok(None);
        }
        let expected = self.at.next()?;
        let original = self
            .owner
            .originals()
            .get(&mut self.pages, expected)
            .await?
            .ok_or(Error::Corruption("retained original missing"))?;
        let payload = match &mut self.source {
            Source::Sqlite(db) => {
                let row=sqlx::query("SELECT id,CASE WHEN typeof(payload)='blob' AND length(payload) BETWEEN 1 AND ? THEN payload ELSE NULL END AS payload,digest FROM commits WHERE watermark=?")
                    .bind(MAX_COMMIT_BYTES as i64).bind(i64::try_from(expected.get()).map_err(|_|Error::Limit("SQLite watermark"))?)
                    .fetch_optional(db).await?.ok_or(Error::Corruption("retained commit missing"))?;
                let payload = row
                    .try_get::<Option<Vec<u8>>, _>("payload")?
                    .ok_or(Error::Corruption("retained commit length"))?;
                if digest_bytes(&payload) != row.try_get::<String, _>("digest")?
                    || original.commit.transaction.id.as_str() != row.try_get::<String, _>("id")?
                {
                    return Err(Error::Corruption("retained commit checksum or identity"));
                }
                payload
            }
            Source::Files {
                file,
                offset,
                chain,
            } => {
                let ReadFrame::Complete(frame) = journal_frame::read(file, *offset, chain)? else {
                    return Err(Error::Corruption("retained journal truncated"));
                };
                *offset = frame.end;
                *chain = frame.chain;
                frame.payload
            }
        };
        if payload != original.bytes {
            return Err(Error::Corruption(
                "retained native bytes differ from validated owner",
            ));
        }
        self.at = expected;
        self.failed = false;
        Ok(Some(original))
    }
    pub(crate) async fn close(self) -> Result<()> {
        let source = match self.source {
            Source::Sqlite(db) => db.close().await.map_err(Error::from),
            Source::Files { .. } => Ok(()),
        };
        let pages = self.pages.close().await;
        source.and(pages)
    }
}
