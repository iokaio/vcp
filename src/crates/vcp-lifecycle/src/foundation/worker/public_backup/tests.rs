// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_domain::workspace::{Binding, Session};
use vcp_store::{
    contract::{CanonicalStore, Mutation, Record, Transaction},
    BackendKind,
};

async fn fixture(path: &std::path::Path, backend: BackendKind) -> (Store, Access) {
    let mut store = Store::open(path, backend, &[]).await.unwrap();
    let workspace = Workspace {
        id: WorkspaceId::new(),
        binding: Binding {
            host: HostId::new(),
            root: "C:/synthetic-publisher".into(),
            repository: "repo".into(),
            worktree: "main".into(),
            revision: Revision::ZERO,
        },
        trust: Trust::Trusted,
        revision: Revision::ZERO,
        authority: AuthorityRevision::ZERO,
        deletion: DeletionEpoch::ZERO,
    };
    let access = Access {
        actor: ActorId::new(),
        workspace: workspace.id.clone(),
        session: SessionId::parse("z-caller-session").unwrap(),
        authority: workspace.authority,
        read: true,
        write: true,
        bootstrap: false,
    };
    let mut mutations = vec![Mutation::Put {
        record: Record::typed(
            Collection::Workspace,
            workspace.id.as_str(),
            workspace.id.clone(),
            workspace.revision,
            &workspace,
        )
        .unwrap(),
        expected: None,
    }];
    for session in [
        SessionId::parse("a-unrelated-session").unwrap(),
        access.session.clone(),
    ] {
        let value = Session {
            id: session.clone(),
            workspace: workspace.id.clone(),
            revision: Revision::ZERO,
            configuration: Revision::ZERO,
            fork_origin: None,
            fork_through: None,
        };
        mutations.push(Mutation::Put {
            record: Record::typed(
                Collection::Session,
                session.as_str(),
                workspace.id.clone(),
                Revision::ZERO,
                &value,
            )
            .unwrap(),
            expected: None,
        });
    }
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: Watermark::ZERO,
            mutations,
            events: vec![],
            command: None,
        })
        .await
        .unwrap();
    (store, access)
}
fn scope(access: &Access) -> methods::Scope {
    methods::Scope {
        workspace: id(access.workspace.as_str()).unwrap(),
        session: id(access.session.as_str()).unwrap(),
    }
}
fn mutation(command: &CommandId) -> methods::Mutation {
    methods::Mutation {
        command_id: id(command.as_str()).unwrap(),
        expected_revision: 0.into(),
        steering_revision: 0.into(),
    }
}

#[tokio::test]
async fn accepted_backup_intent_survives_lost_reply_reopen_and_cancellation_before_job() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, access) = fixture(temp.path(), backend).await;
        let operation = CommandId::new();
        let create = Call::BackupCreate(wire::Create {
            scope: scope(&access),
            mutation: mutation(&operation),
            expected_binding_revision: 0.into(),
            capability: id("opaque-native-capability").unwrap(),
            expected_capability_generation: 7.into(),
        });
        let intent = durable::Intent {
            schema_version: 1,
            operation: operation.clone(),
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            actor: access.actor.clone(),
            revision: Revision::ZERO,
            authority: access.authority,
            deletion: DeletionEpoch::ZERO,
            binding: Revision::ZERO,
            capability: "opaque-native-capability".into(),
            generation: 7,
            configuration_revision: 3,
            cancel_requested: false,
        };
        let accepted = durable::commit(&mut store, &access, &create, &intent, None)
            .await
            .unwrap();
        assert!(snapshot_job(&store, &access, &operation).unwrap().is_none());
        let event = store
            .state()
            .events
            .iter()
            .find(|e| e.watermark == accepted.watermark)
            .unwrap();
        assert_eq!(event.event.session, access.session);
        assert_eq!(event.event.actor, access.actor);
        assert_eq!(event.event.correlation, operation);
        let before = store.state().clone();
        assert_eq!(
            durable::replay(&store, &access, &create).unwrap().unwrap(),
            accepted
        );
        assert_eq!(store.state(), &before);
        let mut changed = create.clone();
        if let Call::BackupCreate(value) = &mut changed {
            value.expected_capability_generation = 8.into();
        }
        assert!(durable::replay(&store, &access, &changed).is_err());
        let mut foreign = access.clone();
        foreign.actor = ActorId::new();
        assert!(durable::read(&store, &foreign, &operation).is_err());
        assert!(durable::replay(&store, &foreign, &create).is_err());
        foreign = access.clone();
        foreign.session = SessionId::parse("a-unrelated-session").unwrap();
        assert!(durable::read(&store, &foreign, &operation).is_err());
        assert!(durable::replay(&store, &foreign, &create).is_err());
        // Simulate process loss after the durable intent, before a background
        // task has captured any sources. Reopen does not schedule native work.
        drop(store);
        let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(
            durable::replay(&store, &access, &create).unwrap().unwrap(),
            accepted
        );
        assert!(snapshot_job(&store, &access, &operation).unwrap().is_none());
        let mut intent = durable::read(&store, &access, &operation).unwrap();
        assert!(!intent.cancel_requested);
        let cancel_id = CommandId::new();
        let cancel = Call::BackupCancel(wire::Cancel {
            scope: scope(&access),
            mutation: mutation(&cancel_id),
            expected_binding_revision: 0.into(),
            operation: id(operation.as_str()).unwrap(),
            expected_operation_revision: intent.revision.get().into(),
            expected_job_revision: None,
        });
        let previous = intent.revision;
        intent.revision = intent.revision.next().unwrap();
        intent.cancel_requested = true;
        let cancelled = durable::commit(&mut store, &access, &cancel, &intent, Some(previous))
            .await
            .unwrap();
        assert_eq!(
            durable::replay(&store, &access, &cancel).unwrap().unwrap(),
            cancelled
        );
        assert!(
            durable::read(&store, &access, &operation)
                .unwrap()
                .cancel_requested
        );
        assert!(snapshot_job(&store, &access, &operation).unwrap().is_none());
        assert!(!store
            .state()
            .records
            .values()
            .any(|r| r.collection == Collection::Artifact));
        drop(store);
        let store = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(
            durable::replay(&store, &access, &create).unwrap().unwrap(),
            accepted
        );
        assert_eq!(
            durable::replay(&store, &access, &cancel).unwrap().unwrap(),
            cancelled
        );
        assert!(
            durable::read(&store, &access, &operation)
                .unwrap()
                .cancel_requested
        );
    }
}
