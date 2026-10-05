// SPDX-License-Identifier: Apache-2.0
use super::*;

async fn time_evidence(f: &mut Fixture) -> ArtifactId {
    let mut writer = f
        .engine
        .store()
        .spool()
        .create(ArtifactSpec {
            id: ArtifactId::new(),
            scope: f.scope.clone(),
            media_type: "application/json".into(),
            schema: "child-execution-time/1".into(),
            source: "explicit test owner policy".into(),
            channel: Channel::Evidence,
            retention: "history".into(),
            omissions: vec![],
        })
        .unwrap();
    writer.write_chunk(br#"{"effective":"unbounded"}"#).unwrap();
    let descriptor = writer.finalize().unwrap();
    let id = descriptor.spec.id.clone();
    f.issue(
        Some(f.scope.task.clone()),
        Revision::ZERO,
        Command::AttachArtifact { descriptor },
    )
    .await
    .unwrap();
    id
}

#[tokio::test]
async fn retained_nested_time_conversion_requires_the_whole_ancestor_chain() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture_with_cap(temp.path(), backend, Limit::Unbounded).await;
        let parent = TaskId::new();
        let payload = f.create(parent.clone(), f.child(200));
        f.issue(Some(f.scope.task.clone()), Revision::new(1), payload)
            .await
            .unwrap();
        f.ready(&parent).await;
        f.issue(
            Some(parent.clone()),
            Revision::ZERO,
            Command::Transition {
                next: TaskState::Running,
                reason: "explicit nested fixture".into(),
                verification: None,
            },
        )
        .await
        .unwrap();
        let mut writer = f
            .engine
            .store()
            .spool()
            .create(ArtifactSpec {
                id: ArtifactId::new(),
                scope: Scope {
                    task: parent.clone(),
                    ..f.scope.clone()
                },
                media_type: "application/json".into(),
                schema: "workspace-snapshot/1".into(),
                source: "nested fixture".into(),
                channel: Channel::Evidence,
                retention: "history".into(),
                omissions: vec![],
            })
            .unwrap();
        writer.write_chunk(b"nested snapshot").unwrap();
        let descriptor = writer.finalize().unwrap();
        let child = TaskId::new();
        let mut spec = f.child(600);
        spec.parent = parent.clone();
        spec.deadline = Limit::Finite(Timestamp::new(90));
        spec.snapshot = descriptor.spec.id.clone();
        spec.snapshot_digest = descriptor.sha256.clone();
        f.issue(
            Some(parent.clone()),
            Revision::ZERO,
            Command::AttachArtifact { descriptor },
        )
        .await
        .unwrap();
        let payload = f.create(child.clone(), spec);
        f.issue(Some(parent.clone()), Revision::new(1), payload)
            .await
            .unwrap();
        f.issue(
            Some(parent.clone()),
            Revision::new(1),
            Command::Transition {
                next: TaskState::Paused,
                reason: "explicit nested pause".into(),
                verification: None,
            },
        )
        .await
        .unwrap();
        let evidence = time_evidence(&mut f).await;
        let original = graph(f.engine.store().current(), &f.scope, &f.scope.task)
            .unwrap()
            .unwrap();
        let before = f.engine.store().archive_state().await.unwrap();
        let facts = HostFacts {
            now: Timestamp::new(150),
            ..Fixture::facts()
        };
        assert!(f
            .engine
            .suspend_child_execution_time(
                &f.scope,
                NativeExecutionTimeEvidence {
                    expected_graph: original.revision,
                    children: BTreeSet::from([child.clone()]),
                    evidence: evidence.clone(),
                },
                &f.access,
                &facts
            )
            .await
            .is_err());
        assert_eq!(before, f.engine.store().archive_state().await.unwrap());
        f.engine
            .suspend_child_execution_time(
                &f.scope,
                NativeExecutionTimeEvidence {
                    expected_graph: original.revision,
                    children: BTreeSet::from([parent.clone(), child.clone()]),
                    evidence,
                },
                &f.access,
                &facts,
            )
            .await
            .unwrap()
            .unwrap();
        let converted = graph(f.engine.store().current(), &f.scope, &f.scope.task)
            .unwrap()
            .unwrap();
        assert_eq!(converted.children, original.children);
        assert_eq!(converted.execution_time.len(), 2);
        assert_eq!(
            converted.execution_time[&parent].graph_revision,
            converted.execution_time[&child].graph_revision
        );
        assert_eq!(
            converted.effective_child(&parent).unwrap().deadline,
            Limit::Unbounded
        );
        assert_eq!(
            converted.effective_child(&child).unwrap().deadline,
            Limit::Unbounded
        );
        f.engine.into_store().close().await.unwrap();
    }
}

