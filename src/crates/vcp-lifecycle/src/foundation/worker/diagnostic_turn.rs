// SPDX-License-Identifier: Apache-2.0
//! One owner-local diagnostic join, valid only for the exact immutable cut and
//! scope. It is never an authority, completion proof or execution capability.
use std::{
    cell::RefCell,
    future::Future,
    sync::{Arc, Weak},
};
use vcp_domain::{workspace::Scope, TurnId};
use vcp_store::CurrentState;

#[derive(Default)]
pub(super) struct Memo {
    entry: RefCell<Option<(Weak<CurrentState>, Scope, Option<TurnId>)>>,
}
impl Memo {
    pub(super) async fn resolve<E, F: Future<Output = Result<Option<TurnId>, E>>>(
        &self,
        current: &Arc<CurrentState>,
        scope: &Scope,
        load: impl FnOnce() -> F,
    ) -> Option<TurnId> {
        let cut = Arc::downgrade(current);
        if let Some((prior, prior_scope, turn)) = self.entry.borrow().as_ref() {
            // Snapshot identity changes on commit, reopen and same-watermark
            // generation replacement. A watermark alone cannot fence erasure.
            if prior.ptr_eq(&cut) && prior_scope == scope {
                return turn.clone();
            }
        }
        // A failed or cancelled read does not become an absent turn. Preserve
        // the existing best-effort diagnostic behavior, but retry next time.
        let turn = load().await.ok()?;
        *self.entry.borrow_mut() = Some((cut, scope.clone(), turn.clone()));
        turn
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use vcp_domain::{SessionId, TaskId, WorkspaceId};
    fn scope() -> Scope {
        Scope {
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
            task: TaskId::new(),
        }
    }
    async fn snapshot() -> Arc<CurrentState> {
        let temporary = tempfile::tempdir().unwrap();
        let store = vcp_store::Store::open(temporary.path(), vcp_store::BackendKind::Files, &[])
            .await
            .unwrap();
        let snapshot = store.current_state();
        store.close().await.unwrap();
        snapshot
    }
    #[tokio::test]
    async fn same_cut_reuses_known_join_and_scope_or_commit_refreshes_it() {
        let memo = Memo::default();
        let scope = scope();
        let turn = TurnId::new();
        let reads = Cell::new(0);
        let cut = snapshot().await;
        for _ in 0..8 {
            let result = memo
                .resolve(&cut, &scope, || async {
                    reads.set(reads.get() + 1);
                    Ok::<_, ()>(Some(turn.clone()))
                })
                .await;
            assert_eq!(result, Some(turn.clone()));
        }
        assert_eq!(reads.get(), 1);
        let replacement = snapshot().await;
        assert_eq!(cut.watermark, replacement.watermark);
        let next = TurnId::new();
        assert_eq!(
            memo.resolve(&replacement, &scope, || async {
                Ok::<_, ()>(Some(next.clone()))
            })
            .await,
            Some(next)
        );
        let mut other = scope.clone();
        other.session = SessionId::new();
        assert_eq!(
            memo.resolve(&replacement, &other, || async { Ok::<_, ()>(None) })
                .await,
            None
        );
        assert_eq!(
            memo.resolve(&replacement, &other, || async {
                panic!("known absence should be reused");
                #[allow(unreachable_code)]
                Ok::<_, ()>(None)
            })
            .await,
            None
        );
    }
    #[tokio::test]
    async fn failed_or_cancelled_lookup_is_retried_and_new_owner_starts_empty() {
        let memo = Memo::default();
        let scope = scope();
        let cut = snapshot().await;
        assert_eq!(
            memo.resolve(&cut, &scope, || async { Err::<Option<TurnId>, _>(()) })
                .await,
            None
        );
        assert!(memo.entry.borrow().is_none());
        assert!(tokio::time::timeout(
            std::time::Duration::from_millis(10),
            memo.resolve(&cut, &scope, || std::future::pending::<
                Result<Option<TurnId>, ()>,
            >())
        )
        .await
        .is_err());
        assert!(memo.entry.borrow().is_none());
        let turn = TurnId::new();
        assert_eq!(
            memo.resolve(&cut, &scope, || async { Ok::<_, ()>(Some(turn.clone())) })
                .await,
            Some(turn)
        );
        assert!(Memo::default().entry.borrow().is_none());
    }
}
