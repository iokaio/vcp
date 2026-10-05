// SPDX-License-Identifier: Apache-2.0
//! Verified index pages retained only during one preparation. This is neither
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
}
impl<'a, P: Pages> ReadSession<'a, P> {
    pub(crate) fn new(inner: &'a mut P) -> Self {
        Self {
            inner,
            verified: VerifiedPages::default(),
        }
    }
}
impl<P: Pages> Pages for ReadSession<'_, P> {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        self.inner.read(digest, limit).await
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        // Preparation is read-only. Keep this adapter safe for other callers:
        // even a failed/cancelled write discards previous physical observations.
        self.verified = VerifiedPages::default();
        self.inner.write(digest, bytes).await
    }
    fn verified_pages(&mut self) -> Option<&mut VerifiedPages> {
        Some(&mut self.verified)
    }
}

#[cfg(test)]
#[path = "history_index_memo_tests.rs"]
mod tests;
