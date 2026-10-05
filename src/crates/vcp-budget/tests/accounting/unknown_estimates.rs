// SPDX-License-Identifier: Apache-2.0
use super::*;

#[tokio::test]
async fn unknown_estimates_require_unbounded_and_preserve_send_settlement_and_reopen_fences() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temporary = tempfile::tempdir().unwrap();
        let mut store = setup(temporary.path(), kind, 100, 0).await;
        let scope = common::task().scope;
        let request = capture(&mut store, &scope, b"unpriced request").await;
        let mut input = admission(&store, &scope, &request, 20);
        let legacy = vcp_protocol::canonical_bytes(&input.quote).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&legacy).unwrap()["amount"]["micros"],
            "20"
        );
        let mut prices = input.quote.price.clone();
        prices.rates.remove(&ChargeCategory::Output);
        input.quote = quote(prices, input.quote.bounds.clone(), actor().now).unwrap();
        let estimate = input.quote.amount.micros;
        assert_eq!(estimate.known(), None);
        assert_eq!(estimate.known_component(), Micros::new(20));
        assert_eq!(estimate.unknown_components(), Units::new(1));
        let watermark = store.current().watermark;
        assert!(reserve(&mut store, input.clone(), &actor()).await.is_err());
        assert_eq!(store.current().watermark, watermark);
        let root = suspend_constraints(&mut store, &scope, &actor())
            .await
            .unwrap();
        input.expected_ledger = root.revision;
        input.policy = root.policy;
        let admitted = reserve(&mut store, input.clone(), &actor()).await.unwrap();
        assert_eq!(ledger(store.current(), &scope).unwrap().active, estimate);
        let watermark = store.current().watermark;
        assert_eq!(
            reserve(&mut store, input, &actor()).await.unwrap(),
            admitted
        );
        assert_eq!(store.current().watermark, watermark);
        submit(
            &mut store,
            &admitted.id,
            &scope,
            admitted.revision,
            &actor(),
        )
        .await
        .unwrap();
        assert!(submit(
            &mut store,
            &admitted.id,
            &scope,
            admitted.revision,
            &actor()
        )
        .await
        .is_err());
        assert!(
            release_before_send(&mut store, &admitted.id, &scope, &actor())
                .await
                .is_err()
        );
        hold_uncertain(
            &mut store,
            &admitted.id,
            &scope,
            &actor(),
            "interrupted response",
        )
        .await
        .unwrap();
        let held = ledger(store.current(), &scope).unwrap();
        assert!(held.active.is_zero());
        assert_eq!(held.unresolved, estimate);
        let partial = usage(&mut store, &admitted, 25, 1, false).await;
        observe(&mut store, partial, &actor()).await.unwrap();
        let held = ledger(store.current(), &scope).unwrap();
        assert_eq!(held.settled, Micros::new(25));
        assert_eq!(held.unresolved.known_component(), Micros::ZERO);
        assert_eq!(held.unresolved.known(), None);
        assert!(!held.unresolved.is_zero());
        let watermark = store.current().watermark;
        store.close().await.unwrap();
        let mut store = Store::open(temporary.path(), kind, &[]).await.unwrap();
        assert_eq!(store.current().watermark, watermark);
        assert_eq!(ledger(store.current(), &scope).unwrap(), held);
        assert!(submit(
            &mut store,
            &admitted.id,
            &scope,
            admitted.revision,
            &actor()
        )
        .await
        .is_err());
        let final_usage = usage(&mut store, &admitted, 30, 2, true).await;
        observe(&mut store, final_usage.clone(), &actor())
            .await
            .unwrap();
        let settled = ledger(store.current(), &scope).unwrap();
        assert_eq!(settled.settled, Micros::new(30));
        assert!(settled.active.is_zero() && settled.unresolved.is_zero());
        let watermark = store.current().watermark;
        observe(&mut store, final_usage, &actor()).await.unwrap();
        assert_eq!(store.current().watermark, watermark);
        store.close().await.unwrap();
    }
}
