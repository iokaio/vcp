// SPDX-License-Identifier: Apache-2.0
//! Bound the public preview's full-state source commitment before the shared
//! workflow canonicalizes it. Counting borrows values and never copies payloads.
use crate::{Error, Result};
#[cfg(test)]
use serde::Serialize;
use std::io::{self, Write};
use vcp_store::contract::CanonicalStore;
#[cfg(test)]
use vcp_store::contract::State;

const MAX_ROWS: usize = 32_768;
const MAX_BYTES: usize = 16 * 1024 * 1024;
struct Count(usize);
impl Write for Count {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_sub(bytes.len())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "retention source limit"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[cfg(test)]
fn count(value: &impl Serialize, limit: usize) -> Result<()> {
    serde_json::to_writer(Count(limit), value)
        .map_err(|_| Error::Conflict("retention source limit"))
}
#[cfg(test)]
fn check(state: &State) -> Result<()> {
    let rows = state
        .records
        .len()
        .saturating_add(state.events.len())
        .saturating_add(state.commands.len());
    if rows > MAX_ROWS {
        return Err(Error::Conflict("retention source limit"));
    }
    // Conservatively include saved-preview records too. Omitting them from the
    // shared source hash does not make their selection/closure traversal free.
    count(&(&state.records, &state.events, &state.commands), MAX_BYTES)
}

pub(super) async fn check_store<S: CanonicalStore>(store: &S) -> Result<()> {
    let watermark = store.current().watermark;
    let events = store.history_event_count().await?;
    let mut rows = (store.current().records.len() as u64).saturating_add(events);
    if rows > MAX_ROWS as u64 {
        return Err(Error::Conflict("retention source limit"));
    }
    let mut after: Option<String> = None;
    loop {
        let page = store.history_commands(after.as_deref(), 4096).await?;
        if page.is_empty() {
            break;
        }
        if page.len() > 4096 {
            return Err(Error::Conflict("retention command page bound"));
        }
        for (key, _) in page {
            if after.as_ref().is_some_and(|a| &key <= a) {
                return Err(Error::Conflict("retention command page order"));
            }
            after = Some(key);
            rows += 1;
            if rows > MAX_ROWS as u64 {
                return Err(Error::Conflict("retention source limit"));
            }
        }
    }
    let mut count = Count(MAX_BYTES);
    let emit = |count: &mut Count, bytes: &[u8]| {
        count
            .write_all(bytes)
            .map_err(|_| Error::Conflict("retention source limit"))
    };
    emit(&mut count, b"[")?;
    serde_json::to_writer(&mut count, store.current().records)
        .map_err(|_| Error::Conflict("retention source limit"))?;
    emit(&mut count, b",[")?;
    let mut next = 0;
    while next < events {
        let limit = (events - next).min(4096) as usize;
        let page = store.history_events(next.checked_sub(1), limit).await?;
        if page.is_empty() || page.len() > limit || page.iter().any(|e| e.watermark > watermark) {
            return Err(Error::Conflict("retention history page"));
        }
        for event in page {
            if next > 0 {
                emit(&mut count, b",")?;
            }
            serde_json::to_writer(&mut count, &event)
                .map_err(|_| Error::Conflict("retention source limit"))?;
            next += 1;
        }
    }
    emit(&mut count, b"],{")?;
    after = None;
    loop {
        let page = store.history_commands(after.as_deref(), 4096).await?;
        if page.is_empty() {
            break;
        }
        if page.len() > 4096 {
            return Err(Error::Conflict("retention command page bound"));
        }
        for (key, receipt) in page {
            if after.as_ref().is_some_and(|a| &key <= a) {
                return Err(Error::Conflict("retention command page order"));
            }
            if after.is_some() {
                emit(&mut count, b",")?;
            }
            serde_json::to_writer(&mut count, &key)
                .map_err(|_| Error::Conflict("retention source limit"))?;
            emit(&mut count, b":")?;
            serde_json::to_writer(&mut count, &receipt)
                .map_err(|_| Error::Conflict("retention source limit"))?;
            after = Some(key);
        }
    }
    emit(&mut count, b"}]")?;
    if store.current().watermark != watermark {
        return Err(Error::Conflict("retention source changed during read"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Reader {
        state: State,
        events: u64,
        commands_read: std::cell::Cell<usize>,
    }
    impl vcp_store::contract::reference::ReferenceStore for Reader {
        fn state(&self) -> &State {
            panic!("public retention must not materialize State")
        }
        fn current(&self) -> vcp_store::CurrentStateView<'_> {
            (&self.state).into()
        }
        async fn history_event_count(&self) -> vcp_store::Result<u64> {
            Ok(self.events)
        }
        async fn history_commands(
            &self,
            _: Option<&str>,
            _: usize,
        ) -> vcp_store::Result<Vec<(String, vcp_protocol::command::CommandReceipt)>> {
            self.commands_read.set(self.commands_read.get() + 1);
            Ok(Vec::new())
        }
        async fn history_events(
            &self,
            _: Option<u64>,
            _: usize,
        ) -> vcp_store::Result<Vec<vcp_protocol::event::EventEnvelope>> {
            panic!("row capacity must reject before payload read")
        }
        async fn transact(
            &mut self,
            _: vcp_store::contract::Transaction,
        ) -> vcp_store::Result<vcp_store::contract::Receipt> {
            panic!("read only")
        }
    }
    #[tokio::test]
    async fn streamed_public_preflight_preserves_row_and_byte_thresholds() {
        let mut reader = Reader {
            state: State::default(),
            events: 0,
            commands_read: std::cell::Cell::new(0),
        };
        assert!(check_store(&reader).await.is_ok());
        assert!(check(&reader.state).is_ok());
        reader.events = MAX_ROWS as u64 + 1;
        reader.commands_read.set(0);
        assert!(check_store(&reader).await.is_err());
        assert_eq!(reader.commands_read.get(), 0);
        reader.events = 0;
        for index in 0..18 {
            let row = vcp_store::contract::Record {
                collection: vcp_store::contract::Collection::Projection,
                id: format!("record-{index}"),
                workspace: vcp_domain::WorkspaceId::new(),
                revision: vcp_domain::Revision::ZERO,
                references: Default::default(),
                value: serde_json::json!({"text":"x".repeat(999_000)}),
            };
            reader.state.records.insert(row.key(), row);
            assert_eq!(
                check_store(&reader).await.is_ok(),
                check(&reader.state).is_ok()
            );
        }
        assert!(check_store(&reader).await.is_err());
    }
    #[test]
    fn borrowing_counter_stops_before_visiting_later_values() {
        struct MustNotVisit;
        impl Serialize for MustNotVisit {
            fn serialize<S: serde::Serializer>(
                &self,
                _: S,
            ) -> std::result::Result<S::Ok, S::Error> {
                panic!("oversized source must stop serialization before later entries");
            }
        }
        assert!(count(&("x".repeat(1024), MustNotVisit), 128).is_err());
        assert!(count(&"x".repeat(126), 128).is_ok());
        assert!(count(&"x".repeat(127), 128).is_err());
    }
}
