// SPDX-License-Identifier: Apache-2.0
//! Test-only layout-3 publication binds an admitted source, qualified transition,
//! next immutable catalog and exact original commit bytes. No persisted root is
//! a replacement for mandatory semantic replay.
use crate::{
    durable_owner::{DurableOwner, PreparedDurable},
    history_catalog::Catalog,
    history_index::Pages,
    Error, Result,
};
use serde::{Deserialize, Serialize};
use vcp_protocol::{canonical_bytes, digest_bytes};
const MAX_PUBLICATION_BYTES: usize = 16 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Publication {
    version: u32,
    source: String,
    next: String,
    current_projection: String,
    history: Catalog,
    originals: crate::original_commits::OriginalCommits,
    original_payload: String,
}
fn descriptor(
    source: &DurableOwner,
    prepared: &PreparedDurable,
    next: &DurableOwner,
    payload: &[u8],
) -> Result<Publication> {
    if payload.is_empty() || payload.len() > crate::contract::MAX_COMMIT_BYTES {
        return Err(Error::Limit("commit bytes"));
    }
    let original: crate::contract::Commit = serde_json::from_slice(payload)?;
    if source.identity() != prepared.source_identity()
        || &original != prepared.commit()
        || prepared.semantic().proposed().current != *next.semantic().current()
    {
        return Err(Error::Corruption("history publication identity"));
    }
    Ok(Publication {
        version: 3,
        source: source.identity().to_owned(),
        next: next.identity().to_owned(),
        current_projection: next.semantic().current().projection_digest()?,
        history: next.semantic().catalog().clone(),
        originals: next.originals().clone(),
        original_payload: digest_bytes(payload),
    })
}
pub(crate) async fn stage(
    pages: &mut impl Pages,
    source: &DurableOwner,
    prepared: &PreparedDurable,
    payload: &[u8],
) -> Result<(DurableOwner, String)> {
    // Only this actual advancement supplies next roots; callers cannot combine
    // an arbitrary same-watermark catalog with an unrelated prepared current.
    let next = source.advance(pages, prepared, payload).await?;
    let bytes = canonical_bytes(&descriptor(source, prepared, &next, payload)?)?;
    if bytes.len() > MAX_PUBLICATION_BYTES {
        return Err(Error::Limit("history publication bytes"));
    }
    let digest = digest_bytes(&bytes);
    pages.write(&digest, &bytes).await?;
    Ok((next, digest))
}
/// Compare every field with the result of full semantic replay, never merely
/// decode a manifest and treat its current/hash/root fields as authority.
pub(crate) async fn verify(
    pages: &mut impl Pages,
    digest: &str,
    source: &DurableOwner,
    prepared: &PreparedDurable,
    next: &DurableOwner,
    original_payload: &[u8],
) -> Result<()> {
    let bytes = pages.read(digest, MAX_PUBLICATION_BYTES).await?;
    if digest_bytes(&bytes) != digest
        || bytes != canonical_bytes(&descriptor(source, prepared, next, original_payload)?)?
    {
        return Err(Error::Corruption("history publication differs from replay"));
    }
    Ok(())
}