#[tokio::test]
async fn retained_finite_child_conversion_is_explicit_immutable_and_keeps_grant_expiry() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let workspace: Workspace = f
            .engine
            .store()
            .current()
            .record(
                Collection::Workspace,
                f.scope.workspace.as_str(),
                &f.scope.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        let grant = Grant {
            id: GrantId::new(),
            actor: f.access.actor.clone(),
            scope: GrantScope::Task {
                scope: f.scope.clone(),
            },
            host: workspace.binding.host,
            binding: Revision::ZERO,
            authority: f.access.authority,
            policy: PolicyRevision::ZERO,
            expires_at: Timestamp::new(200),
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
            reason: "independently expiring authority".into(),
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
        let payload = f.create(child.clone(), spec.clone());
        f.issue(Some(f.scope.task.clone()), Revision::new(1), payload)
            .await
            .unwrap();
        f.ready(&child).await;
        f.issue(
            Some(child.clone()),
            Revision::ZERO,
            Command::Transition {
                next: TaskState::Running,
                reason: "prove selected-child quiescence".into(),
                verification: None,
            },
        )
        .await
        .unwrap();
        let evidence = time_evidence(&mut f).await;
        let running_graph = graph(f.engine.store().current(), &f.scope, &f.scope.task)
            .unwrap()
            .unwrap();
        let running_state = f.engine.store().archive_state().await.unwrap();
        assert!(f
            .engine
            .suspend_child_execution_time(
                &f.scope,
                NativeExecutionTimeEvidence {
                    expected_graph: running_graph.revision,
                    children: BTreeSet::from([child.clone()]),
                    evidence: evidence.clone(),
                },
                &f.access,
                &Fixture::facts()
            )
            .await
            .is_err());
        assert_eq!(
            running_state,
            f.engine.store().archive_state().await.unwrap()
        );
        f.issue(
            Some(child.clone()),
            Revision::new(1),
            Command::Transition {
                next: TaskState::Paused,
                reason: "explicit owner pause".into(),
                verification: None,
            },
        )
        .await
        .unwrap();
        let mut facts = Fixture::facts();
        facts.now = Timestamp::new(150);
        assert!(eligibility_for_resume(
            f.engine.store().current(),
            &f.task(&child),
            facts.now,
            true
        )
        .unwrap()
        .contains(&Blocker::Deadline));
        let original = graph(f.engine.store().current(), &f.scope, &f.scope.task)
            .unwrap()
            .unwrap();
        let before = f.engine.store().archive_state().await.unwrap();
        let native = |expected_graph| NativeExecutionTimeEvidence {
            expected_graph,
            children: BTreeSet::from([child.clone()]),
            evidence: evidence.clone(),
        };
        let mut foreign = f.access.clone();
        foreign.actor = ActorId::new();
        assert!(f
            .engine
            .suspend_child_execution_time(&f.scope, native(original.revision), &foreign, &facts)
            .await
            .is_err());
        assert!(f
            .engine
            .suspend_child_execution_time(
                &f.scope,
                native(original.revision.next().unwrap()),
                &f.access,
                &facts
            )
            .await
            .is_err());
        assert_eq!(before, f.engine.store().archive_state().await.unwrap());
        f.engine
            .suspend_child_execution_time(&f.scope, native(original.revision), &f.access, &facts)
            .await
            .unwrap()
            .unwrap();
        let converted = graph(f.engine.store().current(), &f.scope, &f.scope.task)
            .unwrap()
            .unwrap();
        assert_eq!(converted.children, original.children);
        assert_eq!(
            converted.execution_time[&child].original,
            Limit::Finite(Timestamp::new(100))
        );
        assert_eq!(
            converted.effective_child(&child).unwrap().deadline,
            Limit::Unbounded
        );
        assert_eq!(f.task(&child).state, TaskState::Paused);
        assert!(eligibility_for_resume(
            f.engine.store().current(),
            &f.task(&child),
            facts.now,
            true
        )
        .unwrap()
        .is_empty());
        let after = f.engine.store().archive_state().await.unwrap();
        assert!(f
            .engine
            .suspend_child_execution_time(&f.scope, native(original.revision), &f.access, &facts)
            .await
            .unwrap()
            .is_none());
        assert_eq!(after, f.engine.store().archive_state().await.unwrap());
        let blockers = eligibility_for_resume(
            f.engine.store().current(),
            &f.task(&child),
            Timestamp::new(201),
            true,
        )
        .unwrap();
        assert!(blockers.contains(&Blocker::Scope));
        assert!(!blockers.contains(&Blocker::Deadline));
        let state_grant =
            vcp_engine::policy::grants(f.engine.store().current(), &f.scope.workspace).unwrap();
        assert_eq!(state_grant.iter().find(|g| g.id == grant.id), Some(&grant));
        let mut forged = converted.clone();
        forged.execution_time.remove(&child);
        forged.revision = forged.revision.next().unwrap();
        let result = f
            .engine
            .store_mut()
            .transact(Transaction {
                id: TransactionId::new(),
                expected_watermark: after.watermark,
                mutations: vec![Mutation::Put {
                    expected: Some(converted.revision),
                    record: Record::typed(
                        Collection::Projection,
                        graph_id(&f.scope.task),
                        f.scope.workspace.clone(),
                        forged.revision,
                        &forged,
                    )
                    .unwrap(),
                }],
                command: None,
                events: vec![vcp_protocol::event::EventInput {
                    id: EventId::new(),
                    workspace: f.scope.workspace.clone(),
                    session: f.scope.session.clone(),
                    task: Some(f.scope.task.clone()),
                    actor: f.access.actor.clone(),
                    correlation: CommandId::new(),
                    causation: None,
                    timestamp: facts.now,
                    kind: vcp_protocol::event::EventKind::ChildGraphChanged,
                    artifacts: vec![],
                    data: serde_json::json!({"schema_version":1}),
                    metadata: None,
                }],
            })
            .await;
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("immutable child execution time revision"));
        assert_eq!(after, f.engine.store().archive_state().await.unwrap());
        let mut changed_policy =
            vcp_engine::policy::current(f.engine.store().current(), &f.scope.workspace).unwrap();
        changed_policy.revision = changed_policy.revision.next().unwrap();
        f.issue(
            None,
            Revision::ZERO,
            Command::SetPolicy {
                policy: changed_policy,
            },
        )
        .await
        .unwrap();
        assert!(
            eligibility_for_resume(f.engine.store().current(), &f.task(&child), facts.now, true)
                .unwrap()
                .contains(&Blocker::Scope),
            "later policy revocation remains representable and blocks resume"
        );
        assert_eq!(
            graph(f.engine.store().current(), &f.scope, &f.scope.task)
                .unwrap()
                .unwrap(),
            converted
        );
        let after_revocation = f.engine.store().archive_state().await.unwrap();
        f.engine.into_store().close().await.unwrap();
        let reopened = Store::open(temp.path(), backend, &[]).await.unwrap();
        let replayed = graph(reopened.current(), &f.scope, &f.scope.task)
            .unwrap()
            .unwrap();
        assert_eq!(replayed, converted);
        assert_eq!(after_revocation, reopened.archive_state().await.unwrap());
        reopened.close().await.unwrap();
    }
}

