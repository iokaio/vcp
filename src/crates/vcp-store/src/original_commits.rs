// SPDX-License-Identifier: Apache-2.0
//! Original retained commit bytes, distinct from event watermark groups.
//! Roots are data commitments, never proof of semantic replay or ownership.
use crate::{
    contract::{Commit, FORMAT_VERSION, MAX_COMMIT_BYTES, MAX_STATE_BYTES},
    history_blob::{self, Blob},
    history_index::{Entry, Pages, Root, Table},
    replay_base::ReplayBase,
    Error, Result,
};
use serde::{Deserialize, Serialize};
use vcp_domain::{TransactionId, Watermark};
use vcp_protocol::{canonical_bytes, digest_bytes};
#[path = "original_commits_copy.rs"]
mod copy;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Origin {
    Genesis,
    // Exact existing replay-base bytes preserve unavailable-prefix commitments.
    // They do not invent pre-base transaction bodies or authorize suffix-only
    // validation of commits retained after this explicit retention boundary.
    Legacy {
        watermark: Watermark,
        chain: String,
        blob: Blob,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OriginalCommits {
    version: u32,
    origin: Origin,
    watermark: Watermark,
    rows: Root,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    transaction: TransactionId,
    blob: Blob,
}
pub(crate) struct OriginalCommit {
    pub(crate) bytes: Vec<u8>,
    pub(crate) commit: Commit,
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn decode(bytes: &[u8], watermark: Watermark) -> Result<Commit> {
    if bytes.is_empty() || bytes.len() > MAX_COMMIT_BYTES {
        return Err(Error::Limit("original commit bytes"));
    }
    let commit: Commit = serde_json::from_slice(bytes)?;
    if commit.version != FORMAT_VERSION
        || commit.receipt.transaction != commit.transaction.id
        || commit.receipt.watermark != watermark
        || commit.transaction.expected_watermark.next()? != watermark
        || commit.receipt.digest != digest_bytes(&canonical_bytes(&commit.transaction)?)
    {
        return Err(Error::Corruption("original commit identity"));
    }
    Ok(commit)
}
impl OriginalCommits {
    /// `base` must come from the existing admitted ReplayBase loader. The
    /// pairing check retains its exact validated bytes. Original prefix bodies
    /// before the retention boundary remain unavailable.
    pub(crate) async fn from_validated_base(
        pages: &mut impl Pages,
        base: Option<(&ReplayBase, &[u8])>,
    ) -> Result<Self> {
        let origin = match base {
            None => Origin::Genesis,
            Some((base, bytes)) => {
                if bytes.len() > MAX_STATE_BYTES + 1024 * 1024 {
                    return Err(Error::Limit("original replay base"));
                }
                if base.version != 1 || canonical_bytes(base)? != bytes {
                    return Err(Error::Corruption("original replay base pairing"));
                }
                Origin::Legacy {
                    watermark: base.state.watermark,
                    chain: base.chain()?,
                    blob: history_blob::write_bytes(pages, bytes).await?,
                }
            }
        };
        let watermark = match &origin {
            Origin::Genesis => Watermark::ZERO,
            Origin::Legacy { watermark, .. } => *watermark,
        };
        let result = Self {
            version: 1,
            origin,
            watermark,
            rows: Root::empty(Table::CommitPayload),
        };
        result.validate()?;
        Ok(result)
    }
    pub(crate) fn base_watermark(&self) -> Watermark {
        match &self.origin {
            Origin::Genesis => Watermark::ZERO,
            Origin::Legacy { watermark, .. } => *watermark,
        }
    }
    pub(crate) fn watermark(&self) -> Watermark {
        self.watermark
    }
    pub(crate) fn base_blob(&self) -> Option<&Blob> {
        match &self.origin {
            Origin::Genesis => None,
            Origin::Legacy { blob, .. } => Some(blob),
        }
    }
    pub(crate) fn root(&self) -> &Root {
        &self.rows
    }
    pub(crate) fn validate(&self) -> Result<()> {
        self.rows.validate_table(Table::CommitPayload)?;
        if self.version != 1
            || self
                .watermark
                .get()
                .checked_sub(self.base_watermark().get())
                != Some(self.rows.count())
        {
            return Err(Error::Corruption("original commit range"));
        }
        if let Origin::Legacy { chain, blob, .. } = &self.origin {
            blob.validate()?;
            if !hash(chain)
                || blob.bytes == 0
                || blob.bytes > (MAX_STATE_BYTES + 1024 * 1024) as u64
            {
                return Err(Error::Corruption("original replay base descriptor"));
            }
        }
        Ok(())
    }
    /// Call only after full semantic replay/admission produced `expected`.
    /// Preserve physical bytes, including accepted historical JSON spelling;
    /// never recreate them from transaction or receipt digests.
    pub(crate) async fn append_verified(
        &self,
        pages: &mut impl Pages,
        bytes: &[u8],
        expected: &Commit,
    ) -> Result<Self> {
        self.validate()?;
        let watermark = self.watermark.next()?;
        let parsed = decode(bytes, watermark)?;
        if &parsed != expected {
            return Err(Error::Corruption("original commit differs from replay"));
        }
        let row = Row {
            transaction: parsed.transaction.id,
            blob: history_blob::write_bytes(pages, bytes).await?,
        };
        let rows = self
            .rows
            .insert(
                pages,
                Entry {
                    key: format!("{:016x}", watermark.get()),
                    value: serde_json::to_value(row)?,
                },
            )
            .await?;
        let result = Self {
            version: self.version,
            origin: self.origin.clone(),
            watermark,
            rows,
        };
        result.validate()?;
        Ok(result)
    }
    /// One bounded original body. Missing retained rows are corruption, whereas
    /// a requested future watermark has no committed body yet.
    pub(crate) async fn get(
        &self,
        pages: &mut impl Pages,
        watermark: Watermark,
    ) -> Result<Option<OriginalCommit>> {
        self.validate()?;
        if watermark <= self.base_watermark() {
            return Err(Error::Conflict(
                "original commit precedes retained boundary",
            ));
        }
        if watermark > self.watermark {
            return Ok(None);
        }
        let key = format!("{:016x}", watermark.get());
        let entry = self
            .rows
            .get(pages, &key)
            .await?
            .ok_or(Error::Corruption("original commit missing"))?;
        if entry.key != key {
            return Err(Error::Corruption("original commit key"));
        }
        let row: Row = serde_json::from_value(entry.value)?;
        let bytes = history_blob::read_bounded(pages, &row.blob, MAX_COMMIT_BYTES).await?;
        let commit = decode(&bytes, watermark)?;
        if commit.transaction.id != row.transaction {
            return Err(Error::Corruption("original commit row identity"));
        }
        Ok(Some(OriginalCommit { bytes, commit }))
    }
}

#[cfg(test)]
#[path = "original_commits_tests.rs"]
mod tests;
