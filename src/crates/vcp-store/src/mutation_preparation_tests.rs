// SPDX-License-Identifier: Apache-2.0
use super::*;
#[path = "../tests/common/mod.rs"]
mod common;

// Frozen mutation loop from the pre-extraction canonical preparation.
fn reference(
    source: &State,
    transaction: &Transaction,
    records: &mut SharedStateValue<BTreeMap<String, Record>>,
) -> Result<BTreeSet<String>> {
    let mut touched = BTreeSet::new();
    for mutation in &transaction.mutations {
        match mutation {
            Mutation::Put { expected, record } => {
                let late_accounting = crate::accounting_contract::redacted_attempt_update(
                    source,
                    transaction,
                    record,
                )?;
                if crate::redaction_contract::kind(record)?.is_some()
                    || (matches!(
                        record.collection,
                        Collection::Task
                            | Collection::Turn
                            | Collection::Effect
                            | Collection::Verification
                            | Collection::Attempt
                            | Collection::Settlement
                    ) && record.value.get("redaction").is_some_and(|v| !v.is_null())
                        && !late_accounting)
                    || (record.collection == Collection::Attempt
                        && !late_accounting
                        && record
                            .value
                            .get("redacted_at_revision")
                            .is_some_and(|value| !value.is_null()))
                    || (record.collection == Collection::Settlement
                        && record
                            .value
                            .get("observation_digest")
                            .is_some_and(|v| !v.is_null()))
                    || (record.collection == Collection::Artifact
                        && record.decode::<ArtifactDescriptor>()?.state
                            == vcp_domain::artifact::CaptureState::Purged)
                {
                    return Err(Error::Conflict("redaction requires sealed rewrite"));
                }
                let key = record.key();
                if !touched.insert(key.clone()) {
                    return Err(Error::Conflict("duplicate mutation"));
                }
                match (source.records.get(&key), expected) {
                    (None, None) if record.revision == Revision::ZERO => {
                        if record.controller_lease()? {
                            record
                                .decode::<vcp_domain::controller::Lease>()?
                                .validate()?;
                        }
                        ingestion_contract::insert(record)?;
                        crate::snapshot_jobs::insert(&source, record)?;
                    }
                    (Some(previous), Some(expected))
                        if previous.revision == *expected
                            && record.revision == expected.next()?
                            && previous.workspace == record.workspace =>
                    {
                        if previous.controller_lease()? != record.controller_lease()? {
                            return Err(Error::Conflict("controller lease document type changed"));
                        }
                        if previous.controller_lease()? {
                            previous
                                .decode::<vcp_domain::controller::Lease>()?
                                .validate_transition(&record.decode()?)?;
                        }
                        if matches!(
                            previous.collection,
                            Collection::Task
                                | Collection::Turn
                                | Collection::Effect
                                | Collection::Verification
                                | Collection::Attempt
                                | Collection::Settlement
                        ) && previous
                            .value
                            .get("redaction")
                            .is_some_and(|value| !value.is_null())
                            && !late_accounting
                        {
                            return Err(Error::Conflict("redacted evidence cannot be replaced"));
                        }
                        crate::accounting_contract::transition(previous, record)?;
                        ingestion_contract::transition(previous, record)?;
                        search_contract::transition(previous, record)?;
                        agents_contract::transition(previous, record)?;
                        crate::editor_contract::transition(previous, record)?;
                        crate::snapshot_jobs::transition(previous, record)?;
                        if previous.immutable_memory()? {
                            return Err(Error::Conflict("immutable memory evidence"));
                        }
                        if previous.memory_kind()? != record.memory_kind()? {
                            return Err(Error::Conflict("memory document type changed"));
                        }
                        if previous.memory_kind()? == Some("vcp_memory_sequence_v1") {
                            let before: vcp_domain::memory::MemoryHead = previous.decode()?;
                            let after: vcp_domain::memory::MemoryHead = record.decode()?;
                            if after.sequence <= before.sequence {
                                return Err(Error::Conflict("memory sequence must advance"));
                            }
                        }
                        if previous.memory_kind()? == Some("vcp_memory_index_intent_v1") {
                            let mut before: vcp_domain::memory::IndexIntent = previous.decode()?;
                            let after: vcp_domain::memory::IndexIntent = record.decode()?;
                            before.revision = after.revision;
                            before.status = after.status;
                            if before != after {
                                return Err(Error::Conflict(
                                    "memory index intent identity changed",
                                ));
                            }
                        }
                        if matches!(
                            record.collection,
                            Collection::Verification
                                | Collection::Settlement
                                | Collection::LocalResources
                        ) {
                            return Err(Error::Conflict("immutable evidence record"));
                        }
                        if record.collection == Collection::Artifact {
                            let prior: ArtifactDescriptor = previous.decode()?;
                            let next: ArtifactDescriptor = record.decode()?;
                            if prior.state != vcp_domain::artifact::CaptureState::Pending
                                || next.length < prior.length
                                || prior.spec.id != next.spec.id
                            {
                                return Err(Error::Conflict("immutable or regressing artifact"));
                            }
                        }
                    }
                    _ => return Err(Error::Conflict("entity revision or identity")),
                }
                records.insert(key, record.clone());
            }
            Mutation::DropProjection { id, expected } => {
                let key = key(Collection::Projection, id);
                if source
                    .records
                    .get(&key)
                    .map(crate::editor_contract::kind)
                    .transpose()?
                    .unwrap_or(false)
                {
                    return Err(Error::Conflict(
                        "durable editor authority cannot be dropped",
                    ));
                }
                if source
                    .records
                    .get(&key)
                    .map(agents_contract::kind)
                    .transpose()?
                    .unwrap_or(false)
                {
                    return Err(Error::Conflict("durable graph cannot be dropped"));
                }
                if source
                    .records
                    .get(&key)
                    .map(Record::immutable_memory)
                    .transpose()?
                    .unwrap_or(false)
                {
                    return Err(Error::Conflict("immutable memory evidence"));
                }
                if !touched.insert(key.clone())
                    || source
                        .records
                        .get(&key)
                        .is_none_or(|r| r.revision != *expected)
                {
                    return Err(Error::Conflict("projection revision"));
                }
                records.remove(&key);
            }
        }
    }
    Ok(touched)
}

