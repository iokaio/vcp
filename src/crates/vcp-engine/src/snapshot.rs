// SPDX-License-Identifier: Apache-2.0
//! Bounded session projections and their canonical replay boundary.
//!
//! Pages do not retain a second copy of the canonical store. Any source change
//! invalidates unfinished pagination explicitly. The returned replay cursor is
//! a fixed window starting at S; later windows must be requested after S (or the
//! last delivered sequence). This module does not implement live delivery.
use crate::{
    query::{Query, QueryResult},
    Access, Engine,
};
use serde::{Deserialize, Serialize};
use vcp_domain::{ids::*, revision::*, task::Task, workspace::Workspace};
use vcp_protocol::{
    methods,
    subscription::{Cursor, EventPage, GapReason},
};
use vcp_store::contract::{CanonicalStore, Collection};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotCursor {
    pub version: u32,
    pub actor: ActorId,
    pub replay: Cursor,
    pub after: TaskId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestartReason {
    SourceChanged,
    CursorExpired,
    CursorChanged,
    RetentionChanged,
}

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("current snapshot access denied")]
    Access,
    #[error("snapshot page or projection exceeds the supported limit")]
    Limit,
    #[error("canonical snapshot data could not be projected")]
    InvalidData,
    #[error("snapshot source changed; discard partial pages and restart")]
    Restart {
        reason: RestartReason,
        sequence: SessionSeq,
        watermark: Watermark,
    },
}

