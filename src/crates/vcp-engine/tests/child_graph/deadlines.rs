// SPDX-License-Identifier: Apache-2.0
use super::*;

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
