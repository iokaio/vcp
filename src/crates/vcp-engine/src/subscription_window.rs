// SPDX-License-Identifier: Apache-2.0
//! Fallible bounded reads for one already-authorized fixed subscription window.
use crate::{Access, Error, Result};
use vcp_protocol::{event::EventEnvelope, subscription::Cursor};
use vcp_store::contract::CanonicalStore;

/// Visit only the same filtered prefix as the original resident iterator. The
/// global scan is bounded in memory; short byte-bound pages are continuations.
pub(super) async fn visit<S: CanonicalStore>(
    store: &S,
    access: &Access,
    cursor: &Cursor,
    mut visit: impl FnMut(&EventEnvelope) -> Result<bool>,
) -> Result<()> {
    if cursor.after == cursor.end {
        return Ok(());
    }
    let watermark = store.current().watermark;
    let end = store.history_event_count().await?;
    let mut ordinal = 0u64;
    let mut selected = 0usize;
    'pages: while ordinal < end {
        let limit = (end - ordinal).min(4096) as usize;
        let rows = store.history_events(ordinal.checked_sub(1), limit).await?;
        if rows.is_empty() || rows.len() > limit {
            return Err(Error::Store(vcp_store::Error::Corruption(
                "subscription history page",
            )));
        }
        for event in &rows {
            if event.watermark > watermark {
                return Err(Error::Store(vcp_store::Error::Corruption(
                    "subscription history cut",
                )));
            }
            if event.event.workspace == access.workspace
                && event.event.session == access.session
                && event.sequence > cursor.after
                && event.sequence <= cursor.end
                && event.watermark <= cursor.watermark
            {
                selected += 1;
                if !visit(event)? || selected == cursor.limit as usize {
                    break 'pages;
                }
            }
        }
        ordinal += rows.len() as u64;
    }
    if store.current().watermark != watermark {
        return Err(Error::Store(vcp_store::Error::Conflict(
            "subscription history owner changed",
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use vcp_domain::{ids::*, revision::*};
    use vcp_protocol::event::{EventInput, EventKind};
    use vcp_store::contract::{Receipt, State, Transaction};

    struct Reader {
        state: State,
        reads: Cell<usize>,
        fail: bool,
    }
    impl CanonicalStore for Reader {
        fn state(&self) -> &State {
            panic!("subscription requested resident State")
        }
        fn current(&self) -> vcp_store::CurrentStateView<'_> {
            (&self.state).into()
        }
        async fn history_event_count(&self) -> vcp_store::Result<u64> {
            Ok(self.state.events.len() as u64)
        }
        async fn history_events(
            &self,
            after: Option<u64>,
            limit: usize,
        ) -> vcp_store::Result<Vec<EventEnvelope>> {
            self.reads.set(self.reads.get() + 1);
            if self.fail && after.is_some() {
                return Err(vcp_store::Error::Corruption("injected window page"));
            }
            Ok(self
                .state
                .events
                .iter()
                .skip(after.map_or(0, |n| n as usize + 1))
                .take(limit.min(2))
                .cloned()
                .collect())
        }
        async fn transact(&mut self, _: Transaction) -> vcp_store::Result<Receipt> {
            panic!("subscription wrote")
        }
    }
    #[tokio::test]
    async fn window_keeps_filtered_prefix_and_propagates_interior_failure() {
        let access = Access {
            actor: ActorId::new(),
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
            authority: AuthorityRevision::ZERO,
            read: true,
            write: false,
            bootstrap: false,
        };
        let other = SessionId::new();
        let mut state = State::default();
        state.watermark = Watermark::new(8);
        for n in 1u64..=8 {
            state.events.push(EventEnvelope {
                version: 1,
                sequence: SessionSeq::new(n.div_ceil(2)),
                watermark: Watermark::new(n),
                redaction: None,
                event: EventInput {
                    id: EventId::new(),
                    workspace: access.workspace.clone(),
                    session: if n % 2 == 1 {
                        access.session.clone()
                    } else {
                        other.clone()
                    },
                    task: None,
                    actor: access.actor.clone(),
                    correlation: CommandId::new(),
                    causation: None,
                    timestamp: Timestamp::new(n),
                    kind: EventKind::Diagnostic,
                    artifacts: vec![],
                    data: serde_json::json!({"schema_version":1}),
                    metadata: None,
                },
            });
        }
        let mut cursor = Cursor {
            version: 1,
            snapshot: SnapshotId::new(),
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            after: SessionSeq::new(1),
            end: SessionSeq::new(3),
            watermark: Watermark::new(6),
            authority: access.authority,
            deletion: DeletionEpoch::ZERO,
            expires_at: Timestamp::new(100),
            limit: 2,
        };
        let expected: Vec<_> = state
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
            .map(|event| event.event.id.clone())
            .collect();
        let mut reader = Reader {
            state,
            reads: Cell::new(0),
            fail: false,
        };
        let mut actual = Vec::new();
        visit(&reader, &access, &cursor, |event| {
            actual.push(event.event.id.clone());
            Ok(true)
        })
        .await
        .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(reader.reads.get(), 3, "prefix ends in the third short page");
        reader.fail = true;
        assert!(visit(&reader, &access, &cursor, |_| Ok(true))
            .await
            .is_err());
        cursor.after = cursor.end;
        let count = reader.reads.get();
        visit(&reader, &access, &cursor, |_| {
            panic!("empty window visited an event")
        })
        .await
        .unwrap();
        assert_eq!(reader.reads.get(), count);
    }
}
