// SPDX-License-Identifier: Apache-2.0
//! Qualify admitted send-witness reuse against the complete accounting pass.
use super::history_test_common as common;
use super::*;
use crate::historical_facts::{EventFact, EventFacts, StateEventFacts};
use crate::{admitted_history::AdmittedCut, history_catalog::Catalog, history_index::Pages};
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
    let mut initial = common::initial();
    let mut send = initial.events[0].clone();
    send.id = EventId::parse("send-00").unwrap();
    send.kind = vcp_protocol::event::EventKind::AttemptSubmitted;
    initial.events.push(send);
    let mut state = State::default().prepare(&initial).unwrap().0;
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
    for index in 0..1 {
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
    }
    state.validate().unwrap();
    state
}

#[derive(Default)]
struct Memory(BTreeMap<String, Vec<u8>>);
impl Pages for Memory {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        let bytes = self
            .0
            .get(digest)
            .ok_or(Error::Corruption("test page missing"))?;
        if bytes.len() > limit {
            return Err(Error::Limit("test page"));
        }
        Ok(bytes.clone())
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        if let Some(prior) = self.0.insert(digest.into(), bytes.into()) {
            assert_eq!(prior, bytes);
        }
        Ok(())
    }
}
async fn admitted(state: &State) -> (Memory, AdmittedCut) {
    let mut pages = Memory::default();
    let catalog = Catalog::from_validated_state(&mut pages, state)
        .await
        .unwrap();
    let cut = AdmittedCut::from_replayed(&mut pages, state, catalog)
        .await
        .unwrap();
    (pages, cut)
}
struct CountingFacts<'a> {
    state: &'a State,
    visits: usize,
    fail: bool,
}
impl EventFacts for CountingFacts<'_> {
    fn last(&mut self, _: &EventId) -> Result<Option<EventFact>> {
        panic!("send witness requires any-match semantics")
    }
    fn any(&mut self, id: &EventId, predicate: &dyn Fn(&EventFact) -> bool) -> Result<bool> {
        self.visits += 1;
        if self.fail {
            return Err(Error::Unavailable("test witness read"));
        }
        StateEventFacts::new(self.state).any(id, predicate)
    }
}
fn check(
    state: &State,
    cut: &AdmittedCut,
    appended: &[vcp_protocol::event::EventEnvelope],
    fail: bool,
) -> (std::result::Result<bool, String>, usize) {
    let mut facts = CountingFacts {
        state,
        visits: 0,
        fail,
    };
    let result = (|| {
        let validation = Validation::new(state.into())?;
        let attempt = validation.attempts().next().unwrap();
        let reused = validation.attempt_admitted(attempt, &mut facts, cut, appended)?;
        validation.finish()?;
        Ok(reused)
    })();
    (
        result.map_err(|error: Error| error.to_string()),
        facts.visits,
    )
}

#[tokio::test]
async fn accounting_admitted_unchanged_witness_reuses_without_history_reads() {
    let state = fixture();
    let (_, cut) = admitted(&state).await;
    assert_eq!(check(&state, &cut, &[], true), (Ok(true), 0));
    validate(&state).unwrap();
}

#[tokio::test]
async fn accounting_admitted_changed_new_and_duplicate_witnesses_use_original_lookup() {
    let state = fixture();
    let (_, cut) = admitted(&state).await;
    let mut changed = state.clone();
    changed.records.get_mut("attempt:attempt-00").unwrap().value["uncertain"] =
        serde_json::json!("awaiting observation");
    assert_eq!(check(&changed, &cut, &[], false), (Ok(false), 1));
    validate(&changed).unwrap();
    let (_, empty) = admitted(&State::default()).await;
    assert_eq!(check(&state, &empty, &[], false), (Ok(false), 1));
    assert_eq!(
        check(&state, &cut, &state.events[1..], false),
        (Ok(false), 1)
    );
    for (candidate, source, appended) in [
        (&changed, &cut, &[][..]),
        (&state, &empty, &[][..]),
        (&state, &cut, &state.events[1..]),
    ] {
        let (result, visits) = check(candidate, source, appended, true);
        assert_eq!(
            result,
            Err(Error::Unavailable("test witness read").to_string())
        );
        assert_eq!(visits, 1);
    }
}

