// SPDX-License-Identifier: Apache-2.0
//! Bounded chunk stream for layout-3 rows and neutral-history/2 payloads.
//! A descriptor must be bound to an admitted owner/archive root. Hash agreement
//! alone grants neither semantic trust nor execution authority. Consumers stage
//! partial bytes privately and accept them only after Reader returns None.
use crate::{
    history_index::{Entry, Pages, Root, Table},
    Error, Result,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use vcp_protocol::digest_bytes;

pub(crate) const CHUNK_BYTES: usize = 64 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Blob {
    version: u32,
    pub(crate) bytes: u64,
    pub(crate) sha256: String,
    index: Root,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Chunk {
    bytes: u64,
    sha256: String,
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
impl Blob {
    pub(crate) fn validate(&self) -> Result<()> {
        self.index.validate_table(Table::ArchiveChunks)?;
        let count =
            self.bytes / CHUNK_BYTES as u64 + u64::from(self.bytes % CHUNK_BYTES as u64 != 0);
        if self.version != 2 || !hash(&self.sha256) || self.index.count() != count {
            return Err(Error::Corruption("history blob descriptor"));
        }
        Ok(())
    }
}

pub(crate) struct Writer {
    index: Root,
    pending: Vec<u8>,
    bytes: u64,
    chunks: u64,
    digest: Sha256,
    failed: bool,
}
impl Writer {
    pub(crate) fn new() -> Self {
        Self {
            index: Root::empty(Table::ArchiveChunks),
            pending: Vec::with_capacity(CHUNK_BYTES),
            bytes: 0,
            chunks: 0,
            digest: Sha256::new(),
            failed: false,
        }
    }
    /// Input may be large, but only one chunk is retained by the writer.
    pub(crate) async fn push(&mut self, pages: &mut impl Pages, mut bytes: &[u8]) -> Result<()> {
        if self.failed {
            return Err(Error::Unavailable("history blob writer failed"));
        }
        // A cancelled future must not permit finishing a silently short input.
        self.failed = true;
        let result = async {
            while !bytes.is_empty() {
                let take = bytes.len().min(CHUNK_BYTES - self.pending.len());
                self.bytes = self
                    .bytes
                    .checked_add(take as u64)
                    .ok_or(Error::Limit("history blob length"))?;
                self.digest.update(&bytes[..take]);
                self.pending.extend_from_slice(&bytes[..take]);
                bytes = &bytes[take..];
                if self.pending.len() == CHUNK_BYTES {
                    self.flush(pages).await?;
                }
            }
            Ok(())
        }
        .await;
        if result.is_ok() {
            self.failed = false;
        }
        result
    }
    async fn flush(&mut self, pages: &mut impl Pages) -> Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let chunk = Chunk {
            bytes: self.pending.len() as u64,
            sha256: digest_bytes(&self.pending),
        };
        pages.write(&chunk.sha256, &self.pending).await?;
        self.index = self
            .index
            .insert(
                pages,
                Entry {
                    key: format!("{:016x}", self.chunks),
                    value: serde_json::to_value(chunk)?,
                },
            )
            .await?;
        self.chunks = self
            .chunks
            .checked_add(1)
            .ok_or(Error::Limit("history blob chunks"))?;
        self.pending.clear();
        Ok(())
    }
    pub(crate) async fn finish(mut self, pages: &mut impl Pages) -> Result<Blob> {
        if self.failed {
            return Err(Error::Unavailable("history blob writer failed"));
        }
        self.flush(pages).await?;
        let blob = Blob {
            version: 2,
            bytes: self.bytes,
            sha256: format!("{:x}", self.digest.finalize()),
            index: self.index,
        };
        blob.validate()?;
        Ok(blob)
    }
}

pub(crate) struct Reader {
    blob: Blob,
    offset: u64,
    chunk: u64,
    digest: Sha256,
    failed: bool,
    complete: bool,
}
impl Reader {
    pub(crate) fn new(blob: &Blob) -> Result<Self> {
        blob.validate()?;
        Ok(Self {
            blob: blob.clone(),
            offset: 0,
            chunk: 0,
            digest: Sha256::new(),
            failed: false,
            complete: false,
        })
    }
    /// None is the verified end of the entire stream. An early drop, consumer
    /// error or read failure is not a validated blob; errors poison this reader.
    pub(crate) async fn next(&mut self, pages: &mut impl Pages) -> Result<Option<Vec<u8>>> {
        if self.failed {
            return Err(Error::Unavailable("history blob reader failed"));
        }
        if self.complete {
            return Ok(None);
        }
        self.failed = true;
        let result = async {
            if self.offset == self.blob.bytes {
                if format!("{:x}", self.digest.clone().finalize()) != self.blob.sha256 {
                    return Err(Error::Corruption("history blob whole digest"));
                }
                self.complete = true;
                return Ok(None);
            }
            let key = format!("{:016x}", self.chunk);
            let entry = self
                .blob
                .index
                .get(pages, &key)
                .await?
                .ok_or(Error::Corruption("history blob chunk missing"))?;
            let chunk: Chunk = serde_json::from_value(entry.value)?;
            let expected = (self.blob.bytes - self.offset).min(CHUNK_BYTES as u64);
            if chunk.bytes != expected || !hash(&chunk.sha256) {
                return Err(Error::Corruption("history blob chunk descriptor"));
            }
            let bytes = pages.read(&chunk.sha256, CHUNK_BYTES).await?;
            if bytes.len() as u64 != expected || digest_bytes(&bytes) != chunk.sha256 {
                return Err(Error::Corruption("history blob chunk content"));
            }
            self.digest.update(&bytes);
            self.offset += expected;
            self.chunk += 1;
            Ok(Some(bytes))
        }
        .await;
        if result.is_ok() {
            self.failed = false;
        }
        result
    }
}

pub(crate) async fn write_bytes(pages: &mut impl Pages, bytes: &[u8]) -> Result<Blob> {
    let mut writer = Writer::new();
    writer.push(pages, bytes).await?;
    writer.finish(pages).await
}
/// Row callers name their existing per-object bound before materialization.
/// Streaming archive callers use Reader instead of raising this bound.
pub(crate) async fn read_bounded(
    pages: &mut impl Pages,
    blob: &Blob,
    limit: usize,
) -> Result<Vec<u8>> {
    if blob.bytes > limit as u64 {
        return Err(Error::Limit("history blob materialization"));
    }
    let mut reader = Reader::new(blob)?;
    let mut bytes = Vec::new();
    while let Some(chunk) = reader.next(pages).await? {
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
#[path = "history_blob_tests.rs"]
mod tests;
