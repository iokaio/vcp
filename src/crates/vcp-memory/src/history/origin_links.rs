// SPDX-License-Identifier: Apache-2.0
//! Live navigation retains requested scope/mask facts, never historical payloads.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use vcp_audit::history::RetentionMask;
use vcp_domain::memory::Proposal;
use vcp_protocol::event::EventKind;
use vcp_store::contract::CanonicalStore;

pub(super) trait MaskProof {
    fn masked(
        &mut self,
        ordinal: usize,
        mask: &RetentionMask,
        proposal: &Proposal,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<bool>;
}

pub(super) struct StateMasks<'a>(pub &'a State);
impl MaskProof for StateMasks<'_> {
    fn masked(
        &mut self,
        _: usize,
        mask: &RetentionMask,
        proposal: &Proposal,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<bool> {
        for row in &self.0.events {
            check()?;
            if row.event.workspace == mask.workspace
                && row.event.session == mask.session
                && row.sequence >= mask.first
                && row.sequence <= mask.last
                && (proposal.origins.contains(&row.event.id)
                    || (row.event.correlation == proposal.command
                        && row.event.kind == EventKind::MemoryResolved))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[derive(Default)]
struct MaskFacts {
    ready: bool,
    needed: bool,
    origins: BTreeMap<EventId, usize>,
    commands: BTreeMap<CommandId, usize>,
}
impl MaskProof for MaskFacts {
    fn masked(
        &mut self,
        ordinal: usize,
        _: &RetentionMask,
        proposal: &Proposal,
        _: &dyn Fn() -> Result<()>,
    ) -> Result<bool> {
        if !self.ready {
            self.needed = true;
            return Err(Error::Conflict("origin retention proof pending"));
        }
        Ok(proposal
            .origins
            .iter()
            .any(|id| self.origins.get(id) == Some(&ordinal))
            || self.commands.get(&proposal.command) == Some(&ordinal))
    }
}

fn candidate(
    row: &vcp_store::contract::Record,
    access: &Access,
    origins: &BTreeSet<EventId>,
) -> bool {
    row.workspace == access.workspace
        && row.collection == Collection::Claim
        && row.value["document_type"] == "vcp_memory_version_v1"
        && row.value["proposal"]["origins"]
            .as_array()
            .is_some_and(|ids| {
                ids.iter().any(|id| {
                    id.as_str()
                        .is_some_and(|id| origins.iter().any(|origin| origin.as_str() == id))
                })
            })
}

impl MaskFacts {
    async fn resolve<S: CanonicalStore>(
        &mut self,
        store: &S,
        access: &Access,
        origins: &BTreeSet<EventId>,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        let current = store.current();
        let watermark = current.watermark;
        let mut wanted_origins = BTreeSet::new();
        let mut wanted_commands = BTreeSet::new();
        // Dependency discovery only: malformed later rows must still be reported
        // at their original position, not before the 128-link early-return fence.
        for row in current.records.values() {
            check()?;
            if !candidate(row, access, origins) {
                continue;
            }
            if let Some(ids) = row.value["proposal"]["origins"].as_array() {
                for id in ids.iter().filter_map(|id| id.as_str()) {
                    if let Ok(id) = EventId::parse(id) {
                        wanted_origins.insert(id);
                    }
                }
            }
            if let Some(id) = row.value["proposal"]["command"].as_str() {
                if let Ok(id) = CommandId::parse(id) {
                    wanted_commands.insert(id);
                }
            }
        }
        let mut masks = Vec::new();
        for (ordinal, row) in current.records.values().enumerate() {
            check()?;
            if row.collection == Collection::Tombstone && row.workspace == access.workspace {
                // The shared removal evaluator remains the authority for schema,
                // workspace, range and deletion checks, in its original order.
                if let Ok(mask) = row.decode::<RetentionMask>() {
                    masks.push((ordinal, mask.session, mask.first, mask.last));
                }
            }
        }
        let end = store.history_event_count().await?;
        let mut next = 0u64;
        while next < end {
            check()?;
            let limit = usize::try_from((end - next).min(4096))
                .map_err(|_| Error::Conflict("history ordinal"))?;
            let rows = store.history_events(next.checked_sub(1), limit).await?;
            check()?;
            if rows.is_empty() || rows.len() > limit {
                return Err(Error::Invalid("incomplete origin retention history".into()));
            }
            for row in &rows {
                check()?;
                if row.watermark > watermark {
                    return Err(Error::Invalid("origin retention watermark".into()));
                }
                if row.event.workspace != access.workspace {
                    continue;
                }
                let wanted_origin = wanted_origins.contains(&row.event.id);
                let wanted_command = row.event.kind == EventKind::MemoryResolved
                    && wanted_commands.contains(&row.event.correlation);
                if !wanted_origin && !wanted_command {
                    continue;
                }
                if let Some((ordinal, _, _, _)) = masks.iter().find(|(_, session, first, last)| {
                    *session == row.event.session && row.sequence >= *first && row.sequence <= *last
                }) {
                    if wanted_origin {
                        self.origins
                            .entry(row.event.id.clone())
                            .and_modify(|old| *old = (*old).min(*ordinal))
                            .or_insert(*ordinal);
                    }
                    if wanted_command {
                        self.commands
                            .entry(row.event.correlation.clone())
                            .and_modify(|old| *old = (*old).min(*ordinal))
                            .or_insert(*ordinal);
                    }
                }
            }
            next += rows.len() as u64;
        }
        if store.current().watermark != watermark {
            return Err(Error::Conflict("origin retention owner changed"));
        }
        self.ready = true;
        self.needed = false;
        Ok(())
    }
}

/// Same navigation contract as `origin_links_with_check`, using only current
/// records, exact scoped origin facts and one bounded retention-history scan.
pub async fn origin_links_store_with_check<S: CanonicalStore>(
    store: &S,
    access: &Access,
    origins: &BTreeSet<EventId>,
    check: &dyn Fn() -> Result<()>,
) -> Result<(Vec<OriginLink>, bool)> {
    check()?;
    if origins.len() > 128 {
        return Err(Error::Invalid("origin navigation limit".into()));
    }
    access::authorize(store.current(), access, false)?;
    let watermark = store.current().watermark;
    let mut masks = MaskFacts::default();
    let mut links = Vec::new();
    for row in store.current().records.values() {
        check()?;
        if !candidate(row, access, origins) {
            continue;
        }
        let version: Version = row.decode()?;
        match access::version_scope_store(store, access, &version, check).await {
            Err(Error::Access) => continue,
            result => result?,
        }
        let removed = super::proposal_removed_with_history(
            store.current(),
            &access.workspace,
            &version.proposal,
            check,
            &mut masks,
        );
        let removed = if masks.needed {
            masks.resolve(store, access, origins, check).await?;
            super::proposal_removed_with_history(
                store.current(),
                &access.workspace,
                &version.proposal,
                check,
                &mut masks,
            )?
        } else {
            removed?
        };
        if removed {
            continue;
        }
        for origin in version
            .proposal
            .origins
            .iter()
            .filter(|id| origins.contains(*id))
        {
            check()?;
            if store.current().watermark != watermark {
                return Err(Error::Conflict("origin owner changed"));
            }
            if links.len() == 128 {
                return Ok((links, true));
            }
            links.push(OriginLink {
                origin: origin.clone(),
                claim: version.proposal.claim.clone(),
                version: version.id.clone(),
                memory_seq: version.memory_seq,
            });
        }
    }
    check()?;
    if store.current().watermark != watermark {
        return Err(Error::Conflict("origin owner changed"));
    }
    links.sort_by(|a, b| {
        (&a.origin, a.memory_seq, &a.version).cmp(&(&b.origin, b.memory_seq, &b.version))
    });
    Ok((links, false))
}
