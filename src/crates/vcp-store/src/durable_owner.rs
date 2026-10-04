// SPDX-License-Identifier: Apache-2.0
//! Joint data admission for current state, retained envelopes/receipts and exact
//! original replay bytes. Native ownership and durable publication remain
//! separate mandatory boundaries. Qualification precedes Store activation.
use crate::{
    admitted_history::AdmittedCut,
    contract::{
        current_transition::{self, PreparedCurrent},
        Commit, Receipt, State, Transaction,
    },
    history_catalog::Catalog,
    history_index::Pages,
    original_commits::OriginalCommits,
    replay_base::ReplayBase,
    Error, Result,
};
use vcp_protocol::{canonical_bytes, digest_bytes};

pub(crate) struct DurableOwner {
    semantic: AdmittedCut,
    originals: OriginalCommits,
    identity: String,
}
pub(crate) enum Outcome {
    Duplicate(Receipt),
    Prepared(PreparedDurable),
}
/// Only prepare on the jointly admitted owner can construct this certificate.
/// It cannot be transferred to a same-watermark owner with different roots.
pub(crate) struct PreparedDurable {
    source: String,
    semantic: PreparedCurrent,
}
impl PreparedDurable {
    pub(crate) fn source_identity(&self) -> &str {
        &self.source
    }
    pub(crate) fn semantic(&self) -> &PreparedCurrent {
        &self.semantic
    }
    pub(crate) fn commit(&self) -> &Commit {
        self.semantic.commit()
    }
}
impl DurableOwner {
    /// Caller has completed full semantic replay of every retained original
    /// transition and holds the canonical ownership/generation pin. This pairs
    /// all roots to those admitted results without repeating semantic replay.
    /// A deserialized final State alone does not establish this precondition.
    /// Original payload entries must come from, or be byte-compared with, the
    /// pinned native replay source; receipt equality alone cannot prove the
    /// original JSON spelling of a semantically identical transaction.
    pub(crate) async fn from_replayed(
        pages: &mut impl Pages,
        replayed: &State,
        base: Option<&ReplayBase>,
        catalog: Catalog,
        originals: OriginalCommits,
    ) -> Result<Self> {
        originals.verify_base(pages, base).await?;
        if originals.watermark() != replayed.watermark {
            return Err(Error::Corruption("original history head differs"));
        }
        let prior_count = base.map_or(0, |base| base.state.transactions.len()) as u64;
        if prior_count.checked_add(originals.root().count())
            != Some(replayed.transactions.len() as u64)
        {
            return Err(Error::Corruption("original history receipt count"));
        }
        if let Some(base) = base {
            for (id, receipt) in base.state.transactions.iter() {
                if replayed.transactions.get(id) != Some(receipt) {
                    return Err(Error::Corruption("retained base receipt differs"));
                }
            }
        }
        let mut watermark = originals.base_watermark();
        while watermark < originals.watermark() {
            watermark = watermark.next()?;
            let original = originals
                .get(pages, watermark)
                .await?
                .ok_or(Error::Corruption("original replay commit missing"))?;
            // get proves tx digest, identity and watermark from exact bytes.
            // The receipt from full replay binds that complete transaction.
            if replayed.transactions.get(&original.commit.transaction.id)
                != Some(&original.commit.receipt)
            {
                return Err(Error::Corruption(
                    "original commit differs from admitted replay",
                ));
            }
        }
        let semantic = AdmittedCut::from_replayed(pages, replayed, catalog).await?;
        Self::paired(semantic, originals)
    }
    fn paired(semantic: AdmittedCut, originals: OriginalCommits) -> Result<Self> {
        if semantic.current().watermark != originals.watermark() {
            return Err(Error::Corruption("durable owner cut differs"));
        }
        let identity = digest_bytes(&canonical_bytes(&(
            "vcp-durable-history-owner/1",
            semantic.identity(),
            &originals,
        ))?);
        Ok(Self {
            semantic,
            originals,
            identity,
        })
    }
    pub(crate) fn semantic(&self) -> &AdmittedCut {
        &self.semantic
    }
    pub(crate) fn originals(&self) -> &OriginalCommits {
        &self.originals
    }
    pub(crate) fn identity(&self) -> &str {
        &self.identity
    }
    pub(crate) async fn prepare(
        &self,
        pages: &mut impl Pages,
        transaction: &Transaction,
    ) -> Result<Outcome> {
        Ok(
            match current_transition::prepare(pages, &self.semantic, transaction).await? {
                current_transition::Outcome::Duplicate(receipt) => Outcome::Duplicate(receipt),
                current_transition::Outcome::Prepared(semantic) => {
                    Outcome::Prepared(PreparedDurable {
                        source: self.identity.clone(),
                        semantic,
                    })
                }
            },
        )
    }
    /// Tentative immutable writes only. Native publication must atomically bind
    /// this exact returned owner, original bytes and commit before installing it.
    pub(crate) async fn advance(
        &self,
        pages: &mut impl Pages,
        prepared: &PreparedDurable,
        original_payload: &[u8],
    ) -> Result<Self> {
        if prepared.source != self.identity {
            return Err(Error::Conflict("prepared durable source differs"));
        }
        let originals = self
            .originals
            .append_verified(pages, original_payload, prepared.commit())
            .await?;
        let semantic = self.semantic.advance(pages, &prepared.semantic).await?;
        Self::paired(semantic, originals)
    }
}

#[path = "durable_owner_tests.rs"]
mod tests;
