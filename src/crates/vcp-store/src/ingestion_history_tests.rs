// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_domain::ingestion::*;
use vcp_protocol::{canonical_bytes, digest_bytes};
#[path = "../tests/common/mod.rs"]
mod common;
use common::*;

// Frozen full-history validator from 46a90fa8, before metadata extraction.
fn reference(state: &State) -> Result<()> {
    let mut cursors = BTreeMap::<CommandId, Cursor>::new();
    let mut jobs = BTreeMap::<CommandId, Job>::new();
    for record in state.records.values() {
        match kind(record)? {
            Some("vcp_ingestion_cursor_v1") => {
                let cursor: Cursor = record.decode()?;
                cursors.insert(cursor.id.clone(), cursor);
            }
            Some("vcp_ingestion_job_v1") => {
                let job: Job = record.decode()?;
                jobs.insert(job.id.clone(), job);
            }
            _ => (),
        }
    }
    for cursor in cursors.values() {
        let root: Task = state
            .record(
                Collection::Task,
                cursor.scope.task.as_str(),
                &cursor.scope.workspace,
            )?
            .decode()?;
        if root.scope != cursor.scope || root.parent.is_some() || root.root != cursor.scope.task {
            return Err(Error::Corruption("ingestion root scope"));
        }
        let after = usize::try_from(cursor.after.get())
            .map_err(|_| Error::Corruption("ingestion cursor offset"))?;
        if after > state.events.len()
            || cursor.scanned_through > state.watermark
            || (after == 0 && cursor.scanned_through != Watermark::ZERO)
            || (after > 0 && state.events[after - 1].watermark != cursor.scanned_through)
        {
            return Err(Error::Corruption("ingestion cursor progress"));
        }
        for event in state.events.iter().take(after) {
            if event.event.workspace != cursor.scope.workspace
                || event.event.session != cursor.scope.session
            {
                continue;
            }
            let Some(task_id) = &event.event.task else {
                continue;
            };
            let task: Task = state
                .record(Collection::Task, task_id.as_str(), &cursor.scope.workspace)?
                .decode()?;
            let kind = serde_json::to_value(&event.event.kind)?;
            if task.root == cursor.scope.task
                && kind.as_str().is_some_and(|kind| {
                    cursor.extractor.event_kinds.iter().any(|item| item == kind)
                })
            {
                let id = CommandId::parse(job_id(&cursor.id, &event.event.id)?)?;
                if !jobs.contains_key(&id) {
                    return Err(Error::Corruption(
                        "ingestion cursor advanced without durable job",
                    ));
                }
            }
        }
    }
    let events: BTreeMap<_, _> = state
        .events
        .iter()
        .enumerate()
        .map(|(i, e)| (&e.event.id, (i, e)))
        .collect();
    for job in jobs.values() {
        let cursor = cursors
            .get(&job.cursor)
            .ok_or(Error::Corruption("ingestion job cursor"))?;
        let task: Task = state
            .record(
                Collection::Task,
                job.scope.task.as_str(),
                &job.scope.workspace,
            )?
            .decode()?;
        let (offset, event) = events
            .get(&job.origin)
            .ok_or(Error::Corruption("ingestion origin missing"))?;
        let kind = serde_json::to_value(&event.event.kind)?;
        if job.scope != task.scope
            || task.root != job.root
            || job.root != cursor.scope.task
            || job.scope.workspace != cursor.scope.workspace
            || job.scope.session != cursor.scope.session
            || job.extractor != cursor.extractor
            || event.watermark != job.origin_watermark
            || event.event.workspace != job.scope.workspace
            || event.event.session != job.scope.session
            || event.event.task.as_ref() != Some(&job.scope.task)
            || *offset as u64 >= cursor.after.get()
            || !kind
                .as_str()
                .is_some_and(|kind| job.extractor.event_kinds.iter().any(|item| item == kind))
        {
            return Err(Error::Corruption("ingestion job origin scope or stream"));
        }
        for id in &job.results {
            let row = state.record(Collection::Projection, id.as_str(), &job.scope.workspace)?;
            let (scope, proposal_id) =
                if row.value["document_type"] == vcp_domain::redaction::RESULT {
                    let result: vcp_domain::redaction::RedactedResult = row.decode()?;
                    result.validate()?;
                    (result.scope, result.proposal)
                } else {
                    let result: vcp_domain::memory::ProposalResult = row.decode()?;
                    result.validate()?;
                    (result.scope, result.proposal)
                };
            let row = state.record(
                Collection::Claim,
                proposal_id.as_str(),
                &job.scope.workspace,
            )?;
            let associated = if row.value["document_type"] == vcp_domain::redaction::PROPOSAL {
                let proposal: vcp_domain::redaction::RedactedProposal = row.decode()?;
                proposal.validate()?;
                proposal.sources.origins.contains(&job.origin)
                    && proposal.extractor_digest
                        == vcp_protocol::digest_bytes(job.extractor.identity().as_bytes())
            } else {
                let proposal: vcp_domain::memory::ProposalRecord = row.decode()?;
                proposal.proposal.origins.contains(&job.origin)
                    && proposal.proposal.extractor == job.extractor.identity()
            };
            if scope != job.scope || !associated {
                return Err(Error::Corruption("ingestion completion provenance"));
            }
        }
    }
    Ok(())
}

