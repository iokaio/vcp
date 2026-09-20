// SPDX-License-Identifier: Apache-2.0
#[path = "../../vcp-store/tests/common/mod.rs"]
mod common;
use vcp_domain::{
    retention_selector::{Criterion, Selector, Tree},
    *,
};
use vcp_memory::{access::Access, retention::Action, retention_policy::*};
use vcp_store::{contract::CanonicalStore, BackendKind, Store};
fn access() -> Access {
    Access {
        workspace: common::workspace().id,
        actor: ActorId::parse("owner").unwrap(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        tasks: None,
    }
}
fn automatic() -> Automatic {
    Automatic {
        selector: Selector {
            schema_version: 1,
            tree: Tree::Match(Criterion::Workspace(common::workspace().id)),
        },
        action: Action::Exclude,
        cadence_days: 7,
    }
}
#[tokio::test]
async fn aging_boundary_repeat_and_notification_only_preserve_retained_history() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path(), backend, &[]).await.unwrap();
        store.transact(common::initial()).await.unwrap();
        let access = access();
        let original = store.state().events[0].clone();
        assert!(show(&store, &access).unwrap().automatic.is_none());
        assert!(
            !aging(&store, &access, Timestamp::new(100 + 30 * DAY_MS))
                .unwrap()
                .due
        );
        let now = Timestamp::new(100 + 31 * DAY_MS);
        let notice = aging(&store, &access, now).unwrap();
        assert!(notice.due);
        assert_eq!(notice.oldest, Some(Timestamp::new(100)));
        assert!(queue(&store, &access, now).unwrap().is_none());
        acknowledge_notice(&mut store, &access, now).await.unwrap();
        assert!(!aging(&store, &access, now).unwrap().due);
        assert!(
            !aging(&store, &access, Timestamp::new(now.get() + 7 * DAY_MS - 1))
                .unwrap()
                .due
        );
        assert!(
            aging(&store, &access, Timestamp::new(now.get() + 7 * DAY_MS))
                .unwrap()
                .due
        );
        assert_eq!(store.state().events[0], original);
        store.close().await.unwrap();
        let store = Store::open(dir.path(), backend, &[]).await.unwrap();
        assert!(!aging(&store, &access, now).unwrap().due);
        assert!(show(&store, &access).unwrap().automatic.is_none());
    }
}
#[tokio::test]
async fn disabling_or_editing_policy_revokes_queued_work_and_cadence_is_durable() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path(), backend, &[]).await.unwrap();
        store.transact(common::initial()).await.unwrap();
        let access = access();
        let now = Timestamp::new(40 * DAY_MS);
        let policy = set(&mut store, &access, None, 7, Some(automatic()), now)
            .await
            .unwrap();
        let queued = queue(&store, &access, now).unwrap().unwrap();
        let disabled = set(&mut store, &access, Some(policy.revision), 7, None, now)
            .await
            .unwrap();
        let watermark = store.state().watermark;
        assert!(apply_queued(&mut store, &access, &queued, now)
            .await
            .is_err());
        assert_eq!(store.state().watermark, watermark);
        assert!(queue(&store, &access, now).unwrap().is_none());
        set(
            &mut store,
            &access,
            Some(disabled.revision),
            7,
            Some(automatic()),
            now,
        )
        .await
        .unwrap();
        let queued = queue(&store, &access, now).unwrap().unwrap();
        let receipt = apply_queued(&mut store, &access, &queued, now)
            .await
            .unwrap();
        assert_eq!(receipt.preview.action, Action::Exclude);
        assert!(queue(&store, &access, now).unwrap().is_none());
        let repeat = apply_queued(&mut store, &access, &queued, now)
            .await
            .unwrap();
        assert_eq!(repeat.id, receipt.id);
        store.close().await.unwrap();
        let store = Store::open(dir.path(), backend, &[]).await.unwrap();
        assert!(
            queue(&store, &access, Timestamp::new(now.get() + 7 * DAY_MS - 1))
                .unwrap()
                .is_none()
        );
        assert!(
            queue(&store, &access, Timestamp::new(now.get() + 7 * DAY_MS))
                .unwrap()
                .is_some()
        );
    }
}
