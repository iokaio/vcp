// SPDX-License-Identifier: Apache-2.0
mod common;
use common::*;
use vcp_domain::{ingestion::*, *};
use vcp_protocol::{canonical_bytes, digest_bytes};
use vcp_store::contract::*;

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

#[test]
fn cursor_cannot_lose_origins_or_duplicate_their_jobs() {
    let state = State::default().prepare(&initial()).unwrap().0;
    let cursor = cursor(&state);
    assert!(state
        .prepare(&tx(
            &state,
            vec![put(row(&cursor.id, cursor.revision, &cursor), None)]
        ))
        .is_err());
    let (state, mut cursor, job) = queued();
    let mut duplicate = row(&job.id, job.revision, &job);
    duplicate.id = CommandId::new().to_string();
    duplicate.value["id"] = serde_json::json!(duplicate.id);
    assert!(duplicate.validate_shape().is_err());
    cursor.after = Units::ZERO;
    cursor.scanned_through = Watermark::ZERO;
    cursor.revision = Revision::new(1);
    assert!(state
        .prepare(&tx(
            &state,
            vec![put(
                row(&cursor.id, cursor.revision, &cursor),
                Some(Revision::ZERO)
            )]
        ))
        .is_err());
    let mut event = state.events[0].event.clone();
    event.id = EventId::new();
    let mut append = tx(&state, vec![]);
    append.events.push(event);
    let state = state.prepare(&append).unwrap().0;
    cursor.after = Units::new(2);
    cursor.scanned_through = state.events.last().unwrap().watermark;
    assert!(state
        .prepare(&tx(
            &state,
            vec![put(
                row(&cursor.id, cursor.revision, &cursor),
                Some(Revision::ZERO)
            )]
        ))
        .is_err());
}

#[test]
fn queue_origin_identity_and_lifecycle_are_not_mutable_documents() {
    let (state, _, job) = queued();
    let mut malformed = job.clone();
    malformed.revision = Revision::new(1);
    malformed.state = JobState::Completed;
    malformed.finding = Some("skipped execution".into());
    assert!(state
        .prepare(&tx(
            &state,
            vec![put(
                row(&malformed.id, malformed.revision, &malformed),
                Some(Revision::ZERO)
            )]
        ))
        .is_err());
    let mut leased = job.clone();
    leased.revision = Revision::new(1);
    leased.state = JobState::Leased;
    leased.attempts = Units::new(1);
    leased.lease = Some(Lease {
        token: CommandId::new(),
        owner: ActorId::new(),
        expires_at: Timestamp::new(100),
    });
    for field in ["origin", "specification", "root", "attempts"] {
        let mut changed = leased.clone();
        match field {
            "origin" => changed.origin_watermark = Watermark::new(99),
            "specification" => changed.extractor.version = 2,
            "root" => changed.root = TaskId::new(),
            _ => changed.attempts = Units::new(2),
        }
        assert!(
            state
                .prepare(&tx(
                    &state,
                    vec![put(
                        row(&changed.id, changed.revision, &changed),
                        Some(Revision::ZERO)
                    )]
                ))
                .is_err(),
            "{field}"
        );
    }
    let state = state
        .prepare(&tx(
            &state,
            vec![put(
                row(&leased.id, leased.revision, &leased),
                Some(Revision::ZERO),
            )],
        ))
        .unwrap()
        .0;
    let mut completed = leased;
    completed.revision = Revision::new(2);
    completed.state = JobState::Completed;
    completed.lease = None;
    completed.finding = Some("no supported observation".into());
    let state = state
        .prepare(&tx(
            &state,
            vec![put(
                row(&completed.id, completed.revision, &completed),
                Some(Revision::new(1)),
            )],
        ))
        .unwrap()
        .0;
    completed.revision = Revision::new(3);
    completed.finding = Some("rewritten outcome".into());
    assert!(state
        .prepare(&tx(
            &state,
            vec![put(
                row(&completed.id, completed.revision, &completed),
                Some(Revision::new(2))
            )]
        ))
        .is_err());
    assert!(state
        .prepare(&tx(
            &state,
            vec![Mutation::DropProjection {
                id: job.id.to_string(),
                expected: Revision::new(2)
            }]
        ))
        .is_err());
    let mut wrong_collection = row(&job.id, Revision::ZERO, &job);
    wrong_collection.collection = Collection::Projection;
    assert!(wrong_collection.validate_shape().is_err());
}
