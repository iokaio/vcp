// SPDX-License-Identifier: Apache-2.0
//! Exact digest-v1 State encoding without an intermediate whole-State JSON tree.
//!
//! State map keys are strings (including transparent opaque IDs), ordered by
//! their underlying strings. Top-level fields below follow canonical key order;
//! each row still uses the reference recursive canonical encoder. Temporary
//! encoding storage is therefore bounded by a row, not by retained history.
//! This does not validate State, change its persisted shape or remove its cap.
use crate::{contract::State, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Write;
use vcp_protocol::canonical_bytes;

pub(crate) fn write(state: &State, mut output: impl Write) -> Result<()> {
    // Exhaustive matching makes adding a State field an explicit encoding
    // decision instead of silently omitting it from existing commitments.
    let State {
        watermark,
        records,
        events,
        commands,
        transactions,
        sequences,
    } = state;
    output.write_all(b"{\"commands\":")?;
    map(&mut output, commands.iter())?;
    output.write_all(b",\"events\":[")?;
    for (index, event) in events.iter().enumerate() {
        if index != 0 {
            output.write_all(b",")?;
        }
        output.write_all(&canonical_bytes(event)?)?;
    }
    output.write_all(b"],\"records\":")?;
    map(&mut output, records.iter())?;
    output.write_all(b",\"sequences\":")?;
    map(&mut output, sequences.iter())?;
    output.write_all(b",\"transactions\":")?;
    map(&mut output, transactions.iter())?;
    output.write_all(b",\"watermark\":")?;
    output.write_all(&canonical_bytes(watermark)?)?;
    output.write_all(b"}")?;
    Ok(())
}

fn map<'a, K: Serialize + 'a, V: Serialize + 'a>(
    output: &mut impl Write,
    entries: impl Iterator<Item = (&'a K, &'a V)>,
) -> Result<()> {
    output.write_all(b"{")?;
    for (index, (key, value)) in entries.enumerate() {
        if index != 0 {
            output.write_all(b",")?;
        }
        output.write_all(&canonical_bytes(key)?)?;
        output.write_all(b":")?;
        output.write_all(&canonical_bytes(value)?)?;
    }
    output.write_all(b"}")?;
    Ok(())
}

/// Byte-owning consumers still retain the output, but no second full JSON tree.
pub(crate) fn bytes(state: &State) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    write(state, &mut bytes)?;
    Ok(bytes)
}

pub(crate) fn digest(state: &State) -> Result<String> {
    struct Hash(Sha256);
    impl Write for Hash {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut hash = Hash(Sha256::new());
    write(state, &mut hash)?;
    Ok(format!("{:x}", hash.0.finalize()))
}

/// Compare canonical bytes directly, without allocating another complete copy.
pub(crate) fn matches(state: &State, expected: &[u8]) -> Result<bool> {
    struct Compare<'a> {
        remaining: &'a [u8],
        equal: bool,
    }
    impl Write for Compare<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if let Some(rest) = self.remaining.strip_prefix(bytes) {
                self.remaining = rest;
            } else {
                self.equal = false;
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut compare = Compare {
        remaining: expected,
        equal: true,
    };
    write(state, &mut compare)?;
    Ok(compare.equal && compare.remaining.is_empty())
}

#[cfg(test)]
#[path = "../tests/common/mod.rs"]
#[allow(unused_imports)]
mod fixtures;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::Transaction;
    use vcp_domain::{EventId, TransactionId};
    use vcp_protocol::digest_bytes;

    fn equivalent(state: &State) {
        let reference = canonical_bytes(state).unwrap();
        assert_eq!(bytes(state).unwrap(), reference);
        assert_eq!(digest(state).unwrap(), digest_bytes(&reference));
        assert!(matches(state, &reference).unwrap());
        assert!(!matches(state, &reference[..reference.len() - 1]).unwrap());
        let mut extra = reference.clone();
        extra.push(b' ');
        assert!(!matches(state, &extra).unwrap());
        let mut modified = reference;
        let index = modified.len() / 2;
        modified[index] ^= 1;
        assert!(!matches(state, &modified).unwrap());
    }

    #[test]
    fn every_accepted_rejected_and_duplicate_cut_matches_reference() {
        let mut state = State::default();
        equivalent(&state);
        let initial = fixtures::initial();
        let (next, _) = state.prepare(&initial).unwrap();
        state = next;
        equivalent(&state);
        for index in 0..40 {
            let mut event = initial.events[0].clone();
            event.id = EventId::parse(format!("event_{index}")).unwrap();
            event.data = serde_json::json!({"z": [null, true, {"é":"🦀\n\"\\", "a": -0.25}], "state":"pending", "a": 17});
            let tx = Transaction {
                id: TransactionId::parse(format!("transaction_{index}")).unwrap(),
                expected_watermark: state.watermark,
                mutations: vec![],
                events: vec![event],
                command: None,
            };
            let (next, _) = state.prepare(&tx).unwrap();
            state = next;
            equivalent(&state);
            let duplicate = state.prepare(&tx).unwrap().0;
            assert_eq!(duplicate, state);
            equivalent(&duplicate);
            let mut rejected = tx;
            rejected.id = TransactionId::new();
            assert!(state.prepare(&rejected).is_err());
            equivalent(&state);
        }
    }

    #[test]
    fn literal_json_spelling_and_escaped_map_keys_use_the_reference_encoding() {
        let (mut state, _) = State::default().prepare(&fixtures::initial()).unwrap();
        // Encoding is deliberately independent of semantic validation, matching
        // canonical_bytes. Exercise lexical JSON variants and arbitrary keys.
        state.events[0].event.data = serde_json::from_str(
            r#"{"z":1e3,"a":-0.0,"nested":{"\u0062":"\u00e9","a":"\n\t\\\""}}"#,
        )
        .unwrap();
        let record = state.records.values().next().unwrap().clone();
        for key in ["é", "z", "a\"\\\n", "🦀"] {
            state.records.insert(key.into(), record.clone());
        }
        equivalent(&state);
    }

    #[test]
    fn writer_errors_propagate_and_history_is_emitted_one_row_at_a_time() {
        struct BoundedWrite {
            calls: usize,
            largest: usize,
            fail_after: usize,
        }
        impl Write for BoundedWrite {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self.calls == self.fail_after {
                    return Err(std::io::Error::other("synthetic writer failure"));
                }
                self.calls += 1;
                self.largest = self.largest.max(bytes.len());
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let (mut state, _) = State::default().prepare(&fixtures::initial()).unwrap();
        let mut baseline = BoundedWrite {
            calls: 0,
            largest: 0,
            fail_after: usize::MAX,
        };
        write(&state, &mut baseline).unwrap();
        let event = state.events[0].clone();
        state.events.extend(std::iter::repeat_n(event, 256));
        let mut output = BoundedWrite {
            calls: 0,
            largest: 0,
            fail_after: usize::MAX,
        };
        write(&state, &mut output).unwrap();
        assert!(output.calls > state.events.len());
        assert_eq!(output.largest, baseline.largest);
        assert!(output.calls > baseline.calls + 256);
        for fail_after in [0, 1, output.calls - 1] {
            let mut failing = BoundedWrite {
                calls: 0,
                largest: 0,
                fail_after,
            };
            assert!(matches!(
                write(&state, &mut failing),
                Err(crate::Error::Io(_))
            ));
        }
    }
}
