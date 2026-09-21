// SPDX-License-Identifier: Apache-2.0
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{
    accounting::*, agents::*, artifact::*, policy::*, task::*, verification::*, workspace::*, *,
};
use vcp_engine::{agents::*, *};
use vcp_protocol::command::*;
use vcp_store::{artifact::ArtifactWriter, contract::*, BackendKind, Store};

fn fingerprint() -> Fingerprint {
    Fingerprint {
        repository: "a".repeat(64),
        buffers: "b".repeat(64),
        environment: "c".repeat(64),
    }
}
fn objective() -> Objective {
    Objective {
        text: "Review a concrete parser boundary".into(),
        constraints: vec![],
        acceptance: vec!["cite a tested finding".into()],
        source: EventId::new(),
        steering: SteeringRevision::ZERO,
    }
}
struct Fixture {
    engine: Engine<Store>,
    access: Access,
    scope: Scope,
    root: RootId,
    artifact: ArtifactId,
    digest: String,
}
impl Fixture {
    fn envelope(
        &self,
        task: Option<TaskId>,
        expected: Revision,
        payload: Command,
    ) -> CommandEnvelope {
        CommandEnvelope {
            version: 1,
            id: CommandId::new(),
            workspace: self.scope.workspace.clone(),
            session: self.scope.session.clone(),
            task,
            caller: self.access.actor.clone(),
            controller: self.engine.controller().clone(),
            owner_epoch: self.engine.owner_epoch(),
            expected,
            steering: SteeringRevision::ZERO,
            payload,
        }
    }
    fn facts() -> HostFacts {
        HostFacts {
            now: Timestamp::new(10),
            policy: PolicyRevision::ZERO,
            resume: None,
            may_execute: true,
        }
    }
    async fn issue(
        &mut self,
        task: Option<TaskId>,
        expected: Revision,
        payload: Command,
    ) -> vcp_engine::Result<CommandReceipt> {
        self.engine
            .handle(
                self.envelope(task, expected, payload),
                &self.access,
                &Self::facts(),
            )
            .await
    }
    fn child(&self, amount: u64) -> ChildSpec {
        ChildSpec {
            parent: self.scope.task.clone(),
            actor: self.access.actor.clone(),
            parent_steering: SteeringRevision::ZERO,
            dependencies: BTreeSet::new(),
            mode: ChildMode::ReadOnly,
            role: "reviewer".into(),
            model_policy: "root policy".into(),
            paths: vec![ChildPath {
                root: self.root.clone(),
                path: "src".into(),
                write: false,
            }],
            effects: BTreeSet::from([EffectClass::Read]),
            authority: self.access.authority,
            policy: PolicyRevision::ZERO,
            binding: Revision::ZERO,
            grants: BTreeMap::new(),
            allocation: Micros::new(amount),
            deadline: Timestamp::new(100),
            snapshot: self.artifact.clone(),
            snapshot_digest: self.digest.clone(),
            registration: None,
            registration_digest: None,
            isolated_root: None,
        }
    }
    fn create(&self, id: TaskId, spec: ChildSpec) -> Command {
        let graph = graph(self.engine.store().state(), &self.scope, &self.scope.task).unwrap();
        let ledger: Ledger = self
            .engine
            .store()
            .state()
            .record(
                Collection::Ledger,
                self.scope.task.as_str(),
                &self.scope.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        Command::CreateChild {
            id,
            objective: objective(),
            fingerprint: fingerprint(),
            required_checks: vec![],
            spec,
            limits: GraphLimits::default(),
            expected_graph: graph.map(|g| g.revision),
            expected_ledger: ledger.revision,
        }
    }
    fn task(&self, id: &TaskId) -> Task {
        self.engine
            .store()
            .state()
            .record(Collection::Task, id.as_str(), &self.scope.workspace)
            .unwrap()
            .decode()
            .unwrap()
    }
    async fn ready(&mut self, child: &TaskId) {
        let graph = graph(self.engine.store().state(), &self.scope, &self.scope.task)
            .unwrap()
            .unwrap();
        self.engine
            .record_child_workspace(
                &self.scope,
                NativeWorkspaceEvidence {
                    child: child.clone(),
                    expected_graph: graph.revision,
                    ready: WorkspaceReady {
                        snapshot_digest: self.digest.clone(),
                        registration_digest: None,
                        native_identity: "fixture-volume:file".into(),
                    },
                },
                &self.access,
                &Self::facts(),
            )
            .await
            .unwrap();
    }
}
async fn fixture(path: &std::path::Path, backend: BackendKind) -> Fixture {
    let scope = Scope {
        workspace: WorkspaceId::new(),
        session: SessionId::new(),
        task: TaskId::new(),
    };
    let access = Access {
        actor: ActorId::new(),
        workspace: scope.workspace.clone(),
        session: scope.session.clone(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: true,
    };
    let mut f = Fixture {
        engine: Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap(),
        access,
        scope,
        root: RootId::new(),
        artifact: ArtifactId::new(),
        digest: String::new(),
    };
    f.issue(
        None,
        Revision::ZERO,
        Command::Initialize {
            binding: Binding {
                host: HostId::new(),
                root: "C:/fixture".into(),
                repository: "fixture".into(),
                worktree: "parent".into(),
                revision: Revision::ZERO,
            },
        },
    )
    .await
    .unwrap();
    f.issue(
        None,
        Revision::ZERO,
        Command::SetWorkspaceTrust {
            trust: Trust::Trusted,
        },
    )
    .await
    .unwrap();
    f.access.authority = AuthorityRevision::new(1);
    f.issue(
        None,
        Revision::ZERO,
        Command::SetPolicy {
            policy: Policy {
                workspace: f.scope.workspace.clone(),
                revision: PolicyRevision::ZERO,
                mode: Autonomy::Workspace,
                denials: vec![],
                workspace_roots: BTreeSet::from([f.root.clone()]),
                automatic_effects: BTreeSet::new(),
                timeout_ceiling_ms: Units::new(1000),
                output_ceiling_bytes: ByteCount::new(1000),
            },
        },
    )
    .await
    .unwrap();
    f.access.authority = AuthorityRevision::new(2);
    f.issue(
        Some(f.scope.task.clone()),
        Revision::ZERO,
        Command::CreateTask {
            root: f.scope.task.clone(),
            parent: None,
            fork_origin: None,
            objective: objective(),
            fingerprint: fingerprint(),
            editing: false,
            required_checks: vec![],
        },
    )
    .await
    .unwrap();
    f.issue(
        Some(f.scope.task.clone()),
        Revision::ZERO,
        Command::Transition {
            next: TaskState::Running,
            reason: "fixture".into(),
            verification: None,
        },
    )
    .await
    .unwrap();
    let ledger = Ledger {
        schema_version: 1,
        scope: f.scope.clone(),
        revision: Revision::ZERO,
        policy: PolicyRevision::ZERO,
        currency: "USD".to_string().try_into().unwrap(),
        cap: Micros::new(1000),
        protected: Micros::new(100),
        settled: Micros::ZERO,
        active: Micros::ZERO,
        unresolved: Micros::ZERO,
        allocations: BTreeMap::new(),
        daily: None,
        overrun: false,
    };
    let watermark = f.engine.store().state().watermark;
    f.engine
        .store_mut()
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: watermark,
            mutations: vec![Mutation::Put {
                expected: None,
                record: Record::typed(
                    Collection::Ledger,
                    f.scope.task.to_string(),
                    f.scope.workspace.clone(),
                    Revision::ZERO,
                    &ledger,
                )
                .unwrap(),
            }],
            events: vec![],
            command: None,
        })
        .await
        .unwrap();
    let mut writer = f
        .engine
        .store()
        .spool()
        .create(ArtifactSpec {
            id: f.artifact.clone(),
            scope: f.scope.clone(),
            media_type: "application/json".into(),
            schema: "workspace-snapshot/1".into(),
            source: "fixture".into(),
            channel: Channel::Evidence,
            retention: "history".into(),
            omissions: vec![],
        })
        .unwrap();
    writer.write_chunk(b"bounded snapshot fixture").unwrap();
    let descriptor = writer.finalize().unwrap();
    f.digest = descriptor.sha256.clone();
    f.issue(
        Some(f.scope.task.clone()),
        Revision::ZERO,
        Command::AttachArtifact { descriptor },
    )
    .await
    .unwrap();
    f
}
#[tokio::test]
async fn child_registration_is_atomic_bounded_and_durable_on_both_backends() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let first = TaskId::new();
        let payload = f.create(first.clone(), f.child(600));
        let command = f.envelope(Some(f.scope.task.clone()), Revision::new(1), payload);
        let receipt = f
            .engine
            .handle(command.clone(), &f.access, &Fixture::facts())
            .await
            .unwrap();
        assert_eq!(
            f.engine
                .handle(command.clone(), &f.access, &Fixture::facts())
                .await
                .unwrap(),
            receipt
        );
        assert_eq!(f.task(&first).state, TaskState::Pending);
        assert!(eligibility(
            f.engine.store().state(),
            &f.task(&first),
            Timestamp::new(10),
            true
        )
        .unwrap()
        .contains(&Blocker::Workspace));
        let watermark = f.engine.store().state().watermark;
        let second = TaskId::new();
        let payload = f.create(second.clone(), f.child(301));
        assert!(f
            .issue(Some(f.scope.task.clone()), Revision::new(1), payload)
            .await
            .is_err());
        assert_eq!(f.engine.store().state().watermark, watermark);
        assert!(!f
            .engine
            .store()
            .state()
            .records
            .contains_key(&key(Collection::Task, second.as_str())));
        f.ready(&first).await;
        assert!(eligibility(
            f.engine.store().state(),
            &f.task(&first),
            Timestamp::new(10),
            true
        )
        .unwrap()
        .is_empty());
        drop(f.engine);
        let engine = Engine::new(Store::open(temp.path(), backend, &[]).await.unwrap()).unwrap();
        assert_eq!(
            graph(engine.store().state(), &f.scope, &f.scope.task)
                .unwrap()
                .unwrap()
                .children
                .len(),
            1
        );
        assert!(graph(engine.store().state(), &f.scope, &f.scope.task)
            .unwrap()
            .unwrap()
            .ready
            .contains_key(&first));
    }
}
#[tokio::test]
async fn dependencies_scope_pause_and_cancellation_are_canonical() {
    let temp = tempfile::tempdir().unwrap();
    let mut f = fixture(temp.path(), BackendKind::Files).await;
    let a = TaskId::new();
    let b = TaskId::new();
    let payload = f.create(a.clone(), f.child(200));
    f.issue(Some(f.scope.task.clone()), Revision::new(1), payload)
        .await
        .unwrap();
    let mut spec = f.child(200);
    spec.dependencies.insert(a.clone());
    let payload = f.create(b.clone(), spec);
    f.issue(Some(f.scope.task.clone()), Revision::new(1), payload)
        .await
        .unwrap();
    f.ready(&a).await;
    f.ready(&b).await;
    assert!(eligibility(
        f.engine.store().state(),
        &f.task(&b),
        Timestamp::new(10),
        true
    )
    .unwrap()
    .contains(&Blocker::Dependency));
    let graph = graph(f.engine.store().state(), &f.scope, &f.scope.task)
        .unwrap()
        .unwrap();
    assert!(f
        .issue(
            Some(f.scope.task.clone()),
            Revision::new(1),
            Command::SetChildDependencies {
                child: a.clone(),
                dependencies: BTreeSet::from([b.clone()]),
                expected_graph: graph.revision
            }
        )
        .await
        .is_err());
    let mut invalid = f.child(20);
    invalid.effects.insert(EffectClass::Execute);
    let payload = f.create(TaskId::new(), invalid);
    assert!(f
        .issue(Some(f.scope.task.clone()), Revision::new(1), payload)
        .await
        .is_err());
    f.issue(
        Some(f.scope.task.clone()),
        Revision::new(1),
        Command::Transition {
            next: TaskState::Paused,
            reason: "explicit pause".into(),
            verification: None,
        },
    )
    .await
    .unwrap();
    assert!(eligibility(
        f.engine.store().state(),
        &f.task(&a),
        Timestamp::new(10),
        true
    )
    .unwrap()
    .contains(&Blocker::Ancestor));
    assert!(f
        .issue(
            Some(a.clone()),
            Revision::ZERO,
            Command::Transition {
                next: TaskState::Running,
                reason: "cannot dispatch".into(),
                verification: None
            }
        )
        .await
        .is_err());
    f.issue(
        Some(f.scope.task.clone()),
        Revision::new(2),
        Command::Transition {
            next: TaskState::Cancelled,
            reason: "explicit cancel".into(),
            verification: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(f.task(&a).state, TaskState::Cancelled);
    assert_eq!(f.task(&b).state, TaskState::Cancelled);
    let ledger: Ledger = f
        .engine
        .store()
        .state()
        .record(
            Collection::Ledger,
            f.scope.task.as_str(),
            &f.scope.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(ledger.allocations.len(), 2);
}

#[tokio::test]
async fn cancelled_child_cannot_publish_workspace_ready() {
    let temp = tempfile::tempdir().unwrap();
    let mut f = fixture(temp.path(), BackendKind::Files).await;
    let child = TaskId::new();
    let command = f.create(child.clone(), f.child(100));
    f.issue(Some(f.scope.task.clone()), Revision::new(1), command)
        .await
        .unwrap();
    f.issue(
        Some(child.clone()),
        Revision::ZERO,
        Command::Transition {
            next: TaskState::Cancelled,
            reason: "cancel during materialization".into(),
            verification: None,
        },
    )
    .await
    .unwrap();
    let before = graph(f.engine.store().state(), &f.scope, &f.scope.task)
        .unwrap()
        .unwrap();
    assert!(f
        .engine
        .record_child_workspace(
            &f.scope,
            NativeWorkspaceEvidence {
                child: child.clone(),
                expected_graph: before.revision,
                ready: WorkspaceReady {
                    snapshot_digest: f.digest.clone(),
                    registration_digest: None,
                    native_identity: "fixture-volume:file".into(),
                },
            },
            &f.access,
            &Fixture::facts()
        )
        .await
        .is_err());
    let after = graph(f.engine.store().state(), &f.scope, &f.scope.task)
        .unwrap()
        .unwrap();
    assert_eq!(before.revision, after.revision);
    assert!(!after.ready.contains_key(&child));
}

#[tokio::test]
async fn scheduler_caps_running_children_and_rejects_projection_tampering() {
    let temp = tempfile::tempdir().unwrap();
    let mut f = fixture(temp.path(), BackendKind::Files).await;
    let mut children = Vec::new();
    for _ in 0..5 {
        let child = TaskId::new();
        let payload = f.create(child.clone(), f.child(100));
        f.issue(Some(f.scope.task.clone()), Revision::new(1), payload)
            .await
            .unwrap();
        f.ready(&child).await;
        children.push(child);
    }
    for child in &children[..4] {
        f.issue(
            Some(child.clone()),
            Revision::ZERO,
            Command::Transition {
                next: TaskState::Running,
                reason: "bounded dispatch".into(),
                verification: None,
            },
        )
        .await
        .unwrap();
    }
    assert!(eligibility(
        f.engine.store().state(),
        &f.task(&children[4]),
        Timestamp::new(10),
        true
    )
    .unwrap()
    .contains(&Blocker::Concurrency));
    assert!(eligibility(
        f.engine.store().state(),
        &f.task(&children[4]),
        Timestamp::new(100),
        true
    )
    .unwrap()
    .contains(&Blocker::Deadline));
    assert!(eligibility(
        f.engine.store().state(),
        &f.task(&children[4]),
        Timestamp::new(10),
        false
    )
    .unwrap()
    .contains(&Blocker::Owner));
    let before = graph(f.engine.store().state(), &f.scope, &f.scope.task)
        .unwrap()
        .unwrap();
    let bypass = TaskId::new();
    assert!(
        f.issue(
            Some(bypass),
            Revision::ZERO,
            Command::CreateTask {
                root: f.scope.task.clone(),
                parent: Some(f.scope.task.clone()),
                fork_origin: None,
                objective: objective(),
                fingerprint: fingerprint(),
                editing: false,
                required_checks: vec![],
            }
        )
        .await
        .is_err(),
        "legacy task creation cannot bypass graph assignments"
    );
    let mut tampered = before.clone();
    tampered.revision = tampered.revision.next().unwrap();
    tampered.children.get_mut(&children[0]).unwrap().paths[0]
        .path
        .clear();
    let transaction = Transaction {
        id: TransactionId::new(),
        expected_watermark: f.engine.store().state().watermark,
        mutations: vec![Mutation::Put {
            expected: Some(before.revision),
            record: Record::typed(
                Collection::Projection,
                graph_id(&f.scope.task),
                f.scope.workspace.clone(),
                tampered.revision,
                &tampered,
            )
            .unwrap(),
        }],
        events: vec![],
        command: None,
    };
    assert!(f.engine.store_mut().transact(transaction).await.is_err());
    let transaction = Transaction {
        id: TransactionId::new(),
        expected_watermark: f.engine.store().state().watermark,
        mutations: vec![Mutation::DropProjection {
            id: graph_id(&f.scope.task),
            expected: before.revision,
        }],
        events: vec![],
        command: None,
    };
    assert!(f.engine.store_mut().transact(transaction).await.is_err());
}

#[tokio::test]
async fn nested_assignments_cannot_widen_scope_or_form_parent_dependency_deadlocks() {
    let temp = tempfile::tempdir().unwrap();
    let mut f = fixture(temp.path(), BackendKind::Files).await;
    let parent = TaskId::new();
    let payload = f.create(parent.clone(), f.child(200));
    f.issue(Some(f.scope.task.clone()), Revision::new(1), payload)
        .await
        .unwrap();
    let child = TaskId::new();
    let mut spec = f.child(100);
    spec.parent = parent.clone();
    spec.paths[0].path = "src/parser".into();
    let mut graph = graph(f.engine.store().state(), &f.scope, &f.scope.task)
        .unwrap()
        .unwrap();
    graph.children.insert(child.clone(), spec);
    graph.validate().unwrap();
    let mut too_deep = graph.clone();
    too_deep.limits.depth = 1;
    assert!(too_deep.validate().is_err());
    let mut too_broad = graph.clone();
    too_broad.children.get_mut(&child).unwrap().paths[0]
        .path
        .clear();
    assert!(too_broad.validate().is_err());
    let mut too_large = graph.clone();
    too_large.children.get_mut(&child).unwrap().allocation = Micros::new(201);
    assert!(too_large.validate().is_err());
    let mut deadlock = graph.clone();
    deadlock
        .children
        .get_mut(&parent)
        .unwrap()
        .dependencies
        .insert(child.clone());
    assert!(deadlock.validate().is_err());
    graph
        .children
        .get_mut(&child)
        .unwrap()
        .dependencies
        .insert(parent);
    assert!(graph.validate().is_err());
}

#[test]
fn resource_scopes_use_component_boundaries_and_case_insensitive_windows_names() {
    let root = RootId::new();
    let parent = ChildPath {
        root: root.clone(),
        path: "src".into(),
        write: true,
    };
    let inside = ChildPath {
        root: root.clone(),
        path: "SRC/parser.rs".into(),
        write: true,
    };
    let outside = ChildPath {
        root,
        path: "src-other/parser.rs".into(),
        write: true,
    };
    assert!(parent.covers(&inside));
    assert!(parent.overlaps(&inside));
    assert!(!parent.covers(&outside));
    assert!(!parent.overlaps(&outside));
    let mut readonly = parent.clone();
    readonly.write = false;
    assert!(!readonly.covers(&inside));
    let unicode = ChildPath {
        root: parent.root.clone(),
        path: "src/K".into(),
        write: true,
    };
    let lowercase = ChildPath {
        root: parent.root.clone(),
        path: "src/k/value.rs".into(),
        write: true,
    };
    let exact = ChildPath {
        root: parent.root.clone(),
        path: "src/K/value.rs".into(),
        write: true,
    };
    assert!(
        !unicode.covers(&lowercase),
        "Unicode lowercase cannot broaden authority"
    );
    assert!(unicode.covers(&exact));
    assert!(
        unicode.overlaps(&outside),
        "unqualified Unicode aliases serialize within a root"
    );
    let read_unicode = ChildPath {
        write: false,
        ..unicode.clone()
    };
    let read_exact = ChildPath {
        write: false,
        ..exact
    };
    assert!(!read_unicode.overlaps(&read_exact));
}

#[tokio::test]
async fn nested_grants_require_the_complete_declared_chain() {
    let temp = tempfile::tempdir().unwrap();
    let mut f = fixture(temp.path(), BackendKind::Files).await;
    let workspace: Workspace = f
        .engine
        .store()
        .state()
        .record(
            Collection::Workspace,
            f.scope.workspace.as_str(),
            &f.scope.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    let mut grant = Grant {
        id: GrantId::new(),
        actor: f.access.actor.clone(),
        scope: GrantScope::Task {
            scope: f.scope.clone(),
        },
        host: workspace.binding.host,
        binding: Revision::ZERO,
        authority: f.access.authority,
        policy: PolicyRevision::ZERO,
        expires_at: Timestamp::new(100),
        target: GrantTarget::Configured {
            tool: "vcp_read".into(),
            schema: "a".repeat(64),
            arguments_digest: "b".repeat(64),
            invocation: Invocation::Local,
            effects: BTreeSet::from([EffectClass::Read]),
            roots: BTreeSet::from([f.root.clone()]),
            paths: vec!["src".into()],
            isolation: BTreeSet::from([Isolation::PathContainment]),
            timeout_ms: Units::new(1000),
            output_bytes: ByteCount::new(1000),
        },
        origin: RuleOrigin::User,
        reason: "Explicit nested read ceiling".into(),
        revoked: false,
        revision: Revision::ZERO,
        approval: None,
    };
    f.issue(
        None,
        Revision::ZERO,
        Command::SetGrant {
            grant: grant.clone(),
        },
    )
    .await
    .unwrap();
    let child = TaskId::new();
    let mut spec = f.child(200);
    spec.grants.insert(grant.id.clone(), grant.revision);
    let command = f.create(child.clone(), spec);
    f.issue(Some(f.scope.task.clone()), Revision::new(1), command)
        .await
        .unwrap();
    let task = f.task(&child);
    assert!(inherited_grant(f.engine.store().state(), &task, &grant, Timestamp::new(10)).unwrap());
    grant.revision = Revision::new(1);
    assert!(!inherited_grant(f.engine.store().state(), &task, &grant, Timestamp::new(10)).unwrap());
    grant.revision = Revision::ZERO;
    grant.target = GrantTarget::Exact {
        digest: "a".repeat(64),
    };
    assert!(!inherited_grant(f.engine.store().state(), &task, &grant, Timestamp::new(10)).unwrap());
    grant.target = GrantTarget::Configured {
        tool: "vcp_read".into(),
        schema: "a".repeat(64),
        arguments_digest: "b".repeat(64),
        invocation: Invocation::Local,
        effects: BTreeSet::from([EffectClass::Read]),
        roots: BTreeSet::from([f.root.clone()]),
        paths: vec!["src".into()],
        isolation: BTreeSet::from([Isolation::PathContainment]),
        timeout_ms: Units::new(1000),
        output_bytes: ByteCount::new(1000),
    };
    grant.scope = GrantScope::Task {
        scope: Scope {
            task: TaskId::new(),
            ..f.scope.clone()
        },
    };
    assert!(!inherited_grant(f.engine.store().state(), &task, &grant, Timestamp::new(10)).unwrap());
    grant.scope = GrantScope::Task {
        scope: f.scope.clone(),
    };
    assert!(
        !inherited_grant(f.engine.store().state(), &task, &grant, Timestamp::new(100)).unwrap()
    );
}

#[tokio::test]
async fn child_result_history_is_scoped_append_only_and_not_completion() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let child = TaskId::new();
        let create = f.create(child.clone(), f.child(100));
        f.issue(Some(f.scope.task.clone()), Revision::new(1), create)
            .await
            .unwrap();
        let mut artifacts = Vec::new();
        for schema in ["child-result-packet/1", "child-integration-plan/1"] {
            let mut writer = f
                .engine
                .store()
                .spool()
                .create(ArtifactSpec {
                    id: ArtifactId::new(),
                    scope: f.scope.clone(),
                    media_type: "application/json".into(),
                    schema: schema.into(),
                    source: "fixture".into(),
                    channel: Channel::Evidence,
                    retention: "history".into(),
                    omissions: vec![],
                })
                .unwrap();
            writer.write_chunk(b"{\"untrusted_findings\":[]}").unwrap();
            let descriptor = writer.finalize().unwrap();
            artifacts.push(descriptor.spec.id.clone());
            f.issue(
                Some(f.scope.task.clone()),
                Revision::ZERO,
                Command::AttachArtifact { descriptor },
            )
            .await
            .unwrap();
        }
        let result = ChildResultRef {
            packet: artifacts[0].clone(),
            plan: artifacts[1].clone(),
            effect: None,
        };
        let current = graph(f.engine.store().state(), &f.scope, &f.scope.task)
            .unwrap()
            .unwrap();
        let bad = ChildResultRef {
            packet: f.artifact.clone(),
            ..result.clone()
        };
        assert!(f
            .issue(
                Some(f.scope.task.clone()),
                Revision::new(1),
                Command::SubmitChildResult {
                    child: child.clone(),
                    result: bad,
                    expected_graph: current.revision
                }
            )
            .await
            .is_err());
        for _ in 0..2 {
            let current = graph(f.engine.store().state(), &f.scope, &f.scope.task)
                .unwrap()
                .unwrap();
            f.issue(
                Some(f.scope.task.clone()),
                Revision::new(1),
                Command::SubmitChildResult {
                    child: child.clone(),
                    result: result.clone(),
                    expected_graph: current.revision,
                },
            )
            .await
            .unwrap();
        }
        assert_eq!(f.task(&child).state, TaskState::Pending);
        assert_eq!(f.task(&f.scope.task).state, TaskState::Running);
        let mut current = graph(f.engine.store().state(), &f.scope, &f.scope.task)
            .unwrap()
            .unwrap();
        assert_eq!(current.results[&child].len(), 2);
        let old_revision = current.revision;
        current.revision = current.revision.next().unwrap();
        current.results.get_mut(&child).unwrap().remove(0);
        let watermark = f.engine.store().state().watermark;
        assert!(f
            .engine
            .store_mut()
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: watermark,
                mutations: vec![Mutation::Put {
                    expected: Some(old_revision),
                    record: Record::typed(
                        Collection::Projection,
                        graph_id(&f.scope.task),
                        f.scope.workspace.clone(),
                        current.revision,
                        &current
                    )
                    .unwrap()
                }],
                events: vec![],
                command: None,
            })
            .await
            .is_err());
    }
}

