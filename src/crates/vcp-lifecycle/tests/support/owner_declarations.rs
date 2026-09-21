// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_lifecycle::foundation::routing_state::declarations::{self, Input, Kind, State};

async fn declaration_artifact(store: &mut Store, scope: &Scope, input: &Input) -> ArtifactId {
    let mut spec = common::spec();
    spec.scope = scope.clone();
    spec.schema = declarations::SCHEMA.into();
    spec.channel = Channel::Evidence;
    let mut writer = store.spool().create(spec).unwrap();
    writer
        .write_chunk(&vcp_protocol::canonical_bytes(input).unwrap())
        .unwrap();
    let artifact = writer.finalize().unwrap();
    drop(writer);
    store
        .transact(common::attach(store.state(), artifact.clone(), None))
        .await
        .unwrap();
    artifact.spec.id
}
async fn set_task_state(store: &mut Store, scope: &Scope, state: TaskState) -> Task {
    let mut task: Task = store
        .state()
        .record(Collection::Task, scope.task.as_str(), &scope.workspace)
        .unwrap()
        .decode()
        .unwrap();
    let expected = task.revision;
    task.revision = expected.next().unwrap();
    task.state = state;
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![Mutation::Put {
                record: Record::typed(
                    Collection::Task,
                    scope.task.as_str(),
                    scope.workspace.clone(),
                    task.revision,
                    &task,
                )
                .unwrap(),
                expected: Some(expected),
            }],
            events: vec![],
            command: None,
        })
        .await
        .unwrap();
    task
}
async fn change_workspace(store: &mut Store, scope: &Scope, change: &str) -> AuthorityRevision {
    let mut workspace: Workspace = store
        .state()
        .record(
            Collection::Workspace,
            scope.workspace.as_str(),
            &scope.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    let expected = workspace.revision;
    workspace.revision = expected.next().unwrap();
    match change {
        "binding" => workspace.binding.revision = workspace.binding.revision.next().unwrap(),
        "authority" => workspace.authority = workspace.authority.next().unwrap(),
        "deletion" => workspace.deletion = workspace.deletion.next().unwrap(),
        _ => unreachable!(),
    }
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![Mutation::Put {
                record: Record::typed(
                    Collection::Workspace,
                    scope.workspace.as_str(),
                    scope.workspace.clone(),
                    workspace.revision,
                    &workspace,
                )
                .unwrap(),
                expected: Some(expected),
            }],
            events: vec![],
            command: None,
        })
        .await
        .unwrap();
    workspace.authority
}
fn status(store: &Store, access: &Access, input: &Input) -> serde_json::Value {
    declarations::inspect(store, access, &input.task).unwrap()["declarations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["declaration"]["input"]["command"] == input.command.as_str())
        .unwrap()
        .clone()
}

#[tokio::test]
async fn owner_declaration_claim_is_durable_and_revalidates_task_evidence_and_authority() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
        let mut initial = common::initial();
        for mutation in &mut initial.mutations {
            if let Mutation::Put { record, .. } = mutation {
                if record.collection == Collection::Task {
                    let mut task: Task = record.decode().unwrap();
                    task.state = TaskState::Running;
                    record.value = serde_json::to_value(task).unwrap();
                }
            }
        }
        store.transact(initial).await.unwrap();
        let scope = common::task().scope;
        let mut access = Access {
            workspace: scope.workspace.clone(),
            actor: actor().id,
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            tasks: None,
        };
        vcp_budget::initialize(
            &mut store,
            scope.clone(),
            money(1000),
            Micros::ZERO,
            None,
            &actor(),
        )
        .await
        .unwrap();
        let previous =
            reserve_attempt(&mut store, &scope, RequestRole::Main, 25, "first-model").await;
        let input = Input {
            command: CommandId::new(),
            task: scope.task.clone(),
            expected_revision: Revision::ZERO,
            steering: SteeringRevision::ZERO,
            declaration: Kind::DeclaredComplexity,
            evidence: vec![previous.request.clone()],
        };
        let mut invalid = input.clone();
        invalid.expected_revision = Revision::new(1);
        assert!(declarations::validate(&store, &access, &invalid).is_err());
        invalid = input.clone();
        invalid.evidence.push(previous.request.clone());
        assert!(declarations::validate(&store, &access, &invalid).is_err());
        invalid = input.clone();
        invalid.declaration = Kind::UnsupportedCapability {
            capability: "invented\ncapability".into(),
        };
        assert!(declarations::validate(&store, &access, &invalid).is_err());
        let artifact = declaration_artifact(&mut store, &scope, &input).await;
        let declaration = declarations::record(
            &mut store,
            &access,
            input.clone(),
            artifact.clone(),
            Timestamp::new(1001),
        )
        .await
        .unwrap();
        let watermark = store.state().watermark;
        access.write = false;
        assert_eq!(status(&store, &access, &input)["disposition"], "pending");
        access.write = true;
        assert_eq!(
            declarations::record(
                &mut store,
                &access,
                input.clone(),
                artifact,
                Timestamp::new(1002)
            )
            .await
            .unwrap(),
            declaration
        );
        assert_eq!(store.state().watermark, watermark);
        let mut denied = Access {
            workspace: access.workspace.clone(),
            actor: access.actor.clone(),
            authority: AuthorityRevision::new(1),
            read: true,
            write: true,
            tasks: None,
        };
        assert!(
            declarations::claim(&mut store, &denied, &scope.task, Timestamp::new(1003))
                .await
                .is_err()
        );
        denied.authority = access.authority;
        denied.tasks = Some(BTreeSet::new());
        assert!(declarations::list(&store, &denied, &scope.task).is_err());
        let (claimed, source) =
            declarations::claim(&mut store, &access, &scope.task, Timestamp::new(1004))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(source.id, previous.id);
        assert_eq!(
            claimed.state,
            State::Consumed {
                at: Timestamp::new(1004)
            }
        );
        assert_eq!(
            claimed.trigger().kind,
            vcp_models::escalation::TriggerKind::DeclaredComplexity
        );
        assert_eq!(
            claimed.trigger().evidence,
            vec![declaration.artifact, previous.request]
        );
        assert!(admitted_escalations(&store, &access, &scope)
            .unwrap()
            .is_empty());
        assert_eq!(
            status(&store, &access, &input)["disposition"],
            "consumed_without_admission"
        );
        assert_eq!(
            vcp_budget::ledger(store.state(), &scope).unwrap().active,
            Micros::new(25)
        );
        store.close().await.unwrap();
        let mut store = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert!(
            declarations::claim(&mut store, &access, &scope.task, Timestamp::new(1005))
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            declarations::existing(&store, &access, &input).unwrap(),
            Some(claimed)
        );
        let mut replacement = input;
        replacement.command = CommandId::new();
        let artifact = declaration_artifact(&mut store, &scope, &replacement).await;
        declarations::record(
            &mut store,
            &access,
            replacement.clone(),
            artifact,
            Timestamp::new(1006),
        )
        .await
        .unwrap();
        // A new task attempt invalidates the declaration's captured predecessor.
        reserve_attempt(&mut store, &scope, RequestRole::Main, 25, "new-model").await;
        assert_eq!(
            status(&store, &access, &replacement)["disposition"],
            "stale"
        );
        assert!(status(&store, &access, &replacement)["stale_reason"]
            .as_str()
            .unwrap()
            .contains("predecessor"));
        assert!(
            declarations::claim(&mut store, &access, &scope.task, Timestamp::new(1007))
                .await
                .is_err()
        );
        let held = set_task_state(&mut store, &scope, TaskState::Paused).await;
        replacement.command = CommandId::new();
        replacement.expected_revision = held.revision;
        assert!(declarations::validate(&store, &access, &replacement).is_err());
        assert!(
            declarations::claim(&mut store, &access, &scope.task, Timestamp::new(1008))
                .await
                .unwrap()
                .is_none()
        );
        let resumed = set_task_state(&mut store, &scope, TaskState::Running).await;
        // Explicit resume alone does not make the old pending signal current.
        assert!(
            declarations::claim(&mut store, &access, &scope.task, Timestamp::new(1009))
                .await
                .unwrap()
                .is_none()
        );
        replacement.expected_revision = resumed.revision;
        let artifact = declaration_artifact(&mut store, &scope, &replacement).await;
        declarations::record(
            &mut store,
            &access,
            replacement.clone(),
            artifact,
            Timestamp::new(1010),
        )
        .await
        .unwrap();
        assert!(declarations::list(&store, &access, &scope.task)
            .unwrap()
            .iter()
            .any(|d| matches!(d.state, State::Superseded { .. })));
        assert!(
            declarations::claim(&mut store, &access, &scope.task, Timestamp::new(1011))
                .await
                .unwrap()
                .is_some()
        );
        for change in ["binding", "authority", "deletion"] {
            replacement.command = CommandId::new();
            let artifact = declaration_artifact(&mut store, &scope, &replacement).await;
            declarations::record(
                &mut store,
                &access,
                replacement.clone(),
                artifact,
                Timestamp::new(1012),
            )
            .await
            .unwrap();
            assert_eq!(
                status(&store, &access, &replacement)["disposition"],
                "pending"
            );
            access.authority = change_workspace(&mut store, &scope, change).await;
            assert_eq!(
                status(&store, &access, &replacement)["disposition"],
                "stale",
                "{change}"
            );
            assert!(
                declarations::claim(&mut store, &access, &scope.task, Timestamp::new(1013))
                    .await
                    .is_err()
            );
        }
        store.close().await.unwrap();
    }
}
