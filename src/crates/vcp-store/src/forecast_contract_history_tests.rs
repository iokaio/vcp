// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::historical_facts::EventFact;
use vcp_domain::{ByteCount, EventId, Revision, TaskId, WorkspaceId};
#[path = "../tests/common/mod.rs"]
mod common;
// Frozen pre-extraction validator: preserve exact predicate and error order.
fn reference_validate(state: &State, row: &Record) -> Result<()> {
    if !kind(row) {
        return Ok(());
    }
    shape(row)?;
    let value: forecast::Sources = row.decode()?;
    let artifact_record = state.record(
        Collection::Artifact,
        value.artifact.as_str(),
        &row.workspace,
    )?;
    let artifact: ArtifactDescriptor = artifact_record.decode()?;
    if artifact.sha256 != value.artifact_digest
        || artifact.spec.schema != "vcp-optimization-forecast-v1"
        || !value.source_tasks.contains(&artifact.spec.scope.task)
        || artifact_record.references
            != BTreeSet::from([
                row.key(),
                key(Collection::Task, artifact.spec.scope.task.as_str()),
            ])
    {
        return Err(Error::Corruption("forecast manifest artifact binding"));
    }
    let mut observed = BTreeSet::new();
    for reference in &row.references {
        let source = state
            .records
            .get(reference)
            .ok_or(Error::Corruption("forecast source absent"))?;
        if source.workspace != row.workspace {
            return Err(Error::Access);
        }
        let scope = match source.collection {
            Collection::Artifact => source.decode::<ArtifactDescriptor>()?.spec.scope,
            Collection::Task
            | Collection::Turn
            | Collection::Effect
            | Collection::Attempt
            | Collection::Settlement
            | Collection::Verification => {
                serde_json::from_value::<Scope>(source.value["scope"].clone())?
            }
            Collection::Projection
                if matches!(
                    source.value["document_type"].as_str(),
                    Some("vcp_routing_decision_v1" | "vcp_escalation_admission_v1")
                ) =>
            {
                serde_json::from_value::<Scope>(source.value["scope"].clone())?
            }
            _ => return Err(Error::Corruption("unsupported forecast source type")),
        };
        if scope.workspace != row.workspace || !value.source_tasks.contains(&scope.task) {
            return Err(Error::Access);
        }
        observed.insert(scope.task);
    }
    let events: std::collections::BTreeMap<_, _> = state
        .events
        .iter()
        .filter(|e| value.source_events.contains(&e.event.id))
        .map(|e| (&e.event.id, e))
        .collect();
    for id in &value.source_events {
        let event = events
            .get(id)
            .ok_or(Error::Corruption("forecast source event absent"))?;
        let task = event
            .event
            .task
            .as_ref()
            .ok_or(Error::Corruption("forecast event task absent"))?;
        if event.event.workspace != row.workspace || !value.source_tasks.contains(task) {
            return Err(Error::Access);
        }
        observed.insert(task.clone());
    }
    if observed != value.source_tasks {
        return Err(Error::Corruption("forecast source task manifest differs"));
    }
    Ok(())
}

struct Facts {
    rows: Vec<EventFact>,
    calls: Vec<EventId>,
    fail: bool,
}
impl EventFacts for Facts {
    fn last(&mut self, id: &EventId) -> Result<Option<EventFact>> {
        self.calls.push(id.clone());
        if self.fail {
            return Err(Error::Unavailable("forecast history read failed"));
        }
        Ok(self.rows.iter().rev().find(|row| &row.id == id).cloned())
    }
    fn any(&mut self, _: &EventId, _: &dyn Fn(&EventFact) -> bool) -> Result<bool> {
        panic!("forecast requires last matching event, not any scope match")
    }
}
fn fixture() -> (State, Record) {
    let mut state = State::default().prepare(&common::initial()).unwrap().0;
    let mut spec = common::spec();
    spec.schema = "vcp-optimization-forecast-v1".into();
    let artifact = ArtifactDescriptor {
        spec,
        state: vcp_domain::artifact::CaptureState::Complete,
        length: ByteCount::ZERO,
        sha256: digest_bytes(b""),
        retained: vec![vcp_domain::artifact::Range {
            start: ByteCount::ZERO,
            end: ByteCount::ZERO,
        }],
    };
    let mut value = forecast::Sources {
        document_type: forecast::SOURCES.into(),
        schema_version: 1,
        id: "forecast-source-fixture".into(),
        workspace: common::workspace().id,
        revision: Revision::ZERO,
        report: "forecast-report-fixture".into(),
        artifact: artifact.spec.id.clone(),
        artifact_digest: artifact.sha256.clone(),
        source_tasks: BTreeSet::from([common::task().scope.task]),
        source_records: BTreeSet::new(),
        source_events: BTreeSet::from([EventId::parse("created").unwrap()]),
        sources_digest: String::new(),
    };
    value.sources_digest = digest_bytes(
        &canonical_bytes(&(
            &value.source_tasks,
            &value.source_records,
            &value.source_events,
        ))
        .unwrap(),
    );
    let row = Record {
        collection: Collection::Projection,
        id: value.id.clone(),
        workspace: value.workspace.clone(),
        revision: value.revision,
        references: BTreeSet::from([key(Collection::Task, common::task().scope.task.as_str())]),
        value: serde_json::to_value(&value).unwrap(),
    };
    row.validate_shape().unwrap();
    let mut captured = Record::typed(
        Collection::Artifact,
        artifact.spec.id.as_str(),
        value.workspace.clone(),
        Revision::ZERO,
        &artifact,
    )
    .unwrap();
    captured.references = BTreeSet::from([
        row.key(),
        key(Collection::Task, artifact.spec.scope.task.as_str()),
    ]);
    state.records.insert(captured.key(), captured);
    (state, row)
}

