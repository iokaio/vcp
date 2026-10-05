// SPDX-License-Identifier: Apache-2.0
//! Verified index pages and payload bytes retained only during one preparation. Neither is
//! a persisted validation certificate nor a cache shared by owner operations.
use super::{Link, Page, Pages, Table};
use crate::{Error, Result};
use std::collections::BTreeMap;

const MAX_BYTES: usize = 2 * 1024 * 1024;
const MAX_ENTRIES: usize = 512;

#[derive(Default)]
pub(crate) struct VerifiedPages {
    entries: BTreeMap<String, (Link, Page)>,
    bytes: usize,
}
impl VerifiedPages {
    pub(super) fn get(&self, table: Table, link: &Link) -> Option<Result<Page>> {
        self.entries.get(&link.digest).map(|(verified, page)| {
            if page.table != table {
                Err(Error::Corruption("history index page domain"))
            } else if verified != link {
                Err(Error::Corruption("history index page commitment"))
            } else {
                Ok(page.clone())
            }
        })
    }
    // Only Root::load calls this after verifying the complete physical bytes,
    // canonical encoding, table, and every link commitment. Decoded allocation
    // is also bounded by the page schema's fanout/key/value limits and entry cap.
    pub(super) fn insert(&mut self, link: Link, page: Page, bytes: usize) {
        if self.entries.contains_key(&link.digest)
            || self.entries.len() >= MAX_ENTRIES
            || bytes > MAX_BYTES.saturating_sub(self.bytes)
        {
            return;
        }
        self.bytes += bytes;
        self.entries.insert(link.digest.clone(), (link, page));
    }
}

pub(crate) struct ReadSession<'a, P> {
    inner: &'a mut P,
    verified: VerifiedPages,
    chunks: BTreeMap<String, Vec<u8>>,
    chunk_bytes: usize,
}
impl<'a, P: Pages> ReadSession<'a, P> {
    pub(crate) fn new(inner: &'a mut P) -> Self {
        Self {
            inner,
            verified: VerifiedPages::default(),
            chunks: BTreeMap::new(),
            chunk_bytes: 0,
        }
    }
}
impl<P: Pages> Pages for ReadSession<'_, P> {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        // Index pages already have a decoded, commitment-checked memo. Only
        // small physical payload reads use this byte memo. It supplies bytes,
        // never a semantic validation certificate: callers still check length,
        // whole-blob digest, canonical encoding and every record relationship.
        let eligible = limit > 0 && limit <= crate::history_blob::CHUNK_BYTES;
        if eligible {
            if let Some(bytes) = self.chunks.get(digest).filter(|bytes| bytes.len() <= limit) {
                return Ok(bytes.clone());
            }
        }
        let bytes = self.inner.read(digest, limit).await?;
        if eligible
            && !bytes.is_empty()
            && bytes.len() <= limit
            && vcp_protocol::digest_bytes(&bytes) == digest
            && !self.chunks.contains_key(digest)
            && self.chunks.len() < MAX_ENTRIES
            && bytes.len() <= MAX_BYTES.saturating_sub(self.chunk_bytes)
        {
            self.chunk_bytes += bytes.len();
            self.chunks.insert(digest.to_owned(), bytes.clone());
        }
        Ok(bytes)
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        // Preparation is read-only. Keep this adapter safe for other callers:
        // even a failed/cancelled write discards previous physical observations.
        self.verified = VerifiedPages::default();
        self.chunks.clear();
        self.chunk_bytes = 0;
        self.inner.write(digest, bytes).await
    }
    fn verified_pages(&mut self) -> Option<&mut VerifiedPages> {
        Some(&mut self.verified)
    }
}

#[cfg(test)]
#[path = "history_index_memo_tests.rs"]
mod tests;
