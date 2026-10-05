// SPDX-License-Identifier: Apache-2.0
//! Delay individual historical facts and compare whole-pass and per-attempt
//! resumption, including competing corruptions and physical read failure.
use super::history_test_common as common;
use super::*;
use crate::historical_facts::{EventFact, EventFacts, StateEventFacts};
use std::collections::BTreeSet;
use vcp_domain::{ByteCount, Micros, PolicyRevision, Revision, SteeringRevision, Timestamp, Units};

fn put(state: &mut State, collection: Collection, id: &str, value: &impl serde::Serialize) {
    let row = Record::typed(
        collection,
        id,
        common::workspace().id,
        Revision::ZERO,
        value,
    )
    .unwrap();
    state.records.insert(row.key(), row);
}
fn fixture() -> State {
    let mut state = State::default().prepare(&common::initial()).unwrap().0;
    let scope = common::task().scope;
    let currency: Currency = "USD".to_owned().try_into().unwrap();
    let ledger = Ledger {
        schema_version: 1,
        scope: scope.clone(),
        revision: Revision::ZERO,
        policy: PolicyRevision::ZERO,
        currency: currency.clone(),
        cap: vcp_domain::Limit::Unbounded,
        protected: Micros::ZERO,
        settled: Micros::ZERO,
        active: EstimatedMicros::ZERO,
        unresolved: EstimatedMicros::ZERO,
        allocations: BTreeMap::new(),
        daily: None,
        overrun: false,
    };
    put(&mut state, Collection::Ledger, scope.task.as_str(), &ledger);
    let artifact = ArtifactDescriptor {
        spec: common::spec(),
        state: CaptureState::Complete,
        length: ByteCount::ZERO,
        sha256: "a".repeat(64),
        retained: vec![vcp_domain::artifact::Range {
            start: ByteCount::ZERO,
            end: ByteCount::ZERO,
        }],
    };
    put(
        &mut state,
        Collection::Artifact,
        artifact.spec.id.as_str(),
        &artifact,
    );
    for index in 0..12 {
        let id = AttemptId::parse(format!("attempt-{index:02}")).unwrap();
        let reservation = ReservationId::parse(format!("reservation-{index:02}")).unwrap();
        let mut event = state.events[0].clone();
        event.event.id = EventId::parse(format!("send-{index:02}")).unwrap();
        event.event.kind = vcp_protocol::event::EventKind::AttemptSubmitted;
        let attempt = Attempt {
            schema_version: 1,
            redaction: None,
            redacted_at_revision: None,
            id: id.clone(),
            scope: scope.clone(),
            root: scope.task.clone(),
            reservation: reservation.clone(),
            revision: Revision::ZERO,
            phase: ReservationState::Submitted,
            role: RequestRole::Main,
            agent: AgentId::new(),
            previous: None,
            request: artifact.spec.id.clone(),
            request_digest: artifact.sha256.clone(),
            admission_digest: "b".repeat(64),
            steering: SteeringRevision::ZERO,
            admitted_policy: PolicyRevision::ZERO,
            send_intent: Some(event.event.id.clone()),
            observation_mode: None,
            usage_watermark: Units::ZERO,
            charged: Micros::ZERO,
            uncertain: None,
            provider_request: None,
            quote: CostQuote {
                normalization_version: 1,
                method: "ceil_disjoint_bounds_v1".into(),
                bounds: Usage::default(),
                amount: Money {
                    currency: currency.clone(),
                    micros: Micros::ZERO,
                }
                .into(),
                price: PriceSnapshot {
                    id: "c".repeat(64),
                    provider: "fixture".into(),
                    model: "fixture".into(),
                    currency: currency.clone(),
                    capability: "d".repeat(64),
                    valid_until: Timestamp::new(u64::MAX),
                    rates: [
                        ChargeCategory::Input,
                        ChargeCategory::Output,
                        ChargeCategory::CacheRead,
                        ChargeCategory::CacheWrite,
                        ChargeCategory::Request,
                        ChargeCategory::ProviderTool,
                    ]
                    .into_iter()
                    .map(|kind| {
                        (
                            kind,
                            Rate {
                                micros: Micros::ZERO,
                                per_units: Units::new(1),
                            },
                        )
                    })
                    .collect(),
                },
            },
        };
        let held = Reservation {
            schema_version: 1,
            id: reservation.clone(),
            scope: scope.clone(),
            root: scope.task.clone(),
            attempt: id.clone(),
            revision: Revision::ZERO,
            phase: attempt.phase,
            amount: attempt.quote.amount.clone(),
            charged: Micros::ZERO,
            liability: EstimatedMicros::ZERO,
            protected_draw: Micros::ZERO,
            protected_returned: Micros::ZERO,
            day: 0,
            role: RequestRole::Main,
        };
        put(&mut state, Collection::Attempt, id.as_str(), &attempt);
        put(
            &mut state,
            Collection::Reservation,
            reservation.as_str(),
            &held,
        );
        state.events.push(event);
    }
    validate(&state).unwrap();
    state
}
struct Delayed<'a> {
    state: &'a State,
    loaded: BTreeSet<EventId>,
    pending: Option<EventId>,
    failed: Option<EventId>,
    visits: usize,
}
impl EventFacts for Delayed<'_> {
    fn last(&mut self, _: &EventId) -> Result<Option<EventFact>> {
        panic!("accounting uses any-match")
    }
    fn any(&mut self, id: &EventId, predicate: &dyn Fn(&EventFact) -> bool) -> Result<bool> {
        self.visits += 1;
        if self.failed.as_ref() == Some(id) {
            return Err(Error::Unavailable("physical fact read"));
        }
        if !self.loaded.contains(id) {
            self.pending = Some(id.clone());
            return Err(Error::Unavailable("delayed fact"));
        }
        StateEventFacts::new(self.state).any(id, predicate)
    }
}
fn resolve<T>(
    facts: &mut Delayed<'_>,
    mut operation: impl FnMut(&mut Delayed<'_>) -> Result<T>,
) -> Result<T> {
    loop {
        let result = operation(facts);
        if let Some(id) = facts.pending.take() {
            assert!(matches!(result, Err(Error::Unavailable("delayed fact"))));
            assert!(facts.loaded.insert(id));
        } else {
            return result;
        }
    }
}
fn run(
    state: &State,
    per_attempt: bool,
    failed: Option<EventId>,
) -> (std::result::Result<(), String>, usize) {
    let mut facts = Delayed {
        state,
        loaded: BTreeSet::new(),
        pending: None,
        failed,
        visits: 0,
    };
    let result = (|| {
        if per_attempt {
            let validation = Validation::new(state.into())?;
            for attempt in validation.attempts() {
                resolve(&mut facts, |facts| validation.attempt(attempt, facts))?;
            }
            validation.finish()
        } else {
            resolve(&mut facts, |facts| {
                validate_with_history(state.into(), facts)
            })
        }
    })();
    (
        result.map_err(|error: Error| error.to_string()),
        facts.visits,
    )
}
#[test]
fn delayed_facts_resume_only_the_current_attempt_and_preserve_error_order() {
    let source = fixture();
    let old = run(&source, false, None);
    let current = run(&source, true, None);
    assert_eq!(old.0, current.0);
    assert!(current.0.is_ok());
    assert_eq!(old.1, 90);
    assert_eq!(current.1, 24);
    for variant in 0..6 {
        let mut state = source.clone();
        match variant {
            0 => {
                state.events[1].event.workspace = WorkspaceId::new();
            }
            1 => {
                state.events.remove(2);
            }
            2 => {
                state.records.get_mut("attempt:attempt-05").unwrap().value["request_digest"] =
                    serde_json::json!("e".repeat(64));
            }
            3 => {
                state.records.get_mut("attempt:attempt-05").unwrap().value["charged"] =
                    serde_json::json!("1");
            }
            4 => {
                state.records.get_mut("ledger:task").unwrap().value["active"] =
                    serde_json::json!("1");
            }
            _ => {
                state.events[1].event.workspace = WorkspaceId::new();
                state.records.get_mut("attempt:attempt-05").unwrap().value["charged"] =
                    serde_json::json!("1");
            }
        }
        let expected = validate(&state).map_err(|error| error.to_string());
        assert!(expected.is_err());
        assert_eq!(run(&state, false, None).0, expected);
        assert_eq!(run(&state, true, None).0, expected);
    }
    let failure = Some(EventId::parse("send-05").unwrap());
    assert_eq!(
        run(&source, false, failure.clone()).0,
        run(&source, true, failure).0
    );
}
