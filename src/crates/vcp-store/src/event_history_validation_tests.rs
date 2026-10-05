// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::contract::{Mutation, State, Transaction};
use vcp_domain::{ArtifactId, ByteCount, Revision, TaskId, TransactionId};
#[path = "../tests/common/mod.rs"]
mod common;

// Frozen pre-extraction implementation: compare acceptance and exact error
// ordering independently of the new fallible/page-fed validator.
fn reference(state: &State) -> Result<()> {
    let mut event_ids = BTreeSet::new();
    let mut sequences = BTreeMap::<SessionId, SessionSeq>::new();
    let mut previous_watermark = Watermark::ZERO;
    for event in &state.events {
        if event.version != 1
            || event.watermark > state.watermark
            || event.watermark < previous_watermark
            || !event_ids.insert(event.event.id.clone())
        {
            return Err(Error::Corruption("event identity"));
        }
        previous_watermark = event.watermark;
        state.record(
            Collection::Session,
            event.event.session.as_str(),
            &event.event.workspace,
        )?;
        if let Some(task) = &event.event.task {
            let task: Task = state
                .record(Collection::Task, task.as_str(), &event.event.workspace)?
                .decode()?;
            if task.scope.session != event.event.session {
                return Err(Error::Access);
            }
        }
        for artifact in &event.event.artifacts {
            let artifact: ArtifactDescriptor = state
                .record(
                    Collection::Artifact,
                    artifact.as_str(),
                    &event.event.workspace,
                )?
                .decode()?;
            if artifact.spec.scope.session != event.event.session
                || event
                    .event
                    .task
                    .as_ref()
                    .is_some_and(|t| t != &artifact.spec.scope.task)
            {
                return Err(Error::Access);
            }
        }
        let expected = sequences
            .get(&event.event.session)
            .copied()
            .unwrap_or_default()
            .next()?;
        if event.sequence != expected {
            return Err(Error::Corruption("event sequence gap or duplicate"));
        }
        sequences.insert(event.event.session.clone(), expected);
    }
    if sequences != state.sequences {
        return Err(Error::Corruption("session watermark"));
    }
    Ok(())
}

fn paged(state: &State, width: usize, owned: bool) -> Result<()> {
    let mut validator =
        EventHistoryValidator::new(state.watermark, &state.records, &state.sequences);
    for page in state.events.chunks(width) {
        if owned {
            validator.extend(page.iter().cloned().map(Ok))?;
        } else {
            validator.extend(page.iter().map(Ok))?;
        }
    }
    validator.finish()
}
fn equivalent(state: &State) {
    let expected = reference(state).map_err(|error| error.to_string());
    assert_eq!(
        state
            .validate_event_history()
            .map_err(|error| error.to_string()),
        expected
    );
    for width in [1, 2, 7, 256] {
        for owned in [false, true] {
            assert_eq!(
                paged(state, width, owned).map_err(|error| error.to_string()),
                expected,
                "page width {width}, owned {owned}"
            );
        }
    }
}

#[test]
fn generated_histories_and_corruptions_match_frozen_full_reference() {
    let initial = common::initial();
    let (mut state, _) = State::default().prepare(&initial).unwrap();
    equivalent(&State::default());
    equivalent(&state);
    let corruptions: &[fn(&mut State)] = &[
        |s| s.events[0].version = 2,
        |s| s.events[0].watermark = s.watermark.next().unwrap(),
        |s| s.events[1].watermark = Watermark::ZERO,
        |s| s.events[1].event.id = s.events[0].event.id.clone(),
        |s| s.events[0].sequence = s.events[0].sequence.next().unwrap(),
        |s| s.events[0].event.workspace = WorkspaceId::new(),
        |s| s.events[0].event.session = SessionId::new(),
        |s| s.events[0].event.task = Some(TaskId::new()),
        |s| s.events[0].event.artifacts.push(ArtifactId::new()),
        |s| s.sequences.clear(),
        |s| {
            s.events.pop();
        },
        |s| {
            s.records
                .remove(&key(Collection::Session, common::session().id.as_str()));
        },
        |s| {
            s.records
                .get_mut(&key(Collection::Session, common::session().id.as_str()))
                .unwrap()
                .workspace = WorkspaceId::new();
        },
    ];
    for index in 0..24 {
        let mut event = initial.events[0].clone();
        event.id = EventId::parse(format!("generated_{index}")).unwrap();
        let transaction = Transaction {
            id: TransactionId::parse(format!("transaction_{index}")).unwrap(),
            expected_watermark: state.watermark,
            mutations: vec![],
            events: vec![event],
            command: None,
        };
        state = state.prepare(&transaction).unwrap().0;
        equivalent(&state);
        for mutate in corruptions {
            let mut corrupt = state.clone();
            mutate(&mut corrupt);
            assert!(reference(&corrupt).is_err());
            equivalent(&corrupt);
        }
    }
}

