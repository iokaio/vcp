// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::admitted_history::AdmittedCut;
use crate::{
    contract::{
        self,
        current_preparation::{self, Outcome, StateHistory},
        *,
    },
    CurrentState,
};
use std::collections::BTreeSet;
use vcp_domain::{
    artifact::{ArtifactDescriptor, CaptureState, Range},
    forecast, ByteCount, Revision,
};
use vcp_protocol::{canonical_bytes, digest_bytes};
#[path = "../tests/common/mod.rs"]
mod common;

#[derive(Default)]
struct Memory {
    objects: BTreeMap<String, Vec<u8>>,
    reads: usize,
    writes: usize,
    fail: bool,
    corrupt: bool,
    block: bool,
}
impl Pages for Memory {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        self.reads += 1;
        if self.block {
            std::future::pending::<()>().await;
        }
        if self.fail {
            return Err(Error::Unavailable("resolver injected page failure"));
        }
        let mut bytes = self
            .objects
            .get(digest)
            .ok_or(Error::Corruption("resolver page missing"))?
            .clone();
        if bytes.len() > limit {
            return Err(Error::Limit("resolver fixture page size"));
        }
        if self.corrupt && !bytes.is_empty() {
            bytes[0] ^= 1;
        }
        Ok(bytes)
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        self.writes += 1;
        if let Some(prior) = self.objects.get(digest) {
            if prior != bytes {
                return Err(Error::Corruption("resolver immutable page"));
            }
        } else {
            self.objects.insert(digest.into(), bytes.into());
        }
        Ok(())
    }
}
async fn fixture() -> (State, AdmittedCut, Memory) {
    let mut tx = common::initial();
    let original = tx.events[0].clone();
    for index in 1..40 {
        let mut event = original.clone();
        event.id = EventId::parse(format!("event-{index}")).unwrap();
        event.data = serde_json::json!({"payload":"x".repeat(1024)});
        tx.events.push(event);
    }
    let state = State::default().prepare(&tx).unwrap().0;
    let mut pages = Memory::default();
    let catalog = Catalog::from_validated_state(&mut pages, &state)
        .await
        .unwrap();
    let cut = AdmittedCut::from_replayed(&mut pages, &state, catalog)
        .await
        .unwrap();
    pages.reads = 0;
    pages.writes = 0;
    (state, cut, pages)
}

