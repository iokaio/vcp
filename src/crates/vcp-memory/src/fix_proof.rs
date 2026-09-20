// SPDX-License-Identifier: Apache-2.0
//! A remembered fix describes an observed source delta and its checked result;
//! it does not infer who caused the edits or prove the proposed diagnosis.
use crate::{access::Access, Result};
use vcp_domain::{artifact::ArtifactDescriptor, memory::*};
use vcp_store::{contract::Collection, Store};

pub(crate) fn matches(store: &Store, access: &Access, proposal: &Proposal) -> Result<bool> {
    let ClaimValue::VerifiedFix {
        patch,
        before,
        after,
        ..
    } = &proposal.value
    else {
        return Ok(false);
    };
    let descriptor: ArtifactDescriptor = store
        .state()
        .record(Collection::Artifact, patch.as_str(), &access.workspace)?
        .decode()?;
    if descriptor.spec.schema != "vcp-memory-change/1" || descriptor.length.get() > 256 * 1024 {
        return Ok(false);
    }
    let mut bytes = Vec::new();
    if vcp_audit::history::History::read_artifact(store, &access.history(), patch, &mut bytes)
        .is_err()
    {
        return Ok(false);
    }
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Ok(false);
    };
    let base = &value["base"];
    let current = &value["current"];
    if vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(base)?) != before.repository
        || vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(current)?) != after.repository
        || base["identity"] != current["identity"]
        || base["identity"]["workspace"].as_str() != Some(access.workspace.as_str())
        || base["bounded_scan_complete"] != true
        || current["bounded_scan_complete"] != true
    {
        return Ok(false);
    }
    let (Some(old), Some(new)) = (base["files"].as_array(), current["files"].as_array()) else {
        return Ok(false);
    };
    // Exact, bounded snapshots are required; unrelated prose labelled Patch is
    // never sufficient, and an unchanged snapshot cannot establish a fix.
    if old.len() > 4096 || new.len() > 4096 || old == new || (old.is_empty() && new.is_empty()) {
        return Ok(false);
    }
    for files in [old, new] {
        let mut paths = std::collections::BTreeSet::new();
        for file in files {
            let (Some(path), Some(hash)) = (file["path"].as_str(), file["sha256"].as_str()) else {
                return Ok(false);
            };
            if path.is_empty()
                || path.starts_with('/')
                || path.contains('\\')
                || path.split('/').any(|part| matches!(part, ".." | "." | ""))
                || !paths.insert(path.to_lowercase())
                || hash.len() != 64
                || !hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
