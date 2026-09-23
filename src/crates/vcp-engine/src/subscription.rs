// SPDX-License-Identifier: Apache-2.0
use crate::{Access, Engine, Error, Result};
use vcp_domain::{ids::*, revision::*, workspace::Workspace};
use vcp_protocol::subscription::*;
use vcp_store::contract::{CanonicalStore, Collection};
pub(crate) const MAX_SUBSCRIPTIONS: usize = 16;
pub enum ProjectedEvents {
    Page {
        events: Vec<vcp_protocol::methods::Event>,
        next: Cursor,
        at_end: bool,
    },
    Gap(GapReason),
    TooLarge,
}

#[cfg(test)]
mod projection_tests {
    use super::*;
    use crate::HostFacts;
    use vcp_domain::workspace::Binding;
    use vcp_protocol::{
        command::{Command, CommandEnvelope},
        event::{EventInput, EventKind},
    };
    use vcp_store::{
        contract::{Mutation, Record, Transaction},
        BackendKind, Store,
    };

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
        let command = CommandEnvelope {
            version: 1,
            id: CommandId::new(),
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            task: None,
            caller: access.actor.clone(),
            controller: engine.controller().clone(),
            owner_epoch: engine.owner_epoch(),
            expected: Revision::ZERO,
            steering: SteeringRevision::ZERO,
            payload: Command::Initialize {
                binding: Binding {
                    host: HostId::new(),
                    root: "C:/event-fixture".into(),
                    repository: "fixture".into(),
                    worktree: "main".into(),
                    revision: Revision::ZERO,
                },
            },
        };
        engine
            .handle(command, &access, &HostFacts::inspect(Timestamp::new(1)))
            .await
            .unwrap();
        access.bootstrap = false;
        (engine, access)
    }
    async fn append(engine: &mut Engine<Store>, access: &Access, count: usize) -> Vec<EventId> {
        let events: Vec<_> = (0..count)
            .map(|_| EventInput {
                id: EventId::new(),
                workspace: access.workspace.clone(),
                session: access.session.clone(),
                task: None,
                actor: access.actor.clone(),
                correlation: CommandId::new(),
                causation: None,
                timestamp: Timestamp::new(1),
                kind: EventKind::Diagnostic,
                artifacts: vec![],
                data: serde_json::json!({"secret_internal_fact": "x".repeat(8192)}),
                metadata: None,
            })
            .collect();
        let ids = events.iter().map(|event| event.id.clone()).collect();
        let transaction = Transaction {
            id: TransactionId::new(),
            expected_watermark: engine.store().state().watermark,
            mutations: vec![],
            events,
            command: None,
        };
        engine.store_mut().transact(transaction).await.unwrap();
        ids
    }

    #[tokio::test]
    async fn public_projection_bounds_bytes_without_copying_fact_payloads_and_rolls_without_gaps() {
        for backend in [BackendKind::Files, BackendKind::Sqlite] {
            let temp = tempfile::tempdir().unwrap();
            let (mut engine, access) = fixture(temp.path(), backend).await;
            let start = engine.store().state().sequences[&access.session];
            let ids = append(&mut engine, &access, 64).await;
            let original = engine
                .subscribe(&access, start, 128, Timestamp::new(100))
                .unwrap();
            let before = engine.store().state().clone();
            let mut cursor = original.clone();
            let mut delivered = Vec::new();
            let mut pages = 0;
            loop {
                let ProjectedEvents::Page {
                    events,
                    next,
                    at_end,
                } = engine
                    .projected_events(&access, &cursor, Timestamp::new(101), 16 * 1024)
                    .unwrap()
                else {
                    panic!("page");
                };
                let ProjectedEvents::Page {
                    events: duplicate, ..
                } = engine
                    .projected_events(&access, &cursor, Timestamp::new(101), 16 * 1024)
                    .unwrap()
                else {
                    panic!("retry");
                };
                assert_eq!(events, duplicate);
                assert!(serde_json::to_vec(&events).unwrap().len() < 16 * 1024);
                assert!(!serde_json::to_string(&events)
                    .unwrap()
                    .contains("secret_internal_fact"));
                delivered.extend(events.into_iter().map(|event| event.id.as_str().to_owned()));
                pages += 1;
                cursor = next;
                if at_end {
                    break;
                }
                assert!(engine
                    .advance_event_window(&access, &cursor, Timestamp::new(102))
                    .is_err());
                assert!(pages < 16);
            }
            assert!(pages > 1);
            assert_eq!(
                delivered,
                ids.iter().map(ToString::to_string).collect::<Vec<_>>()
            );
            assert_eq!(*engine.store().state(), before);
            assert!(matches!(
                engine
                    .projected_events(&access, &original, Timestamp::new(101), 8192)
                    .unwrap(),
                ProjectedEvents::TooLarge
            ));
            let last = append(&mut engine, &access, 1).await;
            let next = engine
                .advance_event_window(&access, &cursor, Timestamp::new(103))
                .unwrap();
            assert_eq!(next.expires_at, original.expires_at);
            assert_eq!(next.snapshot, original.snapshot);
            let ProjectedEvents::Page { events, .. } = engine
                .projected_events(&access, &next, Timestamp::new(104), 256 * 1024)
                .unwrap()
            else {
                panic!("poll");
            };
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].id.as_str(), last[0].as_str());
            assert!(matches!(
                engine
                    .projected_events(&access, &next, Timestamp::new(60_100), 256 * 1024)
                    .unwrap(),
                ProjectedEvents::Gap(GapReason::SnapshotExpired)
            ));
        }
    }

    #[tokio::test]
    async fn real_retention_epoch_and_redaction_invalidate_old_cursor_and_mark_public_events() {
        for backend in [BackendKind::Files, BackendKind::Sqlite] {
            let temp = tempfile::tempdir().unwrap();
            let (mut engine, access) = fixture(temp.path(), backend).await;
            let start = engine.store().state().sequences[&access.session];
            let ids = append(&mut engine, &access, 2).await;
            let cursor = engine
                .subscribe(&access, start, 128, Timestamp::new(100))
                .unwrap();
            let mut workspace: Workspace = engine
                .store()
                .state()
                .record(
                    Collection::Workspace,
                    access.workspace.as_str(),
                    &access.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            let expected = workspace.revision;
            workspace.revision = expected.next().unwrap();
            workspace.deletion = DeletionEpoch::new(1);
            let mask = vcp_domain::retention::RetentionMask {
                schema_version: 1,
                workspace: access.workspace.clone(),
                session: access.session.clone(),
                first: start.next().unwrap(),
                last: start.next().unwrap(),
                artifacts: vec![],
                deletion: workspace.deletion,
                reason: "logical before physical".into(),
            };
            let transaction = Transaction {
                id: TransactionId::new(),
                expected_watermark: engine.store().state().watermark,
                mutations: vec![
                    Mutation::Put {
                        expected: Some(expected),
                        record: Record::typed(
                            Collection::Workspace,
                            workspace.id.as_str(),
                            workspace.id.clone(),
                            workspace.revision,
                            &workspace,
                        )
                        .unwrap(),
                    },
                    Mutation::Put {
                        expected: None,
                        record: Record::typed(
                            Collection::Tombstone,
                            "logical-event-mask",
                            access.workspace.clone(),
                            Revision::ZERO,
                            &mask,
                        )
                        .unwrap(),
                    },
                ],
                events: vec![],
                command: None,
            };
            engine.store_mut().transact(transaction).await.unwrap();
            assert!(matches!(
                engine
                    .projected_events(&access, &cursor, Timestamp::new(101), 256 * 1024)
                    .unwrap(),
                ProjectedEvents::Gap(GapReason::RetentionChanged)
            ));
            let logical = engine
                .subscribe(&access, start, 128, Timestamp::new(101))
                .unwrap();
            assert!(engine
                .store()
                .state()
                .events
                .iter()
                .any(|event| event.event.id == ids[0]
                    && event.redaction.is_none()
                    && !event.event.data.is_null()));
            assert!(matches!(
                engine
                    .projected_events(&access, &logical, Timestamp::new(101), 256 * 1024)
                    .unwrap(),
                ProjectedEvents::Gap(GapReason::RetentionChanged)
            ));
            let candidate = engine
                .store()
                .retention_candidate(
                    &Default::default(),
                    &std::collections::BTreeSet::from([ids[1].clone()]),
                    &Default::default(),
                )
                .unwrap();
            engine
                .store_mut()
                .rewrite_base(candidate, &[])
                .await
                .unwrap();
            let fresh = engine
                .subscribe(&access, start.next().unwrap(), 128, Timestamp::new(102))
                .unwrap();
            let ProjectedEvents::Page { events, .. } = engine
                .projected_events(&access, &fresh, Timestamp::new(103), 256 * 1024)
                .unwrap()
            else {
                panic!("redacted page");
            };
            assert_eq!(events.len(), 1);
            assert!(events[0].redacted);
            assert!(!events[0].evidence_complete);
            assert!(events[0].evidence.is_empty());
            let mut denied = access.clone();
            denied.read = false;
            assert!(engine
                .projected_events(&denied, &fresh, Timestamp::new(103), 256 * 1024)
                .is_err());
        }
    }
}
impl<S: CanonicalStore> Engine<S> {
    /// Project only invalidation/evidence metadata, directly from borrowed
    /// events. Raw fact payloads are neither cloned nor exposed to subscribers.
    pub fn projected_events(
        &self,
        access: &Access,
        cursor: &Cursor,
        now: Timestamp,
        maximum_bytes: usize,
    ) -> Result<ProjectedEvents> {
        use vcp_domain::artifact::{ArtifactDescriptor, CaptureState};
        use vcp_protocol::methods;
        if let Some(reason) = self.event_cursor_gap(access, cursor, now)? {
            return Ok(ProjectedEvents::Gap(reason));
        }
        if maximum_bytes > crate::query::MAX_RESULT_BYTES || maximum_bytes < 8192 {
            return Ok(ProjectedEvents::TooLarge);
        }
        let id = |value: &str| methods::Id::try_from(value.to_owned()).map_err(|_| Error::Target);
        let mut bytes = 8192;
        let mut events = Vec::new();
        let mut next = cursor.clone();
        let state = self.store().state();
        let masks = state
            .records
            .values()
            .filter(|row| {
                row.collection == Collection::Tombstone && row.workspace == access.workspace
            })
            .map(|row| {
                let mask: vcp_domain::retention::RetentionMask = row.decode()?;
                mask.validate()?;
                if mask.workspace != access.workspace || mask.deletion > cursor.deletion {
                    return Err(Error::Store(vcp_store::Error::Corruption(
                        "retention scope or epoch",
                    )));
                }
                Ok(mask)
            })
            .collect::<Result<Vec<_>>>()?;
        for event in state
            .events
            .iter()
            .filter(|event| {
                event.event.workspace == access.workspace
                    && event.event.session == access.session
                    && event.sequence > cursor.after
                    && event.sequence <= cursor.end
                    && event.watermark <= cursor.watermark
            })
            .take(cursor.limit as usize)
        {
            if event.sequence != next.after.next()? {
                return Ok(ProjectedEvents::Gap(GapReason::SequenceUnavailable));
            }
            // Match audit history: a logical exclusion hides the entire event,
            // even while original bytes still await a physical rewrite.
            if masks.iter().any(|mask| {
                mask.session == access.session
                    && event.sequence >= mask.first
                    && event.sequence <= mask.last
            }) {
                return Ok(ProjectedEvents::Gap(GapReason::RetentionChanged));
            }
            let mut evidence = Vec::new();
            let mut evidence_complete = event.redaction.is_none();
            if event.redaction.is_none() {
                for artifact in &event.event.artifacts {
                    if masks.iter().any(|mask| mask.artifacts.contains(artifact)) {
                        evidence_complete = false;
                        continue;
                    }
                    if evidence.len() == 128 {
                        evidence_complete = false;
                        break;
                    }
                    let descriptor = state
                        .record(Collection::Artifact, artifact.as_str(), &access.workspace)
                        .and_then(|row| row.decode::<ArtifactDescriptor>());
                    match descriptor {
                        Ok(descriptor)
                            if descriptor.spec.scope.session == access.session
                                && descriptor.spec.scope.workspace == access.workspace
                                && descriptor.state == CaptureState::Complete
                                && (descriptor.length == ByteCount::ZERO
                                    || descriptor.retained.iter().any(|range| {
                                        range.start == ByteCount::ZERO
                                            && range.end == descriptor.length
                                    })) =>
                        {
                            evidence.push(methods::EvidenceReference {
                                artifact: id(artifact.as_str())?,
                                offset: 0.into(),
                                length: descriptor.length.get().into(),
                                sha256: descriptor.sha256,
                            });
                        }
                        _ => evidence_complete = false,
                    }
                }
            }
            let projected = methods::Event {
                id: id(event.event.id.as_str())?,
                scope: methods::Scope {
                    workspace: id(access.workspace.as_str())?,
                    session: id(access.session.as_str())?,
                },
                sequence: event.sequence.get().into(),
                schema_version: format!("vcp-event/{}", event.version),
                timestamp_ms: event.event.timestamp.get().into(),
                kind: serde_json::to_value(&event.event.kind)?
                    .as_str()
                    .ok_or(Error::Target)?
                    .to_owned(),
                command_id: Some(id(event.event.correlation.as_str())?),
                task: event
                    .event
                    .task
                    .as_ref()
                    .map(|task| id(task.as_str()))
                    .transpose()?,
                outcome: None,
                redacted: event.redaction.is_some(),
                evidence_complete,
                evidence,
            };
            let size = serde_json::to_vec(&projected)?.len() + 1;
            if bytes + size > maximum_bytes {
                if events.is_empty() {
                    return Ok(ProjectedEvents::TooLarge);
                }
                break;
            }
            bytes += size;
            events.push(projected);
            next.after = event.sequence;
        }
        if events.is_empty() && cursor.after < cursor.end {
            return Ok(ProjectedEvents::Gap(GapReason::SequenceUnavailable));
        }
        let at_end = next.after == next.end;
        Ok(ProjectedEvents::Page {
            events,
            next,
            at_end,
        })
    }
    pub fn subscribe(
        &mut self,
        access: &Access,
        after: SessionSeq,
        limit: u32,
        now: Timestamp,
    ) -> Result<Cursor> {
        self.authorize(access)?;
        if limit == 0 || limit as usize > vcp_protocol::version::MAX_PAGE_EVENTS {
            return Err(vcp_protocol::version::Error::Limit.into());
        }
        self.subscriptions
            .retain(|_, cursor| cursor.expires_at > now);
        if self.subscriptions.len() >= MAX_SUBSCRIPTIONS {
            return Err(vcp_protocol::version::Error::Limit.into());
        }
        let state = self.store().state();
        let workspace: Workspace = state
            .record(
                Collection::Workspace,
                access.workspace.as_str(),
                &access.workspace,
            )?
            .decode()?;
        let end = state
            .sequences
            .get(&access.session)
            .copied()
            .unwrap_or_default();
        if after > end {
            return Err(Error::Target);
        }
        let cursor = Cursor {
            version: 1,
            snapshot: SnapshotId::new(),
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            after,
            end,
            watermark: state.watermark,
            authority: workspace.authority,
            deletion: workspace.deletion,
            expires_at: Timestamp::new(
                now.get()
                    .checked_add(60_000)
                    .ok_or(vcp_domain::Error::Overflow)?,
            ),
            limit,
        };
        self.subscriptions
            .insert(cursor.snapshot.clone(), cursor.clone());
        Ok(cursor)
    }
    /// Pull-based backpressure: no producer queue is allocated for a slow or
    /// disconnected consumer. Final results remain in canonical ordered events.
    pub fn events(&self, access: &Access, cursor: &Cursor, now: Timestamp) -> Result<EventPage> {
        let gap = |reason| {
            Ok(EventPage::Gap {
                reason,
                restart_from_snapshot: true,
            })
        };
        if let Some(reason) = self.event_cursor_gap(access, cursor, now)? {
            return Ok(EventPage::Gap {
                reason,
                restart_from_snapshot: true,
            });
        }
        let state = self.store().state();
        let events = state
            .events
            .iter()
            .filter(|e| {
                e.event.workspace == access.workspace
                    && e.event.session == access.session
                    && e.sequence > cursor.after
                    && e.sequence <= cursor.end
                    && e.watermark <= cursor.watermark
            })
            .take(cursor.limit as usize)
            .cloned()
            .collect::<Vec<_>>();
        let mut expected = cursor.after;
        for event in &events {
            expected = expected.next()?;
            if event.sequence != expected {
                return gap(GapReason::SequenceUnavailable);
            }
        }
        if events.is_empty() && cursor.after < cursor.end {
            return gap(GapReason::SequenceUnavailable);
        }
        let mut next_cursor = cursor.clone();
        next_cursor.after = expected;
        Ok(EventPage::Events {
            events,
            at_end: next_cursor.after == next_cursor.end,
            next_cursor,
            snapshot_watermark: cursor.watermark,
        })
    }
    /// Validate metadata and current authority without cloning any event payload.
    pub fn event_cursor_gap(
        &self,
        access: &Access,
        cursor: &Cursor,
        now: Timestamp,
    ) -> Result<Option<GapReason>> {
        self.authorize(access)?;
        let gap = |reason| Ok(Some(reason));
        if cursor.workspace != access.workspace || cursor.session != access.session {
            return Err(Error::Access);
        }
        let Some(original) = self.subscriptions.get(&cursor.snapshot) else {
            return gap(GapReason::SnapshotExpired);
        };
        if cursor.workspace != original.workspace || cursor.session != original.session {
            return gap(GapReason::ScopeChanged);
        }
        if now >= original.expires_at {
            return gap(GapReason::SnapshotExpired);
        }
        if cursor.version != 1
            || cursor.end != original.end
            || cursor.watermark != original.watermark
            || cursor.authority != original.authority
            || cursor.deletion != original.deletion
            || cursor.expires_at != original.expires_at
            || cursor.limit != original.limit
            || cursor.after > cursor.end
            || cursor.after < original.after
        {
            return gap(GapReason::CursorChanged);
        }
        let state = self.store().state();
        let workspace: Workspace = state
            .record(
                Collection::Workspace,
                access.workspace.as_str(),
                &access.workspace,
            )?
            .decode()?;
        if cursor.authority != workspace.authority {
            return gap(GapReason::ScopeChanged);
        }
        if cursor.deletion != workspace.deletion {
            return gap(GapReason::RetentionChanged);
        }
        Ok(None)
    }

    /// Explicitly rotate a fully consumed fixed window. Existing callers that
    /// never invoke this retain immutable windows. TTL and allocation stay fixed.
    pub fn advance_event_window(
        &mut self,
        access: &Access,
        cursor: &Cursor,
        now: Timestamp,
    ) -> Result<Cursor> {
        if self.event_cursor_gap(access, cursor, now)?.is_some() || cursor.after != cursor.end {
            return Err(Error::Target);
        }
        let mut next = cursor.clone();
        next.end = self
            .store()
            .state()
            .sequences
            .get(&access.session)
            .copied()
            .unwrap_or_default();
        next.watermark = self.store().state().watermark;
        self.subscriptions
            .insert(next.snapshot.clone(), next.clone());
        Ok(next)
    }

    pub fn unsubscribe(&mut self, id: &SnapshotId) {
        self.subscriptions.remove(id);
    }
}