#[tokio::test]
async fn paused_child_requires_explicit_current_resume_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let mut f = fixture(temp.path(), BackendKind::Files).await;
    let child = TaskId::new();
    let create = f.create(child.clone(), f.child(100));
    f.issue(Some(f.scope.task.clone()), Revision::new(1), create)
        .await
        .unwrap();
    f.ready(&child).await;
    f.issue(
        Some(child.clone()),
        Revision::ZERO,
        Command::Transition {
            next: TaskState::Paused,
            reason: "owner loss".into(),
            verification: None,
        },
    )
    .await
    .unwrap();
    let task = f.task(&child);
    assert!(
        eligibility(f.engine.store().state(), &task, Timestamp::new(10), true)
            .unwrap()
            .contains(&Blocker::State)
    );
    assert!(
        eligibility_for_resume(f.engine.store().state(), &task, Timestamp::new(10), true)
            .unwrap()
            .is_empty()
    );
    assert!(
        eligibility_for_resume(f.engine.store().state(), &task, Timestamp::new(100), true)
            .unwrap()
            .contains(&Blocker::Scope)
    );
    let command = f.envelope(
        Some(child.clone()),
        task.revision,
        Command::Transition {
            next: TaskState::Running,
            reason: "explicit child resume".into(),
            verification: None,
        },
    );
    assert!(f
        .engine
        .handle(command.clone(), &f.access, &Fixture::facts())
        .await
        .is_err());
    let mut facts = Fixture::facts();
    facts.resume = Some(ResumeEvidence {
        workspace_current: true,
        policy_current: true,
        budget_current: true,
        effects_reconciled: false,
        owner_current: true,
    });
    assert!(f
        .engine
        .handle(command.clone(), &f.access, &facts)
        .await
        .is_err());
    facts.resume.as_mut().unwrap().effects_reconciled = true;
    f.engine.handle(command, &f.access, &facts).await.unwrap();
    assert_eq!(f.task(&child).state, TaskState::Running);
}

