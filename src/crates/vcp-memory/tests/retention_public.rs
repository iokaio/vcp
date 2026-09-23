// SPDX-License-Identifier: Apache-2.0
//! Scoped exact retention keeps authenticated rights through replay and cleanup.
use std::collections::BTreeSet;
use vcp_domain::retention_selector::{Criterion, Selector, Tree};
use vcp_domain::task::{Task, TaskState};
use vcp_domain::{
    artifact::{ArtifactDescriptor, ArtifactSpec, Channel},
    ids::*,
    revision::*,
    task::Objective,
    verification::Fingerprint,
    workspace::{Binding, Scope},
};
use vcp_engine::{Engine, HostFacts};
use vcp_memory::{
    access::Access,
    retention::{self, Action, Target},
    retention_public::{self as public, Apply},
};
use vcp_protocol::{
    command::{Command, CommandEnvelope},
    event::EventKind,
};
use vcp_store::{
    artifact::ArtifactWriter,
    contract::{key, CanonicalStore, Collection, Mutation, Record, Transaction},
    BackendKind, Store,
};

fn fingerprint() -> Fingerprint {
    Fingerprint {
        repository: "a".repeat(64),
        buffers: "b".repeat(64),
        environment: "c".repeat(64),
    }
}
fn engine_access(workspace: &str) -> vcp_engine::Access {
    vcp_engine::Access {
        actor: ActorId::parse("owner").unwrap(),
        workspace: WorkspaceId::parse(workspace).unwrap(),
        session: SessionId::parse(format!("session-{workspace}")).unwrap(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: true,
    }
}
fn command(
    engine: &Engine<Store>,
    access: &vcp_engine::Access,
    task: Option<TaskId>,
    expected: Revision,
    payload: Command,
) -> CommandEnvelope {
    CommandEnvelope {
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
    }
}
fn binding() -> Binding {
    Binding {
        host: HostId::new(),
        root: "C:/synthetic-memory-fixture".into(),
        repository: "repository".into(),
        worktree: "main".into(),
        revision: Revision::ZERO,
    }
}

async fn fixture(
    path: &std::path::Path,
    backend: BackendKind,
) -> (Store, Access, Scope, ArtifactDescriptor) {
    let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
    let (scope, _, artifact) = seed(&mut engine, "workspace").await;
    let auth = engine_access("workspace");
    let task: Task = engine
        .store()
        .state()
        .record(Collection::Task, scope.task.as_str(), &scope.workspace)
        .unwrap()
        .decode()
        .unwrap();
    let cancel = command(
        &engine,
        &auth,
        Some(scope.task.clone()),
        task.revision,
        Command::Transition {
            next: TaskState::Cancelled,
            reason: "retention fixture settled".into(),
            verification: None,
        },
    );
    engine
        .handle(cancel, &auth, &HostFacts::inspect(Timestamp::new(190)))
        .await
        .unwrap();
    let access = Access {
        workspace: scope.workspace.clone(),
        actor: auth.actor,
        authority: auth.authority,
        read: true,
        write: true,
        tasks: Some(BTreeSet::from([scope.task.clone()])),
    };
    (engine.into_store(), access, scope, artifact)
}
fn selection() -> Selector {
    Selector {
        schema_version: 1,
        tree: Tree::Match(Criterion::Event("artifact_attached".into())),
    }
}
fn command_request(store: &Store, scope: &Scope) -> Apply {
    let task: Task = store
        .state()
        .record(Collection::Task, scope.task.as_str(), &scope.workspace)
        .unwrap()
        .decode()
        .unwrap();
    Apply {
        command: CommandId::new(),
        command_digest: "a".repeat(64),
        expected_revision: task.revision,
        steering: task.steering,
    }
}

#[tokio::test]
async fn scoped_purge_receipt_replay_and_physical_cleanup_survive_reopen() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, mut access, scope, artifact) = fixture(temp.path(), backend).await;
        let before = store.state().clone();
        let preview = public::preview(
            &store,
            &access,
            scope.clone(),
            selection(),
            Action::Purge,
            Timestamp::new(200),
        )
        .unwrap();
        assert_eq!(
            store.state(),
            &before,
            "preview must not write or widen access"
        );
        public::validate_preview(&store, &access, &scope, &preview).unwrap();
        let selection_bytes = serde_json::to_vec(preview.selection()).unwrap().len();
        assert!(
            preview.cache_bytes(selection_bytes).is_err(),
            "cache accounting must include private scope and task ceiling"
        );
        let full_bytes = preview.cache_bytes(1024 * 1024).unwrap();
        assert!(full_bytes > selection_bytes);
        assert_eq!(preview.cache_bytes(full_bytes).unwrap(), full_bytes);
        assert!(preview.cache_bytes(full_bytes - 1).is_err());
        assert!(preview.selection().selected.len() > 0);
        assert!(preview.selection().dependent.contains(&Target::Record(key(
            Collection::Artifact,
            artifact.spec.id.as_str()
        ))));
        assert!(preview.selection().protected.is_empty());
        let request = command_request(&store, &scope);
        let digest = preview.digest().unwrap();
        assert!(public::apply(
            &mut store,
            &access,
            &preview,
            &"b".repeat(64),
            &request,
            Timestamp::new(201)
        )
        .await
        .is_err());
        assert_eq!(store.state(), &before);
        let committed = public::apply(
            &mut store,
            &access,
            &preview,
            &digest,
            &request,
            Timestamp::new(201),
        )
        .await
        .unwrap();
        assert_eq!(
            committed.receipt.command.as_ref().unwrap().digest,
            request.command_digest
        );
        assert!(
            public::validate_preview(&store, &access, &scope, &preview).is_err(),
            "deletion invalidates cached target pages"
        );
        assert!(committed.job.logical_unavailable && !committed.job.rewrite_complete);
        let after = store.state().clone();
        let replay = public::apply(
            &mut store,
            &access,
            &preview,
            &digest,
            &request,
            Timestamp::new(202),
        )
        .await
        .unwrap();
        assert_eq!(replay.receipt, committed.receipt);
        assert_eq!(store.state(), &after);
        access.write = false;
        assert!(public::read_job(&store, &access, &scope, &committed.job.id).is_ok());
        assert!(public::replay(&store, &access, &scope, preview.id(), &request).is_err());
        access.write = true;
        let mut altered = command_request(&store, &scope);
        altered.command = request.command.clone();
        altered.command_digest = "c".repeat(64);
        assert!(public::replay(&store, &access, &scope, preview.id(), &altered).is_err());
        let snapshot = store.snapshot().unwrap();
        let held = public::cleanup(
            &mut store,
            &access,
            &scope,
            &committed.job.id,
            Timestamp::new(203),
        )
        .await
        .unwrap();
        assert!(held.rewrite_complete && !held.local_cleanup_complete);
        assert!(!held.cleanup.as_ref().unwrap().pinned.is_empty());
        drop(snapshot);
        let cleaned = public::cleanup(
            &mut store,
            &access,
            &scope,
            &committed.job.id,
            Timestamp::new(204),
        )
        .await
        .unwrap();
        assert!(cleaned.rewrite_complete && cleaned.local_cleanup_complete);
        let descriptor: ArtifactDescriptor = store
            .state()
            .record(
                Collection::Artifact,
                artifact.spec.id.as_str(),
                &scope.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(descriptor.state, vcp_domain::artifact::CaptureState::Purged);
        assert!(store.spool().read(&descriptor, std::io::sink()).is_err());
        store.close().await.unwrap();
        store = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(
            public::replay(&store, &access, &scope, preview.id(), &request)
                .unwrap()
                .unwrap()
                .receipt,
            committed.receipt
        );
        assert!(
            public::read_job(&store, &access, &scope, &committed.job.id)
                .unwrap()
                .local_cleanup_complete
        );
        store.close().await.unwrap();
    }
}

