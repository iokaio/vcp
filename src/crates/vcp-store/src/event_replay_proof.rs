// SPDX-License-Identifier: Apache-2.0
//! TEST ONLY: process-local inductive event-phase evidence. This does not
//! authorize skipping physical cold-open validation or persist any trust.
use super::*;
use crate::{
    admitted_history::AdmittedCut, contract::current_preparation::ProposedTransition,
    history_index::Pages,
};

#[derive(Default)]
pub(crate) struct EventReplayProof {
    source: Option<String>,
    ids: BTreeSet<EventId>,
    sequences: BTreeMap<SessionId, SessionSeq>,
    previous_watermark: Watermark,
    pending: Option<Pending>,
    pub(crate) full_rows: u64,
    pub(crate) incremental_rows: u64,
    pub(crate) full_passes: u64,
    pub(crate) incremental_passes: u64,
}
struct Pending {
    source: String,
    current_digest: String,
    prior_count: u64,
    ids: BTreeSet<EventId>,
    sequences: BTreeMap<SessionId, SessionSeq>,
    previous_watermark: Watermark,
    replace: bool,
}
impl EventReplayProof {
    pub(crate) fn discard_pending(&mut self) {
        self.pending = None;
    }
    pub(crate) fn source_identity(&self) -> Option<&str> {
        self.source.as_deref()
    }
    pub(crate) async fn validate(
        &mut self,
        pages: &mut impl Pages,
        source: &AdmittedCut,
        proposed: &ProposedTransition,
    ) -> Result<()> {
        self.pending = None;
        let incremental = self.source.as_deref() == Some(source.identity())
            && self.ids.len() as u64 == source.catalog().event_count()
            && self.sequences == source.current().sequences
            && dependencies_unchanged(source, proposed);
        let current = &proposed.current;
        let mut validator =
            EventHistoryValidator::new(current.watermark, &current.records, &current.sequences);
        if incremental {
            self.incremental_passes += 1;
            validator.previous_watermark = self.previous_watermark;
            validator.sequences = self.sequences.clone();
        } else {
            self.full_passes += 1;
            let mut next = 0u64;
            while next < source.catalog().event_count() {
                let rows = source
                    .catalog()
                    .event_page(pages, next.checked_sub(1), 64)
                    .await?;
                if rows.is_empty() {
                    return Err(Error::Corruption("candidate history ended early"));
                }
                next += rows.len() as u64;
                self.full_rows += rows.len() as u64;
                validator.extend(rows.iter().map(Ok))?;
            }
        }
        for event in &proposed.events {
            if incremental {
                self.incremental_rows += 1;
                // Prior IDs were validated in the exact source cut. This check
                // has the same first-error class as the envelope identity block.
                if self.ids.contains(&event.event.id) {
                    return Err(Error::Corruption("event identity"));
                }
            } else {
                self.full_rows += 1;
            }
            validator.extend(std::iter::once(Ok(event)))?;
        }
        // Only new IDs are copied in the incremental case, never the complete
        // prefix set. Full fallback already visits and retains all IDs.
        let ids = validator.event_ids.clone();
        let sequences = validator.sequences.clone();
        let previous_watermark = validator.previous_watermark;
        validator.finish()?;
        self.pending = Some(Pending {
            source: source.identity().to_owned(),
            current_digest: current.projection_digest()?,
            prior_count: source.catalog().event_count(),
            ids,
            sequences,
            previous_watermark,
            replace: !incremental,
        });
        Ok(())
    }
    /// The test driver calls this only after exact receipt, all other phases,
    /// ComparePages advancement, and complete publication verification succeed.
    pub(crate) fn install(&mut self, prior: &AdmittedCut, next: &AdmittedCut) -> Result<()> {
        let pending = self
            .pending
            .take()
            .ok_or(Error::Conflict("no pending event proof"))?;
        let count = if pending.replace {
            pending.ids.len() as u64
        } else {
            pending.prior_count + pending.ids.len() as u64
        };
        if pending.source != prior.identity()
            || pending.current_digest != next.current().projection_digest()?
            || next.catalog().event_count() != count
            || next.current().watermark != prior.current().watermark.next()?
        {
            return Err(Error::Conflict("event proof publication differs"));
        }
        if pending.replace {
            self.ids = pending.ids;
        } else {
            self.ids.extend(pending.ids);
        }
        self.sequences = pending.sequences;
        self.previous_watermark = pending.previous_watermark;
        self.source = Some(next.identity().to_owned());
        Ok(())
    }
}

fn dependencies_unchanged(source: &AdmittedCut, proposed: &ProposedTransition) -> bool {
    proposed.touched.iter().all(|key| {
        let Some(before) = source.current().records.get(key) else {
            // Every prior reference already existed in the valid source.
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
            Collection::Session => true,
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

#[path = "event_replay_proof_tests.rs"]
mod tests;