#[test]
fn current_forecast_with_explicit_history_matches_frozen_predicates() {
    let (base, row) = fixture();
    reference_validate(&base, &row).unwrap();
    for variant in 0..9 {
        let mut state = base.clone();
        let mut row = row.clone();
        match variant {
            1 => state.events.clear(),
            2 => state.events[0].event.task = None,
            3 => state.events[0].event.workspace = WorkspaceId::new(),
            4 => state.events[0].event.task = Some(TaskId::new()),
            5 | 6 => {
                let mut duplicate = state.events[0].clone();
                duplicate.event.workspace = WorkspaceId::new();
                if variant == 5 {
                    state.events.insert(0, duplicate);
                } else {
                    state.events.push(duplicate);
                }
            }
            7 => {
                let mut unrelated = state.events[0].clone();
                unrelated.event.id = EventId::new();
                unrelated.event.workspace = WorkspaceId::new();
                state.events.push(unrelated);
            }
            8 => row.value["artifact_digest"] = "f".repeat(64).into(),
            _ => (),
        }
        let expected = format!("{:?}", reference_validate(&state, &row));
        let current = crate::CurrentState::from_state(&state);
        let mut facts = Facts {
            rows: state.events.iter().map(EventFact::from).collect(),
            calls: vec![],
            fail: false,
        };
        assert_eq!(
            format!(
                "{:?}",
                validate_with_history((&current).into(), &row, &mut facts)
            ),
            expected,
            "variant {variant}"
        );
        assert_eq!(
            format!("{:?}", validate(&state, &row)),
            expected,
            "State adapter {variant}"
        );
        assert_eq!(
            facts.calls,
            if variant == 8 {
                vec![]
            } else {
                vec![EventId::parse("created").unwrap()]
            }
        );
    }
}

#[test]
fn unavailable_forecast_history_is_not_absence_and_prior_errors_stay_prior() {
    let (state, mut row) = fixture();
    let current = crate::CurrentState::from_state(&state);
    let mut facts = Facts {
        rows: vec![],
        calls: vec![],
        fail: true,
    };
    assert!(matches!(
        validate_with_history((&current).into(), &row, &mut facts),
        Err(Error::Unavailable("forecast history read failed"))
    ));
    row.value["artifact_digest"] = "f".repeat(64).into();
    facts.calls.clear();
    assert!(matches!(
        validate_with_history((&current).into(), &row, &mut facts),
        Err(Error::Corruption("forecast manifest artifact binding"))
    ));
    assert!(facts.calls.is_empty());
}

#[test]
fn forecast_rejects_a_reader_returning_another_event_identity() {
    struct WrongId(EventFact);
    impl EventFacts for WrongId {
        fn last(&mut self, _: &EventId) -> Result<Option<EventFact>> {
            Ok(Some(self.0.clone()))
        }
        fn any(&mut self, _: &EventId, _: &dyn Fn(&EventFact) -> bool) -> Result<bool> {
            panic!("unused")
        }
    }
    let (state, row) = fixture();
    let mut fact = EventFact::from(&state.events[0]);
    fact.id = EventId::new();
    assert!(matches!(
        validate_with_history((&state).into(), &row, &mut WrongId(fact)),
        Err(Error::Corruption("forecast source event identity"))
    ));
}