impl<S: CanonicalStore> Engine<S> {
    /// Capture a typed session/task page and register its after-S replay cursor
    /// under the same serialized engine observation. No mutation or effect is
    /// performed. A caller must discard all partial pages on `Restart`.
    pub fn snapshot_page(
        &mut self,
        access: &Access,
        limit: u32,
        cursor: Option<&SnapshotCursor>,
        now: Timestamp,
    ) -> Result<methods::SessionSnapshot, SnapshotError> {
        use SnapshotError::{Access as Denied, InvalidData, Limit};
        // query also rejects command-only bootstrap and nonexistent sessions.
        let QueryResult::Session { session, watermark } = self
            .query(
                access,
                &Query::Session {
                    session: access.session.clone(),
                },
            )
            .map_err(|error| match error {
                crate::query::QueryError::Access | crate::query::QueryError::Unavailable => Denied,
                _ => InvalidData,
            })?
        else {
            return Err(InvalidData);
        };
        if limit == 0 || limit > crate::query::MAX_PAGE_LIMIT {
            return Err(Limit);
        }
        let state = self.store().state();
        let sequence = state
            .sequences
            .get(&access.session)
            .copied()
            .unwrap_or_default();
        let restart = |reason| SnapshotError::Restart {
            reason,
            sequence,
            watermark,
        };
        let workspace: Workspace = state
            .record(
                Collection::Workspace,
                access.workspace.as_str(),
                &access.workspace,
            )
            .and_then(|record| record.decode())
            .map_err(|_| InvalidData)?;
        if let Some(cursor) = cursor {
            if cursor.actor != access.actor
                || cursor.replay.workspace != access.workspace
                || cursor.replay.session != access.session
            {
                return Err(Denied);
            }
            if cursor.version != 1
                || cursor.replay.limit != limit
                || cursor.replay.after != cursor.replay.end
            {
                return Err(restart(RestartReason::CursorChanged));
            }
            if cursor.replay.deletion != workspace.deletion {
                return Err(restart(RestartReason::RetentionChanged));
            }
            match self
                .events(access, &cursor.replay, now)
                .map_err(|_| Denied)?
            {
                EventPage::Gap { reason, .. } => {
                    return Err(restart(match reason {
                        GapReason::SnapshotExpired => RestartReason::CursorExpired,
                        GapReason::RetentionChanged => RestartReason::RetentionChanged,
                        _ => RestartReason::CursorChanged,
                    }))
                }
                EventPage::Events { .. } => {}
            }
            if cursor.replay.watermark != watermark || cursor.replay.end != sequence {
                return Err(restart(RestartReason::SourceChanged));
            }
            let previous: Task = state
                .record(Collection::Task, cursor.after.as_str(), &access.workspace)
                .and_then(|record| record.decode())
                .map_err(|_| restart(RestartReason::CursorChanged))?;
            if previous.scope.workspace != access.workspace
                || previous.scope.session != access.session
            {
                return Err(restart(RestartReason::CursorChanged));
            }
        }
        let session = crate::rpc::session_view(session).map_err(|_| InvalidData)?;
        let mut tasks = Vec::new();
        let mut last = None;
        let mut complete = true;
        // Leave bounded room for both opaque cursors and the page envelope.
        let mut bytes = serde_json::to_vec(&session).map_err(|_| InvalidData)?.len() + 8192;
        for row in state
            .records
            .values()
            .filter(|row| row.collection == Collection::Task && row.workspace == access.workspace)
        {
            let task: Task = row.decode().map_err(|_| InvalidData)?;
            if task.scope.workspace != access.workspace
                || task.scope.task.as_str() != row.id
                || task.revision != row.revision
            {
                return Err(InvalidData);
            }
            if task.scope.session != access.session
                || cursor.is_some_and(|cursor| task.scope.task <= cursor.after)
            {
                continue;
            }
            if tasks.len() >= limit as usize {
                complete = false;
                break;
            }
            let id = task.scope.task.clone();
            let view = crate::rpc::task_view(state, task).map_err(|_| InvalidData)?;
            let size = serde_json::to_vec(&view).map_err(|_| InvalidData)?.len() + 1;
            if bytes.saturating_add(size) > crate::query::MAX_RESULT_BYTES {
                if tasks.is_empty() {
                    return Err(Limit);
                }
                complete = false;
                break;
            }
            bytes += size;
            tasks.push(view);
            last = Some(id);
        }
        // Registration is last, so invalid projections do not consume a lease.
        let replay = match cursor {
            Some(cursor) => cursor.replay.clone(),
            None => self
                .subscribe(access, sequence, limit, now)
                .map_err(|error| match error {
                    crate::Error::Access => Denied,
                    crate::Error::Protocol(_) => Limit,
                    _ => InvalidData,
                })?,
        };
        let next_cursor = if complete {
            None
        } else {
            Some(
                serde_json::to_string(&SnapshotCursor {
                    version: 1,
                    actor: access.actor.clone(),
                    replay: replay.clone(),
                    after: last.ok_or(InvalidData)?,
                })
                .map_err(|_| InvalidData)?,
            )
        };
        let result = methods::SessionSnapshot {
            session,
            sequence: sequence.get().into(),
            watermark: watermark.get().into(),
            subscription: methods::Id::try_from(replay.snapshot.as_str().to_owned())
                .map_err(|_| InvalidData)?,
            event_cursor: serde_json::to_string(&replay).map_err(|_| InvalidData)?,
            tasks,
            next_cursor,
            complete,
        };
        if serde_json::to_vec(&result).map_err(|_| InvalidData)?.len()
            > crate::query::MAX_RESULT_BYTES
        {
            if cursor.is_none() {
                self.unsubscribe(&replay.snapshot);
            }
            return Err(Limit);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HostFacts;
    use vcp_domain::{task::Objective, verification::Fingerprint, workspace::Binding};
    use vcp_protocol::command::{Command, CommandEnvelope, CommandReceipt};
    use vcp_store::{BackendKind, Store};

    async fn execute(
        engine: &mut Engine<Store>,
        access: &Access,
        payload: Command,
        task: Option<TaskId>,
    ) -> CommandReceipt {
        let envelope = CommandEnvelope {
            version: 1,
            id: CommandId::new(),
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            task,
            caller: access.actor.clone(),
            controller: engine.controller().clone(),
            owner_epoch: engine.owner_epoch(),
            expected: Revision::ZERO,
            steering: SteeringRevision::ZERO,
            payload,
        };
        engine
            .handle(envelope, access, &HostFacts::inspect(Timestamp::new(1)))
            .await
            .unwrap()
    }

    async fn fixture(root: &std::path::Path, backend: BackendKind) -> (Engine<Store>, Access) {
        let mut engine = Engine::new(Store::open(root, backend, &[]).await.unwrap()).unwrap();
        let mut access = Access {
            actor: ActorId::new(),
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            bootstrap: true,
        };
        execute(
            &mut engine,
            &access,
            Command::Initialize {
                binding: Binding {
                    host: HostId::new(),
                    root: "C:/snapshot-fixture".into(),
                    repository: "fixture".into(),
                    worktree: "main".into(),
                    revision: Revision::ZERO,
                },
            },
            None,
        )
        .await;
        access.bootstrap = false;
        for name in ["task-a", "task-b", "task-c"] {
            let task = TaskId::parse(name).unwrap();
            execute(
                &mut engine,
                &access,
                Command::CreateTask {
                    root: task.clone(),
                    parent: None,
                    fork_origin: None,
                    objective: Objective {
                        text: "private internal objective".into(),
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
                Some(task),
            )
            .await;
        }
        (engine, access)
    }

    #[tokio::test]
    async fn pages_and_after_snapshot_replay_share_one_canonical_boundary_on_both_stores() {
        for backend in [BackendKind::Files, BackendKind::Sqlite] {
            let dir = tempfile::tempdir().unwrap();
            let (mut engine, access) = fixture(dir.path(), backend).await;
            let before = engine.store().state().clone();
            let first = engine
                .snapshot_page(&access, 2, None, Timestamp::new(100))
                .unwrap();
            assert!(!first.complete);
            assert_eq!(
                first
                    .tasks
                    .iter()
                    .map(|task| task.task.as_str())
                    .collect::<Vec<_>>(),
                ["task-a", "task-b"]
            );
            let cursor: SnapshotCursor =
                serde_json::from_str(first.next_cursor.as_ref().unwrap()).unwrap();
            let second = engine
                .snapshot_page(&access, 2, Some(&cursor), Timestamp::new(101))
                .unwrap();
            assert!(second.complete);
            assert!(second.next_cursor.is_none());
            assert_eq!(second.tasks[0].task.as_str(), "task-c");
            assert_eq!(second.sequence, first.sequence);
            assert_eq!(second.watermark, first.watermark);
            assert_eq!(second.event_cursor, first.event_cursor);
            assert_eq!(
                engine
                    .snapshot_page(&access, 2, Some(&cursor), Timestamp::new(102))
                    .unwrap(),
                second
            );
            assert_eq!(*engine.store().state(), before);
            let encoded = serde_json::to_string(&first).unwrap();
            assert!(!encoded.contains("private internal objective"));
            assert!(!encoded.contains("fingerprint"));
            let replay: Cursor = serde_json::from_str(&first.event_cursor).unwrap();
            assert_eq!(replay.after, before.sequences[&access.session]);
            assert_eq!(replay.end, replay.after);
            let receipt = execute(&mut engine, &access, Command::Inspect, None).await;
            assert!(
                matches!(engine.events(&access, &replay, Timestamp::new(103)).unwrap(),
                EventPage::Events { events, at_end: true, .. } if events.is_empty())
            );
            // A fresh fixed window includes the commit between capture and poll.
            let next = engine
                .subscribe(&access, replay.after, 128, Timestamp::new(104))
                .unwrap();
            let EventPage::Events { events, at_end, .. } =
                engine.events(&access, &next, Timestamp::new(105)).unwrap()
            else {
                panic!("gap");
            };
            assert!(at_end);
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].sequence, replay.after.next().unwrap());
            assert_eq!(events[0].event.correlation, receipt.command);
        }
    }

    #[tokio::test]
    async fn changed_source_expiry_cursor_and_access_never_complete_a_mixed_snapshot() {
        for backend in [BackendKind::Files, BackendKind::Sqlite] {
            let dir = tempfile::tempdir().unwrap();
            let (mut engine, access) = fixture(dir.path(), backend).await;
            let first = engine
                .snapshot_page(&access, 1, None, Timestamp::new(100))
                .unwrap();
            let cursor: SnapshotCursor =
                serde_json::from_str(first.next_cursor.as_ref().unwrap()).unwrap();
            let mut other = access.clone();
            other.actor = ActorId::new();
            assert!(matches!(
                engine.snapshot_page(&other, 1, Some(&cursor), Timestamp::new(101)),
                Err(SnapshotError::Access)
            ));
            other = access.clone();
            other.read = false;
            assert!(matches!(
                engine.snapshot_page(&other, 1, None, Timestamp::new(101)),
                Err(SnapshotError::Access)
            ));
            other = access.clone();
            other.authority = AuthorityRevision::new(1);
            assert!(matches!(
                engine.snapshot_page(&other, 1, Some(&cursor), Timestamp::new(101)),
                Err(SnapshotError::Access)
            ));
            assert!(matches!(
                engine.snapshot_page(&access, 2, Some(&cursor), Timestamp::new(101)),
                Err(SnapshotError::Restart {
                    reason: RestartReason::CursorChanged,
                    ..
                })
            ));
            assert!(matches!(
                engine.snapshot_page(&access, 1, Some(&cursor), Timestamp::new(60_100)),
                Err(SnapshotError::Restart {
                    reason: RestartReason::CursorExpired,
                    ..
                })
            ));
            execute(&mut engine, &access, Command::Inspect, None).await;
            let end = engine.store().state().sequences[&access.session];
            let mark = engine.store().state().watermark;
            assert!(
                matches!(engine.snapshot_page(&access, 1, Some(&cursor), Timestamp::new(102)),
                Err(SnapshotError::Restart { reason: RestartReason::SourceChanged, sequence, watermark }) if sequence == end && watermark == mark)
            );
            assert_eq!(
                engine
                    .snapshot_page(&access, 128, None, Timestamp::new(103))
                    .unwrap()
                    .sequence
                    .as_str(),
                end.get().to_string()
            );
        }
    }

    #[tokio::test]
    async fn snapshot_allocation_obeys_existing_subscription_and_page_limits() {
        for backend in [BackendKind::Files, BackendKind::Sqlite] {
            let dir = tempfile::tempdir().unwrap();
            let (mut engine, access) = fixture(dir.path(), backend).await;
            for limit in [0, 129] {
                assert!(matches!(
                    engine.snapshot_page(&access, limit, None, Timestamp::new(100)),
                    Err(SnapshotError::Limit)
                ));
            }
            let mut first = None;
            for _ in 0..16 {
                let page = engine
                    .snapshot_page(&access, 128, None, Timestamp::new(100))
                    .unwrap();
                first.get_or_insert(serde_json::from_str::<Cursor>(&page.event_cursor).unwrap());
            }
            assert!(matches!(
                engine.snapshot_page(&access, 128, None, Timestamp::new(100)),
                Err(SnapshotError::Limit)
            ));
            engine.unsubscribe(&first.unwrap().snapshot);
            assert!(engine
                .snapshot_page(&access, 128, None, Timestamp::new(100))
                .is_ok());
            // Expired registrations are reclaimed, not an unbounded producer queue.
            assert!(engine
                .snapshot_page(&access, 128, None, Timestamp::new(60_100))
                .is_ok());
        }
    }

    #[tokio::test]
    async fn byte_bounded_pages_do_not_silently_drop_tasks() {
        use vcp_store::contract::{Mutation, Record, Transaction};
        for backend in [BackendKind::Files, BackendKind::Sqlite] {
            let dir = tempfile::tempdir().unwrap();
            let (mut engine, access) = fixture(dir.path(), backend).await;
            let template: Task = engine
                .store()
                .state()
                .record(Collection::Task, "task-a", &access.workspace)
                .unwrap()
                .decode()
                .unwrap();
            let mut mutations = Vec::new();
            // A single validated canonical transaction keeps this projection
            // limit fixture small; it dispatches no commands or external work.
            for index in 0..64 {
                let mut task = template.clone();
                task.scope.task = TaskId::parse(format!("large-{index:03}")).unwrap();
                task.root = task.scope.task.clone();
                task.reason = "x".repeat(4096);
                mutations.push(Mutation::Put {
                    expected: None,
                    record: Record::typed(
                        Collection::Task,
                        task.scope.task.as_str(),
                        access.workspace.clone(),
                        task.revision,
                        &task,
                    )
                    .unwrap(),
                });
            }
            let transaction = Transaction {
                id: TransactionId::new(),
                expected_watermark: engine.store().state().watermark,
                mutations,
                events: vec![],
                command: None,
            };
            engine.store_mut().transact(transaction).await.unwrap();
            let before = engine.store().state().clone();
            let mut cursor = None;
            let mut seen = std::collections::BTreeSet::new();
            let mut pages = 0;
            loop {
                let page = engine
                    .snapshot_page(&access, 128, cursor.as_ref(), Timestamp::new(100))
                    .unwrap();
                pages += 1;
                assert!(serde_json::to_vec(&page).unwrap().len() <= crate::query::MAX_RESULT_BYTES);
                for task in page.tasks {
                    assert!(seen.insert(task.task.as_str().to_owned()));
                }
                if page.complete {
                    assert!(page.next_cursor.is_none());
                    break;
                }
                cursor = Some(
                    serde_json::from_str::<SnapshotCursor>(page.next_cursor.as_ref().unwrap())
                        .unwrap(),
                );
                assert!(pages < 4);
            }
            assert!(pages > 1);
            assert_eq!(seen.len(), 67);
            assert_eq!(*engine.store().state(), before);
        }
    }
}
