// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{contract::CanonicalStore, legacy_store_fixture::LegacyFixture as Store};
#[path = "../tests/common/mod.rs"]
mod common;

#[tokio::test]
async fn legacy_owner_transfer_replays_original_native_history_before_returning_current_owner() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canonical");
        let mut source = Store::open(&root, kind, &[]).await.unwrap();
        source.transact(common::initial()).await.unwrap();
        let expected = source.state().clone();
        source.close().await.unwrap();
        let marker = std::fs::read(root.join("format.json")).unwrap();
        let lock = CanonicalLock::acquire(&root, &[]).unwrap();
        let mut diagnostics = crate::StoreDiagnostics::new(kind);
        let (backend, state, _) = Backend::open_observed(&root, kind, None, &mut diagnostics)
            .await
            .unwrap();
        let owner = Opened::from_legacy(
            lock,
            backend,
            state,
            None,
            crate::artifact::DEFAULT_ARTIFACT_LIMIT,
            &mut diagnostics,
        )
        .await
        .unwrap();
        assert_eq!(owner.archive_state().await.unwrap(), expected);
        assert_eq!(owner.current().watermark, expected.watermark);
        assert_eq!(std::fs::read(root.join("format.json")).unwrap(), marker);
        assert!(CanonicalLock::acquire(&root, &[]).is_err());
        let mut cursor = owner.history().await.unwrap();
        assert_eq!(
            cursor
                .next_original()
                .await
                .unwrap()
                .unwrap()
                .commit
                .transaction,
            common::initial()
        );
        assert!(cursor.next_original().await.unwrap().is_none());
        cursor.close().await.unwrap();
        owner.close().await.unwrap();
    }
}

#[tokio::test]
async fn altered_same_cut_legacy_source_cannot_admit_a_native_owner_or_change_selection() {
    for kind in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("canonical");
        let mut source = Store::open(&root, kind, &[]).await.unwrap();
        source.transact(common::initial()).await.unwrap();
        let expected = source.state().clone();
        source.close().await.unwrap();
        let marker = std::fs::read(root.join("format.json")).unwrap();
        let lock = CanonicalLock::acquire(&root, &[]).unwrap();
        let mut diagnostics = crate::StoreDiagnostics::new(kind);
        let (backend, mut state, _) = Backend::open_observed(&root, kind, None, &mut diagnostics)
            .await
            .unwrap();
        let key = crate::contract::key(crate::contract::Collection::Task, "task");
        state.records.get_mut(&key).unwrap().value["reason"] =
            "different validated current state".into();
        state.validate().unwrap();
        assert!(Opened::from_legacy(
            lock,
            backend,
            state,
            None,
            crate::artifact::DEFAULT_ARTIFACT_LIMIT,
            &mut diagnostics
        )
        .await
        .is_err());
        assert_eq!(std::fs::read(root.join("format.json")).unwrap(), marker);
        let reopened = Store::open(&root, kind, &[]).await.unwrap();
        assert_eq!(reopened.state(), &expected);
        reopened.close().await.unwrap();
    }
}
