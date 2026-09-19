// SPDX-License-Identifier: Apache-2.0
//! Portable, attributed continuation data. A packet never grants dispatch,
//! artifact access, or money; a destination must reassemble and readmit it.
use crate::{
    manifest::*,
    selection::{assemble, Counter},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use vcp_domain::{accounting::Ledger, artifact::ArtifactDescriptor, ArtifactId, Micros};
use vcp_protocol::{canonical_bytes, digest_bytes};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Discarded {
    pub artifact: ArtifactId,
    pub field: String,
    pub sha256: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Packet {
    pub version: u32,
    pub manifest: Manifest,
    /// Includes protected, active and uncertain liability, never just spend.
    pub ledger: Ledger,
    pub remaining: Micros,
    pub current_state: serde_json::Value,
    pub references: Vec<ArtifactDescriptor>,
    pub discarded: Vec<Discarded>,
}

fn remaining(ledger: &Ledger) -> Result<Micros> {
    let held = [
        ledger.settled,
        ledger.active,
        ledger.unresolved,
        ledger.protected,
    ]
    .into_iter()
    .try_fold(0u64, |n, x| n.checked_add(x.get()))
    .ok_or(Error::Invalid("handoff accounting overflow"))?;
    Ok(Micros::new(ledger.cap.get().saturating_sub(held)))
}

impl Packet {
    pub fn new(
        manifest: Manifest,
        ledger: Ledger,
        current_state: serde_json::Value,
        references: Vec<ArtifactDescriptor>,
        discarded: Vec<Discarded>,
    ) -> Result<Self> {
        let packet = Self {
            version: 1,
            remaining: remaining(&ledger)?,
            manifest,
            ledger,
            current_state,
            references,
            discarded,
        };
        packet.validate()?;
        Ok(packet)
    }

    fn validate(&self) -> Result<()> {
        let scope = &self.manifest.revisions.scope;
        if self.version != 1
            || self.references.len() > 4096
            || self.discarded.len() > 4096
            || self.ledger.scope.workspace != scope.workspace
            || self.ledger.scope.session != scope.session
            || self.remaining != remaining(&self.ledger)?
            || canonical_bytes(self)?.len() > 16 * 1024 * 1024
        {
            return Err(Error::Invalid("handoff scope, accounting or bounds"));
        }
        let mut ids = BTreeSet::new();
        for source in &self.references {
            source
                .validate()
                .map_err(|_| Error::Invalid("handoff source"))?;
            if source.spec.scope != *scope || !ids.insert(&source.spec.id) {
                return Err(Error::Invalid("handoff source scope or duplicate"));
            }
        }
        for part in &self.manifest.included {
            part.validate(scope)?;
            if !self.references.iter().any(|r| {
                r.spec.id == part.artifact
                    && r.sha256 == part.source_hash
                    && r.length == part.source_length
            }) {
                return Err(Error::Invalid("handoff part has no original reference"));
            }
        }
        for field in &self.discarded {
            if !ids.contains(&field.artifact)
                || field.field.is_empty()
                || field.field.len() > 512
                || field.reason.is_empty()
                || field.reason.len() > 1024
                || field.sha256.len() != 64
                || !field.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(Error::Invalid("discarded provider field provenance"));
            }
        }
        Ok(())
    }

    /// Recheck current authority, deletion, task and accounting snapshots and
    /// read every retained dependency, then fit the *destination* envelope.
    /// The resulting sealed context still needs normal host send admission.
    pub fn reassemble(
        &self,
        current: &Revisions,
        ledger: &Ledger,
        envelope: Envelope,
        schemas: serde_json::Value,
        counter: &dyn Counter,
        mut read: impl FnMut(&ArtifactId) -> Result<Vec<u8>>,
        encode: impl Fn(&[Part], &Envelope, &serde_json::Value) -> Result<Vec<u8>>,
    ) -> Result<Sealed> {
        self.validate()?;
        if current != &self.manifest.revisions || ledger != &self.ledger {
            return Err(Error::Stale);
        }
        let mut total = 0u64;
        for source in &self.references {
            total = total
                .checked_add(source.length.get())
                .ok_or(Error::Capacity)?;
            if total > 64 * 1024 * 1024 {
                return Err(Error::Capacity);
            }
            let bytes = read(&source.spec.id)?;
            if bytes.len() as u64 != source.length.get() || digest_bytes(&bytes) != source.sha256 {
                return Err(Error::Stale);
            }
            for part in self
                .manifest
                .included
                .iter()
                .filter(|p| p.artifact == source.spec.id)
            {
                let start = usize::try_from(part.start.get()).map_err(|_| Error::Stale)?;
                let end = usize::try_from(part.end.get()).map_err(|_| Error::Stale)?;
                if bytes.get(start..end) != Some(part.content.bytes()?.as_slice()) {
                    return Err(Error::Stale);
                }
            }
        }
        assemble(
            self.manifest.included.clone(),
            current.clone(),
            envelope,
            schemas,
            self.manifest.instruction_probes.clone(),
            counter,
            encode,
        )
    }
}
