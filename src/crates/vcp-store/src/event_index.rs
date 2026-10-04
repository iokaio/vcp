// SPDX-License-Identifier: Apache-2.0
//! Test-only qualification. This is deliberately NOT a production shortcut.
#![cfg(test)]
use super::{Collection, Mutation, State, Transaction};
use crate::{Error, Result};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{EventId, SessionId, SessionSeq, Watermark};

struct EventIndex {
    watermark: Watermark,
    pub(super) count: usize,
    pub(super) ids: BTreeSet<EventId>,
    pub(super) sequences: BTreeMap<SessionId, SessionSeq>,
}

impl EventIndex {
    fn validate_suffix(&self, state: &State) -> Result<()> {
        use vcp_domain::{artifact::ArtifactDescriptor, task::Task};
        let mut seen = BTreeSet::new();
        let mut sequences = self.sequences.clone();
        for event in state.events.iter().skip(self.count) {
            if event.version != 1
                || event.watermark > state.watermark
                || self.ids.contains(&event.event.id)
                || !seen.insert(event.event.id.clone())
            {
                return Err(Error::Corruption("event identity"));
            }
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
                        .is_some_and(|task| task != &artifact.spec.scope.task)
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
    fn qualified_candidate(
        &self,
        source: &State,
        candidate: &State,
        transaction: &Transaction,
    ) -> Result<()> {
        // Deliberately keep an exact prefix comparison in this experiment.
        // Removing it needs a separately reviewed proof that only the trusted
        // append constructor can produce candidates and rewrites reset indexes.
        // Watermark, length and a mutable checksum alone are not that proof.
        if !self.reusable(source, transaction)?
            || candidate.events.get(..self.count) != Some(source.events.as_slice())
        {
            return candidate.validate_event_history();
        }
        self.validate_suffix(candidate)
    }
}

#[path = "../tests/common/mod.rs"]
mod common;

#[test]
fn candidate_prefix_qualification_rejects_scope_identity_sequence_and_interior_corruption() {
    use vcp_domain::{ArtifactId, TransactionId};
    let initial = common::initial();
    let (source, _) = State::default().prepare(&initial).unwrap();
    let index = EventIndex::from_validated(&source);
    let mut event = initial.events[0].clone();
    event.id = EventId::parse("qualification-next").unwrap();
    let transaction = Transaction {
        id: TransactionId::new(),
        expected_watermark: source.watermark,
        mutations: vec![],
        events: vec![event],
        command: None,
    };
    let (candidate, _) = source.prepare(&transaction).unwrap();
    let mutations: &[fn(&mut State)] = &[
        |state| state.events[1].version = 2,
        |state| state.events[1].watermark = state.watermark.next().unwrap(),
        |state| state.events[1].event.id = state.events[0].event.id.clone(),
        |state| state.events[1].sequence = state.events[1].sequence.next().unwrap(),
        |state| state.events[1].event.workspace = vcp_domain::WorkspaceId::new(),
        |state| state.events[1].event.session = SessionId::new(),
        |state| state.events[1].event.task = Some(vcp_domain::TaskId::new()),
        |state| state.events[1].event.artifacts.push(ArtifactId::new()),
        |state| state.sequences.clear(),
        |state| state.events[0].event.task = Some(vcp_domain::TaskId::new()),
    ];
    assert!(index
        .qualified_candidate(&source, &candidate, &transaction)
        .is_ok());
    for (number, mutate) in mutations.iter().enumerate() {
        let mut invalid = candidate.clone();
        mutate(&mut invalid);
        let reference = invalid.validate_event_history().unwrap_err().to_string();
        let qualified = index
            .qualified_candidate(&source, &invalid, &transaction)
            .unwrap_err()
            .to_string();
        assert_eq!(reference, qualified, "mutation {number}");
    }
}

#[test]
fn dependency_changes_force_full_recheck_and_stale_index_is_rejected() {
    let initial = common::initial();
    let (source, _) = State::default().prepare(&initial).unwrap();
    let mut index = EventIndex::from_validated(&source);
    for collection in [Collection::Session, Collection::Task, Collection::Artifact] {
        let mut transaction = initial.clone();
        let Mutation::Put { record, .. } = &mut transaction.mutations[0] else {
            unreachable!()
        };
        record.collection = collection;
        assert!(!index.reusable(&source, &transaction).unwrap());
    }
    let mut next = initial.clone();
    next.id = vcp_domain::TransactionId::new();
    next.expected_watermark = source.watermark;
    next.mutations.clear();
    next.command = None;
    next.events[0].id = EventId::new();
    let (changed, _) = source.prepare(&next).unwrap();
    assert!(matches!(
        index.reusable(&changed, &next),
        Err(Error::Corruption("event index prefix"))
    ));
    index.extend_validated(&changed);
    assert!(index.reusable(&changed, &next).unwrap());
}

#[test]
fn changing_task_session_invalidates_a_historically_valid_event() {
    use super::{key, Record};
    let initial = common::initial();
    let (source, _) = State::default().prepare(&initial).unwrap();
    let index = EventIndex::from_validated(&source);
    let mut candidate = source.clone();
    let mut session = common::session();
    session.id = SessionId::new();
    let mut task = common::task();
    task.scope.session = session.id.clone();
    task.revision = task.revision.next().unwrap();
    let task_record = Record::typed(
        Collection::Task,
        task.scope.task.to_string(),
        task.scope.workspace.clone(),
        task.revision,
        &task,
    )
    .unwrap();
    candidate.records.insert(
        key(Collection::Task, task.scope.task.as_str()),
        task_record.clone(),
    );
    candidate.records.insert(
        key(Collection::Session, session.id.as_str()),
        Record::typed(
            Collection::Session,
            session.id.to_string(),
            session.workspace.clone(),
            session.revision,
            &session,
        )
        .unwrap(),
    );
    let mut mutation = initial.clone();
    mutation.mutations = vec![Mutation::Put {
        expected: Some(common::task().revision),
        record: task_record,
    }];
    assert!(matches!(
        candidate.validate_event_history(),
        Err(Error::Access)
    ));
    assert!(matches!(
        index.qualified_candidate(&source, &candidate, &mutation),
        Err(Error::Access)
    ));
}

#[test]
fn changing_artifact_scope_invalidates_its_historical_event_reference() {
    use super::{key, Record};
    use vcp_domain::{
        artifact::{ArtifactDescriptor, CaptureState, Range},
        ByteCount, Revision,
    };
    let mut initial = common::initial();
    let mut descriptor = ArtifactDescriptor {
        spec: common::spec(),
        state: CaptureState::Pending,
        length: ByteCount::ZERO,
        sha256: vcp_protocol::digest_bytes(b""),
        retained: vec![Range {
            start: ByteCount::ZERO,
            end: ByteCount::ZERO,
        }],
    };
    initial.events[0].artifacts.push(descriptor.spec.id.clone());
    initial.mutations.push(Mutation::Put {
        expected: None,
        record: Record::typed(
            Collection::Artifact,
            descriptor.spec.id.to_string(),
            common::workspace().id,
            Revision::ZERO,
            &descriptor,
        )
        .unwrap(),
    });
    let (source, _) = State::default().prepare(&initial).unwrap();
    let index = EventIndex::from_validated(&source);
    let mut candidate = source.clone();
    descriptor.spec.scope.session = SessionId::new();
    let record = Record::typed(
        Collection::Artifact,
        descriptor.spec.id.to_string(),
        common::workspace().id,
        Revision::ZERO.next().unwrap(),
        &descriptor,
    )
    .unwrap();
    candidate.records.insert(
        key(Collection::Artifact, descriptor.spec.id.as_str()),
        record.clone(),
    );
    let mut mutation = initial.clone();
    mutation.mutations = vec![Mutation::Put {
        expected: Some(Revision::ZERO),
        record,
    }];
    assert!(matches!(
        candidate.validate_event_history(),
        Err(Error::Access)
    ));
    assert!(matches!(
        index.qualified_candidate(&source, &candidate, &mutation),
        Err(Error::Access)
    ));
}
impl EventIndex {
    pub(crate) fn from_validated(state: &State) -> Self {
        Self {
            watermark: state.watermark,
            count: state.events.len(),
            ids: state
                .events
                .iter()
                .map(|row| row.event.id.clone())
                .collect(),
            sequences: state.sequences.clone(),
        }
    }
    pub(super) fn reusable(&self, source: &State, transaction: &Transaction) -> Result<bool> {
        if self.watermark != source.watermark
            || self.count != source.events.len()
            || self.sequences != source.sequences
        {
            return Err(Error::Corruption("event index prefix"));
        }
        // Event validation reads exactly these record types. Changing any of
        // them requires rechecking earlier references; generic projections and
        // accounting rows do not change the meaning of an old event's scope.
        Ok(!transaction.mutations.iter().any(|mutation| {
            matches!(mutation,
            Mutation::Put { record, .. } if matches!(record.collection,
                Collection::Task | Collection::Session | Collection::Artifact))
        }))
    }
    pub(crate) fn extend_validated(&mut self, state: &State) {
        self.ids.extend(
            state.events[self.count..]
                .iter()
                .map(|row| row.event.id.clone()),
        );
        self.count = state.events.len();
        self.sequences.clone_from(&state.sequences);
        self.watermark = state.watermark;
    }
}
