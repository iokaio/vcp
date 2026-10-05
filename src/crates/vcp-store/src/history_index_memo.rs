// SPDX-License-Identifier: Apache-2.0
//! Verified index pages and payload bytes retained only during one preparation. Neither is
//! a persisted validation certificate nor a cache shared by owner operations.
use super::{Link, Page, Pages, Table};
use crate::{Error, HistoryReads, Result};
use std::collections::BTreeMap;

const MAX_BYTES: usize = 2 * 1024 * 1024;
const MAX_ENTRIES: usize = 512;

#[derive(Default)]
pub(crate) struct VerifiedPages {
    entries: BTreeMap<String, (Link, Page)>,
    bytes: usize,
    observations: HistoryReads,
}
impl VerifiedPages {
    pub(super) fn get(&mut self, table: Table, link: &Link) -> Option<Result<Page>> {
        let found = self.entries.get(&link.digest);
        if found.is_some() {
            self.observations.index_hits = self.observations.index_hits.saturating_add(1);
        } else {
            self.observations.index_misses = self.observations.index_misses.saturating_add(1);
        }
        found.map(|(verified, page)| {
            if page.table != table {
                self.observations.index_rejected_hits =
                    self.observations.index_rejected_hits.saturating_add(1);
                Err(Error::Corruption("history index page domain"))
            } else if verified != link {
                self.observations.index_rejected_hits =
                    self.observations.index_rejected_hits.saturating_add(1);
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
        if self.entries.contains_key(&link.digest) {
            return;
        }
        if self.entries.len() >= MAX_ENTRIES {
            self.observations.index_refused_entries =
                self.observations.index_refused_entries.saturating_add(1);
            return;
        }
        if bytes > MAX_BYTES.saturating_sub(self.bytes) {
            self.observations.index_refused_bytes =
                self.observations.index_refused_bytes.saturating_add(1);
            return;
        }
        self.bytes += bytes;
        self.observations.index_admitted = self.observations.index_admitted.saturating_add(1);
        self.entries.insert(link.digest.clone(), (link, page));
    }
}

pub(crate) struct ReadSession<'a, P> {
    inner: &'a mut P,
    verified: VerifiedPages,
    chunks: BTreeMap<String, Vec<u8>>,
    chunk_bytes: usize,
    observations: HistoryReads,
}
impl<'a, P: Pages> ReadSession<'a, P> {
    pub(crate) fn new(inner: &'a mut P) -> Self {
        Self {
            inner,
            verified: VerifiedPages::default(),
            chunks: BTreeMap::new(),
            chunk_bytes: 0,
            observations: HistoryReads::default(),
        }
    }
    pub(crate) fn observations(&self) -> HistoryReads {
        let mut observations = self.observations;
        observations.add(self.verified.observations);
        observations
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
                self.observations.payload_hits = self.observations.payload_hits.saturating_add(1);
                return Ok(bytes.clone());
            }
            self.observations.payload_misses = self.observations.payload_misses.saturating_add(1);
        }
        self.observations.physical_started = self.observations.physical_started.saturating_add(1);
        let started = std::time::Instant::now();
        let result = self.inner.read(digest, limit).await;
        self.observations.physical_completed =
            self.observations.physical_completed.saturating_add(1);
        self.observations.physical_elapsed_micros = self
            .observations
            .physical_elapsed_micros
            .saturating_add(crate::diagnostics::micros(started));
        self.observations.physical_failed = self
            .observations
            .physical_failed
            .saturating_add(u64::from(result.is_err()));
        let bytes = result?;
        self.observations.physical_bytes = self
            .observations
            .physical_bytes
            .saturating_add(bytes.len() as u64);
        if eligible
            && !bytes.is_empty()
            && bytes.len() <= limit
            && vcp_protocol::digest_bytes(&bytes) == digest
            && !self.chunks.contains_key(digest)
        {
            if self.chunks.len() >= MAX_ENTRIES {
                self.observations.payload_refused_entries =
                    self.observations.payload_refused_entries.saturating_add(1);
            } else if bytes.len() > MAX_BYTES.saturating_sub(self.chunk_bytes) {
                self.observations.payload_refused_bytes =
                    self.observations.payload_refused_bytes.saturating_add(1);
            } else {
                self.chunk_bytes += bytes.len();
                self.chunks.insert(digest.to_owned(), bytes.clone());
                self.observations.payload_admitted =
                    self.observations.payload_admitted.saturating_add(1);
            }
        }
        Ok(bytes)
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        // Preparation is read-only. Keep this adapter safe for other callers:
        // even a failed/cancelled write discards previous physical observations.
        self.observations.add(self.verified.observations);
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