async fn foreign_task(store: Store) -> (Store, Scope) {
    let mut engine = Engine::new(store).unwrap();
    let mut access = engine_access("workspace");
    let session = SessionId::new();
    let create = command(
        &engine,
        &access,
        None,
        Revision::ZERO,
        Command::CreateSession {
            id: session.clone(),
            fork_through: None,
        },
    );
    engine
        .handle(create, &access, &HostFacts::inspect(Timestamp::new(191)))
        .await
        .unwrap();
    access.session = session.clone();
    let task = TaskId::new();
    let create = command(
        &engine,
        &access,
        Some(task.clone()),
        Revision::ZERO,
        Command::CreateTask {
            root: task.clone(),
            parent: None,
            fork_origin: None,
            objective: Objective {
                text: r#"{"memory_preference":{"key":"foreign-output","value":"compact"}}"#.into(),
                constraints: vec![],
                acceptance: vec![],
                source: EventId::new(),
                steering: SteeringRevision::ZERO,
            },
            fingerprint: fingerprint(),
            editing: false,
            required_checks: vec![],
        },
    );
    engine
        .handle(create, &access, &HostFacts::inspect(Timestamp::new(192)))
        .await
        .unwrap();
    let cancel = command(
        &engine,
        &access,
        Some(task.clone()),
        Revision::ZERO,
        Command::Transition {
            next: TaskState::Cancelled,
            reason: "foreign fixture settled".into(),
            verification: None,
        },
    );
    engine
        .handle(cancel, &access, &HostFacts::inspect(Timestamp::new(193)))
        .await
        .unwrap();
    (
        engine.into_store(),
        Scope {
            workspace: access.workspace,
            session,
            task,
        },
    )
}

