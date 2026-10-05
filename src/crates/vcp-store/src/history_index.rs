// SPDX-License-Identifier: Apache-2.0
//! Layout-3 authenticated ordered index. Roots must come from the validated
//! owner; decoding a persisted root alone confers no semantic replay trust.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use vcp_protocol::{canonical_bytes, digest_bytes};
#[path = "history_index_copy.rs"]
mod copy;
#[path = "history_index_io.rs"]
pub(crate) mod io;
#[path = "history_index_memo.rs"]
pub(crate) mod memo;

const FANOUT: usize = 32;
const MAX_KEY_BYTES: usize = 512;
const MAX_VALUE_BYTES: usize = 2048;
const MAX_PAGE_BYTES: usize = 128 * 1024;
const MAX_HEIGHT: u8 = 16;
const MAX_READ_ROWS: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Table {
    EventOrdinal,
    EventIdentity,
    ArtifactReference,
    Command,
    Transaction,
    Commit,
    CommitPayload,
    ArchiveChunks,
    ArchiveObjects,
    ArchiveParts,
    ArchiveInputs,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Entry {
    pub key: String,
    /// Bounded canonical locator/validation metadata, never event/body content.
    pub value: serde_json::Value,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Link {
    digest: String,
    first: String,
    last: String,
    count: u64,
    height: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Root {
    version: u32,
    table: Table,
    head: Option<Link>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    version: u32,
    table: Table,
    body: Body,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "rows",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Body {
    Leaf(Vec<Entry>),
    Branch(Vec<Link>),
}

/// Backend implementations must bound bytes before allocation and make writes
/// immutable. Root publication is a separate atomic owner operation.
#[allow(async_fn_in_trait)]
pub(crate) trait Pages {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>>;
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()>;
    fn verified_pages(&mut self) -> Option<&mut memo::VerifiedPages> {
        None
    }
}

impl Root {
    pub(crate) fn empty(table: Table) -> Self {
        Self {
            version: 3,
            table,
            head: None,
        }
    }
    pub(crate) fn count(&self) -> u64 {
        self.head.as_ref().map_or(0, |head| head.count)
    }
    pub(crate) fn validate_table(&self, expected: Table) -> Result<()> {
        self.validate()?;
        if self.table != expected {
            return Err(Error::Corruption("history index root domain"));
        }
        Ok(())
    }
    fn validate(&self) -> Result<()> {
        if self.version != 3 {
            return Err(Error::Incompatible);
        }
        if let Some(head) = &self.head {
            valid_link(head)?;
        }
        Ok(())
    }
    async fn load(&self, pages: &mut impl Pages, link: &Link) -> Result<Page> {
        valid_link(link)?;
        if let Some(page) = pages
            .verified_pages()
            .and_then(|memo| memo.get(self.table, link))
        {
            return page;
        }
        let bytes = pages.read(&link.digest, MAX_PAGE_BYTES).await?;
        if bytes.len() > MAX_PAGE_BYTES || digest_bytes(&bytes) != link.digest {
            return Err(Error::Corruption("history index page digest"));
        }
        let page: Page = serde_json::from_slice(&bytes)?;
        if page.version != 3 || page.table != self.table {
            return Err(Error::Corruption("history index page domain"));
        }
        if canonical_bytes(&page)? != bytes || describe(&page, link.digest.clone())? != *link {
            return Err(Error::Corruption("history index page commitment"));
        }
        if let Some(memo) = pages.verified_pages() {
            memo.insert(link.clone(), page.clone(), bytes.len());
        }
        Ok(page)
    }
    async fn save(&self, pages: &mut impl Pages, body: Body) -> Result<Link> {
        let page = Page {
            version: 3,
            table: self.table,
            body,
        };
        let bytes = canonical_bytes(&page)?;
        if bytes.len() > MAX_PAGE_BYTES {
            return Err(Error::Limit("history index page"));
        }
        let digest = digest_bytes(&bytes);
        let link = describe(&page, digest.clone())?;
        pages.write(&digest, &bytes).await?;
        Ok(link)
    }

    pub(crate) async fn get(&self, pages: &mut impl Pages, key: &str) -> Result<Option<Entry>> {
        self.validate()?;
        valid_key(key)?;
        let Some(mut link) = self.head.clone() else {
            return Ok(None);
        };
        loop {
            if key < link.first.as_str() || key > link.last.as_str() {
                return Ok(None);
            }
            match self.load(pages, &link).await?.body {
                Body::Leaf(rows) => {
                    return Ok(rows
                        .binary_search_by(|row| row.key.as_str().cmp(key))
                        .ok()
                        .map(|i| rows[i].clone()))
                }
                Body::Branch(children) => {
                    let Some(child) = children
                        .into_iter()
                        .find(|child| key <= child.last.as_str())
                    else {
                        return Ok(None);
                    };
                    link = child;
                }
            }
        }
    }

    /// Persistent insert. An exact duplicate is idempotent; rewriting an
    /// existing historical identity is refused. Rewrites use another generation.
    pub(crate) async fn insert(&self, pages: &mut impl Pages, entry: Entry) -> Result<Self> {
        self.validate()?;
        valid_entry(&entry)?;
        let mut path = Vec::<(Vec<Link>, usize)>::new();
        let mut link = self.head.clone();
        let mut rows = loop {
            let Some(current) = link else {
                break Vec::new();
            };
            match self.load(pages, &current).await?.body {
                Body::Leaf(rows) => break rows,
                Body::Branch(children) => {
                    let index = children
                        .iter()
                        .position(|child| entry.key <= child.last)
                        .unwrap_or(children.len() - 1);
                    link = Some(children[index].clone());
                    path.push((children, index));
                }
            }
        };
        match rows.binary_search_by(|row| row.key.cmp(&entry.key)) {
            Ok(index) if rows[index] == entry => return Ok(self.clone()),
            Ok(_) => return Err(Error::Corruption("history index identity changed")),
            Err(index) => rows.insert(index, entry),
        }
        let mut replacement = if rows.len() > FANOUT {
            let right = rows.split_off(rows.len() / 2);
            vec![
                self.save(pages, Body::Leaf(rows)).await?,
                self.save(pages, Body::Leaf(right)).await?,
            ]
        } else {
            vec![self.save(pages, Body::Leaf(rows)).await?]
        };
        while let Some((mut children, index)) = path.pop() {
            children.splice(index..=index, replacement);
            replacement = if children.len() > FANOUT {
                let right = children.split_off(children.len() / 2);
                vec![
                    self.save(pages, Body::Branch(children)).await?,
                    self.save(pages, Body::Branch(right)).await?,
                ]
            } else {
                vec![self.save(pages, Body::Branch(children)).await?]
            };
        }
        let head = if replacement.len() == 1 {
            replacement.pop()
        } else {
            Some(self.save(pages, Body::Branch(replacement)).await?)
        };
        Ok(Self {
            version: self.version,
            table: self.table,
            head,
        })
    }

    /// Strict keyset continuation. Returned rows are bounded independently of
    /// total history, and each visited page is checked through its parent link.
    pub(crate) async fn page(
        &self,
        pages: &mut impl Pages,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Entry>> {
        self.validate()?;
        if limit == 0 || limit > MAX_READ_ROWS {
            return Err(Error::Limit("history index read rows"));
        }
        if let Some(after) = after {
            valid_key(after)?;
        }
        let mut pending: Vec<Link> = self.head.iter().cloned().collect();
        let mut rows = Vec::new();
        while let Some(link) = pending.pop() {
            if after.is_some_and(|after| link.last.as_str() <= after) {
                continue;
            }
            match self.load(pages, &link).await?.body {
                Body::Branch(children) => pending.extend(children.into_iter().rev()),
                Body::Leaf(entries) => {
                    for entry in entries {
                        if after.is_none_or(|after| entry.key.as_str() > after) {
                            rows.push(entry);
                            if rows.len() == limit {
                                return Ok(rows);
                            }
                        }
                    }
                }
            }
        }
        Ok(rows)
    }
}

fn valid_key(key: &str) -> Result<()> {
    if key.is_empty() || key.len() > MAX_KEY_BYTES || key.chars().any(char::is_control) {
        return Err(Error::Corruption("history index key"));
    }
    Ok(())
}
fn valid_entry(entry: &Entry) -> Result<()> {
    valid_key(&entry.key)?;
    if canonical_bytes(&entry.value)?.len() > MAX_VALUE_BYTES {
        return Err(Error::Limit("history index metadata"));
    }
    Ok(())
}
fn valid_link(link: &Link) -> Result<()> {
    valid_key(&link.first)?;
    valid_key(&link.last)?;
    if link.first > link.last
        || link.count == 0
        || link.height > MAX_HEIGHT
        || link.digest.len() != 64
        || !link
            .digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::Corruption("history index link"));
    }
    Ok(())
}
fn describe(page: &Page, digest: String) -> Result<Link> {
    let (first, last, count, height) = match &page.body {
        Body::Leaf(rows) => {
            if rows.is_empty() || rows.len() > FANOUT {
                return Err(Error::Corruption("history index leaf count"));
            }
            for row in rows {
                valid_entry(row)?;
            }
            if rows.windows(2).any(|pair| pair[0].key >= pair[1].key) {
                return Err(Error::Corruption("history index key order"));
            }
            (
                &rows[0].key,
                &rows[rows.len() - 1].key,
                rows.len() as u64,
                0,
            )
        }
        Body::Branch(children) => {
            if children.len() < 2 || children.len() > FANOUT {
                return Err(Error::Corruption("history index branch count"));
            }
            let mut count = 0u64;
            for child in children {
                valid_link(child)?;
                count = count
                    .checked_add(child.count)
                    .ok_or(Error::Corruption("history index count overflow"))?;
            }
            if children
                .windows(2)
                .any(|pair| pair[0].last >= pair[1].first || pair[0].height != pair[1].height)
            {
                return Err(Error::Corruption("history index branch order"));
            }
            (
                &children[0].first,
                &children[children.len() - 1].last,
                count,
                children[0]
                    .height
                    .checked_add(1)
                    .ok_or(Error::Corruption("history index height"))?,
            )
        }
    };
    let link = Link {
        digest,
        first: first.clone(),
        last: last.clone(),
        count,
        height,
    };
    valid_link(&link)?;
    Ok(link)
}

#[cfg(test)]
#[path = "history_index_tests.rs"]
mod tests;