fn equivalent(source: &State, transaction: &Transaction) {
    let mut expected = source.records.clone();
    let expected_result =
        reference(source, transaction, &mut expected).map_err(|error| error.to_string());
    let mut actual = source.records.clone();
    let actual_result = prepare(source.into(), transaction, &mut actual, &mut || {
        crate::legacy_state_stream::digest(source)
    })
    .map_err(|error| error.to_string());
    assert_eq!(actual_result, expected_result);
    assert_eq!(actual, expected);
}

#[test]
fn current_mutation_preparation_matches_frozen_reference_for_generated_revisions_and_rejections() {
    equivalent(&State::default(), &common::initial());
    let (state, _) = State::default().prepare(&common::initial()).unwrap();
    for original in state.records.values() {
        for expected in [None, Some(Revision::ZERO), Some(Revision::new(1))] {
            for revision in [Revision::ZERO, Revision::new(1), Revision::new(2)] {
                for foreign in [false, true] {
                    for redacted in [false, true] {
                        let mut record = original.clone();
                        record.revision = revision;
                        if foreign {
                            record.workspace = WorkspaceId::parse("foreign").unwrap();
                        }
                        if redacted {
                            record.value["redaction"] = serde_json::json!({"invalid":"attempt"});
                        }
                        let mut transaction = common::initial();
                        transaction.mutations = vec![Mutation::Put { expected, record }];
                        equivalent(&state, &transaction);
                        transaction.mutations.push(transaction.mutations[0].clone());
                        equivalent(&state, &transaction);
                    }
                }
            }
        }
    }
    for document in [
        "fixture",
        "vcp_memory_index_intent_v1",
        "vcp_editor_buffer_v1",
        "vcp_agent_graph_v1",
    ] {
        let mut source = state.clone();
        let record = Record {
            collection: Collection::Projection,
            id: "candidate".into(),
            workspace: common::workspace().id,
            revision: Revision::ZERO,
            value: serde_json::json!({"document_type":document}),
            references: BTreeSet::new(),
        };
        source.records.insert(record.key(), record.clone());
        for id in ["candidate", "absent"] {
            for expected in [Revision::ZERO, Revision::new(1)] {
                let mut transaction = common::initial();
                transaction.mutations = vec![Mutation::DropProjection {
                    id: id.into(),
                    expected,
                }];
                equivalent(&source, &transaction);
            }
        }
    }
}

#[test]
fn current_mutations_do_not_request_historical_digest_without_snapshot_pin() {
    let state = State::default();
    let mut records = state.records.clone();
    let mut digest_requested = false;
    prepare(
        (&state).into(),
        &common::initial(),
        &mut records,
        &mut || {
            digest_requested = true;
            Err(Error::Unavailable("history deliberately unavailable"))
        },
    )
    .unwrap();
    assert!(!digest_requested);
}
