// SPDX-License-Identifier: Apache-2.0
//! Private P0 checkpoint journal. One locked file, checksum-chained frames and
//! sync-before-acknowledgement. This is not the selected canonical store format.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};

const MAX_FRAME: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Thread {
    pub id: codex_protocol::ThreadId,
    pub parent: Option<codex_protocol::ThreadId>,
    pub held: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Checkpoint {
    pub format: u32,
    pub workspace: String,
    pub revision: u64,
    pub threads: Vec<Thread>,
    pub work: Vec<Work>,
    pub commands: Vec<super::control::CommandRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Work {
    pub id: u64,
    pub thread: codex_protocol::ThreadId,
    pub kind: String,
    pub label: String,
    pub receipt: Option<String>,
}

pub(crate) struct Journal {
    file: File,
    digest: [u8; 32],
    failed: bool,
}

impl Journal {
    pub fn open(path: &Path, workspace: &str) -> io::Result<(Self, Option<Checkpoint>)> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        file.try_lock().map_err(io::Error::other)?;
        let mut last = None;
        let mut digest = [0; 32];
        let mut valid_end = 0;
        loop {
            let start = file.stream_position()?;
            let remaining = file.metadata()?.len().saturating_sub(start);
            if remaining == 0 {
                break;
            }
            // Only an incomplete final append is recoverable. Complete malformed
            // frames fail closed, including an impossible length or digest.
            if remaining < 8 {
                file.set_len(valid_end)?;
                file.sync_all()?;
                break;
            }
            let mut length = [0; 4];
            file.read_exact(&mut length)?;
            let mut inverse = [0; 4];
            file.read_exact(&mut inverse)?;
            let length = u32::from_le_bytes(length);
            if length != !u32::from_le_bytes(inverse) {
                return Err(invalid("checkpoint frame header"));
            }
            let length = length as usize;
            if length == 0 || length > MAX_FRAME {
                return Err(invalid("checkpoint frame length"));
            }
            if remaining < 8 + length as u64 + 32 {
                file.set_len(valid_end)?;
                file.sync_all()?;
                break;
            }
            let mut payload = vec![0; length];
            let mut actual = [0; 32];
            file.read_exact(&mut payload)?;
            file.read_exact(&mut actual)?;
            if frame_digest(&digest, &payload) != actual {
                return Err(invalid("checkpoint checksum"));
            }
            let checkpoint: Checkpoint = serde_json::from_slice(&payload).map_err(invalid)?;
            if checkpoint.format != 1 || checkpoint.workspace != workspace {
                return Err(invalid("checkpoint format or workspace mismatch"));
            }
            if last
                .as_ref()
                .is_some_and(|old: &Checkpoint| checkpoint.revision <= old.revision)
            {
                return Err(invalid("checkpoint revision did not advance"));
            }
            validate(&checkpoint)?;
            last = Some(checkpoint);
            digest = actual;
            valid_end = file.stream_position()?;
        }
        file.seek(SeekFrom::End(0))?;
        Ok((
            Self {
                file,
                digest,
                failed: false,
            },
            last,
        ))
    }

    pub fn append(&mut self, checkpoint: &Checkpoint) -> io::Result<()> {
        if self.failed {
            return Err(invalid("checkpoint writer requires recovery"));
        }
        validate(checkpoint)?;
        let payload = serde_json::to_vec(checkpoint).map_err(invalid)?;
        if payload.len() > MAX_FRAME {
            return Err(invalid("checkpoint exceeds prototype limit"));
        }
        let digest = frame_digest(&self.digest, &payload);
        // Once any write is attempted, never append after failure in this owner.
        self.failed = true;
        self.file.write_all(&(payload.len() as u32).to_le_bytes())?;
        self.file
            .write_all(&(!(payload.len() as u32)).to_le_bytes())?;
        self.file.write_all(&payload)?;
        self.file.write_all(&digest)?;
        self.file.sync_all()?;
        self.digest = digest;
        self.failed = false;
        Ok(())
    }
}

fn frame_digest(previous: &[u8; 32], payload: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"vcp-p0-lifecycle-v1\0");
    hash.update(previous);
    hash.update((payload.len() as u32).to_le_bytes());
    hash.update(payload);
    hash.finalize().into()
}

fn invalid(message: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_string())
}

fn validate(checkpoint: &Checkpoint) -> io::Result<()> {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    let mut roots = 0;
    // Parents must precede their children, proving a single acyclic tree.
    for thread in &checkpoint.threads {
        if thread.parent.is_some_and(|id| !seen.contains(&id)) || !seen.insert(thread.id) {
            return Err(invalid("checkpoint lineage"));
        }
        if thread.parent.is_none() {
            roots += 1;
        }
    }
    if roots > 1 {
        return Err(invalid("multiple checkpoint roots"));
    }
    let mut work_ids = HashSet::new();
    for work in &checkpoint.work {
        if !seen.contains(&work.thread)
            || work.id > checkpoint.revision
            || !work_ids.insert(work.id)
        {
            return Err(invalid("checkpoint work identity"));
        }
    }
    let mut command_ids = HashSet::new();
    for command in &checkpoint.commands {
        if !seen.contains(&command.thread)
            || command.id.is_empty()
            || command.id.len() > 256
            || !command_ids.insert(&command.id)
        {
            return Err(invalid("checkpoint command identity"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(revision: u64) -> Checkpoint {
        Checkpoint {
            format: 1,
            workspace: "fixture".into(),
            revision,
            threads: vec![],
            work: vec![],
            commands: vec![],
        }
    }

    #[test]
    fn exclusive_writer_and_workspace_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal");
        let (mut first, _) = Journal::open(&path, "fixture").unwrap();
        first.append(&snapshot(1)).unwrap();
        assert!(Journal::open(&path, "fixture").is_err());
        drop(first);
        assert!(Journal::open(&path, "foreign").is_err());
        assert_eq!(
            Journal::open(&path, "fixture").unwrap().1,
            Some(snapshot(1))
        );
    }

    #[test]
    fn every_torn_tail_recovers_only_complete_frames() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("original");
        let (mut journal, _) = Journal::open(&original, "fixture").unwrap();
        journal.append(&snapshot(1)).unwrap();
        let first_end = journal.file.metadata().unwrap().len() as usize;
        journal.append(&snapshot(2)).unwrap();
        drop(journal);
        let bytes = std::fs::read(original).unwrap();
        for end in first_end..bytes.len() {
            let path = dir.path().join(format!("torn-{end}"));
            std::fs::write(&path, &bytes[..end]).unwrap();
            let (mut recovered, last) = Journal::open(&path, "fixture").unwrap();
            assert_eq!(last, Some(snapshot(1)), "tail {end}");
            recovered.append(&snapshot(3)).unwrap();
            drop(recovered);
            assert_eq!(
                Journal::open(&path, "fixture").unwrap().1,
                Some(snapshot(3))
            );
        }
    }

    #[test]
    fn complete_corruption_is_not_silently_discarded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal");
        let (mut journal, _) = Journal::open(&path, "fixture").unwrap();
        journal.append(&snapshot(1)).unwrap();
        drop(journal);
        let bytes = std::fs::read(&path).unwrap();
        for offset in [0, 4, 12, bytes.len() - 1] {
            let mut changed = bytes.clone();
            changed[offset] ^= 1;
            std::fs::write(&path, changed).unwrap();
            assert!(Journal::open(&path, "fixture").is_err());
        }
    }
}