fn cursor(state: &State) -> Cursor {
    let scope = task().scope;
    let extractor = ExtractorSpec {
        name: "fixture".into(),
        version: 1,
        event_kinds: vec!["task_created".into()],
    };
    let id = CommandId::parse(digest_bytes(
        &canonical_bytes(&("ingestion-cursor/1", &scope, &extractor)).unwrap(),
    ))
    .unwrap();
    Cursor {
        document_type: DocumentType::Cursor,
        schema_version: 1,
        id,
        scope,
        revision: Revision::ZERO,
        extractor,
        after: Units::new(state.events.len() as u64),
        scanned_through: state.events.last().unwrap().watermark,
    }
}
fn job(cursor: &Cursor, state: &State) -> Job {
    let event = state.events.last().unwrap();
    let id = CommandId::parse(digest_bytes(
        &canonical_bytes(&("ingestion-job/1", &cursor.id, &event.event.id)).unwrap(),
    ))
    .unwrap();
    Job {
        document_type: DocumentType::Job,
        schema_version: 1,
        id,
        scope: cursor.scope.clone(),
        root: cursor.scope.task.clone(),
        revision: Revision::ZERO,
        cursor: cursor.id.clone(),
        extractor: cursor.extractor.clone(),
        origin: event.event.id.clone(),
        origin_watermark: event.watermark,
        state: JobState::Pending,
        attempts: Units::ZERO,
        max_attempts: Units::new(2),
        lease: None,
        not_before: Timestamp::ZERO,
        last_failure: None,
        results: vec![],
        finding: None,
    }
}
fn row<T: serde::Serialize>(id: &CommandId, revision: Revision, value: &T) -> Record {
    Record::typed(
        Collection::Claim,
        id.to_string(),
        workspace().id,
        revision,
        value,
    )
    .unwrap()
}
fn tx(state: &State, mutations: Vec<Mutation>) -> Transaction {
    Transaction {
        id: TransactionId::new(),
        expected_watermark: state.watermark,
        mutations,
        events: vec![],
        command: None,
    }
}
fn put(record: Record, expected: Option<Revision>) -> Mutation {
    Mutation::Put { expected, record }
}
fn queued() -> (State, Cursor, Job) {
    let state = State::default().prepare(&initial()).unwrap().0;
    let cursor = cursor(&state);
    let job = job(&cursor, &state);
    let state = state
        .prepare(&tx(
            &state,
            vec![
                put(row(&cursor.id, cursor.revision, &cursor), None),
                put(row(&job.id, job.revision, &job), None),
            ],
        ))
        .unwrap()
        .0;
    (state, cursor, job)
}

