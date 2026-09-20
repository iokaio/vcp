// SPDX-License-Identifier: Apache-2.0
//! Repair disposable heads from immutable versions and retry results.
use crate::{
    access::{self, Access},
    Error, Result,
};
use std::collections::BTreeMap;
use vcp_domain::{ids::*, memory::*, revision::*};
use vcp_protocol::event::{EventInput, EventKind};
use vcp_store::{contract::*, Store};

/// Rebuild under the existing exclusive owner. This allocates neither a memory
/// sequence nor a proposal result and never replays an external effect.
pub async fn rebuild(
    store: &mut Store,
    access: &Access,
    now: Timestamp,
) -> Result<Option<Receipt>> {
    access::authorize(store.state(), access, true)?;
    let state = store.state();
    let mut versions = Vec::<Version>::new();
    let mut results = Vec::<ProposalResult>::new();
    for record in state
        .records
        .values()
        .filter(|r| r.workspace == access.workspace)
    {
        match record.value.get("document_type").and_then(|v| v.as_str()) {
            Some("vcp_memory_proposal_v1") if record.collection == Collection::Claim => {
                let proposal: ProposalRecord = record.decode()?;
                if !access.allows_task(&proposal.scope.task) {
                    return Err(Error::Access);
                }
                // A rejected proposal can name a missing origin. Existing
                // origins still cannot expose a hidden task during repair.
                for event in state
                    .events
                    .iter()
                    .filter(|event| proposal.proposal.origins.contains(&event.event.id))
                {
                    if event.event.workspace != access.workspace
                        || event
                            .event
                            .task
                            .as_ref()
                            .is_some_and(|task| !access.allows_task(task))
                    {
                        return Err(Error::Access);
                    }
                }
            }
            Some("vcp_memory_version_v1") if record.collection == Collection::Claim => {
                let version: Version = record.decode()?;
                if !access.allows_task(&version.scope.task) {
                    return Err(Error::Access);
                }
                versions.push(version);
            }
            Some("vcp_memory_result_v1") if record.collection == Collection::Projection => {
                let result: ProposalResult = record.decode()?;
                if !access.allows_task(&result.scope.task) {
                    return Err(Error::Access);
                }
                results.push(result);
            }
            _ => (),
        }
    }
    results.sort_by(|a, b| (a.memory_seq, &a.id).cmp(&(b.memory_seq, &b.id)));
    versions.sort_by(|a, b| (a.memory_seq, &a.id).cmp(&(b.memory_seq, &b.id)));
    let maximum = results
        .last()
        .map_or(MemorySeq::ZERO, |result| result.memory_seq);
    let mut heads = BTreeMap::<ClaimId, Head>::new();
    for version in &versions {
        if !results.iter().any(|result| {
            result.version.as_ref() == Some(&version.id)
                && result.memory_seq == version.memory_seq
                && result.proposal == version.proposal.id
        }) {
            return Err(Error::Conflict(
                "memory version has no authoritative retry result",
            ));
        }
        let head = heads
            .entry(version.proposal.claim.clone())
            .or_insert_with(|| Head {
                document_type: DocumentType::Head,
                schema_version: 1,
                id: version.proposal.claim.clone(),
                scope: version.scope.clone(),
                revision: Revision::ZERO,
                current: None,
                disputed: vec![],
            });
        match version.resolution.outcome {
            Outcome::Accepted => head.current = Some(version.id.clone()),
            Outcome::Disputed => head.disputed.push(version.id.clone()),
            _ => return Err(Error::Conflict("immutable version has no claim outcome")),
        }
    }
    for record in state.records.values().filter(|r| {
        r.workspace == access.workspace
            && r.collection == Collection::Projection
            && r.value["document_type"] == "vcp_memory_head_v1"
    }) {
        let head: Head = record.decode()?;
        if !access.allows_task(&head.scope.task) {
            return Err(Error::Access);
        }
        if !heads.contains_key(&head.id) {
            return Err(Error::Conflict(
                "memory head has no immutable version source",
            ));
        }
    }
    let mut mutations = Vec::new();
    for (_, mut head) in heads {
        let prior = state
            .records
            .get(&key(Collection::Projection, head.id.as_str()));
        let expected = if let Some(row) = prior {
            if row.workspace != access.workspace {
                return Err(Error::Access);
            }
            let current: Head = row.decode()?;
            head.revision = current.revision;
            if head == current {
                continue;
            }
            head.revision = current.revision.next()?;
            Some(current.revision)
        } else {
            None
        };
        mutations.push(Mutation::Put {
            expected,
            record: Record::typed(
                Collection::Projection,
                head.id.as_str(),
                access.workspace.clone(),
                head.revision,
                &head,
            )?,
        });
    }
    let prior = state
        .records
        .get(&key(Collection::Projection, access.workspace.as_str()));
    let mut sequence = MemoryHead {
        document_type: DocumentType::Sequence,
        schema_version: 1,
        id: access.workspace.clone(),
        workspace: access.workspace.clone(),
        revision: Revision::ZERO,
        sequence: maximum,
    };
    let mut write_sequence = !results.is_empty();
    let expected = if let Some(row) = prior {
        if row.workspace != access.workspace {
            return Err(Error::Access);
        }
        let current: MemoryHead = row.decode()?;
        if current.sequence > maximum {
            return Err(Error::Conflict(
                "memory sequence exceeds authoritative retry results",
            ));
        }
        if current.sequence == maximum {
            write_sequence = false;
        } else {
            sequence.revision = current.revision.next()?;
        }
        Some(current.revision)
    } else {
        None
    };
    if write_sequence {
        mutations.push(Mutation::Put {
            expected,
            record: Record::typed(
                Collection::Projection,
                sequence.id.as_str(),
                access.workspace.clone(),
                sequence.revision,
                &sequence,
            )?,
        });
    }
    if mutations.is_empty() {
        return Ok(None);
    }
    let scope = &results
        .first()
        .ok_or(Error::Conflict("memory projection source unavailable"))?
        .scope;
    let facts: Vec<_> = mutations
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::Put { record, .. } => Some(record),
            _ => None,
        })
        .collect();
    let event = EventInput {
        id: EventId::new(),
        workspace: access.workspace.clone(),
        session: scope.session.clone(),
        task: Some(scope.task.clone()),
        actor: access.actor.clone(),
        correlation: CommandId::new(),
        causation: None,
        timestamp: now,
        kind: EventKind::Diagnostic,
        artifacts: vec![],
        data: serde_json::json!({"schema_version":1,"diagnostic":"memory_projections_rebuilt","memory_sequence":maximum,"facts":facts}),
        metadata: None,
    };
    let transaction = Transaction {
        id: TransactionId::new(),
        expected_watermark: state.watermark,
        mutations,
        events: vec![event],
        command: None,
    };
    Ok(Some(store.transact(transaction).await?))
}