#[tokio::test]
async fn eligibility_blocks_zero_remaining_capacity_without_charging_unused_allocations() {
    let temp = tempfile::tempdir().unwrap();
    let mut f = fixture(temp.path(), BackendKind::Files).await;
    let child = TaskId::new();
    let sibling = TaskId::new();
    for id in [&child, &sibling] {
        let create = f.create(id.clone(), f.child(400));
        f.issue(Some(f.scope.task.clone()), Revision::new(1), create)
            .await
            .unwrap();
        f.ready(id).await;
    }
    let task = f.task(&child);
    let initial = f.engine.store().state().clone();
    let ledger_key = key(Collection::Ledger, f.scope.task.as_str());
    for (charged, liability, exhausted) in [
        (399, 0, false),
        (400, 0, true),
        (0, 399, false),
        (0, 400, true),
    ] {
        // Pure projection inputs: actual reserve/settle contracts are tested by
        // vcp-budget. These rows isolate equality and uncertain exposure math.
        let mut state = initial.clone();
        let row = state.records.get_mut(&ledger_key).unwrap();
        let mut ledger: Ledger = row.decode().unwrap();
        ledger.settled = Micros::new(charged);
        ledger.unresolved = Micros::new(liability);
        row.value = serde_json::to_value(ledger).unwrap();
        let reservation = Reservation {
            schema_version: 1,
            id: ReservationId::new(),
            scope: task.scope.clone(),
            root: f.scope.task.clone(),
            attempt: AttemptId::new(),
            revision: Revision::ZERO,
            phase: if liability == 0 {
                ReservationState::Settled
            } else {
                ReservationState::ReconciliationPending
            },
            amount: Money {
                currency: "USD".to_string().try_into().unwrap(),
                micros: Micros::new(charged + liability),
            },
            charged: Micros::new(charged),
            liability: Micros::new(liability),
            protected_draw: Micros::ZERO,
            protected_returned: Micros::ZERO,
            day: 0,
            role: RequestRole::Child,
        };
        state.records.insert(
            key(Collection::Reservation, reservation.id.as_str()),
            Record::typed(
                Collection::Reservation,
                reservation.id.to_string(),
                f.scope.workspace.clone(),
                Revision::ZERO,
                &reservation,
            )
            .unwrap(),
        );
        assert_eq!(
            eligibility(&state, &task, Timestamp::new(10), true)
                .unwrap()
                .contains(&Blocker::Budget),
            exhausted
        );
        assert!(
            !eligibility(&state, &f.task(&sibling), Timestamp::new(10), true)
                .unwrap()
                .contains(&Blocker::Budget),
            "unused sibling capacity is not charged a second time"
        );
    }
    for (settled, active, unresolved) in [(900, 0, 0), (0, 900, 0), (0, 0, 900), (300, 300, 300)] {
        let mut state = initial.clone();
        let row = state.records.get_mut(&ledger_key).unwrap();
        let mut ledger: Ledger = row.decode().unwrap();
        ledger.settled = Micros::new(settled);
        ledger.active = Micros::new(active);
        ledger.unresolved = Micros::new(unresolved);
        row.value = serde_json::to_value(ledger).unwrap();
        assert!(
            eligibility(&state, &task, Timestamp::new(10), true)
                .unwrap()
                .contains(&Blocker::Budget),
            "100 protected plus 900 exposure exhausts root"
        );
    }
}
