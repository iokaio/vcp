// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::historical_facts::EventFact;
// Frozen pre-extraction validator: preserve exact predicate and error order.
fn reference_validate(state: &State, row: &Record) -> Result<()> {
    match kind(row)? {
        Some(SUBMISSION) => {
            let v: Submission = row.decode()?;
            for origin in &v.candidate.origins {
                if !state.events.iter().any(|e| {
                    e.event.id == *origin
                        && e.event.workspace == v.scope.workspace
                        && e.event.session == v.scope.session
                }) {
                    return Err(Error::Corruption("manual submission origin scope"));
                }
            }
            for evidence in &v.candidate.evidence {
                let artifact: vcp_domain::artifact::ArtifactDescriptor = state
                    .record(
                        Collection::Artifact,
                        evidence.artifact.as_str(),
                        &v.scope.workspace,
                    )?
                    .decode()?;
                if artifact.spec.scope.session != v.scope.session
                    || artifact.sha256 != evidence.sha256
                    || evidence
                        .range
                        .as_ref()
                        .is_some_and(|r| r.end.get() > artifact.length.get())
                {
                    return Err(Error::Corruption("manual submission evidence identity"));
                }
            }
        }
        Some(DECISION) => {
            let v: Decision = row.decode()?;
            let submission: Submission = state
                .record(Collection::Claim, v.submission.as_str(), &row.workspace)?
                .decode()?;
            v.validate_submission(&submission)?;
            if let Some(id) = &v.governed_proposal {
                let proposal: ProposalRecord = state
                    .record(Collection::Claim, id.as_str(), &row.workspace)?
                    .decode()?;
                let mut expected = submission.candidate.clone();
                expected.id = id.clone();
                expected.command = v.command.clone();
                expected.actor = v.actor.clone();
                expected.epochs = v.epochs.clone();
                if proposal.proposal != expected
                    || proposal.resolution != v.resolution
                    || proposal.recorded_at != v.recorded_at
                    || proposal.payload_digest != digest_bytes(&canonical_bytes(&expected)?)
                {
                    return Err(Error::Corruption("manual decision governed candidate"));
                }
                let result: ProposalResult = state
                    .record(Collection::Projection, v.command.as_str(), &row.workspace)?
                    .decode()?;
                if result.proposal != *id
                    || result.resolution != v.resolution
                    || result.scope != v.scope
                {
                    return Err(Error::Corruption("manual decision governed result"));
                }
            }
        }
        Some(REDACTED_DECISION) => {
            let v: RedactedDecision = row.decode()?;
            let source = state.record(Collection::Claim, v.submission.as_str(), &row.workspace)?;
            let (scope, digest) = match kind(source)? {
                Some(SUBMISSION) => {
                    let s: Submission = source.decode()?;
                    (s.scope, s.candidate_digest)
                }
                Some(REDACTED_SUBMISSION) => {
                    let s: RedactedSubmission = source.decode()?;
                    (s.scope, s.candidate_digest)
                }
                _ => return Err(Error::Corruption("redacted manual submission type")),
            };
            if scope != v.scope || digest != v.submission_digest {
                return Err(Error::Corruption("redacted manual decision linkage"));
            }
        }
        _ => (),
    }
    Ok(())
}

struct Facts {
    rows: Vec<EventFact>,
    calls: Vec<EventId>,
    fail: bool,
}
impl EventFacts for Facts {
    fn last(&mut self, _: &EventId) -> Result<Option<EventFact>> {
        panic!("manual origins require any matching scope, not last event")
    }
    fn any(&mut self, id: &EventId, predicate: &dyn Fn(&EventFact) -> bool) -> Result<bool> {
        self.calls.push(id.clone());
        if self.fail {
            return Err(Error::Unavailable("manual history read failed"));
        }
        Ok(self.rows.iter().filter(|row| &row.id == id).any(predicate))
    }
}