#[tokio::test]
async fn out_of_scope_copied_context_dependency_denies_the_entire_preview() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (store, access, scope, artifact) = fixture(temp.path(), backend).await;
        let (mut store, foreign) = foreign_task(store).await;
        let mut writer = store
            .spool()
            .create(ArtifactSpec {
                id: ArtifactId::new(),
                scope: foreign,
                media_type: "application/json".into(),
                schema: "memory-context/1".into(),
                source: "foreign retained context".into(),
                channel: Channel::Evidence,
                retention: "history".into(),
                omissions: vec![],
            })
            .unwrap();
        writer.write_chunk(&serde_json::to_vec(&serde_json::json!([{"source":{"kind":"artifact","id":artifact.spec.id},"evidence":[artifact.spec.id]}])).unwrap()).unwrap();
        let descriptor = writer.finalize().unwrap();
        drop(writer);
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: store.state().watermark,
                mutations: vec![Mutation::Put {
                    expected: None,
                    record: Record::typed(
                        Collection::Artifact,
                        descriptor.spec.id.as_str(),
                        scope.workspace.clone(),
                        Revision::ZERO,
                        &descriptor,
                    )
                    .unwrap(),
                }],
                events: vec![],
                command: None,
            })
            .await
            .unwrap();
        let before = store.state().clone();
        assert!(
            matches!(
                public::preview(
                    &store,
                    &access,
                    scope,
                    selection(),
                    Action::Purge,
                    Timestamp::new(200)
                ),
                Err(vcp_memory::Error::Access)
            ),
            "foreign copied text cannot be silently excluded from purge closure"
        );
        assert_eq!(store.state(), &before);
    }
}

