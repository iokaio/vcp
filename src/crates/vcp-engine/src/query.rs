// SPDX-License-Identifier: Apache-2.0
//! Read-only queries under the same current access boundary as commands.
//!
//! Access grants name one session: session discovery returns only that session.
//! A workspace-wide discovery capability is intentionally not inferred from it.
use crate::{Access, Engine};
use serde::{Deserialize, Serialize};
use vcp_domain::{
    ids::*,
    revision::*,
    task::Task,
    workspace::{Session, Workspace},
};
use vcp_protocol::command::CommandReceipt;
use vcp_store::contract::{command_key, CanonicalStore, Collection};

pub const MAX_PAGE_LIMIT: u32 = 128;
pub const MAX_RESULT_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Query {
    Sessions {
        limit: u32,
        cursor: Option<SessionCursor>,
    },
    Session {
        session: SessionId,
    },
    Task {
        task: TaskId,
    },
    Command {
        command: CommandId,
    },
}

/// A cursor is a revision precondition, never an access grant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionCursor {
    pub actor: ActorId,
    pub workspace: WorkspaceId,
    pub session: SessionId,
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
    pub watermark: Watermark,
    pub after: SessionId,
    pub limit: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum QueryResult {
    Sessions {
        watermark: Watermark,
        sessions: Vec<Session>,
        next: Option<SessionCursor>,
    },
    Session {
        watermark: Watermark,
        session: Session,
    },
    Task {
        watermark: Watermark,
        task: Task,
    },
    Command {
        watermark: Watermark,
        receipt: CommandReceipt,
    },
}

/// Deliberately does not expose storage paths or distinguish absent and denied IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum QueryError {
    #[error("current query access denied")]
    Access,
    #[error("query target unavailable in the authorized session")]
    Unavailable,
    #[error("query cursor changed or expired; restart from a fresh snapshot")]
    StaleCursor,
    #[error("query page or result exceeds the supported limit")]
    Limit,
    #[error("canonical query data could not be decoded")]
    InvalidData,
}

impl<S: CanonicalStore> Engine<S> {
    pub fn query(&self, access: &Access, query: &Query) -> Result<QueryResult, QueryError> {
        self.authorize(access).map_err(|_| QueryError::Access)?;
        let state = self.store().state();
        // Bootstrap permits command initialization only, never reads of an
        // unbound workspace/session even when authorize accepts that bootstrap.
        let workspace: Workspace = state
            .record(
                Collection::Workspace,
                access.workspace.as_str(),
                &access.workspace,
            )
            .map_err(|_| QueryError::Access)?
            .decode()
            .map_err(|_| QueryError::InvalidData)?;
        let session: Session = state
            .record(
                Collection::Session,
                access.session.as_str(),
                &access.workspace,
            )
            .map_err(|_| QueryError::Access)?
            .decode()
            .map_err(|_| QueryError::InvalidData)?;
        if workspace.id != access.workspace
            || session.workspace != access.workspace
            || session.id != access.session
        {
            return Err(QueryError::Access);
        }
        let watermark = state.watermark;
        let result = match query {
            Query::Sessions { limit, cursor } => {
                if *limit == 0 || *limit > MAX_PAGE_LIMIT {
                    return Err(QueryError::Limit);
                }
                if let Some(cursor) = cursor {
                    if cursor.actor != access.actor
                        || cursor.workspace != access.workspace
                        || cursor.session != access.session
                        || cursor.authority != workspace.authority
                        || cursor.deletion != workspace.deletion
                        || cursor.watermark != watermark
                        || cursor.limit != *limit
                        || cursor.after != access.session
                    {
                        return Err(QueryError::StaleCursor);
                    }
                }
                // There is at most one visible session for this grant. The
                // explicit page contract avoids silent truncation when adapted.
                QueryResult::Sessions {
                    watermark,
                    sessions: if cursor.is_none() {
                        vec![session]
                    } else {
                        vec![]
                    },
                    next: None,
                }
            }
            Query::Session { session: requested } => {
                if requested != &access.session {
                    return Err(QueryError::Unavailable);
                }
                QueryResult::Session { watermark, session }
            }
            Query::Task { task: requested } => {
                let task: Task = state
                    .record(Collection::Task, requested.as_str(), &access.workspace)
                    .map_err(|_| QueryError::Unavailable)?
                    .decode()
                    .map_err(|_| QueryError::InvalidData)?;
                if task.scope.workspace != access.workspace
                    || task.scope.session != access.session
                    || task.scope.task != *requested
                {
                    return Err(QueryError::Unavailable);
                }
                QueryResult::Task { watermark, task }
            }
            Query::Command { command } => {
                let receipt = state
                    .commands
                    .get(&command_key(&access.workspace, command))
                    .ok_or(QueryError::Unavailable)?;
                // Receipts predate public queries and carry no session. Prove
                // their scope using the retained commit's correlated events;
                // pruned evidence makes this lookup unavailable, never global.
                let mut found = false;
                for event in state.events.iter().filter(|event| {
                    event.watermark == receipt.watermark && event.event.correlation == *command
                }) {
                    if event.event.workspace != access.workspace
                        || event.event.session != access.session
                    {
                        return Err(QueryError::Unavailable);
                    }
                    found = true;
                }
                if !found || receipt.workspace != access.workspace || receipt.command != *command {
                    return Err(QueryError::Unavailable);
                }
                if let vcp_protocol::command::CommandResult::Inspection { task: Some(task) } =
                    &receipt.result
                {
                    if task.scope.workspace != access.workspace
                        || task.scope.session != access.session
                    {
                        return Err(QueryError::Unavailable);
                    }
                }
                QueryResult::Command {
                    watermark,
                    receipt: receipt.clone(),
                }
            }
        };
        check_size(&result)?;
        Ok(result)
    }
}

