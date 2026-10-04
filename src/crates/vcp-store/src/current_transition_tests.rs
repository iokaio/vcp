// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{contract::*, history_catalog::Catalog, CurrentState};
use std::collections::BTreeMap;
use vcp_domain::{CommandId, EventId, Revision, SessionId, TransactionId, Watermark};
#[path = "../tests/common/mod.rs"]
mod common;

#[derive(Clone, Default)]
struct Memory {
    objects: BTreeMap<String, Vec<u8>>,
    reads: usize,
    failed: bool,
    writes_remaining: Option<usize>,
}
impl Pages for Memory {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        self.reads += 1;
        if self.failed {
            return Err(Error::Unavailable("candidate injected read"));
        }
        let bytes = self
            .objects
            .get(digest)
            .ok_or(Error::Corruption("candidate missing object"))?;
        if bytes.len() > limit {
            return Err(Error::Limit("candidate object read"));
        }
        Ok(bytes.clone())
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        if let Some(remaining) = self.writes_remaining.as_mut() {
            if *remaining == 0 {
                return Err(Error::Unavailable("candidate injected write"));
            }
            *remaining -= 1;
        }
        if let Some(existing) = self.objects.get(digest) {
            assert_eq!(existing, bytes);
        } else {
            self.objects.insert(digest.to_owned(), bytes.to_vec());
        }
        Ok(())
    }
}
fn next(source: &State, index: usize) -> Transaction {
    let mut tx = common::initial();
    tx.id = TransactionId::parse(format!("current-transition-{index}")).unwrap();
    tx.expected_watermark = source.watermark;
    let mut task: vcp_domain::task::Task = source
        .record(Collection::Task, "task", &common::workspace().id)
        .unwrap()
        .decode()
        .unwrap();
    let prior = task.revision;
    task.revision = prior.next().unwrap();
    task.reason = format!("durable candidate {index}");
    tx.mutations = vec![Mutation::Put {
        expected: Some(prior),
        record: Record::typed(
            Collection::Task,
            "task",
            task.scope.workspace.clone(),
            task.revision,
            &task,
        )
        .unwrap(),
    }];
    tx.events[0].id = EventId::parse(format!("current-transition-event-{index}")).unwrap();
    tx.events[0].correlation =
        CommandId::parse(format!("current-transition-command-{index}")).unwrap();
    let command = tx.command.as_mut().unwrap();
    command.command = tx.events[0].correlation.clone();
    command.digest = format!("{index:064x}");
    tx
}
fn archival(source: &State, prepared: &PreparedCurrent) -> State {
    let proposed = prepared.proposed();
    let mut state = State {
        watermark: proposed.current.watermark,
        records: proposed.current.records.clone(),
        sequences: proposed.current.sequences.clone(),
        events: source.events.clone(),
        commands: source.commands.clone(),
        transactions: source.transactions.clone(),
    };
    state.events.extend(proposed.events.iter().cloned());
    if let Some(command) = &proposed.receipt.command {
        state.commands.insert(
            super::super::command_key(&command.workspace, &command.command),
            command.clone(),
        );
    }
    state.transactions.insert(
        proposed.receipt.transaction.clone(),
        proposed.receipt.clone(),
    );
    state
}
async fn compare(pages: &mut Memory, cut: &AdmittedCut, source: &State, tx: &Transaction) {
    let expected = source
        .prepare_reference(tx)
        .map_err(|error| error.to_string());
    let actual = prepare(pages, cut, tx)
        .await
        .map(|outcome| match outcome {
            Outcome::Duplicate(receipt) => (
                source.clone(),
                Commit {
                    version: FORMAT_VERSION,
                    transaction: tx.clone(),
                    receipt,
                },
            ),
            Outcome::Prepared(prepared) => {
                assert_eq!(prepared.source_identity(), cut.identity());
                let work = prepared.history_work();
                assert_eq!(work.full_passes, 2);
                assert_eq!(work.rows, (source.events.len() * 2) as u64);
                assert!(work.maximum_page_rows <= 64);
                assert_eq!(
                    prepared.size().bytes(),
                    CurrentSize::measure((&prepared.proposed().current).into())
                        .unwrap()
                        .bytes()
                );
                (archival(source, &prepared), prepared.commit().clone())
            }
        })
        .map_err(|error| error.to_string());
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn full_current_pipeline_matches_frozen_preparation_across_history_pages() {
    let mut source = State::default();
    let mut initial = common::initial();
    let first = initial.events[0].clone();
    initial.events = (0..70)
        .map(|index| {
            let mut event = first.clone();
            event.id = EventId::parse(format!("initial-event-{index}")).unwrap();
            event
        })
        .collect();
    let mut pages = Memory::default();
    let catalog = Catalog::from_validated_state(&mut pages, &source)
        .await
        .unwrap();
    let cut = AdmittedCut::from_replayed(&mut pages, &source, catalog)
        .await
        .unwrap();
    compare(&mut pages, &cut, &source, &initial).await;
    source = source.prepare_reference(&initial).unwrap().0;
    for index in 0..8 {
        let mut pages = Memory::default();
        let catalog = Catalog::from_validated_state(&mut pages, &source)
            .await
            .unwrap();
        let cut = AdmittedCut::from_replayed(&mut pages, &source, catalog)
            .await
            .unwrap();
        compare(&mut pages, &cut, &source, &initial).await;
        let tx = next(&source, index);
        for invalid in 0..9 {
            let mut rejected = tx.clone();
            match invalid {
                0 => rejected.expected_watermark = Watermark::ZERO,
                1 => rejected.events[0].id = EventId::parse("initial-event-0").unwrap(),
                2 => {
                    rejected.events[0].correlation = CommandId::parse("create").unwrap();
                    rejected.command.as_mut().unwrap().command =
                        CommandId::parse("create").unwrap();
                }
                3 => rejected.command.as_mut().unwrap().digest = "g".repeat(64),
                4 => rejected.events[0].session = SessionId::parse("other").unwrap(),
                5 => rejected.mutations.push(rejected.mutations[0].clone()),
                6 => {
                    if let Mutation::Put { record, .. } = &mut rejected.mutations[0] {
                        record.revision = Revision::ZERO;
                    }
                }
                7 => rejected.id = initial.id.clone(),
                8 => {
                    rejected.events[0].task =
                        Some(vcp_domain::TaskId::parse("missing-task").unwrap())
                }
                _ => unreachable!(),
            }
            compare(&mut pages, &cut, &source, &rejected).await;
        }
        compare(&mut pages, &cut, &source, &tx).await;
        source = source.prepare_reference(&tx).unwrap().0;
    }
}

#[tokio::test]
async fn preparation_owns_no_archival_state_and_unavailable_history_cannot_issue_certificate() {
    let source = State::default()
        .prepare_reference(&common::initial())
        .unwrap()
        .0;
    let tx = next(&source, 0);
    let expected = source.prepare_reference(&tx).unwrap();
    let mut pages = Memory::default();
    let catalog = Catalog::from_validated_state(&mut pages, &source)
        .await
        .unwrap();
    let cut = AdmittedCut::from_replayed(&mut pages, &source, catalog)
        .await
        .unwrap();
    drop(source);
    let result = prepare(&mut pages, &cut, &tx).await.unwrap();
    let Outcome::Prepared(result) = result else {
        panic!("new receipt expected")
    };
    assert_eq!(
        result.proposed().current,
        CurrentState::from_state(&expected.0)
    );
    assert_eq!(result.commit(), &expected.1);
    assert_eq!(cut.current().watermark.get(), 1);
    pages.failed = true;
    assert!(matches!(
        prepare(&mut pages, &cut, &tx).await,
        Err(Error::Unavailable(_))
    ));
    pages.failed = false;
    for bytes in pages.objects.values_mut() {
        bytes[0] ^= 1;
    }
    assert!(prepare(&mut pages, &cut, &tx).await.is_err());
    assert_eq!(cut.current().watermark.get(), 1);
}

#[tokio::test]
async fn catalog_append_preserves_exact_reference_rows_and_failed_staging_keeps_prior_cut() {
    let source = State::default()
        .prepare_reference(&common::initial())
        .unwrap()
        .0;
    let tx = next(&source, 7);
    let expected = source.prepare_reference(&tx).unwrap().0;
    let mut pages = Memory::default();
    let catalog = Catalog::from_validated_state(&mut pages, &source)
        .await
        .unwrap();
    let cut = AdmittedCut::from_replayed(&mut pages, &source, catalog)
        .await
        .unwrap();
    let Outcome::Prepared(prepared) = prepare(&mut pages, &cut, &tx).await.unwrap() else {
        panic!("new receipt expected")
    };
    let identity = cut.identity().to_owned();
    for allowed in [0, 1, 4] {
        let mut failing = pages.clone();
        failing.writes_remaining = Some(allowed);
        assert!(matches!(
            cut.catalog()
                .append_current(&mut failing, &cut, &prepared)
                .await,
            Err(Error::Unavailable(_))
        ));
        assert_eq!(cut.identity(), identity);
        cut.catalog()
            .verify_replayed_state(&mut failing, &source)
            .await
            .unwrap();
    }
    let appended = cut
        .catalog()
        .append_current(&mut pages, &cut, &prepared)
        .await
        .unwrap();
    appended
        .verify_replayed_state(&mut pages, &expected)
        .await
        .unwrap();
    assert_eq!(
        appended
            .legacy_digest(&mut pages, (&prepared.proposed().current).into())
            .await
            .unwrap(),
        crate::legacy_state_stream::digest(&expected).unwrap()
    );
    assert_eq!(cut.identity(), identity);
    cut.catalog()
        .verify_replayed_state(&mut pages, &source)
        .await
        .unwrap();
}