#[tokio::test]
async fn scoped_generation_cleanup_proves_full_inventory_or_waits_for_authorized_maintenance() {
    use std::sync::atomic::AtomicBool;
    use vcp_memory::{
        publication::{self, Publisher},
        search_record,
    };
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for mixed in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let (mut store, access, scope, _) = fixture(temp.path(), backend).await;
            let mut scopes = vec![scope.clone()];
            if mixed {
                let (new_store, foreign) = foreign_task(store).await;
                store = new_store;
                scopes.push(foreign);
            }
            // Publication is a separately authorized workspace-maintenance setup,
            // never a grant constructed inside the task-scoped retention API.
            let maintenance = Access {
                workspace: access.workspace.clone(),
                actor: access.actor.clone(),
                authority: access.authority,
                read: true,
                write: true,
                tasks: None,
            };
            for own in &scopes {
                let origin = store
                    .state()
                    .events
                    .iter()
                    .find(|e| {
                        e.event.kind == EventKind::TaskCreated
                            && e.event.task.as_ref() == Some(&own.task)
                    })
                    .unwrap()
                    .clone();
                let proposal =
                    vcp_memory::preferences::materialize(&mut store, &maintenance, &origin)
                        .await
                        .unwrap()
                        .unwrap();
                vcp_memory::repository::propose(
                    &mut store,
                    &maintenance,
                    proposal,
                    Timestamp::new(200),
                )
                .await
                .unwrap();
            }
            let inventory = search_record::inventory(
                &store,
                &maintenance,
                &[],
                &search_record::ChunkerSpec::default(),
                search_record::Limits::default(),
            )
            .unwrap();
            assert_eq!(inventory.records.len(), scopes.len());
            let publisher =
                Publisher::new(&store.canonical_anchor().join("search-generations")).unwrap();
            let prepared = publisher
                .prepare(
                    publication::capture(&store, &maintenance, &scope, inventory).unwrap(),
                    None,
                    &AtomicBool::new(false),
                    &|_| {},
                )
                .unwrap();
            let generation = prepared.manifest().id.clone();
            publisher
                .publish(
                    &mut store,
                    &maintenance,
                    &prepared,
                    Timestamp::new(201),
                    &|_| {},
                )
                .await
                .unwrap();
            let foreign_rows: Vec<_> = if mixed {
                store
                    .state()
                    .records
                    .values()
                    .filter(|row| {
                        row.value["scope"]["session"] == serde_json::json!(scopes[1].session)
                            || row.value["spec"]["scope"]["session"]
                                == serde_json::json!(scopes[1].session)
                    })
                    .cloned()
                    .collect()
            } else {
                Vec::new()
            };
            if mixed {
                assert!(!foreign_rows.is_empty());
            }
            let preview = public::preview(
                &store,
                &access,
                scope.clone(),
                selection(),
                Action::Purge,
                Timestamp::new(202),
            )
            .unwrap();
            assert!(preview.selection().protected.is_empty());
            let request = command_request(&store, &scope);
            let committed = public::apply(
                &mut store,
                &access,
                &preview,
                &preview.digest().unwrap(),
                &request,
                Timestamp::new(203),
            )
            .await
            .unwrap();
            let cleaned = public::cleanup(
                &mut store,
                &access,
                &scope,
                &committed.job.id,
                Timestamp::new(204),
            )
            .await
            .unwrap();
            assert!(cleaned.rewrite_complete);
            if mixed {
                assert!(!cleaned.local_cleanup_complete);
                assert_eq!(cleaned.pending_generations, vec![generation.clone()]);
                assert!(store
                    .canonical_anchor()
                    .join("search-generations")
                    .join(generation.as_str())
                    .exists());
                let complete = retention::cleanup(
                    &mut store,
                    &maintenance,
                    &committed.job.id,
                    Timestamp::new(205),
                )
                .await
                .unwrap();
                assert!(
                    complete.local_cleanup_complete,
                    "separate current workspace authority can finish mixed-cache cleanup"
                );
            } else {
                assert!(cleaned.local_cleanup_complete);
                assert!(cleaned.pending_generations.is_empty());
            }
            assert!(!store
                .canonical_anchor()
                .join("search-generations")
                .join(generation.as_str())
                .exists());
            for row in foreign_rows {
                assert_eq!(store.state().records.get(&row.key()), Some(&row), "scoped physical rewrite must preserve foreign canonical evidence byte-for-byte");
            }
        }
    }
}

