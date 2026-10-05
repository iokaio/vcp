// SPDX-License-Identifier: Apache-2.0
//! Bounded source traversal retaining the historical preview commitment bytes.
use super::*;
use std::io::Write;
use vcp_protocol::{event::EventEnvelope, DigestWriter};

pub(super) struct Pages {
    watermark: Watermark,
    end: u64,
    next: u64,
}
impl Pages {
    pub(super) async fn open<S: CanonicalStore>(store: &S) -> Result<Self> {
        Ok(Self {
            watermark: store.current().watermark,
            end: store.history_event_count().await?,
            next: 0,
        })
    }
    pub(super) async fn next<S: CanonicalStore>(
        &mut self,
        store: &S,
    ) -> Result<Option<Vec<EventEnvelope>>> {
        self.check(store)?;
        if self.next == self.end {
            return Ok(None);
        }
        let limit = (self.end - self.next).min(4096) as usize;
        let rows = store
            .history_events(self.next.checked_sub(1), limit)
            .await?;
        if rows.is_empty()
            || rows.len() > limit
            || rows.iter().any(|e| e.watermark > self.watermark)
        {
            return Err(Error::Conflict(
                "retention history page incomplete or outside owner",
            ));
        }
        self.check(store)?;
        self.next += rows.len() as u64;
        Ok(Some(rows))
    }
    pub(super) fn check<S: CanonicalStore>(&self, store: &S) -> Result<()> {
        if store.current().watermark != self.watermark {
            return Err(Error::Conflict("retention source changed during read"));
        }
        Ok(())
    }
}

fn write(sink: &mut impl Write, bytes: &[u8]) -> Result<()> {
    sink.write_all(bytes).map_err(vcp_store::Error::from)?;
    Ok(())
}

/// Byte-for-byte equivalent to canonical((filtered record entry array,
/// filtered event array, complete command map)); no historical payload retained.
pub(super) async fn source_digest<S: CanonicalStore>(store: &S) -> Result<String> {
    let mut pages = Pages::open(store).await?;
    let mut sink = DigestWriter::default();
    write(&mut sink, b"[[")?;
    let mut first = true;
    for (key, record) in store
        .current()
        .records
        .iter()
        .filter(|(_, r)| r.value["document_type"] != PREVIEW)
    {
        if !first {
            write(&mut sink, b",")?;
        }
        first = false;
        write(&mut sink, &canonical_bytes(&(key, record))?)?;
    }
    write(&mut sink, b"],[")?;
    first = true;
    while let Some(rows) = pages.next(store).await? {
        for event in rows
            .iter()
            .filter(|e| e.event.data["document_type"] != PREVIEW)
        {
            if !first {
                write(&mut sink, b",")?;
            }
            first = false;
            write(&mut sink, &canonical_bytes(event)?)?;
        }
    }
    write(&mut sink, b"],{")?;
    let mut after: Option<String> = None;
    first = true;
    loop {
        let rows = store.history_commands(after.as_deref(), 4096).await?;
        pages.check(store)?;
        if rows.len() > 4096 {
            return Err(Error::Conflict("retention command page bound"));
        }
        if rows.is_empty() {
            break;
        }
        for (key, receipt) in rows {
            if after.as_ref().is_some_and(|after| &key <= after)
                || key != vcp_store::contract::command_key(&receipt.workspace, &receipt.command)
                || receipt.watermark > store.current().watermark
            {
                return Err(Error::Conflict("retention command page identity"));
            }
            if !first {
                write(&mut sink, b",")?;
            }
            first = false;
            write(&mut sink, &canonical_bytes(&key)?)?;
            write(&mut sink, b":")?;
            write(&mut sink, &canonical_bytes(&receipt)?)?;
            after = Some(key);
        }
    }
    write(&mut sink, b"}]")?;
    Ok(sink.finish().0)
}

pub(super) fn event_scope(event: &EventEnvelope) -> Option<Scope> {
    event.event.task.as_ref().map(|task| Scope {
        workspace: event.event.workspace.clone(),
        session: event.event.session.clone(),
        task: task.clone(),
    })
}

