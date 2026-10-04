// SPDX-License-Identifier: Apache-2.0
//! Test-only layout-3 publication binds an admitted source, qualified transition,
//! next immutable catalog and exact original commit bytes. No persisted root is
//! a replacement for mandatory semantic replay.
use crate::{
    admitted_history::AdmittedCut, contract::current_transition::PreparedCurrent,
    history_catalog::Catalog, history_index::Pages, Error, Result,
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
    original_payload: String,
}
fn descriptor(
    source: &AdmittedCut,
    prepared: &PreparedCurrent,
    next: &AdmittedCut,
    payload: &[u8],
) -> Result<Publication> {
    if source.identity() != prepared.source_identity()
        || canonical_bytes(prepared.commit())? != payload
        || prepared.proposed().current != *next.current()
    {
        return Err(Error::Corruption("history publication identity"));
    }
    Ok(Publication {
        version: 3,
        source: source.identity().to_owned(),
        next: next.identity().to_owned(),
        current_projection: next.current().projection_digest()?,
        history: next.catalog().clone(),
        original_payload: digest_bytes(payload),
    })
}
pub(crate) async fn stage(
    pages: &mut impl Pages,
    source: &AdmittedCut,
    prepared: &PreparedCurrent,
    payload: &[u8],
) -> Result<(AdmittedCut, String)> {
    // Only this actual advancement supplies next roots; callers cannot combine
    // an arbitrary same-watermark catalog with an unrelated prepared current.
    let next = source.advance(pages, prepared).await?;
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
    source: &AdmittedCut,
    prepared: &PreparedCurrent,
    next: &AdmittedCut,
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