#[tokio::test]
async fn admitted_cut_rejects_mismatched_history_and_commits_reproducible_identity() {
    let (state, cut, mut pages) = fixture().await;
    let again = AdmittedCut::from_replayed(&mut pages, &state, cut.catalog().clone())
        .await
        .unwrap();
    assert_eq!(again.identity(), cut.identity());
    assert_eq!(
        again.current().projection_digest().unwrap(),
        cut.current().projection_digest().unwrap()
    );
    let mut transaction = common::initial();
    transaction.events = state.events.iter().map(|row| row.event.clone()).collect();
    transaction.events[1].data =
        serde_json::json!({"different":"retained history, same current projection"});
    let other = State::default().prepare(&transaction).unwrap().0;
    assert_eq!(other.watermark, state.watermark);
    assert_eq!(
        CurrentState::from_state(&other)
            .projection_digest()
            .unwrap(),
        cut.current().projection_digest().unwrap()
    );
    assert!(
        AdmittedCut::from_replayed(&mut pages, &other, cut.catalog().clone())
            .await
            .is_err(),
        "matching watermarks/current records cannot substitute foreign historical bytes"
    );
    let other_catalog = Catalog::from_validated_state(&mut pages, &other)
        .await
        .unwrap();
    let other_cut = AdmittedCut::from_replayed(&mut pages, &other, other_catalog)
        .await
        .unwrap();
    assert_ne!(
        other_cut.identity(),
        cut.identity(),
        "history roots contribute to admitted identity"
    );
    let advanced = state
        .prepare(&Transaction {
            id: TransactionId::new(),
            expected_watermark: state.watermark,
            mutations: vec![],
            events: vec![],
            command: None,
        })
        .unwrap()
        .0;
    assert!(
        AdmittedCut::from_replayed(&mut pages, &advanced, cut.catalog().clone())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn resolver_only_retries_private_pending() {
    let (_state, cut, mut pages) = fixture().await;
    let mut resolver = ResolvedHistory::new(&cut);
    let mut calls = 0;
    let result: Result<()> = resolver
        .run(&mut pages, |_| {
            calls += 1;
            Err(Error::Io(std::io::Error::other(
                "internal historical obligation",
            )))
        })
        .await;
    assert!(matches!(result, Err(Error::Io(_))));
    assert_eq!(calls, 1);
    assert_eq!(resolver.diagnostics().resolutions, 0);
    assert_eq!(pages.reads, 0);
    // A genuine validation error may be evaluated again; it was not a failed
    // historical read or swallowed obligation.
    resolver.run(&mut pages, |_| Ok(())).await.unwrap();
    let mut forged = ResolvedHistory::new(&cut);
    assert!(matches!(
        forged
            .run::<()>(&mut pages, |_| Err(Error::Io(std::io::Error::other(
                Pending
            ))))
            .await,
        Err(Error::Corruption("historical obligation missing"))
    ));
    assert!(forged.poisoned);
}

#[tokio::test]
async fn swallowed_obligations_never_succeed_or_allow_resolver_reuse() {
    let (state, cut, mut pages) = fixture().await;
    let id = state.events[0].event.id.clone();
    for return_error in [false, true] {
        let mut resolver = ResolvedHistory::new(&cut);
        let result = resolver
            .run(&mut pages, |facts| {
                let _ = facts.last(&id);
                if return_error {
                    Err(Error::Conflict("masked obligation"))
                } else {
                    Ok(())
                }
            })
            .await;
        assert!(result.is_err());
        assert!(resolver.poisoned);
        let reads = pages.reads;
        let mut called = false;
        assert!(matches!(
            resolver
                .run(&mut pages, |_| {
                    called = true;
                    Ok(())
                })
                .await,
            Err(Error::Unavailable("historical resolver failed"))
        ));
        assert!(!called);
        assert_eq!(pages.reads, reads);
    }
}

#[tokio::test]
async fn resolver_caches_only_requested_facts_and_explicit_absence() {
    let (state, cut, mut pages) = fixture().await;
    let mut resolver = ResolvedHistory::new(&cut);
    let id = state.events[1].event.id.clone();
    let absent = EventId::parse("absent-event").unwrap();
    let transaction = state.transactions.keys().next().unwrap().clone();
    let absent_transaction = TransactionId::parse("absent-transaction").unwrap();
    resolver
        .run(&mut pages, |facts| {
            for _ in 0..10 {
                assert_eq!(facts.last(&id)?, Some(EventFact::from(&state.events[1])));
                assert_eq!(facts.last(&absent)?, None);
                assert_eq!(facts.watermark(&transaction)?, Some(state.watermark));
                assert_eq!(facts.watermark(&absent_transaction)?, None);
            }
            Ok(())
        })
        .await
        .unwrap();
    let diagnostics = resolver.diagnostics();
    assert_eq!(diagnostics.passes, 5);
    assert_eq!(diagnostics.resolutions, 4);
    assert_eq!(diagnostics.event_reads, 2);
    assert_eq!(diagnostics.transaction_reads, 2);
    assert_eq!(resolver.facts.len(), 2);
    assert_eq!(resolver.watermarks.len(), 2);
    assert!(
        resolver.envelopes.is_empty()
            && resolver.receipts.is_empty()
            && resolver.commands.is_empty()
            && resolver.sessions.is_empty()
    );
    let reads = pages.reads;
    resolver
        .run(&mut pages, |facts| {
            facts.last(&absent)?;
            facts.watermark(&absent_transaction)?;
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(pages.reads, reads);
    let another = state.events[2].event.id.clone();
    resolver
        .run(&mut pages, |facts| facts.last(&another))
        .await
        .unwrap();
    assert_eq!(resolver.facts.len(), 3);
    assert_eq!(resolver.diagnostics().event_reads, 3);
    assert_eq!(pages.writes, 0);
}

#[tokio::test]
async fn read_failure_corruption_and_cancel_poison_without_publishing() {
    let (state, cut, mut pages) = fixture().await;
    let id = state.events[0].event.id.clone();
    for mode in 0..3 {
        let mut resolver = ResolvedHistory::new(&cut);
        pages.fail = mode == 0;
        pages.corrupt = mode == 1;
        pages.block = mode == 2;
        if mode == 2 {
            assert!(tokio::time::timeout(
                std::time::Duration::from_millis(5),
                resolver.run(&mut pages, |facts| facts.last(&id))
            )
            .await
            .is_err());
        } else {
            let error = resolver
                .run(&mut pages, |facts| facts.last(&id))
                .await
                .unwrap_err();
            if mode == 0 {
                assert!(matches!(
                    error,
                    Error::Unavailable("resolver injected page failure")
                ));
            }
        }
        assert!(resolver.poisoned);
        assert!(resolver.facts.is_empty());
        pages.fail = false;
        pages.corrupt = false;
        pages.block = false;
        let reads = pages.reads;
        assert!(matches!(
            resolver.run(&mut pages, |_| Ok(())).await,
            Err(Error::Unavailable("historical resolver failed"))
        ));
        assert_eq!(pages.reads, reads);
        assert_eq!(pages.writes, 0);
    }
}

fn outcome(value: Result<Outcome>) -> std::result::Result<serde_json::Value, String> {
    value.map(|outcome| match outcome {
        Outcome::Duplicate(receipt) => serde_json::json!({"duplicate":receipt}),
        Outcome::Proposed(value) => serde_json::json!({"current":value.current,"events":value.events,"receipt":value.receipt,"touched":value.touched}),
    }).map_err(|error| error.to_string())
}

#[tokio::test]
async fn source_resolver_proposes_identical_current_changes_and_errors() {
    let (state, cut, mut pages) = fixture().await;
    let source = CurrentState::from_state(&state);
    let frozen_source = canonical_bytes(&state).unwrap();
    for variant in 0..5 {
        let mut transaction = common::initial();
        transaction.id = TransactionId::new();
        transaction.expected_watermark = state.watermark;
        transaction.mutations.clear();
        transaction.events[0].id = EventId::new();
        let command = vcp_domain::CommandId::new();
        transaction.events[0].correlation = command.clone();
        transaction.command.as_mut().unwrap().command = command;
        match variant {
            1 => transaction.expected_watermark = Watermark::ZERO,
            2 => {
                transaction.command.as_mut().unwrap().command =
                    common::initial().command.unwrap().command
            }
            3 => {
                transaction.id = state.transactions.keys().next().unwrap().clone();
            }
            4 => {
                transaction.command.as_mut().unwrap().digest = "invalid".into();
            }
            _ => (),
        }
        let expected = outcome(current_preparation::propose(
            &source,
            &transaction,
            &mut StateHistory(&state),
        ));
        let mut resolver = ResolvedHistory::new(&cut);
        let actual = resolver
            .run(&mut pages, |facts| {
                current_preparation::propose(&source, &transaction, facts)
            })
            .await;
        assert_eq!(outcome(actual), expected, "variant {variant}");
        assert!(resolver.diagnostics().resolutions >= 1);
        assert_eq!(canonical_bytes(&state).unwrap(), frozen_source);
    }
    assert_eq!(pages.writes, 0);
}

#[tokio::test]
async fn real_current_forecast_validation_resolves_only_its_source_event() {
    let (mut state, _, _) = fixture().await;
    let mut spec = common::spec();
    spec.schema = "vcp-optimization-forecast-v1".into();
    let artifact = ArtifactDescriptor {
        spec,
        state: CaptureState::Complete,
        length: ByteCount::ZERO,
        sha256: digest_bytes(b""),
        retained: vec![Range {
            start: ByteCount::ZERO,
            end: ByteCount::ZERO,
        }],
    };
    let mut forecast = forecast::Sources {
        document_type: forecast::SOURCES.into(),
        schema_version: 1,
        id: "resolved-forecast".into(),
        workspace: common::workspace().id,
        revision: Revision::ZERO,
        report: "report".into(),
        artifact: artifact.spec.id.clone(),
        artifact_digest: artifact.sha256.clone(),
        source_tasks: BTreeSet::from([common::task().scope.task]),
        source_records: BTreeSet::new(),
        source_events: BTreeSet::from([state.events[1].event.id.clone()]),
        sources_digest: String::new(),
    };
    forecast.sources_digest = digest_bytes(
        &canonical_bytes(&(
            &forecast.source_tasks,
            &forecast.source_records,
            &forecast.source_events,
        ))
        .unwrap(),
    );
    let row = Record {
        collection: Collection::Projection,
        id: forecast.id.clone(),
        workspace: forecast.workspace.clone(),
        revision: Revision::ZERO,
        value: serde_json::to_value(&forecast).unwrap(),
        references: BTreeSet::from([key(Collection::Task, common::task().scope.task.as_str())]),
    };
    let mut captured = Record::typed(
        Collection::Artifact,
        artifact.spec.id.as_str(),
        forecast.workspace.clone(),
        Revision::ZERO,
        &artifact,
    )
    .unwrap();
    captured.references = BTreeSet::from([
        row.key(),
        key(Collection::Task, common::task().scope.task.as_str()),
    ]);
    state.records.insert(row.key(), row);
    state.records.insert(captured.key(), captured);
    state.validate().unwrap();
    let current = CurrentState::from_state(&state);
    let mut pages = Memory::default();
    let catalog = Catalog::from_validated_state(&mut pages, &state)
        .await
        .unwrap();
    let cut = AdmittedCut::from_replayed(&mut pages, &state, catalog)
        .await
        .unwrap();
    pages.writes = 0;
    let expected = contract::current_validation::validate_records(
        (&current).into(),
        &mut crate::historical_facts::StateEventFacts::new(&state),
    );
    let mut resolver = ResolvedHistory::new(&cut);
    let actual = resolver
        .run(&mut pages, |facts| {
            contract::current_validation::validate_records((&current).into(), facts)
        })
        .await;
    assert_eq!(
        actual.map_err(|error| error.to_string()),
        expected.map_err(|error| error.to_string())
    );
    assert_eq!(resolver.diagnostics().resolutions, 1);
    assert_eq!(
        resolver.facts.keys().cloned().collect::<BTreeSet<_>>(),
        forecast.source_events
    );
    assert!(resolver.envelopes.is_empty());
    assert_eq!(pages.writes, 0);
}

#[tokio::test]
async fn candidate_overlay_matches_append_order_without_contaminating_source_facts() {
    let (state, cut, mut pages) = fixture().await;
    let current = CurrentState::from_state(&state);
    let duplicate = state.events[0].event.id.clone();
    // Inside the retained key range so proving absence needs a real page;
    // out-of-range absence can legitimately be proved by the admitted root.
    let fresh = EventId::parse("event-1-candidate").unwrap();
    let missing = EventId::parse("absent-everywhere").unwrap();
    let mut transaction = common::initial();
    transaction.id = TransactionId::new();
    transaction.expected_watermark = state.watermark;
    transaction.mutations.clear();
    transaction.command = None;
    let mut first = state.events[0].event.clone();
    first.task = Some(vcp_domain::TaskId::parse("candidate-first").unwrap());
    let mut last = first.clone();
    last.task = Some(vcp_domain::TaskId::parse("candidate-last").unwrap());
    let mut added = last.clone();
    added.id = fresh.clone();
    transaction.events = vec![first, last, added];
    let proposed =
        match current_preparation::propose(&current, &transaction, &mut StateHistory(&state))
            .unwrap()
        {
            Outcome::Proposed(value) => value,
            Outcome::Duplicate(_) => panic!("new proposal expected"),
        };
    // Duplicate IDs deliberately test standalone reference predicate semantics;
    // this is not a valid/publishable transition. Global event checks still
    // reject this append in the full canonical path.
    let mut appended = state.clone();
    appended.events.extend(proposed.events.clone());
    appended.transactions.insert(
        proposed.receipt.transaction.clone(),
        proposed.receipt.clone(),
    );
    let mut resolver = ResolvedHistory::new(&cut);
    resolver
        .run(&mut pages, |source| {
            assert_eq!(source.last(&fresh)?, None);
            assert_eq!(source.watermark(&proposed.receipt.transaction)?, None);
            let mut overlay = CandidateHistory::new(source, &proposed);
            let mut reference = crate::historical_facts::StateEventFacts::new(&appended);
            for id in [&duplicate, &fresh, &missing, &state.events[1].event.id] {
                assert_eq!(overlay.last(id)?, reference.last(id)?);
                for task in ["task", "candidate-first", "candidate-last", "never"] {
                    let matches = |event: &EventFact| {
                        event
                            .task
                            .as_ref()
                            .is_some_and(|value| value.as_str() == task)
                    };
                    assert_eq!(overlay.any(id, &matches)?, reference.any(id, &matches)?);
                }
            }
            assert_eq!(
                overlay.watermark(&proposed.receipt.transaction)?,
                Some(proposed.receipt.watermark)
            );
            assert_eq!(
                overlay.watermark(state.transactions.keys().next().unwrap())?,
                Some(state.watermark)
            );
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(
        resolver.facts.get(&duplicate),
        Some(&Some(EventFact::from(&state.events[0])))
    );
    assert_eq!(resolver.facts.get(&fresh), Some(&None));
    assert_eq!(
        resolver.watermarks.get(&proposed.receipt.transaction),
        Some(&None)
    );
    assert!(resolver.envelopes.is_empty() && resolver.receipts.is_empty());
    assert_eq!(pages.writes, 0);

    let mut failing = ResolvedHistory::new(&cut);
    pages.fail = true;
    let reads = pages.reads;
    failing
        .run(&mut pages, |source| {
            CandidateHistory::new(source, &proposed).last(&fresh)
        })
        .await
        .unwrap();
    assert_eq!(
        pages.reads, reads,
        "last appended fact needs no source read"
    );
    assert!(
        matches!(
            failing
                .run(&mut pages, |source| CandidateHistory::new(
                    source, &proposed
                )
                .any(&fresh, &|_| true))
                .await,
            Err(Error::Unavailable("resolver injected page failure"))
        ),
        "any preserves the source-first fallible order even when an appended fact matches"
    );
    assert!(failing.poisoned);
}