/// Only one timestamp per current eligible record is retained. All historical
/// rows, including redacted rows, retain the original earliest-match meaning.
pub(super) async fn record_timestamps<S: CanonicalStore>(
    store: &S,
    workspace: &WorkspaceId,
) -> Result<BTreeMap<String, Timestamp>> {
    let current = store.current();
    let eligible = |key: &String| {
        current.records.get(key).is_some_and(|row| {
            &row.workspace == workspace && valid_record(row) && !already_redacted(row)
        })
    };
    let mut result = BTreeMap::new();
    let mut pages = Pages::open(store).await?;
    while let Some(rows) = pages.next(store).await? {
        for event in rows
            .iter()
            .filter(|event| &event.event.workspace == workspace)
        {
            let timestamp = event.event.timestamp;
            for artifact in &event.event.artifacts {
                let key = key(Collection::Artifact, artifact.as_str());
                if eligible(&key) {
                    result.entry(key).or_insert(timestamp);
                }
            }
            if let Some(task) = &event.event.task {
                let key = key(Collection::Task, task.as_str());
                if eligible(&key) {
                    result.entry(key).or_insert(timestamp);
                }
            }
            if let Some(facts) = event
                .event
                .data
                .get("facts")
                .or_else(|| event.event.data.get("records"))
                .and_then(|v| v.as_array())
            {
                for fact in facts {
                    if let Ok(record) = serde_json::from_value::<Record>(fact.clone()) {
                        if &record.workspace == workspace
                            && !matches!(record.collection, Collection::Task | Collection::Artifact)
                        {
                            let key = record.key();
                            if eligible(&key) {
                                result.entry(key).or_insert(timestamp);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vcp_protocol::command::{CommandReceipt, CommandResult};
    struct Reader {
        state: State,
        failure: u8,
    }
    impl vcp_store::contract::reference::ReferenceStore for Reader {
        fn state(&self) -> &State {
            panic!("retention digest must not materialize State")
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
            _: usize,
        ) -> vcp_store::Result<Vec<EventEnvelope>> {
            if self.failure == 1 {
                return Err(vcp_store::Error::Corruption("injected history read"));
            }
            if self.failure == 2 {
                return Ok(Vec::new());
            }
            Ok(self
                .state
                .events
                .iter()
                .skip(after.map_or(0, |n| n as usize + 1))
                .take(1)
                .cloned()
                .collect())
        }
        async fn history_commands(
            &self,
            after: Option<&str>,
            _: usize,
        ) -> vcp_store::Result<Vec<(String, CommandReceipt)>> {
            if self.failure == 3 {
                return Err(vcp_store::Error::Corruption("injected receipt read"));
            }
            let mut rows: Vec<_> = self
                .state
                .commands
                .iter()
                .filter(|(key, _)| after.is_none_or(|a| key.as_str() > a))
                .take(1)
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            if self.failure == 4 {
                if let Some((key, _)) = rows.first_mut() {
                    *key = "wrong-key".into();
                }
            }
            Ok(rows)
        }
        async fn transact(
            &mut self,
            _: Transaction,
        ) -> vcp_store::Result<vcp_store::contract::Receipt> {
            panic!("read only")
        }
    }
    #[tokio::test]
    async fn exact_legacy_digest_streams_short_pages_and_rejects_failed_facts() {
        let workspace = WorkspaceId::new();
        let mut state = State::default();
        state.watermark = Watermark::new(9);
        for index in 0..3 {
            let record = Record {
                collection: Collection::Projection,
                id: format!("row-{index}"),
                workspace: workspace.clone(),
                revision: Revision::ZERO,
                references: BTreeSet::new(),
                value: serde_json::json!({"z":{"ü":"line\nquote\""},"a":index,"document_type":if index==1 {PREVIEW} else {"fixture"}}),
            };
            state.records.insert(record.key(), record);
            let command = CommandId::new();
            state.events.push(EventEnvelope {
                version:1, sequence:SessionSeq::new(index+1), watermark:Watermark::new(index+1), redaction:None,
                event:EventInput { id:EventId::new(), workspace:workspace.clone(), session:SessionId::new(), task:None,
                    actor:ActorId::new(), correlation:command.clone(), causation:None, timestamp:Timestamp::new(index),
                    kind:EventKind::RetentionChanged, artifacts:Vec::new(), metadata:None,
                    data:serde_json::json!({"document_type":if index==1 {PREVIEW} else {"fixture"},"nested":{"z":2,"a":1}}),
                },
            });
            let receipt = CommandReceipt {
                version: 1,
                command: command.clone(),
                workspace: workspace.clone(),
                digest: "a".repeat(64),
                transaction: TransactionId::new(),
                watermark: Watermark::new(index + 1),
                first_event: SessionSeq::new(index + 1),
                last_event: SessionSeq::new(index + 1),
                result: CommandResult::Accepted {
                    revision: Revision::ZERO,
                },
            };
            state.commands.insert(
                vcp_store::contract::command_key(&workspace, &command),
                receipt,
            );
        }
        let expected = super::super::source_digest(&state).unwrap();
        let mut reader = Reader { state, failure: 0 };
        assert_eq!(source_digest(&reader).await.unwrap(), expected);
        for failure in 1..=4 {
            reader.failure = failure;
            assert!(source_digest(&reader).await.is_err());
        }
        let empty = Reader {
            state: State::default(),
            failure: 0,
        };
        assert_eq!(
            source_digest(&empty).await.unwrap(),
            super::super::source_digest(&empty.state).unwrap()
        );
    }

    #[tokio::test]
    async fn timestamp_accumulator_matches_original_first_event_predicates() {
        let workspace = WorkspaceId::new();
        let task = TaskId::new();
        let artifact = ArtifactId::new();
        let mut state = State::default();
        state.watermark = Watermark::new(2);
        for (collection, id) in [
            (Collection::Task, task.to_string()),
            (Collection::Artifact, artifact.to_string()),
            (Collection::Effect, "effect".into()),
        ] {
            let record = Record {
                collection,
                id,
                workspace: workspace.clone(),
                revision: Revision::ZERO,
                references: BTreeSet::new(),
                value: serde_json::json!({}),
            };
            state.records.insert(record.key(), record);
        }
        for index in 0..2 {
            let effect = &state.records[&key(Collection::Effect, "effect")];
            state.events.push(EventEnvelope{version:1,sequence:SessionSeq::new(index+1),watermark:Watermark::new(index+1),redaction:None,
                event:EventInput{id:EventId::new(),workspace:workspace.clone(),session:SessionId::new(),task:Some(task.clone()),actor:ActorId::new(),correlation:CommandId::new(),causation:None,
                    timestamp:Timestamp::new(10+index),kind:EventKind::RetentionChanged,artifacts:vec![artifact.clone()],metadata:None,
                    data:serde_json::json!({"document_type":if index==0 {PREVIEW}else{"other"},"facts":[{"id":"effect"},effect]}),
                },
            });
        }
        let expected: BTreeMap<_, _> = state
            .records
            .values()
            .filter_map(|row| {
                state
                    .events
                    .iter()
                    .filter(|e| e.event.workspace == workspace)
                    .find(|event| match row.collection {
                        Collection::Artifact => {
                            event.event.artifacts.iter().any(|id| id.as_str() == row.id)
                        }
                        Collection::Task => event
                            .event
                            .task
                            .as_ref()
                            .is_some_and(|id| id.as_str() == row.id),
                        _ => event
                            .event
                            .data
                            .get("facts")
                            .or_else(|| event.event.data.get("records"))
                            .and_then(|v| v.as_array())
                            .is_some_and(|facts| {
                                facts.iter().any(|fact| {
                                    serde_json::from_value::<Record>(fact.clone()).is_ok_and(
                                        |fact| {
                                            fact.key() == row.key()
                                                && fact.workspace == row.workspace
                                        },
                                    )
                                })
                            }),
                    })
                    .map(|event| (row.key(), event.event.timestamp))
            })
            .collect();
        let reader = Reader { state, failure: 0 };
        let actual = record_timestamps(&reader, &workspace).await.unwrap();
        assert_eq!(actual, expected);
        assert_eq!(actual.len(), 3);
        assert!(actual
            .values()
            .all(|timestamp| *timestamp == Timestamp::new(10)));
    }
}
