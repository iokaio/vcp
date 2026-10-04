// SPDX-License-Identifier: Apache-2.0
//! Qualification candidate for complete semantic preparation over an admitted
//! current projection and authenticated, bounded history reads. No Store calls
//! this module until reference equivalence and publication are qualified.
use super::{
    agents_contract, current_preparation, current_validation, event_history_validation,
    ingestion_contract, search_contract, Commit, Receipt, Transaction, FORMAT_VERSION,
};
use crate::{
    admitted_history::AdmittedCut,
    current_size::CurrentSize,
    history_index::Pages,
    resolved_history::{CandidateHistory, ResolvedHistory},
    Error, Result,
};
use vcp_protocol::event::EventEnvelope;

pub(crate) enum Outcome {
    Duplicate(Receipt),
    Prepared(PreparedCurrent),
}
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct HistoryWork {
    pub(crate) full_passes: u64,
    pub(crate) pages: u64,
    pub(crate) rows: u64,
    pub(crate) maximum_page_rows: usize,
    pub(crate) resolutions: crate::resolved_history::ResolutionDiagnostics,
}
/// Construction is private to the full validator below. The source identity
/// binds every catalog root and the current projection, not just its watermark.
pub(crate) struct PreparedCurrent {
    source: String,
    proposed: current_preparation::ProposedTransition,
    commit: Commit,
    size: CurrentSize,
    work: HistoryWork,
}
impl PreparedCurrent {
    pub(crate) fn source_identity(&self) -> &str {
        &self.source
    }
    pub(crate) fn proposed(&self) -> &current_preparation::ProposedTransition {
        &self.proposed
    }
    pub(crate) fn commit(&self) -> &Commit {
        &self.commit
    }
    pub(crate) fn size(&self) -> CurrentSize {
        self.size
    }
    pub(crate) fn history_work(&self) -> HistoryWork {
        self.work
    }
}

pub(crate) async fn prepare(
    pages: &mut impl Pages,
    source: &AdmittedCut,
    transaction: &Transaction,
) -> Result<Outcome> {
    let mut history = ResolvedHistory::new(source);
    let proposed = match history
        .run(pages, |history| {
            current_preparation::propose(source.current(), transaction, history)
        })
        .await?
    {
        current_preparation::Outcome::Duplicate(receipt) => return Ok(Outcome::Duplicate(receipt)),
        current_preparation::Outcome::Proposed(proposed) => proposed,
    };
    let current = (&proposed.current).into();
    let mut work = HistoryWork::default();
    // Layout 3 capacity concerns the actual current projection. Complete legacy
    // archival State retains its separate unchanged 64 MiB format contract.
    let size = CurrentSize::measure(source.current().into())?.next(
        source.current().into(),
        current,
        &proposed.touched,
    )?;
    size.validate(current)?;
    history
        .run(pages, |history| {
            current_validation::validate_records(
                current,
                &mut CandidateHistory::new(history, &proposed),
            )
        })
        .await?;
    let mut events = event_history_validation::EventHistoryValidator::new(
        current.watermark,
        current.records,
        current.sequences,
    );
    visit_events(pages, source, &proposed.events, &mut work, |rows| {
        events.extend(rows.iter().map(Ok))
    })
    .await?;
    events.finish()?;
    history
        .run(pages, |history| {
            crate::redaction_contract::validate_current(
                current,
                &mut CandidateHistory::new(history, &proposed),
            )
        })
        .await?;
    visit_events(pages, source, &proposed.events, &mut work, |rows| {
        crate::redaction_contract::validate_event_rows(current.records, rows.iter().map(Ok))
    })
    .await?;
    history
        .run(pages, |history| {
            crate::accounting_contract::validate_with_history(
                current,
                &mut CandidateHistory::new(history, &proposed),
            )
        })
        .await?;
    let inputs = ingestion_contract::Inputs::new(current)?;
    if inputs.needs_history() {
        let count = usize::try_from(source.catalog().event_count())
            .ok()
            .and_then(|count| count.checked_add(proposed.events.len()))
            .ok_or(Error::Limit("historical event count"))?;
        let mut ingestion = inputs.history(count);
        visit_events(pages, source, &proposed.events, &mut work, |rows| {
            ingestion.extend(rows.iter().map(Ok))
        })
        .await?;
        inputs.finish(ingestion.finish()?)?;
    }
    history
        .run(pages, |history| {
            search_contract::validate_with_history(
                current,
                &mut CandidateHistory::new(history, &proposed),
            )
        })
        .await?;
    agents_contract::validate_current(current)?;
    crate::accounting_contract::admission_current(source.current().into(), current, transaction)?;
    // Publication reads the source resolver, deliberately excluding the new
    // receipt. Candidate validation above sees the appended receipt instead.
    history
        .run(pages, |history| {
            search_contract::publication_with_history(
                source.current().into(),
                current,
                transaction,
                history,
            )
        })
        .await?;
    agents_contract::publication_current(source.current().into(), current)?;
    work.resolutions = history.diagnostics();
    let commit = Commit {
        version: FORMAT_VERSION,
        transaction: transaction.clone(),
        receipt: proposed.receipt.clone(),
    };
    Ok(Outcome::Prepared(PreparedCurrent {
        source: source.identity().to_owned(),
        proposed,
        commit,
        size,
        work,
    }))
}

// Preserve validation order: each phase receives every row before the next
// phase starts. Multiple bounded passes currently trade I/O for exact semantics;
// no page prefix or historical dependency is silently trusted or skipped.
async fn visit_events(
    pages: &mut impl Pages,
    source: &AdmittedCut,
    appended: &[EventEnvelope],
    work: &mut HistoryWork,
    mut visit: impl FnMut(&[EventEnvelope]) -> Result<()>,
) -> Result<()> {
    work.full_passes = work.full_passes.saturating_add(1);
    let mut next = 0u64;
    let end = source.catalog().event_count();
    while next < end {
        let rows = source
            .catalog()
            .event_page(pages, next.checked_sub(1), 64)
            .await?;
        if rows.is_empty() {
            return Err(Error::Corruption("candidate history ended early"));
        }
        next = next
            .checked_add(rows.len() as u64)
            .filter(|next| *next <= end)
            .ok_or(Error::Corruption("candidate history row count"))?;
        work.pages = work.pages.saturating_add(1);
        work.rows = work.rows.saturating_add(rows.len() as u64);
        work.maximum_page_rows = work.maximum_page_rows.max(rows.len());
        visit(&rows)?;
    }
    visit(appended)
}

#[path = "current_transition_tests.rs"]
mod tests;
