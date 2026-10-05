// SPDX-License-Identifier: Apache-2.0
//! Select the preceding repair from authenticated bounded history, without
//! retaining an event vector in the execution owner.
use super::*;

pub(super) async fn prior_repair<S: CanonicalStore>(
    store: &S,
    scope: &Scope,
) -> Result<Option<ArtifactId>> {
    let current = store.current();
    let watermark = current.watermark;
    let end = store.history_event_count().await?;
    let mut ordinal = 0u64;
    let mut prior = None;
    while ordinal < end {
        let limit = (end - ordinal).min(4096) as usize;
        let events = store.history_events(ordinal.checked_sub(1), limit).await?;
        if events.is_empty() || events.len() > limit {
            return Err("completion repair history page is incomplete".into());
        }
        for event in &events {
            if event.watermark > watermark {
                return Err("completion repair history exceeds owner cut".into());
            }
            if event.event.task.as_ref() != Some(&scope.task) {
                continue;
            }
            // Reverse event order with forward artifact order was the original
            // selection rule. A forward scan keeps the first eligible artifact
            // from each newer event and only that bounded identity.
            if let Some(id) = event.event.artifacts.iter().find_map(|id| {
                current
                    .records
                    .get(&vcp_store::contract::key(Collection::Artifact, id.as_str()))
                    .and_then(|record| record.decode::<ArtifactDescriptor>().ok())
                    .filter(|descriptor| {
                        descriptor.spec.scope == *scope
                            && descriptor.spec.schema == "execution-completion-repair/1"
                    })
                    .map(|descriptor| descriptor.spec.id)
            }) {
                prior = Some(id);
            }
        }
        ordinal += events.len() as u64;
    }
    if store.current().watermark != watermark {
        return Err("completion repair history owner changed".into());
    }
    Ok(prior)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use vcp_domain::artifact::{ArtifactSpec, Range};
    use vcp_protocol::event::{EventEnvelope, EventInput, EventKind};
    use vcp_store::contract::{Receipt, State, Transaction};

    struct Reader {
        source: State,
        fail: bool,
        reads: Cell<usize>,
    }
    impl vcp_store::contract::reference::ReferenceStore for Reader {
        fn state(&self) -> &State {
            panic!("repair lookup requested resident State")
        }
        fn current(&self) -> vcp_store::CurrentStateView<'_> {
            (&self.source).into()
        }
        async fn history_event_count(&self) -> vcp_store::Result<u64> {
            Ok(self.source.events.len() as u64)
        }
        async fn history_events(
            &self,
            after: Option<u64>,
            limit: usize,
        ) -> vcp_store::Result<Vec<EventEnvelope>> {
            self.reads.set(self.reads.get() + 1);
            if self.fail && after.is_some() {
                return Err(vcp_store::Error::Corruption("injected repair page"));
            }
            Ok(self
                .source
                .events
                .iter()
                .skip(after.map_or(0, |n| n as usize + 1))
                .take(limit.min(2))
                .cloned()
                .collect())
        }
        async fn transact(&mut self, _: Transaction) -> vcp_store::Result<Receipt> {
            panic!("repair lookup wrote canonical state")
        }
    }
    fn descriptor(state: &mut State, scope: &Scope, name: &str) -> ArtifactId {
        let id = ArtifactId::parse(name).unwrap();
        let descriptor = ArtifactDescriptor {
            spec: ArtifactSpec {
                id: id.clone(),
                scope: scope.clone(),
                media_type: "application/json".into(),
                schema: "execution-completion-repair/1".into(),
                source: "fixture".into(),
                channel: Channel::Evidence,
                retention: "history".into(),
                omissions: vec![],
            },
            state: CaptureState::Complete,
            length: ByteCount::new(1),
            sha256: "a".repeat(64),
            retained: vec![Range {
                start: ByteCount::ZERO,
                end: ByteCount::new(1),
            }],
        };
        let record = Record::typed(
            Collection::Artifact,
            name,
            scope.workspace.clone(),
            Revision::ZERO,
            &descriptor,
        )
        .unwrap();
        state.records.insert(record.key(), record);
        id
    }
    fn event(state: &mut State, scope: &Scope, artifacts: Vec<ArtifactId>) {
        let sequence = state.events.len() as u64 + 1;
        state.watermark = Watermark::new(sequence);
        state.events.push(EventEnvelope {
            version: 1,
            sequence: SessionSeq::new(sequence),
            watermark: state.watermark,
            redaction: None,
            event: EventInput {
                id: EventId::new(),
                workspace: scope.workspace.clone(),
                session: scope.session.clone(),
                task: Some(scope.task.clone()),
                actor: ActorId::new(),
                correlation: CommandId::new(),
                causation: None,
                timestamp: Timestamp::new(1),
                kind: EventKind::ArtifactAttached,
                artifacts,
                data: serde_json::json!({"schema_version":1}),
                metadata: None,
            },
        });
    }
    #[tokio::test]
    async fn bounded_repair_lookup_keeps_reverse_event_forward_artifact_order_and_fails_on_gaps() {
        let scope = Scope {
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
            task: TaskId::new(),
        };
        let mut source = State::default();
        let old = descriptor(&mut source, &scope, "old");
        let first = descriptor(&mut source, &scope, "new-first");
        let second = descriptor(&mut source, &scope, "new-second");
        let mut foreign = scope.clone();
        foreign.session = SessionId::new();
        let outside = descriptor(&mut source, &foreign, "outside");
        event(&mut source, &scope, vec![old]);
        event(&mut source, &scope, vec![outside.clone()]);
        event(&mut source, &scope, vec![]);
        event(
            &mut source,
            &scope,
            vec![ArtifactId::new(), first.clone(), second],
        );
        event(&mut source, &scope, vec![outside]);
        let expected = source
            .events
            .iter()
            .rev()
            .filter(|row| row.event.task.as_ref() == Some(&scope.task))
            .flat_map(|row| row.event.artifacts.iter())
            .find_map(|id| {
                source
                    .records
                    .get(&vcp_store::contract::key(Collection::Artifact, id.as_str()))
                    .and_then(|row| row.decode::<ArtifactDescriptor>().ok())
                    .filter(|descriptor| {
                        descriptor.spec.scope == scope
                            && descriptor.spec.schema == "execution-completion-repair/1"
                    })
                    .map(|descriptor| descriptor.spec.id)
            });
        assert_eq!(expected, Some(first));
        let mut reader = Reader {
            source,
            fail: false,
            reads: Cell::new(0),
        };
        assert_eq!(prior_repair(&reader, &scope).await.unwrap(), expected);
        assert_eq!(reader.reads.get(), 3);
        reader.fail = true;
        assert!(prior_repair(&reader, &scope).await.is_err());
        reader.fail = false;
        reader.source.events.clear();
        assert!(prior_repair(&reader, &scope).await.unwrap().is_none());
    }
}