fn check_size(result: &QueryResult) -> Result<(), QueryError> {
    struct BoundedCounter(usize);
    impl std::io::Write for BoundedCounter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > MAX_RESULT_BYTES.saturating_sub(self.0) {
                return Err(std::io::Error::other("query result exceeds limit"));
            }
            self.0 += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(BoundedCounter(0), result).map_err(|_| QueryError::Limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HostFacts;
    use vcp_domain::{task::Objective, verification::Fingerprint, workspace::Binding};
    use vcp_protocol::command::{Command, CommandEnvelope};
    use vcp_store::{BackendKind, Store};

    fn access() -> Access {
        Access {
            actor: ActorId::new(),
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            bootstrap: true,
        }
    }

    async fn execute(
        engine: &mut Engine<Store>,
        access: &Access,
        payload: Command,
        task: Option<TaskId>,
    ) -> CommandReceipt {
        let current: Option<Task> = task
            .as_ref()
            .and_then(|id| {
                engine
                    .store()
                    .state()
                    .record(Collection::Task, id.as_str(), &access.workspace)
                    .ok()
            })
            .map(|record| record.decode().unwrap());
        let command = CommandEnvelope {
            version: 1,
            id: CommandId::new(),
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            task,
            caller: access.actor.clone(),
            controller: engine.controller().clone(),
            owner_epoch: engine.owner_epoch(),
            expected: current
                .as_ref()
                .map_or(Revision::ZERO, |task| task.revision),
            steering: current
                .as_ref()
                .map_or(SteeringRevision::ZERO, |task| task.steering),
            payload,
        };
        engine
            .handle(command, access, &HostFacts::inspect(Timestamp::new(1)))
            .await
            .unwrap()
    }

    async fn initialize(engine: &mut Engine<Store>, access: &Access) -> CommandReceipt {
        execute(
            engine,
            access,
            Command::Initialize {
                binding: Binding {
                    host: HostId::new(),
                    root: "C:/query-fixture".into(),
                    repository: "query-fixture".into(),
                    worktree: "main".into(),
                    revision: Revision::ZERO,
                },
            },
            None,
        )
        .await
    }

    async fn create_task(
        engine: &mut Engine<Store>,
        access: &Access,
        text: String,
    ) -> (TaskId, CommandReceipt) {
        let task = TaskId::new();
        let receipt = execute(
            engine,
            access,
            Command::CreateTask {
                root: task.clone(),
                parent: None,
                fork_origin: None,
                objective: Objective {
                    text,
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
            Some(task.clone()),
        )
        .await;
        (task, receipt)
    }

    fn cursor(engine: &Engine<Store>, access: &Access) -> SessionCursor {
        SessionCursor {
            actor: access.actor.clone(),
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            authority: access.authority,
            deletion: DeletionEpoch::ZERO,
            watermark: engine.store().state().watermark,
            after: access.session.clone(),
            limit: 1,
        }
    }

    #[tokio::test]
    async fn queries_are_pure_scoped_and_receipts_survive_reopen_on_both_stores() {
        for backend in [BackendKind::Sqlite, BackendKind::Files] {
            let temporary = tempfile::tempdir().unwrap();
            let root = temporary.path().join("store");
            let mut engine = Engine::new(Store::open(&root, backend, &[]).await.unwrap()).unwrap();
            let mut owner = access();
            assert_eq!(
                engine.query(
                    &owner,
                    &Query::Sessions {
                        limit: 1,
                        cursor: None
                    }
                ),
                Err(QueryError::Access)
            );
            let initialized = initialize(&mut engine, &owner).await;
            let (task, receipt) = create_task(&mut engine, &owner, "read this task".into()).await;
            let other_session = SessionId::new();
            execute(
                &mut engine,
                &owner,
                Command::CreateSession {
                    id: other_session.clone(),
                    fork_through: None,
                },
                None,
            )
            .await;
            let mut other = access();
            other.workspace = owner.workspace.clone();
            other.session = other_session.clone();
            let (foreign_task, foreign_receipt) =
                create_task(&mut engine, &other, "other session".into()).await;
            let foreign_workspace = access();
            let foreign_init = initialize(&mut engine, &foreign_workspace).await;
            owner.write = false;
            owner.bootstrap = false;
            let before = serde_json::to_vec(engine.store().state()).unwrap();
            let watermark = engine.store().state().watermark;
            let QueryResult::Sessions { sessions, next, .. } = engine
                .query(
                    &owner,
                    &Query::Sessions {
                        limit: 128,
                        cursor: None,
                    },
                )
                .unwrap()
            else {
                panic!("wrong result")
            };
            assert_eq!(sessions.len(), 1);
            assert_eq!(sessions[0].id, owner.session);
            assert!(next.is_none());
            assert!(matches!(
                engine.query(
                    &owner,
                    &Query::Session {
                        session: owner.session.clone()
                    }
                ),
                Ok(QueryResult::Session { .. })
            ));
            assert!(matches!(
                engine.query(&owner, &Query::Task { task: task.clone() }),
                Ok(QueryResult::Task { .. })
            ));
            assert_eq!(
                engine
                    .query(
                        &owner,
                        &Query::Command {
                            command: receipt.command.clone()
                        }
                    )
                    .unwrap(),
                QueryResult::Command {
                    watermark,
                    receipt: receipt.clone()
                }
            );
            assert!(engine
                .query(
                    &owner,
                    &Query::Command {
                        command: initialized.command
                    }
                )
                .is_ok());
            for denied in [
                Query::Session {
                    session: other_session,
                },
                Query::Task { task: foreign_task },
                Query::Command {
                    command: foreign_receipt.command,
                },
                Query::Command {
                    command: foreign_init.command,
                },
                Query::Command {
                    command: CommandId::new(),
                },
            ] {
                assert_eq!(engine.query(&owner, &denied), Err(QueryError::Unavailable));
            }
            assert_eq!(before, serde_json::to_vec(engine.store().state()).unwrap());
            engine.into_store().close().await.unwrap();
            let reopened = Engine::new(Store::open(&root, backend, &[]).await.unwrap()).unwrap();
            assert_eq!(
                reopened
                    .query(
                        &owner,
                        &Query::Command {
                            command: receipt.command.clone()
                        }
                    )
                    .unwrap(),
                QueryResult::Command {
                    watermark,
                    receipt: receipt.clone()
                }
            );
            owner.read = false;
            assert_eq!(
                reopened.query(
                    &owner,
                    &Query::Command {
                        command: receipt.command.clone()
                    }
                ),
                Err(QueryError::Access)
            );
            owner.read = true;
            owner.authority = AuthorityRevision::new(1);
            assert_eq!(
                reopened.query(
                    &owner,
                    &Query::Command {
                        command: receipt.command
                    }
                ),
                Err(QueryError::Access)
            );
            reopened.into_store().close().await.unwrap();
        }
    }

    #[tokio::test]
    async fn cursor_scope_revisions_and_limits_fail_explicitly_on_both_stores() {
        for backend in [BackendKind::Sqlite, BackendKind::Files] {
            let temporary = tempfile::tempdir().unwrap();
            let mut engine =
                Engine::new(Store::open(temporary.path(), backend, &[]).await.unwrap()).unwrap();
            let owner = access();
            initialize(&mut engine, &owner).await;
            let original = cursor(&engine, &owner);
            assert!(
                matches!(engine.query(&owner, &Query::Sessions { limit: 1, cursor: Some(original.clone()) }), Ok(QueryResult::Sessions { sessions, .. }) if sessions.is_empty())
            );
            let mut variants = Vec::new();
            let mut changed = original.clone();
            changed.actor = ActorId::new();
            variants.push(changed);
            let mut changed = original.clone();
            changed.workspace = WorkspaceId::new();
            variants.push(changed);
            let mut changed = original.clone();
            changed.session = SessionId::new();
            variants.push(changed);
            let mut changed = original.clone();
            changed.authority = AuthorityRevision::new(1);
            variants.push(changed);
            let mut changed = original.clone();
            changed.deletion = DeletionEpoch::new(1);
            variants.push(changed);
            let mut changed = original.clone();
            changed.watermark = Watermark::ZERO;
            variants.push(changed);
            let mut changed = original.clone();
            changed.after = SessionId::new();
            variants.push(changed);
            let mut changed = original.clone();
            changed.limit = 2;
            variants.push(changed);
            for cursor in variants {
                assert_eq!(
                    engine.query(
                        &owner,
                        &Query::Sessions {
                            limit: 1,
                            cursor: Some(cursor)
                        }
                    ),
                    Err(QueryError::StaleCursor)
                );
            }
            for limit in [0, MAX_PAGE_LIMIT + 1, u32::MAX] {
                assert_eq!(
                    engine.query(
                        &owner,
                        &Query::Sessions {
                            limit,
                            cursor: None
                        }
                    ),
                    Err(QueryError::Limit)
                );
            }
            create_task(&mut engine, &owner, "advance snapshot".into()).await;
            assert_eq!(
                engine.query(
                    &owner,
                    &Query::Sessions {
                        limit: 1,
                        cursor: Some(original)
                    }
                ),
                Err(QueryError::StaleCursor)
            );
            engine.into_store().close().await.unwrap();
        }
    }

    #[tokio::test]
    async fn oversized_results_are_rejected_without_truncation_or_mutation() {
        for backend in [BackendKind::Sqlite, BackendKind::Files] {
            let temporary = tempfile::tempdir().unwrap();
            let mut engine =
                Engine::new(Store::open(temporary.path(), backend, &[]).await.unwrap()).unwrap();
            let owner = access();
            initialize(&mut engine, &owner).await;
            let (task, _) = create_task(&mut engine, &owner, "x".repeat(60_000)).await;
            for revision in 1..=4 {
                execute(
                    &mut engine,
                    &owner,
                    Command::Steer {
                        objective: Objective {
                            text: "x".repeat(60_000),
                            constraints: vec![],
                            acceptance: vec![],
                            source: EventId::new(),
                            steering: SteeringRevision::new(revision),
                        },
                    },
                    Some(task.clone()),
                )
                .await;
            }
            let watermark = engine.store().state().watermark;
            assert_eq!(
                engine.query(&owner, &Query::Task { task }),
                Err(QueryError::Limit)
            );
            assert_eq!(engine.store().state().watermark, watermark);
            engine.into_store().close().await.unwrap();
        }
    }

    #[test]
    fn queries_reject_unknown_semantics_and_round_trip_decimal_cursors() {
        assert!(serde_json::from_value::<Query>(
            serde_json::json!({"kind":"sessions","limit":1,"cursor":null,"all_workspaces":true})
        )
        .is_err());
        assert!(
            serde_json::from_value::<Query>(serde_json::json!({"kind":"future_query"})).is_err()
        );
        let owner = access();
        let request = Query::Sessions {
            limit: 1,
            cursor: Some(SessionCursor {
                actor: owner.actor,
                workspace: owner.workspace,
                session: owner.session.clone(),
                authority: AuthorityRevision::ZERO,
                deletion: DeletionEpoch::ZERO,
                watermark: Watermark::new(u64::MAX),
                after: owner.session,
                limit: 1,
            }),
        };
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value["cursor"]["watermark"], u64::MAX.to_string());
        assert_eq!(serde_json::from_value::<Query>(value).unwrap(), request);
    }
}
