// SPDX-License-Identifier: Apache-2.0
//! Bound the public preview's full-state source commitment before the shared
//! workflow canonicalizes it. Counting borrows values and never copies payloads.
use crate::{Error, Result};
use serde::Serialize;
use std::io::{self, Write};
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
fn count(value: &impl Serialize, limit: usize) -> Result<()> {
    serde_json::to_writer(Count(limit), value)
        .map_err(|_| Error::Conflict("retention source limit"))
}
pub(super) fn check(state: &State) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
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