fn inputs(state: &State) -> (BTreeMap<CommandId, Cursor>, BTreeMap<CommandId, Job>) {
    let mut cursors = BTreeMap::new();
    let mut jobs = BTreeMap::new();
    for record in state.records.values() {
        match kind(record).unwrap() {
            Some("vcp_ingestion_cursor_v1") => {
                let cursor: Cursor = record.decode().unwrap();
                cursors.insert(cursor.id.clone(), cursor);
            }
            Some("vcp_ingestion_job_v1") => {
                let job: Job = record.decode().unwrap();
                jobs.insert(job.id.clone(), job);
            }
            _ => (),
        }
    }
    (cursors, jobs)
}
fn equivalent(state: &State) {
    let expected = reference(state).map_err(|error| error.to_string());
    assert_eq!(validate(state).map_err(|error| error.to_string()), expected);
    let (cursors, jobs) = inputs(state);
    let current = crate::CurrentState::from_state(state);
    let current_inputs = Inputs::new((&current).into()).unwrap();
    for width in [1, 2, 7, 256] {
        for owned in [false, true] {
            let mut history = ingestion_history::IngestionHistory::new(
                &state.records,
                &cursors,
                &jobs,
                state.events.len(),
            );
            for page in state.events.chunks(width) {
                if owned {
                    history.extend(page.iter().cloned().map(Ok)).unwrap();
                } else {
                    history.extend(page.iter().map(Ok)).unwrap();
                }
            }
            assert_eq!(
                validate_history(state, &cursors, &jobs, history.finish().unwrap())
                    .map_err(|error| error.to_string()),
                expected
            );
            let mut feed = current_inputs.history(state.events.len());
            for page in state.events.chunks(width) {
                if owned {
                    feed.extend(page.to_vec().into_iter().map(Ok)).unwrap();
                } else {
                    feed.extend(page.iter().map(Ok)).unwrap();
                }
            }
            assert_eq!(
                current_inputs
                    .finish(feed.finish().unwrap())
                    .map_err(|error| error.to_string()),
                expected
            );
        }
    }
}

#[test]
fn generated_ingestion_histories_match_original_errors_and_absolute_ordinals() {
    let (mut state, cursor, job) = queued();
    for index in 0..24 {
        equivalent(&state);
        for number in 0..12 {
            let mut changed = state.clone();
            let mut cursor = cursor.clone();
            let mut job = job.clone();
            match number {
                0 => cursor.after = Units::new(state.events.len() as u64 + 1),
                1 => cursor.after = Units::ZERO,
                2 => cursor.scanned_through = Watermark::ZERO,
                3 => cursor.scope.task = TaskId::new(),
                4 => job.origin = EventId::new(),
                5 => job.origin_watermark = state.watermark.next().unwrap(),
                6 => job.scope.session = SessionId::new(),
                7 => job.cursor = CommandId::new(),
                8 => job.root = TaskId::new(),
                9 => job.extractor.event_kinds = vec!["turn_created".into()],
                10 => job.results.push(CommandId::new()),
                11 => {}
                _ => unreachable!(),
            }
            // Inject malformed persisted input after typed construction so the
            // targeted validator, rather than Record::typed, is compared.
            changed
                .records
                .get_mut(&key(Collection::Claim, cursor.id.as_str()))
                .unwrap()
                .value = serde_json::to_value(&cursor).unwrap();
            changed
                .records
                .get_mut(&key(Collection::Claim, job.id.as_str()))
                .unwrap()
                .value = serde_json::to_value(&job).unwrap();
            if number == 11 {
                changed
                    .records
                    .remove(&key(Collection::Claim, job.id.as_str()));
            }
            assert!(reference(&changed).is_err(), "corruption {number}");
            equivalent(&changed);
        }
        let mut append = tx(&state, vec![]);
        let mut event = state.events[0].event.clone();
        event.id = EventId::parse(format!("generated_{index}")).unwrap();
        append.events.push(event);
        state = state.prepare(&append).unwrap().0;
    }
    // Advancing the cursor without all required origin jobs must still reject,
    // including when the new boundary falls exactly on a page boundary.
    let mut changed = cursor.clone();
    changed.after = Units::new(state.events.len() as u64);
    changed.scanned_through = state.events.last().unwrap().watermark;
    state.records.insert(
        key(Collection::Claim, changed.id.as_str()),
        row(&changed.id, changed.revision, &changed),
    );
    assert!(reference(&state).is_err());
    equivalent(&state);
}