#[tokio::test]
async fn nested_funding_uses_effective_root_ledger_without_widening_scope() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        for cap in [Limit::Finite(Micros::new(1000)), Limit::Unbounded] {
            let temp = tempfile::tempdir().unwrap();
            let mut f = fixture_with_cap(temp.path(), backend, cap).await;
            let parent = TaskId::new();
            let mut assignment = f.child(200);
            assignment.deadline = Limit::Unbounded;
            let payload = f.create(parent.clone(), assignment);
            f.issue(Some(f.scope.task.clone()), Revision::new(1), payload)
                .await
                .unwrap();
            f.ready(&parent).await;
            f.issue(
                Some(parent.clone()),
                Revision::ZERO,
                Command::Transition {
                    next: TaskState::Running,
                    reason: "explicit nested test dispatch".into(),
                    verification: None,
                },
            )
            .await
            .unwrap();
            let snapshot = ArtifactId::new();
            let mut writer = f
                .engine
                .store()
                .spool()
                .create(ArtifactSpec {
                    id: snapshot.clone(),
                    scope: Scope {
                        task: parent.clone(),
                        ..f.scope.clone()
                    },
                    media_type: "application/json".into(),
                    schema: "workspace-snapshot/1".into(),
                    source: "nested fixture".into(),
                    channel: Channel::Evidence,
                    retention: "history".into(),
                    omissions: vec![],
                })
                .unwrap();
            writer.write_chunk(b"nested snapshot fixture").unwrap();
            let descriptor = writer.finalize().unwrap();
            let digest = descriptor.sha256.clone();
            f.issue(
                Some(parent.clone()),
                Revision::ZERO,
                Command::AttachArtifact { descriptor },
            )
            .await
            .unwrap();
            let child = TaskId::new();
            let mut spec = f.child(600);
            spec.snapshot = snapshot;
            spec.snapshot_digest = digest;
            spec.parent = parent.clone();
            spec.deadline = Limit::Unbounded;
            spec.paths[0].path = "src/parser".into();
            let mut outside = spec.clone();
            outside.paths[0].path = "outside".into();
            let payload = f.create(TaskId::new(), outside);
            assert!(f
                .issue(Some(parent.clone()), Revision::new(1), payload)
                .await
                .is_err());
            let before = f.engine.store().archive_state().await.unwrap();
            let payload = f.create(child.clone(), spec);
            let result = f.issue(Some(parent), Revision::new(1), payload).await;
            if cap.is_unbounded() {
                result.unwrap();
                let graph = graph(f.engine.store().current(), &f.scope, &f.scope.task)
                    .unwrap()
                    .unwrap();
                assert_eq!(graph.children[&child].allocation, Micros::new(600));
                assert_eq!(graph.children[&child].deadline, Limit::Unbounded);
            } else {
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("child allocation exceeds finite parent"),
                    "finite parent allocation remains authoritative"
                );
                assert_eq!(before, f.engine.store().archive_state().await.unwrap());
            }
            f.engine.into_store().close().await.unwrap();
        }
    }
}