#[tokio::test]
async fn scoped_preview_never_expands_after_new_source_or_wrong_scope() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let (mut store, mut access, scope, _) = fixture(temp.path(), backend).await;
        let preview = public::preview(
            &store,
            &access,
            scope.clone(),
            selection(),
            Action::Exclude,
            Timestamp::new(200),
        )
        .unwrap();
        let request = command_request(&store, &scope);
        let mut foreign = scope.clone();
        foreign.session = SessionId::new();
        assert!(public::preview(
            &store,
            &access,
            foreign,
            selection(),
            Action::Exclude,
            Timestamp::new(200)
        )
        .is_err());
        let current = store.state().watermark;
        store
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: current,
                mutations: vec![],
                events: vec![vcp_protocol::event::EventInput {
                    id: EventId::new(),
                    workspace: scope.workspace.clone(),
                    session: scope.session.clone(),
                    task: Some(scope.task.clone()),
                    actor: access.actor.clone(),
                    correlation: CommandId::new(),
                    causation: None,
                    timestamp: Timestamp::new(201),
                    kind: EventKind::Commentary,
                    artifacts: vec![],
                    data: serde_json::json!({"new":"source"}),
                    metadata: None,
                }],
                command: None,
            })
            .await
            .unwrap();
        let after = store.state().clone();
        public::validate_preview(&store, &access, &scope, &preview).unwrap();
        assert!(public::apply(
            &mut store,
            &access,
            &preview,
            &preview.digest().unwrap(),
            &request,
            Timestamp::new(202)
        )
        .await
        .is_err());
        assert_eq!(store.state(), &after);
        access.tasks = Some(BTreeSet::new());
        assert!(public::preview(
            &store,
            &access,
            scope,
            selection(),
            Action::Exclude,
            Timestamp::new(202)
        )
        .is_err());
    }
}
async fn seed(engine: &mut Engine<Store>, workspace: &str) -> (Scope, EventId, ArtifactDescriptor) {
    let access = engine_access(workspace);
    let facts = HostFacts::inspect(Timestamp::new(100));
    let initialize = command(
        engine,
        &access,
        None,
        Revision::ZERO,
        Command::Initialize { binding: binding() },
    );
    engine.handle(initialize, &access, &facts).await.unwrap();
    let scope = Scope {
        workspace: access.workspace.clone(),
        session: access.session.clone(),
        task: TaskId::new(),
    };
    let create = command(
        engine,
        &access,
        Some(scope.task.clone()),
        Revision::ZERO,
        Command::CreateTask {
            root: scope.task.clone(),
            parent: None,
            fork_origin: None,
            objective: Objective {
                text: r#"{"memory_preference":{"key":"test-output","value":"retain evidence"}}"#
                    .into(),
                constraints: vec![],
                acceptance: vec!["scope preserved".into()],
                source: EventId::new(),
                steering: SteeringRevision::ZERO,
            },
            fingerprint: fingerprint(),
            editing: false,
            required_checks: vec!["cargo-test".into()],
        },
    );
    engine.handle(create, &access, &facts).await.unwrap();
    let origin = engine
        .store()
        .state()
        .events
        .iter()
        .find(|e| {
            e.event.workspace == scope.workspace
                && e.event.task.as_ref() == Some(&scope.task)
                && e.event.kind == EventKind::TaskCreated
        })
        .unwrap()
        .event
        .id
        .clone();
    let mut writer = engine
        .store()
        .spool()
        .create(ArtifactSpec {
            id: ArtifactId::new(),
            scope: scope.clone(),
            media_type: "text/plain".into(),
            schema: "memory-source/1".into(),
            source: "synthetic-governance-fixture".into(),
            channel: Channel::Evidence,
            retention: "history".into(),
            omissions: vec![],
        })
        .unwrap();
    writer
        .write_chunk(b"mod parser; // observed source revision A\n")
        .unwrap();
    let artifact = writer.finalize().unwrap();
    let attach = command(
        engine,
        &access,
        Some(scope.task.clone()),
        Revision::ZERO,
        Command::AttachArtifact {
            descriptor: artifact.clone(),
        },
    );
    engine.handle(attach, &access, &facts).await.unwrap();
    (scope, origin, artifact)
}