#[test]
fn current_task_and_artifact_dependencies_recheck_old_rows() {
    use vcp_domain::artifact::{CaptureState, Range};
    let mut initial = common::initial();
    let mut artifact = ArtifactDescriptor {
        spec: common::spec(),
        state: CaptureState::Pending,
        length: ByteCount::ZERO,
        sha256: vcp_protocol::digest_bytes(b""),
        retained: vec![Range {
            start: ByteCount::ZERO,
            end: ByteCount::ZERO,
        }],
    };
    initial.events[0].artifacts.push(artifact.spec.id.clone());
    initial.mutations.push(Mutation::Put {
        expected: None,
        record: Record::typed(
            Collection::Artifact,
            artifact.spec.id.to_string(),
            common::workspace().id,
            Revision::ZERO,
            &artifact,
        )
        .unwrap(),
    });
    let (state, _) = State::default().prepare(&initial).unwrap();
    equivalent(&state);
    for change_task in [false, true] {
        let mut changed = state.clone();
        let record = if change_task {
            let mut task = common::task();
            task.scope.session = SessionId::new();
            Record::typed(
                Collection::Task,
                task.scope.task.to_string(),
                task.scope.workspace.clone(),
                Revision::ZERO,
                &task,
            )
            .unwrap()
        } else {
            artifact.spec.scope.session = SessionId::new();
            Record::typed(
                Collection::Artifact,
                artifact.spec.id.to_string(),
                common::workspace().id,
                Revision::ZERO,
                &artifact,
            )
            .unwrap()
        };
        changed
            .records
            .insert(key(record.collection, &record.id), record);
        assert!(matches!(reference(&changed), Err(Error::Access)));
        equivalent(&changed);
    }
}

#[test]
fn reader_errors_are_not_end_of_history_or_successful_finalization() {
    let (state, _) = State::default().prepare(&common::initial()).unwrap();
    for after_all_rows in [false, true] {
        let mut validator =
            EventHistoryValidator::new(state.watermark, &state.records, &state.sequences);
        if after_all_rows {
            validator.extend(state.events.iter().map(Ok)).unwrap();
        }
        let error = validator
            .extend::<EventEnvelope>([Err(Error::Unavailable("synthetic history read failure"))])
            .unwrap_err();
        assert!(matches!(
            error,
            Error::Unavailable("synthetic history read failure")
        ));
        assert!(validator.extend(state.events.iter().map(Ok)).is_err());
        assert!(validator.finish().is_err());
    }
    let mut validator =
        EventHistoryValidator::new(state.watermark, &state.records, &state.sequences);
    let mut corrupt = state.events[0].clone();
    corrupt.version = 2;
    assert!(validator.extend([Ok(corrupt)]).is_err());
    assert!(validator.finish().is_err());
    assert!(
        EventHistoryValidator::new(state.watermark, &state.records, &state.sequences)
            .finish()
            .is_err()
    );
}

fn repeated_dependencies(count: usize) -> State {
    use vcp_domain::artifact::{CaptureState, Range};
    let mut initial = common::initial();
    let artifact = ArtifactDescriptor {
        spec: common::spec(),
        state: CaptureState::Pending,
        length: ByteCount::ZERO,
        sha256: vcp_protocol::digest_bytes(b""),
        retained: vec![Range {
            start: ByteCount::ZERO,
            end: ByteCount::ZERO,
        }],
    };
    initial.events[0].artifacts.push(artifact.spec.id.clone());
    initial.mutations.push(Mutation::Put {
        expected: None,
        record: Record::typed(
            Collection::Artifact,
            artifact.spec.id.to_string(),
            common::workspace().id,
            Revision::ZERO,
            &artifact,
        )
        .unwrap(),
    });
    let (mut state, _) = State::default().prepare(&initial).unwrap();
    let first = state.events[0].clone();
    for index in 1..count {
        let mut event = first.clone();
        event.event.id = EventId::parse(format!("memo-event-{index}")).unwrap();
        event.sequence = SessionSeq::new(index as u64 + 1);
        state.events.push(event);
    }
    state
        .sequences
        .insert(first.event.session, SessionSeq::new(count as u64));
    state
}

#[test]
fn repeated_current_dependencies_decode_once_without_skipping_event_predicates() {
    let state = repeated_dependencies(600);
    reference(&state).unwrap();
    let mut validator =
        EventHistoryValidator::new(state.watermark, &state.records, &state.sequences);
    for rows in state.events.chunks(7) {
        validator.extend(rows.iter().map(Ok)).unwrap();
    }
    assert_eq!(validator.event_ids.len(), 600);
    assert_eq!(validator.task_decodes, 1);
    assert_eq!(validator.artifact_decodes, 1);
    assert_eq!(validator.task_sessions.len(), 1);
    assert_eq!(validator.artifact_scopes.len(), 1);
    validator.finish().unwrap();
    // An independent validation has its own immutable current-map facts.
    for collection in [Collection::Task, Collection::Artifact] {
        let mut changed = state.clone();
        let id = if collection == Collection::Task {
            common::task().scope.task.to_string()
        } else {
            changed.events[0].event.artifacts[0].to_string()
        };
        changed
            .records
            .get_mut(&key(collection, &id))
            .unwrap()
            .value = serde_json::json!({"malformed":"current dependency"});
        equivalent(&changed);
        assert!(reference(&changed).is_err());
        changed.records.remove(&key(collection, &id));
        equivalent(&changed);
        assert!(reference(&changed).is_err());
    }
    for change in 0..4 {
        let mut bad = state.clone();
        match change {
            0 => bad.events[599].event.id = bad.events[0].event.id.clone(),
            1 => bad.events[599].sequence = SessionSeq::new(1),
            2 => bad.events[599].watermark = Watermark::ZERO,
            _ => bad.events[599].version = 2,
        }
        assert!(reference(&bad).is_err());
        equivalent(&bad);
    }
}

