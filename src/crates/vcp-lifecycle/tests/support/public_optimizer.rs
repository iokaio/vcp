// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_lifecycle::foundation::routing_state::public_optimizer as public;
fn session(store: &Store) -> SessionId {
    store
        .state()
        .records
        .values()
        .find(|r| r.collection == Collection::Session)
        .unwrap()
        .decode::<Session>()
        .unwrap()
        .id
}
fn command(store: &Store, access: &Access, id: &str) -> public::Command {
    let workspace: Workspace = store
        .state()
        .record(
            Collection::Workspace,
            access.workspace.as_str(),
            &access.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    public::Command {
        workspace: access.workspace.clone(),
        session: session(store),
        actor: access.actor.clone(),
        id: CommandId::parse(id).unwrap(),
        digest: vcp_protocol::digest_bytes(id.as_bytes()),
        expected_revision: workspace.revision,
        expected_binding_revision: workspace.binding.revision,
    }
}
fn window() -> HistoryWindow {
    HistoryWindow {
        from: None,
        until: Timestamp::new(1000),
    }
}
fn observer(access: &Access, tasks: BTreeSet<TaskId>) -> Access {
    Access {
        workspace: access.workspace.clone(),
        actor: access.actor.clone(),
        authority: access.authority,
        read: true,
        write: false,
        tasks: Some(tasks),
    }
}
#[tokio::test]
async fn public_optimizer_atomic_capture_apply_replay_and_rollback_reopen() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let directory = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(directory.path(), backend).await;
        let ceilings = policy();
        initialize_policy(&mut store, &access, ceilings.clone(), Timestamp::new(2))
            .await
            .unwrap();
        let capture = command(&store, &access, &"x".repeat(96));
        let committed = public::capture(
            &mut store,
            &access,
            &capture,
            public::Coverage::Workspace,
            window(),
            Timestamp::new(3),
            &|| Ok(()),
        )
        .await
        .unwrap();
        let before = store.state().clone();
        assert_eq!(
            public::replay(&store, &access, &capture).unwrap().unwrap(),
            committed
        );
        assert_eq!(
            public::capture(
                &mut store,
                &access,
                &capture,
                public::Coverage::Workspace,
                window(),
                Timestamp::new(4),
                &|| Ok(())
            )
            .await
            .unwrap(),
            committed
        );
        assert_eq!(store.state(), &before);
        let report =
            public::read_report(&store, &access, &capture.session, &capture.id, &|| Ok(()))
                .unwrap();
        assert_ne!(report.report.id, capture.id.as_str());
        assert!(public::read_report(
            &store,
            &observer(&access, BTreeSet::new()),
            &capture.session,
            &capture.id,
            &|| Ok(())
        )
        .is_err());
        let mut changed = capture.clone();
        changed.digest = "f".repeat(64);
        assert_eq!(
            public::replay(&store, &access, &changed),
            Err(public::Error::CommandConflict)
        );
        changed = capture.clone();
        changed.actor = ActorId::new();
        assert_eq!(
            public::replay(&store, &access, &changed),
            Err(public::Error::Access)
        );
        let proposal = preview(
            &store,
            &access,
            &report.report.id,
            vec![Edit::QualityFloorBps(8000)],
            &ceilings,
        )
        .unwrap();
        let apply = command(&store, &access, "public-apply");
        let accepted = public::apply(
            &mut store,
            &access,
            &apply,
            &proposal,
            &ceilings,
            Timestamp::new(4),
            &|| Ok(()),
        )
        .await
        .unwrap();
        assert_eq!(
            current_policy(&store, &access).unwrap().unwrap().revision,
            Revision::new(1)
        );
        assert_eq!(
            public::apply(
                &mut store,
                &access,
                &apply,
                &proposal,
                &ceilings,
                Timestamp::new(5),
                &|| Ok(())
            )
            .await
            .unwrap(),
            accepted
        );
        let stale = command(&store, &access, "public-stale");
        assert_eq!(
            public::apply(
                &mut store,
                &access,
                &stale,
                &proposal,
                &ceilings,
                Timestamp::new(5),
                &|| Ok(())
            )
            .await,
            Err(public::Error::Stale)
        );
        let rollback = public::rollback_preview(
            &store,
            &access,
            Revision::new(1),
            Revision::ZERO,
            &ceilings,
            &|| Ok(()),
        )
        .unwrap();
        let rollback_command = command(&store, &access, "public-rollback");
        let mut narrowed = ceilings.clone();
        narrowed.quality_floor_bps = 9000;
        narrowed = narrowed.seal().unwrap();
        assert_eq!(
            public::rollback(
                &mut store,
                &access,
                &rollback_command,
                &rollback,
                &narrowed,
                Timestamp::new(5),
                &|| Ok(())
            )
            .await,
            Err(public::Error::Stale)
        );
        let rolled = public::rollback(
            &mut store,
            &access,
            &rollback_command,
            &rollback,
            &ceilings,
            Timestamp::new(5),
            &|| Ok(()),
        )
        .await
        .unwrap();
        assert_eq!(
            current_policy(&store, &access).unwrap().unwrap().revision,
            Revision::new(2)
        );
        assert_eq!(
            current_policy(&store, &access)
                .unwrap()
                .unwrap()
                .value
                .quality_floor_bps,
            7000
        );
        drop(store);
        let reopened = Store::open(directory.path(), backend, &[]).await.unwrap();
        assert_eq!(
            public::replay(&reopened, &access, &capture)
                .unwrap()
                .unwrap(),
            committed
        );
        assert_eq!(
            public::replay(&reopened, &access, &apply).unwrap().unwrap(),
            accepted
        );
        assert_eq!(
            public::replay(&reopened, &access, &rollback_command)
                .unwrap()
                .unwrap(),
            rolled
        );
        public::read_report(
            &reopened,
            &access,
            &capture.session,
            &capture.id,
            &|| Ok(()),
        )
        .unwrap();
    }
}
#[tokio::test]
async fn public_optimizer_source_capture_preserves_foreign_scope_and_receipt_session() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let directory = tempfile::tempdir().unwrap();
        let (mut store, access) = setup(directory.path(), backend).await;
        let task =
            action_evidence::create_task(&mut store, &access, vcp_domain::task::TaskState::Paused)
                .await;
        let other = Session {
            id: SessionId::new(),
            workspace: access.workspace.clone(),
            revision: Revision::ZERO,
            configuration: Revision::ZERO,
            fork_origin: None,
            fork_through: None,
        };
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![Mutation::Put {
                    expected: None,
                    record: Record::typed(
                        Collection::Session,
                        other.id.as_str(),
                        access.workspace.clone(),
                        Revision::ZERO,
                        &other,
                    )
                    .unwrap(),
                }],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        let mut capture = command(&store, &access, "public-source-capture");
        capture.session = other.id.clone();
        #[cfg(feature = "qualification")]
        {
            let before = store.state().clone();
            assert_eq!(
                public::qualification_capture_after_spool(
                    &mut store,
                    &access,
                    &capture,
                    public::Coverage::Workspace,
                    window(),
                    Timestamp::new(3),
                    &|| Ok(())
                )
                .await,
                Err(public::Error::Unavailable)
            );
            assert_eq!(store.state(), &before);
            assert!(public::replay(&store, &access, &capture).unwrap().is_none());
            assert!(public::read_report(
                &store,
                &access,
                &capture.session,
                &capture.id,
                &|| Ok(())
            )
            .is_err());
        }
        let committed = public::capture(
            &mut store,
            &access,
            &capture,
            public::Coverage::Workspace,
            window(),
            Timestamp::new(3),
            &|| Ok(()),
        )
        .await
        .unwrap();
        let report = public::read_report(&store, &access, &other.id, &capture.id, &|| Ok(()))
            .unwrap()
            .report;
        let pin = report
            .forecast
            .as_ref()
            .expect("task-backed source forecast is retained");
        let descriptor: vcp_domain::artifact::ArtifactDescriptor = store
            .state()
            .record(
                Collection::Artifact,
                pin.artifact.as_str(),
                &access.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(descriptor.spec.scope, task.scope);
        assert!(store
            .state()
            .events
            .iter()
            .filter(|e| e.watermark == committed.receipt.watermark)
            .all(|e| e.event.session == other.id
                && e.event.correlation == capture.id
                && e.event.actor == access.actor));
        assert!(public::read_report(
            &store,
            &access,
            &task.scope.session,
            &capture.id,
            &|| Ok(())
        )
        .is_err());
        let mut scoped = observer(&access, BTreeSet::from([task.scope.task.clone()]));
        scoped.write = true;
        let mut selected = command(&store, &access, "public-session-capture");
        selected.session = task.scope.session.clone();
        public::capture(
            &mut store,
            &scoped,
            &selected,
            public::Coverage::Session,
            window(),
            Timestamp::new(4),
            &|| Ok(()),
        )
        .await
        .unwrap();
        scoped.write = false;
        public::read_report(&store, &scoped, &selected.session, &selected.id, &|| Ok(())).unwrap();
        scoped.tasks = Some(BTreeSet::new());
        assert!(
            public::read_report(&store, &scoped, &selected.session, &selected.id, &|| Ok(()))
                .is_err()
        );
        let before = store.state().clone();
        let interrupted = command(&store, &access, "public-interrupted");
        assert_eq!(
            public::capture(
                &mut store,
                &access,
                &interrupted,
                public::Coverage::Workspace,
                window(),
                Timestamp::new(5),
                &|| Err(public::Error::Cancelled)
            )
            .await,
            Err(public::Error::Cancelled)
        );
        assert_eq!(store.state(), &before);
        let calls = std::cell::Cell::new(0);
        let cancel = || {
            calls.set(calls.get() + 1);
            if calls.get() > 8 {
                Err("cancelled".to_owned())
            } else {
                Ok(())
            }
        };
        assert!(forecasts::observe_with_check(&store, &access, window(), &cancel).is_err());
        assert!(calls.get() > 8);
        assert_eq!(store.state(), &before);
    }
}
