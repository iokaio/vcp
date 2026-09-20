// SPDX-License-Identifier: Apache-2.0
//! Canonical publication invariants. Reopen/checksums of derivative files are
//! validated by the publishing adapter; these checks protect the durable commit.
use super::*;
use vcp_domain::{
    memory::{IndexIntent, IndexStatus, ProposalResult},
    search::{Active, Generation, ACTIVE, GENERATION},
    workspace::Scope,
};

pub(super) fn kind(record: &Record) -> Result<Option<&str>> {
    let Some(tag) = record.value["document_type"].as_str() else {
        return Ok(None);
    };
    match tag {
        GENERATION | ACTIVE if record.collection == Collection::Generation => Ok(Some(tag)),
        _ if tag.starts_with("vcp_search_") => {
            Err(Error::Corruption("search document type or collection"))
        }
        _ => Ok(None),
    }
}
pub(super) fn shape(record: &Record) -> Result<()> {
    match kind(record)? {
        Some(GENERATION) => {
            let value: Generation = record.decode()?;
            value.validate()?;
            if value.id.as_str() != record.id
                || value.scope.workspace != record.workspace
                || value.revision != record.revision
            {
                return Err(Error::Corruption("search generation identity"));
            }
        }
        Some(ACTIVE) => {
            let value: Active = record.decode()?;
            value.validate()?;
            if value.id.as_str() != record.id
                || value.workspace != record.workspace
                || value.revision != record.revision
            {
                return Err(Error::Corruption("search active identity"));
            }
        }
        _ => return Err(Error::Corruption("search document expected")),
    }
    Ok(())
}
pub(super) fn scope(record: &Record) -> Result<Option<Scope>> {
    Ok(match kind(record)? {
        Some(GENERATION) => Some(record.decode::<Generation>()?.scope),
        Some(ACTIVE) => None,
        _ => return Err(Error::Corruption("search document expected")),
    })
}
pub(super) fn references(record: &Record) -> Result<BTreeSet<String>> {
    let mut refs = BTreeSet::new();
    match kind(record)? {
        Some(GENERATION) => {
            let value: Generation = record.decode()?;
            refs.insert(key(Collection::Task, value.scope.task.as_str()));
            if let Some(previous) = value.previous {
                refs.insert(key(Collection::Generation, previous.as_str()));
            }
            refs.extend(
                value
                    .covered_intents
                    .iter()
                    .map(|id| key(Collection::IndexIntent, id.as_str())),
            );
        }
        Some(ACTIVE) => {
            refs.insert(key(
                Collection::Generation,
                record.decode::<Active>()?.generation.as_str(),
            ));
        }
        _ => return Err(Error::Corruption("search document expected")),
    }
    Ok(refs)
}
pub(super) fn transition(previous: &Record, next: &Record) -> Result<()> {
    if kind(previous)? != kind(next)? {
        return Err(Error::Conflict("search document type changed"));
    }
    if kind(previous)? == Some(GENERATION) {
        return Err(Error::Conflict("immutable search generation"));
    }
    if kind(previous)? == Some(ACTIVE) {
        let before: Active = previous.decode()?;
        let after: Active = next.decode()?;
        if before.generation == after.generation || before.transaction == after.transaction {
            return Err(Error::Conflict("search publication must advance"));
        }
    }
    Ok(())
}
fn generation(state: &State, workspace: &WorkspaceId, id: &GenerationId) -> Result<Generation> {
    let record = state.record(Collection::Generation, id.as_str(), workspace)?;
    if kind(record)? != Some(GENERATION) {
        return Err(Error::Corruption("search generation reference type"));
    }
    record.decode()
}
fn active(state: &State, workspace: &WorkspaceId) -> Result<Option<Active>> {
    state
        .records
        .get(&key(Collection::Generation, workspace.as_str()))
        .map(|record| {
            if kind(record)? != Some(ACTIVE) || record.workspace != *workspace {
                return Err(Error::Corruption("search active reference type"));
            }
            record.decode()
        })
        .transpose()
}
fn intents(state: &State, manifest: &Generation) -> Result<BTreeMap<IndexIntentId, IndexIntent>> {
    let mut values = BTreeMap::new();
    for record in state.records.values().filter(|record| {
        record.workspace == manifest.scope.workspace
            && record.collection == Collection::IndexIntent
            && record.value["document_type"] == "vcp_memory_index_intent_v1"
    }) {
        let value: IndexIntent = record.decode()?;
        if value.canonical_watermark <= manifest.canonical_watermark {
            values.insert(value.id.clone(), value);
        }
    }
    Ok(values)
}
fn covered_sequence(state: &State, manifest: &Generation) -> Result<MemorySeq> {
    let mut sequence = MemorySeq::ZERO;
    for row in state.records.values().filter(|row| {
        row.workspace == manifest.scope.workspace
            && row.collection == Collection::Projection
            && matches!(
                row.value["document_type"].as_str(),
                Some("vcp_memory_result_v1" | vcp_domain::redaction::RESULT)
            )
    }) {
        let (transaction, memory_seq) =
            if row.value["document_type"] == vcp_domain::redaction::RESULT {
                let value: vcp_domain::redaction::RedactedResult = row.decode()?;
                (value.transaction, value.memory_seq)
            } else {
                let value: ProposalResult = row.decode()?;
                (value.transaction, value.memory_seq)
            };
        let watermark = state
            .transactions
            .get(&transaction)
            .ok_or(Error::Corruption("memory result receipt missing"))?
            .watermark;
        if watermark <= manifest.canonical_watermark {
            sequence = sequence.max(memory_seq);
        }
    }
    Ok(sequence)
}
fn coverage(state: &State, manifest: &Generation) -> Result<BTreeMap<IndexIntentId, IndexIntent>> {
    if !manifest.empty_complete
        && (manifest.vector_checksum.is_none() || !manifest.vector_deficits.is_empty())
    {
        if !manifest.covered_intents.is_empty() || manifest.memory_seq != MemorySeq::ZERO {
            return Err(Error::Corruption(
                "incomplete hybrid generation cannot acknowledge coverage",
            ));
        }
        return Ok(BTreeMap::new());
    }
    let expected = intents(state, manifest)?;
    if expected.keys().collect::<BTreeSet<_>>()
        != manifest.covered_intents.iter().collect::<BTreeSet<_>>()
        || manifest.memory_seq != covered_sequence(state, manifest)?
    {
        return Err(Error::Corruption(
            "search manifest has incomplete or inflated canonical coverage",
        ));
    }
    Ok(expected)
}
pub(super) fn validate(state: &State) -> Result<()> {
    for row in state.records.values() {
        match kind(row)? {
            Some(GENERATION) => {
                shape(row)?;
                let value: Generation = row.decode()?;
                let task: Task = state
                    .record(
                        Collection::Task,
                        value.scope.task.as_str(),
                        &value.scope.workspace,
                    )?
                    .decode()?;
                if task.scope != value.scope
                    || task.parent.is_some()
                    || task.root != value.scope.task
                {
                    return Err(Error::Corruption("search publication root scope"));
                }
                let receipt = state
                    .transactions
                    .get(&value.transaction)
                    .ok_or(Error::Corruption("search publication receipt missing"))?;
                if value.canonical_watermark >= receipt.watermark {
                    return Err(Error::Corruption(
                        "search snapshot exceeds publication boundary",
                    ));
                }
                if coverage(state, &value)?
                    .values()
                    .any(|intent| intent.status != IndexStatus::Ready)
                {
                    return Err(Error::Corruption(
                        "published search intent is not acknowledged",
                    ));
                }
                if let Some(previous) = &value.previous {
                    let old = generation(state, &value.scope.workspace, previous)?;
                    if old.canonical_watermark > value.canonical_watermark
                        || old.transaction == value.transaction
                    {
                        return Err(Error::Corruption("search generation history regressed"));
                    }
                }
            }
            Some(ACTIVE) => {
                shape(row)?;
                let value: Active = row.decode()?;
                let manifest = generation(state, &value.workspace, &value.generation)?;
                if manifest.transaction != value.transaction {
                    return Err(Error::Corruption(
                        "search active publication receipt mismatch",
                    ));
                }
            }
            _ => (),
        }
    }
    Ok(())
}
/// Call after mutations/receipt are assembled in State::prepare, before return.
/// Replay follows the same hook, preventing generic Put from bypassing publish.
pub(super) fn publication(before: &State, after: &State, transaction: &Transaction) -> Result<()> {
    let mut manifests = Vec::new();
    let mut pointers = Vec::new();
    let mut ready = BTreeSet::new();
    for mutation in &transaction.mutations {
        if let Mutation::Put { record, .. } = mutation {
            match kind(record)? {
                Some(GENERATION) => {
                    if before.records.contains_key(&record.key()) {
                        return Err(Error::Conflict("immutable search generation"));
                    }
                    manifests.push(record.decode::<Generation>()?);
                }
                Some(ACTIVE) => pointers.push(record.decode::<Active>()?),
                _ => (),
            }
            if record.collection == Collection::IndexIntent
                && record.value["document_type"] == "vcp_memory_index_intent_v1"
            {
                let next: IndexIntent = record.decode()?;
                let prior = before
                    .records
                    .get(&record.key())
                    .map(Record::decode::<IndexIntent>)
                    .transpose()?;
                if next.status == IndexStatus::Ready
                    && prior
                        .as_ref()
                        .is_none_or(|old| old.status != IndexStatus::Ready)
                {
                    ready.insert(next.id);
                } else if prior.is_some_and(|old| old.status == IndexStatus::Ready) {
                    return Err(Error::Conflict("acknowledged search intent is immutable"));
                }
            }
        }
    }
    if manifests.is_empty() && pointers.is_empty() && ready.is_empty() {
        return Ok(());
    }
    if manifests.len() != 1 || pointers.len() != 1 {
        return Err(Error::Conflict(
            "search publication requires one manifest and active pointer",
        ));
    }
    let manifest = &manifests[0];
    let pointer = &pointers[0];
    let workspace: Workspace = before
        .record(
            Collection::Workspace,
            manifest.scope.workspace.as_str(),
            &manifest.scope.workspace,
        )?
        .decode()?;
    let next_workspace: Workspace = after
        .record(Collection::Workspace, workspace.id.as_str(), &workspace.id)?
        .decode()?;
    if manifest.transaction != transaction.id
        || pointer.transaction != transaction.id
        || pointer.generation != manifest.id
        || pointer.workspace != workspace.id
        || manifest.authority != workspace.authority
        || manifest.deletion != workspace.deletion
        || next_workspace.authority != workspace.authority
        || next_workspace.deletion != workspace.deletion
        || manifest.canonical_watermark > before.watermark
    {
        return Err(Error::Conflict(
            "search publication scope epochs or snapshot changed",
        ));
    }
    let previous = active(before, &workspace.id)?;
    if manifest.previous != previous.as_ref().map(|old| old.generation.clone()) {
        return Err(Error::Conflict("search publication predecessor changed"));
    }
    if let Some(previous) = previous {
        let old = generation(before, &workspace.id, &previous.generation)?;
        if old.canonical_watermark > manifest.canonical_watermark {
            return Err(Error::Conflict("search publication watermark regressed"));
        }
    }
    let expected = coverage(before, manifest)?;
    let required: BTreeSet<_> = expected
        .values()
        .filter(|intent| intent.status != IndexStatus::Ready)
        .map(|intent| intent.id.clone())
        .collect();
    if ready != required {
        return Err(Error::Conflict(
            "search publication intent acknowledgements differ",
        ));
    }
    for id in required {
        let next: IndexIntent = after
            .record(Collection::IndexIntent, id.as_str(), &workspace.id)?
            .decode()?;
        if next.status != IndexStatus::Ready {
            return Err(Error::Conflict("search intent acknowledgement missing"));
        }
    }
    Ok(())
}
