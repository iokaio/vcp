// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]
use vcp_domain::{artifact::*, ids::*, revision::*, task::*, verification::*, workspace::*};
use vcp_protocol::{command::*, event::*};
use vcp_store::contract::*;
pub fn workspace() -> Workspace {
    Workspace {
        id: WorkspaceId::parse("workspace").unwrap(),
        binding: Binding {
            host: HostId::parse("host").unwrap(),
            root: "C:/synthetic".into(),
            repository: "fixture".into(),
            worktree: "main".into(),
            revision: Revision::ZERO,
        },
        trust: Trust::Trusted,
        revision: Revision::ZERO,
        authority: AuthorityRevision::ZERO,
        deletion: DeletionEpoch::ZERO,
    }
}
pub fn session() -> Session {
    Session {
        id: SessionId::parse("session").unwrap(),
        workspace: workspace().id,
        revision: Revision::ZERO,
        configuration: Revision::ZERO,
        fork_origin: None,
    }
}
pub fn task() -> Task {
    let id = TaskId::parse("task").unwrap();
    Task {
        scope: Scope {
            workspace: workspace().id,
            session: session().id,
            task: id.clone(),
        },
        root: id,
        parent: None,
        fork_origin: None,
        revision: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        objectives: vec![Objective {
            text: "Explain a synthetic fixture".into(),
            constraints: vec![],
            acceptance: vec!["cite evidence".into()],
            source: EventId::parse("created").unwrap(),
            steering: SteeringRevision::ZERO,
        }],
        state: TaskState::Pending,
        fingerprint: Fingerprint {
            repository: "a".repeat(64),
            buffers: "b".repeat(64),
            environment: "c".repeat(64),
        },
        editing: false,
        required_checks: vec![],
        cause: EventId::parse("created").unwrap(),
        reason: "user request".into(),
    }
}
pub fn spec() -> ArtifactSpec {
    ArtifactSpec {
        id: ArtifactId::new(),
        scope: task().scope,
        media_type: "application/octet-stream".into(),
        schema: "fixture/1".into(),
        source: "synthetic".into(),
        channel: Channel::Stdout,
        retention: "history".into(),
        omissions: vec![],
    }
}
pub fn initial() -> Transaction {
    let w = workspace();
    let s = session();
    let t = task();
    let command = CommandId::parse("create").unwrap();
    Transaction {
        id: TransactionId::parse("initial").unwrap(),
        expected_watermark: Watermark::ZERO,
        mutations: vec![
            Mutation::Put {
                expected: None,
                record: Record::typed(
                    Collection::Workspace,
                    w.id.to_string(),
                    w.id.clone(),
                    w.revision,
                    &w,
                )
                .unwrap(),
            },
            Mutation::Put {
                expected: None,
                record: Record::typed(
                    Collection::Session,
                    s.id.to_string(),
                    w.id.clone(),
                    s.revision,
                    &s,
                )
                .unwrap(),
            },
            Mutation::Put {
                expected: None,
                record: Record::typed(
                    Collection::Task,
                    t.scope.task.to_string(),
                    w.id.clone(),
                    t.revision,
                    &t,
                )
                .unwrap(),
            },
        ],
        events: vec![EventInput {
            id: EventId::parse("created").unwrap(),
            workspace: w.id.clone(),
            session: s.id.clone(),
            task: Some(t.scope.task),
            actor: ActorId::parse("human").unwrap(),
            correlation: command.clone(),
            causation: None,
            timestamp: Timestamp::new(100),
            kind: EventKind::TaskCreated,
            artifacts: vec![],
            data: serde_json::json!({"state":"pending"}),
        }],
        command: Some(ReceiptInput {
            command,
            workspace: w.id,
            session: s.id,
            digest: "a".repeat(64),
            result: CommandResult::Accepted {
                revision: Revision::ZERO,
            },
        }),
    }
}
pub fn attach(
    state: &State,
    descriptor: ArtifactDescriptor,
    expected: Option<Revision>,
) -> Transaction {
    let revision = expected.map(|r| r.next().unwrap()).unwrap_or_default();
    Transaction {
        id: TransactionId::new(),
        expected_watermark: state.watermark,
        mutations: vec![Mutation::Put {
            expected,
            record: Record::typed(
                Collection::Artifact,
                descriptor.spec.id.to_string(),
                descriptor.spec.scope.workspace.clone(),
                revision,
                &descriptor,
            )
            .unwrap(),
        }],
        events: vec![],
        command: None,
    }
}