#[tokio::test]
async fn accounting_admitted_rechecks_current_dependencies_and_error_order() {
    let state = fixture();
    let (_, cut) = admitted(&state).await;
    let attempt: Attempt = state.records["attempt:attempt-00"].decode().unwrap();
    for variant in 0..4 {
        let mut changed = state.clone();
        match variant {
            0 => {
                changed
                    .records
                    .get_mut(&key(Collection::Artifact, attempt.request.as_str()))
                    .unwrap()
                    .value["sha256"] = serde_json::json!("f".repeat(64))
            }
            1 => {
                changed
                    .records
                    .get_mut("reservation:reservation-00")
                    .unwrap()
                    .value["root"] = serde_json::json!("missing-root")
            }
            2 => {
                changed.records.get_mut("ledger:task").unwrap().value["currency"] =
                    serde_json::json!("EUR")
            }
            _ => {
                changed.records.get_mut("ledger:task").unwrap().value["overrun"] =
                    serde_json::json!(true)
            }
        }
        let expected = validate(&changed).unwrap_err().to_string();
        let (actual, visits) = check(&changed, &cut, &[], true);
        assert_eq!(actual, Err(expected), "variant {variant}");
        assert_eq!(visits, 0, "variant {variant}");
    }
}

#[tokio::test]
async fn accounting_admitted_preparation_matches_reference_and_reports_saved_lookups() {
    use crate::contract::current_transition::{prepare_observed, Outcome};
    let state = fixture();
    let (mut pages, cut) = admitted(&state).await;
    let mut tx = common::initial();
    tx.id = TransactionId::new();
    tx.expected_watermark = state.watermark;
    tx.mutations.clear();
    tx.events.clear();
    tx.command = None;
    let expected = state.prepare_reference(&tx).unwrap();
    let mut diagnostics = crate::StoreDiagnostics::new(crate::BackendKind::Files);
    let Outcome::Prepared(prepared) =
        prepare_observed(&mut pages, &cut, &tx, Some(&mut diagnostics))
            .await
            .unwrap()
    else {
        panic!("fresh transaction")
    };
    assert_eq!(prepared.commit(), &expected.1);
    assert_eq!(
        prepared.proposed().current,
        crate::CurrentState::from_state(&expected.0)
    );
    assert_eq!(diagnostics.accounting_send_validation.prefix_reuses, 1);
    assert_eq!(diagnostics.accounting_send_validation.lookups, 0);
    assert_eq!(
        diagnostics.validation_history_reads.accounting,
        crate::HistoryReads::default()
    );

    let mut attempt: Attempt = state.records["attempt:attempt-00"].decode().unwrap();
    let previous = attempt.revision;
    attempt.revision = previous.next().unwrap();
    attempt.uncertain = Some("awaiting observation".into());
    tx.id = TransactionId::new();
    tx.mutations.push(Mutation::Put {
        expected: Some(previous),
        record: Record::typed(
            Collection::Attempt,
            attempt.id.to_string(),
            attempt.scope.workspace.clone(),
            attempt.revision,
            &attempt,
        )
        .unwrap(),
    });
    let expected = state.prepare_reference(&tx).unwrap();
    let mut diagnostics = crate::StoreDiagnostics::new(crate::BackendKind::Files);
    let Outcome::Prepared(prepared) =
        prepare_observed(&mut pages, &cut, &tx, Some(&mut diagnostics))
            .await
            .unwrap()
    else {
        panic!("fresh attempt update")
    };
    assert_eq!(prepared.commit(), &expected.1);
    assert_eq!(
        prepared.proposed().current,
        crate::CurrentState::from_state(&expected.0)
    );
    assert_eq!(diagnostics.accounting_send_validation.prefix_reuses, 0);
    assert_eq!(diagnostics.accounting_send_validation.lookups, 1);

    for (field, invalid) in [
        ("request_digest", "e".repeat(64)),
        ("send_intent", "missing-send-witness".to_owned()),
    ] {
        let mut rejected = tx.clone();
        rejected.id = TransactionId::new();
        let Mutation::Put { record, .. } = &mut rejected.mutations[0] else {
            panic!("attempt update")
        };
        record.value[field] = serde_json::json!(invalid);
        let expected = state.prepare_reference(&rejected).unwrap_err().to_string();
        let mut diagnostics = crate::StoreDiagnostics::new(crate::BackendKind::Files);
        let actual = prepare_observed(&mut pages, &cut, &rejected, Some(&mut diagnostics))
            .await
            .err()
            .unwrap()
            .to_string();
        assert_eq!(actual, expected, "invalid {field}");
        assert_eq!(diagnostics.accounting_send_validation.prefix_reuses, 0);
        assert_eq!(diagnostics.accounting_send_validation.lookups, 0);
        assert_eq!(cut.current(), &crate::CurrentState::from_state(&state));
    }
}
