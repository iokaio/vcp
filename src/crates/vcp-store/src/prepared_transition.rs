// SPDX-License-Identifier: Apache-2.0
//! Crate-private evidence that an append candidate came from canonical prepare.
//! There is no constructor accepting an arbitrary alleged-valid next State.
use super::{Commit, State, StateSize, Transaction};
use crate::{Result, StoreDiagnostics};
use vcp_domain::Watermark;

pub(crate) struct PreparedTransition {
    source_watermark: Watermark,
    source_events: u64,
    next: State,
    commit: Commit,
}
impl PreparedTransition {
    pub(crate) fn prepare(
        source: &State,
        transaction: &Transaction,
        diagnostics: &mut StoreDiagnostics,
        size: &mut StateSize,
    ) -> Result<Self> {
        let (next, commit) = source.prepare_observed(transaction, diagnostics, size)?;
        Ok(Self {
            source_watermark: source.watermark,
            source_events: source.events.len() as u64,
            next,
            commit,
        })
    }
    pub(crate) fn source_watermark(&self) -> Watermark {
        self.source_watermark
    }
    pub(crate) fn source_events(&self) -> u64 {
        self.source_events
    }
    pub(crate) fn state(&self) -> &State {
        &self.next
    }
    pub(crate) fn commit(&self) -> &Commit {
        &self.commit
    }
    pub(crate) fn into_parts(self) -> (State, Commit) {
        (self.next, self.commit)
    }
}
