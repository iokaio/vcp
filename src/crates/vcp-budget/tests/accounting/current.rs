// SPDX-License-Identifier: Apache-2.0
use super::*;

struct CurrentOwner(Store);
impl CanonicalStore for CurrentOwner {
    fn state(&self) -> &State {
        panic!("accounting requested complete history")
    }
    fn current(&self) -> vcp_store::CurrentStateView<'_> {
        self.0.current()
    }
    async fn transact(&mut self, transaction: Transaction) -> vcp_store::Result<Receipt> {
        self.0.transact(transaction).await
    }
}

#[tokio::test]
async fn accounting_uses_current_records_without_weakening_finite_or_send_fences() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let store = setup(temp.path(), kind, 100, 10).await;
        let scope = common::task().scope;
        let mut spec = common::spec();
        spec.scope = scope.clone();
        spec.channel = Channel::RequestBody;
        let mut writer = store.spool().create(spec).unwrap();
        writer
            .write_chunk(b"current-only captured request")
            .unwrap();
        let captured = writer.finalize().unwrap();
        drop(writer);
        let input = admission(&store, &scope, &captured, 60);
        let prior = store.current().watermark;
        let mut owner = CurrentOwner(store);
        let created = reserve_captured(&mut owner, input.clone(), captured.clone(), &actor())
            .await
            .unwrap();
        assert_eq!(owner.current().watermark, prior.next().unwrap());
        assert_eq!(reserve(&mut owner, input, &actor()).await.unwrap(), created);
        assert_eq!(ledger(owner.current(), &scope).unwrap().active.get(), 60);
        let denied = admission(&owner.0, &scope, &captured, 40);
        assert!(
            reserve(&mut owner, denied, &actor()).await.is_err(),
            "protected reserve still unavailable to ordinary admission"
        );
        let permit = submit(&mut owner, &created.id, &scope, Revision::ZERO, &actor())
            .await
            .unwrap();
        assert_eq!(permit.attempt(), &created.id);
        assert!(
            submit(&mut owner, &created.id, &scope, Revision::ZERO, &actor())
                .await
                .is_err()
        );
        assert!(
            release_before_send(&mut owner, &created.id, &scope, &actor())
                .await
                .is_err()
        );
        hold_uncertain(&mut owner, &created.id, &scope, &actor(), "lost transport")
            .await
            .unwrap();
        assert_eq!(
            ledger(owner.current(), &scope).unwrap().unresolved.get(),
            60
        );
        let observation = usage(&mut owner.0, &created, 70, 1, true).await;
        let settlement = observe(&mut owner, observation.clone(), &actor())
            .await
            .unwrap();
        assert_eq!(
            observe(&mut owner, observation, &actor()).await.unwrap(),
            settlement
        );
        let root = ledger(owner.current(), &scope).unwrap();
        assert_eq!(root.settled.get(), 70);
        assert_eq!(root.unresolved.get(), 0);
        assert_eq!(root.active.get(), 0);
        owner.0.close().await.unwrap();
    }
}
