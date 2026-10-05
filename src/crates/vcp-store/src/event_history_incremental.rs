// SPDX-License-Identifier: Apache-2.0
//! Inductive event validation for an admitted immutable logical cut.
//!
//! EE-02b / execution-history-layout3.md requires exact dependency invalidation,
//! not repeated physical reads of untouched history on every append. The old
//! resident Store (d16d3339) likewise validated previously admitted RAM. Cold
//! replay still checks every original transaction and publication; every newly
//! consumed historical byte remains authenticated to the owner-known root.
//! No persisted checksum grants admission, and no history-sized ID cache lives
//! in the owner. Out-of-band writes during ownership are not canonical changes.
use super::*;
use crate::{
    admitted_history::AdmittedCut, contract::current_preparation::ProposedTransition,
    history_index::Pages,
};

/// `false` requires the original complete event phase. `true` means that every
/// old predicate remains true under exactly unchanged semantic dependencies and
/// all new rows passed the same event predicate implementation in original order.
pub(crate) async fn validate_appended(
    pages: &mut impl Pages,
    source: &AdmittedCut,
    proposed: &ProposedTransition,
    observed: &mut crate::EventValidationWork,
) -> Result<bool> {
    if !dependencies_unchanged(source, proposed) {
        observed.dependency_fallbacks = observed.dependency_fallbacks.saturating_add(1);
        return Ok(false);
    }
    observed.prefix_reuses = observed.prefix_reuses.saturating_add(1);
    let current = &proposed.current;
    let mut validator =
        EventHistoryValidator::new(current.watermark, &current.records, &current.sequences);
    validator.previous_watermark = source.last_event_watermark();
    validator.sequences = source.current().sequences.clone();
    for event in &proposed.events {
        observed.rows_examined = observed.rows_examined.saturating_add(1);
        // Preserve the identity block before scope/sequence checks and before
        // the historical lookup, including duplicate IDs within this proposal.
        if event.version != 1
            || event.watermark > current.watermark
            || event.watermark < validator.previous_watermark
            || validator.event_ids.contains(&event.event.id)
        {
            return Err(Error::Corruption("event identity"));
        }
        // Exact authenticated lookup; unavailable is never interpreted as absent.
        // Source admission proves index completeness. Any returned envelope is
        // fully authenticated by the existing Catalog API before we use it.
        if source
            .catalog()
            .event(pages, &event.event.id)
            .await?
            .is_some()
        {
            return Err(Error::Corruption("event identity"));
        }
        validator.extend(std::iter::once(Ok(event)))?;
    }
    validator.finish()?;
    Ok(true)
}

fn dependencies_unchanged(source: &AdmittedCut, proposed: &ProposedTransition) -> bool {
    proposed.touched.iter().all(|key| {
        let Some(before) = source.current().records.get(key) else {
            // Full source validation established every old reference existed.
            return true;
        };
        if !matches!(
            before.collection,
            Collection::Session | Collection::Task | Collection::Artifact
        ) {
            return true;
        }
        let Some(after) = proposed.current.records.get(key) else {
            return false;
        };
        if before.collection != after.collection || before.workspace != after.workspace {
            return false;
        }
        match before.collection {
            Collection::Session => true, // Event predicates read only Record.workspace.
            Collection::Task => match (before.decode::<Task>(), after.decode::<Task>()) {
                (Ok(a), Ok(b)) => a.scope.session == b.scope.session,
                _ => false,
            },
            Collection::Artifact => match (
                before.decode::<ArtifactDescriptor>(),
                after.decode::<ArtifactDescriptor>(),
            ) {
                (Ok(a), Ok(b)) => {
                    a.spec.scope.session == b.spec.scope.session
                        && a.spec.scope.task == b.spec.scope.task
                }
                _ => false,
            },
            _ => unreachable!(),
        }
    })
}
