// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{Access,Engine,HostFacts,public::PublicError,query::{Query,QueryError,QueryResult}};
use vcp_domain::{artifact::{ArtifactSpec,Channel},task::Objective,verification::Fingerprint,workspace::Binding};
use vcp_protocol::{command::{Command,CommandReceipt},methods::{self,Call}};
use vcp_store::{Store,BackendKind,artifact::ArtifactWriter,contract::CanonicalStore};
    fn access() -> Access {
        Access {
            actor: ActorId::parse("owner").unwrap(),
            workspace: WorkspaceId::parse("workspace").unwrap(),
            session: SessionId::parse("session").unwrap(),
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            bootstrap: true,
        }
    }
    fn id(value: &str) -> methods::Id {
        value.to_owned().try_into().unwrap()
    }
    fn scope() -> methods::Scope {
        methods::Scope {
            workspace: id("workspace"),
            session: id("session"),
        }
    }
    fn mutation(command: &str, expected: u64, steering: u64) -> methods::Mutation {
        methods::Mutation {
            command_id: id(command),
            expected_revision: expected.into(),
            steering_revision: steering.into(),
        }
    }
    fn facts() -> HostFacts {
        HostFacts {
            may_execute: true,
            ..HostFacts::inspect(Timestamp::new(10))
        }
    }
    fn objective(text: &str) -> Objective {
        Objective {
            text: text.into(),
            constraints: vec![],
            acceptance: vec![],
            source: EventId::new(),
            steering: SteeringRevision::ZERO,
        }
    }
    async fn internal(
        engine: &mut Engine<Store>,
        payload: Command,
        task: Option<TaskId>,
        expected: u64,
        steering: u64,
    ) -> CommandReceipt {
        let access = access();
        let envelope = CommandEnvelope {
            version: 1,
            id: CommandId::new(),
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            task,
            caller: access.actor.clone(),
            controller: engine.controller().clone(),
            owner_epoch: engine.owner_epoch(),
            expected: Revision::new(expected),
            steering: SteeringRevision::new(steering),
            payload,
        };
        engine.handle(envelope, &access, &facts()).await.unwrap()
    }
    async fn setup(path: &std::path::Path, backend: BackendKind) -> Engine<Store> {
        let mut engine = Engine::new(Store::open(path, backend, &[]).await.unwrap()).unwrap();
        internal(
            &mut engine,
            Command::Initialize {
                binding: Binding {
                    host: HostId::parse("host").unwrap(),
                    root: "C:/public-fixture".into(),
                    repository: "fixture".into(),
                    worktree: "main".into(),
                    revision: Revision::ZERO,
                },
            },
            None,
            0,
            0,
        )
        .await;
        engine
    }
    async fn task(engine: &mut Engine<Store>) -> TaskId {
        let task = TaskId::parse("task").unwrap();
        internal(
            engine,
            Command::CreateTask {
                root: task.clone(),
                parent: None,
                fork_origin: None,
                objective: objective("original"),
                fingerprint: Fingerprint {
                    repository: "a".repeat(64),
                    buffers: "b".repeat(64),
                    environment: "c".repeat(64),
                },
                editing: false,
                required_checks: vec![],
            },
            Some(task.clone()),
            0,
            0,
        )
        .await;
        task
    }