#[test]
fn current_manual_origins_with_explicit_history_match_frozen_predicates() {
    let base = State::default().prepare(&common::initial()).unwrap().0;
    let submission = submission();
    let record = row(Collection::Claim, submission.id.as_str(), &submission);
    reference_validate(&base, &record).unwrap();
    for variant in 0..7 {
        let mut state = base.clone();
        match variant {
            1 => state.events.clear(),
            2 => state.events[0].event.workspace = WorkspaceId::new(),
            3 => state.events[0].event.session = SessionId::new(),
            4 | 5 => {
                let mut duplicate = state.events[0].clone();
                duplicate.event.session = SessionId::new();
                if variant == 4 {
                    state.events.insert(0, duplicate);
                } else {
                    state.events.push(duplicate);
                }
            }
            6 => {
                // This historical predicate intentionally binds workspace and
                // session, not task/kind; extraction must not invent a fence.
                state.events[0].event.task = None;
                state.events[0].event.kind = EventKind::MemoryResolved;
            }
            _ => (),
        }
        let expected = format!("{:?}", reference_validate(&state, &record));
        let current = crate::CurrentState::from_state(&state);
        let mut facts = Facts {
            rows: state.events.iter().map(EventFact::from).collect(),
            calls: vec![],
            fail: false,
        };
        assert_eq!(
            format!(
                "{:?}",
                validate_with_history((&current).into(), &record, &mut facts)
            ),
            expected,
            "variant {variant}"
        );
        assert_eq!(
            format!("{:?}", validate(&state, &record)),
            expected,
            "State adapter {variant}"
        );
        assert_eq!(facts.calls, submission.candidate.origins);
    }
}

#[test]
fn manual_history_failure_propagates_but_current_decisions_do_not_read_history() {
    let state = State::default().prepare(&common::initial()).unwrap().0;
    let submission = submission();
    let record = row(Collection::Claim, submission.id.as_str(), &submission);
    let mut facts = Facts {
        rows: vec![],
        calls: vec![],
        fail: true,
    };
    let current = crate::CurrentState::from_state(&state);
    assert!(matches!(
        validate_with_history((&current).into(), &record, &mut facts),
        Err(Error::Unavailable("manual history read failed"))
    ));
    let tx = review_tx(&state, record);
    transaction(&current, &tx).unwrap();
    let state = state.prepare(&tx).unwrap().0;
    let current = crate::CurrentState::from_state(&state);
    let decision = decision(&submission, Choice::Reject, Outcome::Rejected);
    let record = row(Collection::Projection, &decision.id, &decision);
    facts.calls.clear();
    validate_with_history((&current).into(), &record, &mut facts).unwrap();
    reference_validate(&state, &record).unwrap();
    transaction(&current, &review_tx(&state, record.clone())).unwrap();
    assert_eq!(
        redact(&current, &record, DeletionEpoch::new(1)).unwrap(),
        redact(&state, &record, DeletionEpoch::new(1)).unwrap()
    );
    assert!(facts.calls.is_empty());
}

#[test]
fn manual_origin_predicate_retains_requested_event_identity() {
    struct WrongId(EventFact);
    impl EventFacts for WrongId {
        fn last(&mut self, _: &EventId) -> Result<Option<EventFact>> {
            panic!("unused")
        }
        fn any(&mut self, _: &EventId, predicate: &dyn Fn(&EventFact) -> bool) -> Result<bool> {
            Ok(predicate(&self.0))
        }
    }
    let state = State::default().prepare(&common::initial()).unwrap().0;
    let submission = submission();
    let row = row(Collection::Claim, submission.id.as_str(), &submission);
    let mut fact = EventFact::from(&state.events[0]);
    fact.id = EventId::new();
    assert!(matches!(
        validate_with_history((&state).into(), &row, &mut WrongId(fact)),
        Err(Error::Corruption("manual submission origin scope"))
    ));
}
