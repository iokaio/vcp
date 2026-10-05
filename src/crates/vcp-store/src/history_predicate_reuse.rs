// SPDX-License-Identifier: Apache-2.0
//! Exact current dependencies of the admitted redaction/ingestion predicates.
//!
//! EE-02b / execution-history-layout3.md admits immutable logical prefixes,
//! not persisted validation shortcuts. These proofs borrow one admitted cut;
//! reopen/rewrite must admit that cut again. Every original transition still
//! runs the ordered preparation pipeline. Uncertain dependencies use its full
//! reference pass. No historical payload or identity set is retained here.
use crate::{
    admitted_history::AdmittedCut,
    contract::{current_preparation::ProposedTransition, Collection},
};
use vcp_domain::{task::Task, workspace::Workspace};

pub(crate) fn redaction_unchanged(source: &AdmittedCut, proposed: &ProposedTransition) -> bool {
    proposed.touched.iter().all(|key| {
        let Some(before) = source.current().records.get(key) else {
            // A successfully validated old redacted envelope cannot depend on
            // a Workspace record which did not exist in the admitted source.
            return true;
        };
        if before.collection != Collection::Workspace {
            return true;
        }
        let Some(after) = proposed.current.records.get(key) else {
            return false;
        };
        if before.collection != after.collection || before.workspace != after.workspace {
            return false;
        }
        match (before.decode::<Workspace>(), after.decode::<Workspace>()) {
            (Ok(a), Ok(b)) => a.deletion == b.deletion,
            _ => false,
        }
    })
}

pub(crate) fn ingestion_unchanged(source: &AdmittedCut, proposed: &ProposedTransition) -> bool {
    proposed.touched.iter().all(|key| {
        let before = source.current().records.get(key);
        let after = proposed.current.records.get(key);
        // This deliberately conservative boundary covers cursor/job identity,
        // membership, progress and outputs, including result -> proposal
        // provenance. Additions and removals also invalidate the proof.
        if before.into_iter().chain(after).any(|record| {
            matches!(
                record.collection,
                Collection::Claim | Collection::Projection
            )
        }) {
            return false;
        }
        let Some(before) = before else {
            return true;
        };
        if before.collection != Collection::Task {
            return true;
        }
        let Some(after) = after else {
            return false;
        };
        if before.collection != after.collection || before.workspace != after.workspace {
            return false;
        }
        match (before.decode::<Task>(), after.decode::<Task>()) {
            (Ok(a), Ok(b)) => a.scope == b.scope && a.root == b.root && a.parent == b.parent,
            _ => false,
        }
    })
}