#[test]
fn retained_metadata_depends_on_current_jobs_and_cursors_not_event_count() {
    let (mut state, _, _) = queued();
    for count in [1, 33, 257] {
        while state.events.len() < count {
            let mut append = tx(&state, vec![]);
            let mut event = state.events[0].event.clone();
            event.id = EventId::new();
            event.data = serde_json::json!({"retained_payload":"x".repeat(4096)});
            append.events.push(event);
            state = state.prepare(&append).unwrap().0;
        }
        let (cursors, jobs) = inputs(&state);
        let mut history = ingestion_history::IngestionHistory::new(
            &state.records,
            &cursors,
            &jobs,
            state.events.len(),
        );
        for page in state.events.chunks(7) {
            history.extend(page.iter().map(Ok)).unwrap();
        }
        let history = history.finish().unwrap();
        assert_eq!(history.count, count);
        assert_eq!(history.origins.len(), 1);
        assert_eq!(history.cursors.len(), 1);
        assert_eq!(history.origins.values().next().unwrap().offset, 0);
        validate_history(&state, &cursors, &jobs, history).unwrap();
        reference(&state).unwrap();
    }
}

#[test]
fn read_failure_and_missing_tail_cannot_be_successful_ingestion_validation() {
    let (state, _, _) = queued();
    let (cursors, jobs) = inputs(&state);
    for after_rows in [false, true] {
        let mut history = ingestion_history::IngestionHistory::new(
            &state.records,
            &cursors,
            &jobs,
            state.events.len(),
        );
        if after_rows {
            history.extend(state.events.iter().map(Ok)).unwrap();
        }
        assert!(matches!(
            history.extend::<EventEnvelope>([Err(Error::Unavailable("synthetic reader"))]),
            Err(Error::Unavailable("synthetic reader"))
        ));
        assert!(history.finish().is_err());
    }
    let history = ingestion_history::IngestionHistory::new(
        &state.records,
        &cursors,
        &jobs,
        state.events.len(),
    );
    assert!(history.finish().is_err());
}

#[test]
fn current_ingestion_inputs_accept_owned_pages_but_never_failed_or_partial_history() {
    let (state, _, _) = queued();
    let current = crate::CurrentState::from_state(&state);
    let events = state.events.to_vec();
    let expected = reference(&state).map_err(|error| error.to_string());
    drop(state);
    let inputs = Inputs::new((&current).into()).unwrap();
    assert!(inputs.needs_history());
    let mut full = inputs.history(events.len());
    for page in events.chunks(1) {
        full.extend(page.to_vec().into_iter().map(Ok)).unwrap();
    }
    assert_eq!(
        inputs
            .finish(full.finish().unwrap())
            .map_err(|error| error.to_string()),
        expected
    );
    for after_rows in [false, true] {
        let mut failed = inputs.history(events.len());
        if after_rows {
            failed.extend(events.clone().into_iter().map(Ok)).unwrap();
        }
        assert!(matches!(
            failed.extend::<EventEnvelope>([Err(Error::Unavailable("owned page failed"))]),
            Err(Error::Unavailable("owned page failed"))
        ));
        assert!(
            failed.extend(events.clone().into_iter().map(Ok)).is_err(),
            "a failed stream cannot recover by adding later pages"
        );
        assert!(failed.finish().is_err());
    }
    let mut partial = inputs.history(events.len());
    partial
        .extend(events[..events.len() - 1].iter().cloned().map(Ok))
        .unwrap();
    assert!(
        partial.finish().is_err(),
        "an incomplete global prefix is not valid ingestion evidence"
    );
}

#[test]
fn absent_ingestion_consumers_do_not_change_global_history_validation() {
    let (mut state, _) = State::default().prepare(&initial()).unwrap();
    equivalent(&state);
    // Ingestion has no rule for these rows. Its empty-consumer path and the
    // frozen reference agree even on arbitrary historical data; the enclosing
    // State validator must continue to reject corrupt events independently.
    let mut event = state.events[0].clone();
    event.version = 9;
    state.events.extend(std::iter::repeat_n(event, 256));
    assert!(reference(&state).is_ok());
    assert!(validate(&state).is_ok());
    assert!(state.validate().is_err());
    let (mut state, _) = State::default().prepare(&initial()).unwrap();
    let record = state.records.values_mut().next().unwrap();
    record.value["document_type"] = serde_json::json!("vcp_ingestion_unknown");
    assert_eq!(
        validate(&state).unwrap_err().to_string(),
        reference(&state).unwrap_err().to_string()
    );
}
