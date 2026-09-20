// SPDX-License-Identifier: Apache-2.0
//! Explicit replay boundary. Historical receipt digests are commitments to
//! unavailable transaction bodies, not hashes of reconstructed transactions.
use crate::{
    artifact::{immutable_file, read_bounded},
    contract::*,
    Error, Result,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use vcp_protocol::{canonical_bytes, digest_bytes};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrefixCommitment {
    pub watermark: vcp_domain::Watermark,
    pub digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReplayBase {
    pub version: u32,
    pub source_digest: String,
    pub state: State,
    pub prefixes: Vec<PrefixCommitment>,
}
impl ReplayBase {
    pub fn load(root: &Path) -> Result<Option<Self>> {
        let path = root.join("replay-base.json");
        let seal = root.join("replay-base.seal");
        if !path.exists() && !seal.exists() {
            return Ok(None);
        }
        let bytes = read_bounded(&path, MAX_STATE_BYTES + 1024 * 1024)?;
        if read_bounded(&seal, 64)? != digest_bytes(&bytes).as_bytes() {
            return Err(Error::Corruption("replay base seal"));
        }
        let value: Self = serde_json::from_slice(&bytes)?;
        if canonical_bytes(&value)? != bytes {
            return Err(Error::Corruption("noncanonical replay base"));
        }
        if value.version != 1
            || value.source_digest.len() != 64
            || !value
                .source_digest
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(Error::Corruption("replay base format"));
        }
        if value.prefixes.len() > 4096 {
            return Err(Error::Limit("retained prefix commitments"));
        }
        let mut unique = std::collections::BTreeSet::new();
        for prefix in &value.prefixes {
            if prefix.watermark > value.state.watermark
                || prefix.digest.len() != 64
                || !prefix
                    .digest
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                || !unique.insert((prefix.watermark, prefix.digest.clone()))
            {
                return Err(Error::Corruption("retained prefix commitment"));
            }
        }
        if !value
            .prefixes
            .iter()
            .any(|p| p.watermark == value.state.watermark && p.digest == value.source_digest)
        {
            return Err(Error::Corruption("replay source commitment missing"));
        }
        value.state.validate()?;
        validate_receipts(&value.state)?;
        Ok(Some(value))
    }
    pub fn write(
        root: &Path,
        source: &State,
        state: &State,
        prefixes: &[PrefixCommitment],
    ) -> Result<()> {
        state.validate()?;
        validate_receipts(state)?;
        // Redaction authorization and content DTOs are a separate layer. This
        // seam cannot change ordering, retry commitments or command outcomes.
        crate::redaction_contract::validate_rewrite(source, state)?;
        let mut prefixes = prefixes.to_vec();
        let source_prefix = PrefixCommitment {
            watermark: source.watermark,
            digest: digest_bytes(&canonical_bytes(source)?),
        };
        if !prefixes.contains(&source_prefix) {
            prefixes.push(source_prefix);
        }
        if prefixes.len() > 4096 {
            return Err(Error::Limit("retained prefix commitments"));
        }
        let value = Self {
            version: 1,
            source_digest: digest_bytes(&canonical_bytes(source)?),
            state: state.clone(),
            prefixes,
        };
        let bytes = canonical_bytes(&value)?;
        if bytes.len() > MAX_STATE_BYTES + 1024 * 1024 {
            return Err(Error::Limit("replay base"));
        }
        immutable_file(&root.join("replay-base.json"), &bytes)?;
        immutable_file(
            &root.join("replay-base.seal"),
            digest_bytes(&bytes).as_bytes(),
        )
    }
    pub fn chain(&self) -> Result<String> {
        Ok(digest_bytes(&canonical_bytes(self)?))
    }
}
fn validate_receipts(state: &State) -> Result<()> {
    if state.transactions.len() as u64 != state.watermark.get() {
        return Err(Error::Corruption("replay receipt count"));
    }
    let mut watermarks = std::collections::BTreeSet::new();
    for (id, receipt) in &state.transactions {
        if id != &receipt.transaction
            || receipt.watermark.get() == 0
            || receipt.watermark > state.watermark
            || !watermarks.insert(receipt.watermark)
            || receipt.digest.len() != 64
            || !receipt
                .digest
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(Error::Corruption("replay receipt identity"));
        }
        if let Some(command) = &receipt.command {
            if state
                .commands
                .get(&command_key(&command.workspace, &command.command))
                != Some(command)
                || command.transaction != *id
                || command.watermark != receipt.watermark
            {
                return Err(Error::Corruption("replay command commitment"));
            }
        }
    }
    for command in state.commands.values() {
        if state
            .transactions
            .get(&command.transaction)
            .and_then(|r| r.command.as_ref())
            != Some(command)
        {
            return Err(Error::Corruption("orphan replay command"));
        }
    }
    Ok(())
}