#[test]
fn warmed_scope_facts_never_bypass_workspace_lookup_or_relationship_checks() {
    for artifact_only in [false, true] {
        let mut state = repeated_dependencies(2);
        let workspace = WorkspaceId::new();
        let mut session = common::session();
        session.id = SessionId::new();
        session.workspace = workspace.clone();
        let row = Record::typed(
            Collection::Session,
            session.id.to_string(),
            workspace.clone(),
            Revision::ZERO,
            &session,
        )
        .unwrap();
        state.records.insert(row.key(), row);
        state.events[1].event.workspace = workspace;
        state.events[1].event.session = session.id;
        if artifact_only {
            state.events[1].event.task = None;
        }
        assert!(matches!(reference(&state), Err(Error::Access)));
        equivalent(&state);
    }
    let mut state = repeated_dependencies(2);
    let mut task = common::task();
    task.scope.task = TaskId::new();
    task.root = task.scope.task.clone();
    let row = Record::typed(
        Collection::Task,
        task.scope.task.to_string(),
        task.scope.workspace.clone(),
        Revision::ZERO,
        &task,
    )
    .unwrap();
    state.records.insert(row.key(), row);
    state.events[1].event.task = Some(task.scope.task);
    // The same warmed artifact is not valid for a different task.
    assert!(matches!(reference(&state), Err(Error::Access)));
    equivalent(&state);
}

#[test]
fn scope_fact_cap_falls_back_to_decode_and_keys_are_actual_map_identity() {
    let count = MAX_SCOPE_FACTS + 8;
    let mut state = repeated_dependencies(count * 2);
    let artifact_id = state.events[0].event.artifacts[0].clone();
    let artifact: ArtifactDescriptor = state
        .record(
            Collection::Artifact,
            artifact_id.as_str(),
            &common::workspace().id,
        )
        .unwrap()
        .decode()
        .unwrap();
    for index in 0..count {
        let task_key = format!("scope-task-{index}");
        // Deliberately retain the same decoded task ID in different map slots:
        // this isolated validator historically checks session, not task self-ID.
        let task = common::task();
        // Construct the intentionally mismatched reference input without the
        // full record-shape constructor, which correctly rejects it upstream.
        let mut row = state
            .record(
                Collection::Task,
                common::task().scope.task.as_str(),
                &common::workspace().id,
            )
            .unwrap()
            .clone();
        row.id = task_key.clone();
        row.value = serde_json::to_value(&task).unwrap();
        state.records.insert(row.key(), row);
        let mut artifact = artifact.clone();
        artifact.spec.scope.task = TaskId::parse(&task_key).unwrap();
        let artifact_key = format!("scope-artifact-{index}");
        let mut row = state
            .record(
                Collection::Artifact,
                artifact_id.as_str(),
                &common::workspace().id,
            )
            .unwrap()
            .clone();
        row.id = artifact_key.clone();
        row.value = serde_json::to_value(&artifact).unwrap();
        state.records.insert(row.key(), row);
        for position in [index, index + count] {
            state.events[position].event.task = Some(TaskId::parse(&task_key).unwrap());
            state.events[position].event.artifacts =
                vec![ArtifactId::parse(&artifact_key).unwrap()];
        }
    }
    reference(&state).unwrap();
    let mut validator =
        EventHistoryValidator::new(state.watermark, &state.records, &state.sequences);
    validator.extend(state.events.iter().map(Ok)).unwrap();
    assert_eq!(validator.task_sessions.len(), MAX_SCOPE_FACTS);
    assert_eq!(validator.artifact_scopes.len(), MAX_SCOPE_FACTS);
    assert_eq!(validator.task_decodes, MAX_SCOPE_FACTS + 16);
    assert_eq!(validator.artifact_decodes, MAX_SCOPE_FACTS + 16);
    validator.finish().unwrap();
    // Same typed self-ID must not alias a different, invalid map slot.
    state
        .records
        .get_mut(&key(Collection::Task, "scope-task-1"))
        .unwrap()
        .value["scope"]["session"] = serde_json::json!(SessionId::new());
    assert!(matches!(reference(&state), Err(Error::Access)));
    equivalent(&state);
}