#[tokio::test]
async fn finite_child_deadline_legacy_meaning_and_explicit_unbounded_scope() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut f = fixture(temp.path(), backend).await;
        let finite = f.child(200);
        let mut child = finite.clone();
        child.deadline = Limit::Unbounded;
        assert!(
            !child.within(&finite),
            "finite parent cannot grant unlimited time"
        );
        assert!(finite.within(&child));
        assert!(child.within(&child));
        let mut expanded = child.clone();
        expanded.paths[0].path = "outside".into();
        assert!(!expanded.within(&child));

        let command = f.envelope(
            Some(f.scope.task.clone()),
            Revision::new(1),
            f.create(TaskId::new(), finite.clone()),
        );
        let mut legacy = serde_json::to_value(&command).unwrap();
        legacy["payload"]["spec"]["deadline"] = serde_json::to_value(Timestamp::new(100)).unwrap();
        let decoded: CommandEnvelope = serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(decoded, command);
        assert_eq!(
            decoded.digest().unwrap(),
            vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&legacy).unwrap())
        );
        let Command::CreateChild { spec, .. } = &decoded.payload else {
            panic!("child command")
        };
        assert_eq!(spec.deadline, Limit::Finite(Timestamp::new(100)));
        assert_eq!(
            serde_json::to_value(spec).unwrap()["deadline"]["kind"],
            "finite"
        );

        let id = TaskId::new();
        let command = f.create(id.clone(), child);
        f.issue(Some(f.scope.task.clone()), Revision::new(1), command)
            .await
            .unwrap();
        f.ready(&id).await;
        let task = f.task(&id);
        assert!(eligibility(
            f.engine.store().current(),
            &task,
            Timestamp::new(10000),
            true
        )
        .unwrap()
        .is_empty());
        let graph = graph(f.engine.store().current(), &f.scope, &f.scope.task)
            .unwrap()
            .unwrap();
        let mut spec = graph.children[&id].clone();
        spec.parent_steering = SteeringRevision::new(1);
        assert!(!current_scope(
            f.engine.store().current(),
            &f.task(&f.scope.task),
            &spec,
            Timestamp::new(10000)
        )
        .unwrap());
        f.engine.into_store().close().await.unwrap();
    }
}
