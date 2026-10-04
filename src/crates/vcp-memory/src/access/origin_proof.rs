// SPDX-License-Identifier: Apache-2.0
//! Historical source scopes, separate from current-record authority checks.
use super::{Access, Error, Result};
use std::collections::BTreeMap;
use vcp_domain::{ids::*, memory::Version};
use vcp_store::contract::{CanonicalStore, State};

pub(super) trait OriginProof {
    fn check(&mut self, access: &Access, ids: &[EventId], required: bool) -> Result<()>;
}

pub(super) struct StateOrigins<'a>(pub &'a State);

impl OriginProof for StateOrigins<'_> {
    fn check(&mut self, access: &Access, ids: &[EventId], required: bool) -> Result<()> {
        if required {
            // Preserve the original first-match rule for explicit redacted provenance.
            for id in ids {
                let event = self
                    .0
                    .events
                    .iter()
                    .find(|row| &row.event.id == id)
                    .ok_or(Error::Access)?;
                allowed(access, &event.event.workspace, event.event.task.as_ref())?;
            }
        } else {
            // Ordinary missing origins are availability facts. Every known match
            // is checked, including duplicates in a reference State fixture.
            for event in self
                .0
                .events
                .iter()
                .filter(|row| ids.contains(&row.event.id))
            {
                allowed(access, &event.event.workspace, event.event.task.as_ref())?;
            }
        }
        Ok(())
    }
}

fn allowed(access: &Access, workspace: &WorkspaceId, task: Option<&TaskId>) -> Result<()> {
    if workspace != &access.workspace || task.is_some_and(|task| !access.allows_task(task)) {
        return Err(Error::Access);
    }
    Ok(())
}

#[derive(Default)]
struct ResolvedOrigins {
    // Only scope metadata for identities actually requested by the current
    // version graph. No event payload or history-wide map is retained.
    scopes: BTreeMap<EventId, Option<(WorkspaceId, Option<TaskId>)>>,
    needed: Vec<EventId>,
}

impl OriginProof for ResolvedOrigins {
    fn check(&mut self, access: &Access, ids: &[EventId], required: bool) -> Result<()> {
        self.needed = ids
            .iter()
            .filter(|id| !self.scopes.contains_key(*id))
            .cloned()
            .collect();
        if !self.needed.is_empty() {
            // Suspension is never authorization. The async owner consumes the
            // typed needed list, resolves it, then repeats this same evaluator.
            return Err(Error::Conflict("origin scope proof pending"));
        }
        for id in ids {
            match self.scopes.get(id).and_then(Option::as_ref) {
                Some((workspace, task)) => allowed(access, workspace, task.as_ref())?,
                None if required => return Err(Error::Access),
                None => {}
            }
        }
        Ok(())
    }
}

pub(crate) async fn version_scope_store<S: CanonicalStore>(
    store: &S,
    access: &Access,
    version: &Version,
    check: &dyn Fn() -> Result<()>,
) -> Result<()> {
    let watermark = store.current().watermark;
    let mut history = ResolvedOrigins::default();
    loop {
        check()?;
        history.needed.clear();
        let result =
            super::version_scope_with_history(store.current(), access, version, &mut history);
        if history.needed.is_empty() {
            if store.current().watermark != watermark {
                return Err(Error::Conflict("origin scope owner changed"));
            }
            return result;
        }
        // A needed list can only be emitted together with a rejected proof.
        if result.is_ok() {
            return Err(Error::Invalid("incomplete origin proof accepted".into()));
        }
        for id in std::mem::take(&mut history.needed) {
            check()?;
            let event = store.history_event(&id).await?;
            check()?;
            if event
                .as_ref()
                .is_some_and(|row| row.event.id != id || row.watermark > watermark)
            {
                return Err(Error::Invalid(
                    "origin history identity or watermark".into(),
                ));
            }
            history
                .scopes
                .insert(id, event.map(|row| (row.event.workspace, row.event.task)));
        }
    }
}
