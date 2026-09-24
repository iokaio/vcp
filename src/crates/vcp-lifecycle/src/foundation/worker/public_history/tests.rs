// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_domain::{
    task::Objective,
    verification::Fingerprint,
    workspace::{Binding, Trust},
};
use vcp_engine::{Engine, HostFacts};
use vcp_protocol::command::{Command, CommandEnvelope};
use vcp_store::BackendKind;
fn wid(text: &str) -> methods::Id {
    text.to_owned().try_into().unwrap()
}
async fn send(
    engine: &mut Engine<Store>,
    access: &Access,
    task: Option<TaskId>,
    expected: Revision,
    payload: Command,
) {
    let command = CommandEnvelope {
        version: 1,
        id: CommandId::new(),
        workspace: access.workspace.clone(),
        session: access.session.clone(),
        task,
        caller: access.actor.clone(),
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected,
        steering: SteeringRevision::ZERO,
        payload,
    };
    engine
        .handle(command, access, &HostFacts::inspect(Timestamp::new(10)))
        .await
        .unwrap();
}
async fn fixture(path: &std::path::Path, backend: BackendKind) -> (Engine<Store>, Access, TaskId) {
    let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
    let access = Access {
        actor: ActorId::new(),
        workspace: WorkspaceId::new(),
        session: SessionId::new(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: true,
    };
    send(
        &mut engine,
        &access,
        None,
        Revision::ZERO,
        Command::Initialize {
            binding: Binding {
                host: HostId::new(),
                root: "C:/history-fixture".into(),
                repository: "fixture".into(),
                worktree: "main".into(),
                revision: Revision::ZERO,
            },
        },
    )
    .await;
    let first = TaskId::new();
    for task in [first.clone(), TaskId::new()] {
        send(
            &mut engine,
            &access,
            Some(task.clone()),
            Revision::ZERO,
            Command::CreateTask {
                root: task,
                parent: None,
                fork_origin: None,
                objective: Objective {
                    text: "private history needle must never become a wire snippet".into(),
                    constraints: vec![],
                    acceptance: vec![],
                    source: EventId::new(),
                    steering: SteeringRevision::ZERO,
                },
                fingerprint: Fingerprint {
                    repository: "a".repeat(64),
                    buffers: "b".repeat(64),
                    environment: "c".repeat(64),
                },
                editing: false,
                required_checks: vec![],
            },
        )
        .await;
    }
    (engine, access, first)
}
fn request(access: &Access) -> wire::Query {
    wire::Query {
        scope: methods::Scope {
            workspace: wid(access.workspace.as_str()),
            session: wid(access.session.as_str()),
        },
        task: None,
        selector: None,
        text: None,
        artifact: None,
        expand_compacted: false,
        limit: 1,
        cursor: None,
    }
}
#[tokio::test]
async fn history_pages_match_cli_metadata_without_payloads_and_recheck_access() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let directory = tempfile::tempdir().unwrap();
        let (mut engine, access, first_task) = fixture(directory.path(), backend).await;
        let mut observer = access.clone();
        observer.write = false;
        observer.bootstrap = false;
        let mut query = request(&observer);
        let before = engine.store().state().clone();
        let first = inspect(engine.store(), &observer, &query, &|| Ok(())).unwrap();
        assert_eq!(first.rows.len(), 1);
        assert!(!first.complete);
        assert!(first.claim_links.is_empty());
        assert!(
            !serde_json::to_string(&first)
                .unwrap()
                .contains("private history needle")
        );
        let cli = vcp_audit::history_query::query_session(
            engine.store().state(),
            &vcp_audit::history::Access {
                workspace: observer.workspace.clone(),
                authority: observer.authority,
                read: true,
                tasks: None,
            },
            &vcp_audit::history_query::Query {
                selector: vcp_domain::retention_selector::Selector {
                    schema_version: 1,
                    tree: vcp_domain::retention_selector::Tree::Match(
                        vcp_domain::retention_selector::Criterion::Workspace(
                            observer.workspace.clone(),
                        ),
                    ),
                },
                text: None,
                limit: 1,
                cursor: None,
                artifact: None,
                expand_compacted: false,
            },
            &observer.session,
        )
        .unwrap();
        assert_eq!(
            first.rows[0].id.as_str(),
            cli.rows[0].event.event.id.as_str()
        );
        assert_eq!(first.rows[0].visibility, cli.rows[0].visibility);
        query.cursor = first.next_cursor;
        let second = inspect(engine.store(), &observer, &query, &|| Ok(())).unwrap();
        assert_eq!(second.rows.len(), 1);
        assert_ne!(first.rows[0].id, second.rows[0].id);
        let mut other = observer.clone();
        other.actor = ActorId::new();
        assert!(inspect(engine.store(), &other, &query, &|| Ok(())).is_err());
        let mut changed = query.clone();
        changed.limit = 2;
        assert!(inspect(engine.store(), &observer, &changed, &|| Ok(())).is_err());
        let mut denied = observer.clone();
        denied.read = false;
        assert!(inspect(engine.store(), &denied, &query, &|| Ok(())).is_err());
        let mut foreign = query.clone();
        foreign.scope.session = wid("foreign-session");
        assert!(inspect(engine.store(), &observer, &foreign, &|| Ok(())).is_err());
        let mut search = request(&observer);
        search.task = Some(wid(first_task.as_str()));
        search.text = Some("private history needle".into());
        let search = inspect(engine.store(), &observer, &search, &|| Ok(())).unwrap();
        assert_eq!(search.rows.len(), 1);
        assert!(
            !serde_json::to_string(&search)
                .unwrap()
                .contains("private history needle")
        );
        assert_eq!(
            engine.store().state(),
            &before,
            "observer pagination cannot write canonical state"
        );
        send(
            &mut engine,
            &access,
            None,
            Revision::ZERO,
            Command::SetWorkspaceTrust {
                trust: Trust::Trusted,
            },
        )
        .await;
        assert!(inspect(engine.store(), &observer, &query, &|| Ok(())).is_err());
    }
}
