// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    contract::{
        current_transition::{prepare, Outcome, PreparedCurrent},
        *,
    },
    Error,
};
use std::collections::BTreeMap;
use vcp_domain::{CommandId, EventId, TransactionId};
#[path = "../tests/common/mod.rs"]
mod common;

#[derive(Default)]
struct Memory {
    objects: BTreeMap<String, Vec<u8>>,
    writes: usize,
    fail_after: Option<usize>,
    block_after: Option<usize>,
}
impl Pages for Memory {
    async fn read(&mut self, digest: &str, limit: usize) -> Result<Vec<u8>> {
        let bytes = self
            .objects
            .get(digest)
            .ok_or(Error::Corruption("advance missing page"))?;
        if bytes.len() > limit {
            return Err(Error::Limit("advance page"));
        }
        Ok(bytes.clone())
    }
    async fn write(&mut self, digest: &str, bytes: &[u8]) -> Result<()> {
        self.writes += 1;
        if self.block_after.is_some_and(|n| self.writes >= n) {
            std::future::pending::<()>().await;
        }
        if self.fail_after.is_some_and(|n| self.writes >= n) {
            return Err(Error::Unavailable("advance injected write"));
        }
        if let Some(prior) = self.objects.get(digest) {
            if prior != bytes {
                return Err(Error::Corruption("advance immutable page"));
            }
        } else {
            self.objects.insert(digest.into(), bytes.into());
        }
        Ok(())
    }
}
async fn admit(pages: &mut Memory, source: &State) -> AdmittedCut {
    let catalog = Catalog::from_validated_state(pages, source).await.unwrap();
    AdmittedCut::from_replayed(pages, source, catalog)
        .await
        .unwrap()
}
fn append(source: &State) -> Transaction {
    let mut tx = common::initial();
    tx.id = TransactionId::parse("advance-next").unwrap();
    tx.expected_watermark = source.watermark;
    tx.mutations.clear();
    tx.events[0].id = EventId::parse("advance-next-event").unwrap();
    tx.events[0].correlation = CommandId::parse("advance-next-command").unwrap();
    let command = tx.command.as_mut().unwrap();
    command.command = tx.events[0].correlation.clone();
    command.digest = "b".repeat(64);
    tx
}
async fn prepared(pages: &mut Memory, cut: &AdmittedCut, tx: &Transaction) -> PreparedCurrent {
    match prepare(pages, cut, tx).await.unwrap() {
        Outcome::Prepared(value) => value,
        Outcome::Duplicate(_) => panic!("new transition required"),
    }
}

#[tokio::test]
async fn advancing_cut_matches_replayed_history_without_replacing_source() {
    let source = State::default().prepare(&common::initial()).unwrap().0;
    let mut pages = Memory::default();
    let cut = admit(&mut pages, &source).await;
    let source_identity = cut.identity().to_owned();
    let tx = append(&source);
    let certificate = prepared(&mut pages, &cut, &tx).await;
    let next = cut.advance(&mut pages, &certificate).await.unwrap();
    let expected = source.prepare(&tx).unwrap().0;
    assert_eq!(next.current(), &CurrentState::from_state(&expected));
    next.catalog()
        .verify_replayed_state(&mut pages, &expected)
        .await
        .unwrap();
    // Identity binds the actual descriptor tree, whose shape can depend on
    // insertion order. Re-admission must reproduce this exact stored cut.
    let rebuilt = AdmittedCut::from_replayed(&mut pages, &expected, next.catalog().clone())
        .await
        .unwrap();
    assert_eq!(next.identity(), rebuilt.identity());
    assert_ne!(next.identity(), source_identity);
    assert_eq!(cut.identity(), source_identity);
    cut.catalog()
        .verify_replayed_state(&mut pages, &source)
        .await
        .unwrap();
    assert!(
        matches!(prepare(&mut pages, &next, &tx).await.unwrap(), Outcome::Duplicate(ref receipt) if receipt == &certificate.commit().receipt)
    );
    let writes = pages.writes;
    assert!(next.advance(&mut pages, &certificate).await.is_err());
    assert_eq!(
        pages.writes, writes,
        "stale certificate must reject before writes"
    );
}

#[tokio::test]
async fn advancement_rejects_other_history_and_preserves_cut_on_failed_or_cancelled_writes() {
    let source = State::default().prepare(&common::initial()).unwrap().0;
    let mut pages = Memory::default();
    let cut = admit(&mut pages, &source).await;
    let tx = append(&source);
    let certificate = prepared(&mut pages, &cut, &tx).await;
    let mut other_tx = common::initial();
    other_tx.events[0].data = serde_json::json!({"different": "retained evidence"});
    let other_state = State::default().prepare(&other_tx).unwrap().0;
    let other = admit(&mut pages, &other_state).await;
    assert_eq!(cut.current(), other.current());
    let writes = pages.writes;
    assert!(other.advance(&mut pages, &certificate).await.is_err());
    assert_eq!(pages.writes, writes);
    let original_identity = cut.identity().to_owned();
    pages.fail_after = Some(pages.writes + 2);
    assert!(matches!(
        cut.advance(&mut pages, &certificate).await,
        Err(Error::Unavailable("advance injected write"))
    ));
    pages.fail_after = None;
    cut.catalog()
        .verify_replayed_state(&mut pages, &source)
        .await
        .unwrap();
    pages.block_after = Some(pages.writes + 2);
    assert!(tokio::time::timeout(
        std::time::Duration::from_millis(10),
        cut.advance(&mut pages, &certificate)
    )
    .await
    .is_err());
    pages.block_after = None;
    assert_eq!(cut.identity(), original_identity);
    cut.catalog()
        .verify_replayed_state(&mut pages, &source)
        .await
        .unwrap();
    let retried = cut.advance(&mut pages, &certificate).await.unwrap();
    let expected = source.prepare(&tx).unwrap().0;
    retried
        .catalog()
        .verify_replayed_state(&mut pages, &expected)
        .await
        .unwrap();
}
