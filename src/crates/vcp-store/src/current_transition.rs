// SPDX-License-Identifier: Apache-2.0
//! Full semantic preparation over current records and authenticated bounded
//! history reads. Preserve predicate order and errors without resident history.
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
    prepare_observed(pages, source, transaction, None).await
}
pub(crate) async fn prepare_observed(
    pages: &mut impl Pages,
    source: &AdmittedCut,
    transaction: &Transaction,
    mut diagnostics: Option<&mut crate::StoreDiagnostics>,
) -> Result<Outcome> {
    // Discard verified physical index observations at this operation boundary,
    // including cancellation/error; the next operation must read its own pages.
    let mut read_session = crate::history_index::memo::ReadSession::new(pages);
    let result = prepare_in_session(
        &mut read_session,
        source,
        transaction,
        diagnostics.as_deref_mut(),
    )
    .await;
    if let Some(diagnostics) = diagnostics {
        diagnostics.history_reads.add(read_session.observations());
    }
    result
}

async fn prepare_in_session(
    pages: &mut crate::history_index::memo::ReadSession<'_, impl Pages>,
    source: &AdmittedCut,
    transaction: &Transaction,
    mut diagnostics: Option<&mut crate::StoreDiagnostics>,
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
    let current: crate::CurrentStateView<'_> = (&proposed.current).into();
    let mut work = HistoryWork::default();
    macro_rules! phase {
        ($name:ident, $operation:expr) => {{
            let started = std::time::Instant::now();
            let reads_before = pages.observations();
            let result = $operation.await;
            if let Some(diagnostics) = diagnostics.as_deref_mut() {
                diagnostics
                    .validation_history_reads
                    .$name
                    .add(pages.observations().since(reads_before));
                diagnostics
                    .validation_phases
                    .$name
                    .record(started, result.is_ok());
            }
            result
        }};
    }
    let validation_started = std::time::Instant::now();
    if let Some(diagnostics) = diagnostics.as_deref_mut() {
        diagnostics.validation_input_records = diagnostics
            .validation_input_records
            .saturating_add(current.records.len() as u64);
        diagnostics.validation_input_events = diagnostics.validation_input_events.saturating_add(
            source
                .catalog()
                .event_count()
                .saturating_add(proposed.events.len() as u64),
        );
        diagnostics.state_size_delta_updates =
            diagnostics.state_size_delta_updates.saturating_add(1);
    }
    let validated = async {
        let size = phase!(capacity, async {
            let size = source
                .size()
                .next(source.current().into(), current, &proposed.touched)?;
            size.validate(current)?;
            Ok::<_, Error>(size)
        })?;
        phase!(records, async {
            history
                .run(pages, |history| {
                    current_validation::validate_records(
                        current,
                        &mut CandidateHistory::new(history, &proposed),
                    )
                })
                .await?;
            Ok::<_, Error>(())
        })?;
        phase!(events, async {
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
            Ok::<_, Error>(())
        })?;
        phase!(redaction, async {
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
            Ok::<_, Error>(())
        })?;
        phase!(accounting, async {
            let validation = crate::accounting_contract::Validation::new(current)?;
            for attempt in validation.attempts() {
                history
                    .run(pages, |history| {
                        validation.attempt(attempt, &mut CandidateHistory::new(history, &proposed))
                    })
                    .await?;
            }
            validation.finish()
        })?;
        phase!(ingestion, async {
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
            Ok::<_, Error>(())
        })?;
        phase!(search, async {
            history
                .run(pages, |history| {
                    search_contract::validate_with_history(
                        current,
                        &mut CandidateHistory::new(history, &proposed),
                    )
                })
                .await?;
            Ok::<_, Error>(())
        })?;
        phase!(agents, async {
            agents_contract::validate_current(current)?;
            Ok::<_, Error>(())
        })?;
        Ok::<_, Error>(size)
    }
    .await;
    if let Some(diagnostics) = diagnostics.as_deref_mut() {
        diagnostics
            .validation
            .record(validation_started, validated.is_ok());
    }
    let size = validated?;
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

#[cfg(test)]
#[path = "current_transition_tests.rs"]
mod tests;
