// SPDX-License-Identifier: Apache-2.0
//! Retained proposed changes, never synthesized from current disk state.
use crate::{query::QueryError, Access, Engine};
use serde::{de::DeserializeOwned, Deserialize};
use std::io::{self, Read, Write};
use vcp_domain::{
    artifact::{ArtifactDescriptor, CaptureState, Channel},
    effect::{Effect, EffectState},
    ids::*,
    public_diff::{self, Content, Document, FileChange},
    retention::RetentionMask,
    revision::*,
    workspace::Workspace,
};
use vcp_protocol::{
    event::{EventEnvelope, EventKind},
    methods::{self, Call},
};
use vcp_store::{contract::Collection, Store};

// Legacy private Prepared evidence uses decimal byte arrays. Its bound is
// intentionally separate from the smaller public base64 document.
const MAX_PREPARED_BYTES: u64 = 304 * 1024 * 1024;
fn byte_limit(descriptor: &ArtifactDescriptor) -> u64 {
    if descriptor.spec.schema == public_diff::SCHEMA {
        public_diff::MAX_DOCUMENT_BYTES
    } else {
        MAX_PREPARED_BYTES
    }
}

impl Engine<Store> {
    pub fn public_diff(
        &self,
        access: &Access,
        request: &methods::DiffRead,
    ) -> Result<methods::ArtifactRange, QueryError> {
        let task = self.read_task(access, &request.scope, &request.task)?;
        Call::DiffRead(request.clone())
            .validate()
            .map_err(|_| QueryError::Limit)?;
        if task.redaction.is_some() {
            return Err(QueryError::Unavailable);
        }
        let state = self.store().state();
        let effect: Effect = state
            .record(
                Collection::Effect,
                request.change.as_str(),
                &access.workspace,
            )
            .map_err(|_| QueryError::Unavailable)?
            .decode()
            .map_err(|_| QueryError::InvalidData)?;
        if effect.id.as_str() != request.change.as_str()
            || effect.scope != task.scope
            || effect.redaction.is_some()
        {
            return Err(QueryError::Unavailable);
        }
        let workspace: Workspace = state
            .record(
                Collection::Workspace,
                access.workspace.as_str(),
                &access.workspace,
            )
            .map_err(|_| QueryError::Access)?
            .decode()
            .map_err(|_| QueryError::InvalidData)?;
        let masks = state
            .records
            .values()
            .filter(|r| r.collection == Collection::Tombstone && r.workspace == access.workspace)
            .map(|r| r.decode::<RetentionMask>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| QueryError::InvalidData)?;
        for mask in &masks {
            mask.validate().map_err(|_| QueryError::InvalidData)?;
            if mask.workspace != access.workspace || mask.deletion > workspace.deletion {
                return Err(QueryError::InvalidData);
            }
        }
        // Later effect outcomes replace observed_changes. The exact initial
        // Validated fact permanently identifies what was proposed, not applied.
        let mut selected = None;
        for event in state.events.iter().filter(|e| {
            e.event.kind == EventKind::EffectTransition
                && e.event.workspace == task.scope.workspace
                && e.event.session == task.scope.session
                && e.event.task.as_ref() == Some(&task.scope.task)
        }) {
            if event.event.data["schema_version"] != 1 {
                continue;
            }
            for fact in event.event.data["facts"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|f| {
                    f["collection"] == "effect"
                        && f["id"].as_str() == Some(effect.id.as_str())
                        && f["revision"] == "1"
                })
            {
                let original: Effect = serde_json::from_value(fact["value"].clone())
                    .map_err(|_| QueryError::InvalidData)?;
                if original.id != effect.id
                    || original.scope != effect.scope
                    || original.operation_digest != effect.operation_digest
                    || original.revision != Revision::new(1)
                    || original.state != EffectState::Validated
                    || original.cause != event.event.id
                    || original.redaction.is_some()
                    || event_hidden(event, &masks)
                {
                    return Err(QueryError::Unavailable);
                }
                if selected.is_some() {
                    return Err(QueryError::Unavailable);
                }
                selected = Some((original, event));
            }
        }
        let (original, anchor) = selected.ok_or(QueryError::Unavailable)?;
        let mut documents = Vec::new();
        for id in &original.observed_changes {
            let descriptor: ArtifactDescriptor = state
                .record(Collection::Artifact, id.as_str(), &access.workspace)
                .map_err(|_| QueryError::Unavailable)?
                .decode()
                .map_err(|_| QueryError::InvalidData)?;
            if descriptor.spec.schema == public_diff::SCHEMA {
                self.diff_descriptor(&descriptor, &task.scope, &masks)?;
                if !anchor.event.artifacts.contains(id) {
                    return Err(QueryError::Unavailable);
                }
                documents.push(descriptor);
            }
        }
        if documents.len() != 1 {
            return Err(QueryError::Unavailable);
        }
        let descriptor = &documents[0];
        let document: Document = decode(self.store(), descriptor)?;
        document.validate().map_err(|_| QueryError::InvalidData)?;
        if document.scope != effect.scope
            || document.change != effect.id
            || document.operation_digest != effect.operation_digest
            || !original.observed_changes.contains(&document.prepared)
            || !anchor.event.artifacts.contains(&document.prepared)
        {
            return Err(QueryError::Unavailable);
        }
        let prepared: ArtifactDescriptor = state
            .record(
                Collection::Artifact,
                document.prepared.as_str(),
                &access.workspace,
            )
            .map_err(|_| QueryError::Unavailable)?
            .decode()
            .map_err(|_| QueryError::InvalidData)?;
        self.diff_descriptor(&prepared, &task.scope, &masks)?;
        if prepared.spec.schema != "vcp-prepared-tool-v2" {
            return Err(QueryError::Unavailable);
        }
        let source: PreparedDocument = decode(self.store(), &prepared)?;
        if source.schema != "vcp-prepared-tool/2"
            || source.prepared.schema_version != 1
            || source.prepared.operation.scope != effect.scope
        {
            return Err(QueryError::Unavailable);
        }
        let operation = vcp_policy::Prepared::new(source.prepared.operation)
            .map_err(|_| QueryError::InvalidData)?;
        if operation.digest() != effect.operation_digest
            || source.prepared.changes.len() != document.files.len()
        {
            return Err(QueryError::Unavailable);
        }
        for (source, public) in source.prepared.changes.into_iter().zip(&document.files) {
            if source.project()? != *public {
                return Err(QueryError::Unavailable);
            }
        }
        self.public_artifact(
            access,
            &methods::ArtifactRead {
                scope: request.scope.clone(),
                task: request.task.clone(),
                artifact: descriptor
                    .spec
                    .id
                    .to_string()
                    .try_into()
                    .map_err(|_| QueryError::InvalidData)?,
                offset: request.offset.clone(),
                length: request.length,
            },
        )
    }
    fn diff_descriptor(
        &self,
        descriptor: &ArtifactDescriptor,
        scope: &vcp_domain::workspace::Scope,
        masks: &[RetentionMask],
    ) -> Result<(), QueryError> {
        descriptor.validate().map_err(|_| QueryError::InvalidData)?;
        if &descriptor.spec.scope != scope
            || descriptor.state != CaptureState::Complete
            || descriptor.spec.channel != Channel::Evidence
            || descriptor.spec.omissions.iter().any(|omission| {
                !matches!(
                    omission,
                    vcp_domain::artifact::Omission::AuthenticationHeaders
                        | vcp_domain::artifact::Omission::RecoveryMaterial
                )
            })
            || descriptor.length.get() > byte_limit(descriptor)
            || masks
                .iter()
                .any(|mask| mask.artifacts.contains(&descriptor.spec.id))
        {
            return Err(QueryError::Unavailable);
        }
        Ok(())
    }
}
fn event_hidden(event: &EventEnvelope, masks: &[RetentionMask]) -> bool {
    event.redaction.is_some()
        || masks.iter().any(|mask| {
            mask.session == event.event.session
                && event.sequence >= mask.first
                && event.sequence <= mask.last
        })
}

// Decode only explicit source fields. Private host configuration is discarded;
// none of it becomes a public result or an interpreted authority grant.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreparedDocument {
    schema: String,
    #[serde(rename = "controller")]
    _controller: serde::de::IgnoredAny,
    #[serde(rename = "owner")]
    _owner: serde::de::IgnoredAny,
    #[serde(rename = "host_tool_denials")]
    _denials: serde::de::IgnoredAny,
    #[serde(rename = "skills_revision")]
    _skills: serde::de::IgnoredAny,
    prepared: PreparedEvidence,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreparedEvidence {
    schema_version: u32,
    operation: vcp_domain::policy::Operation,
    #[serde(rename = "result")]
    _result: serde::de::IgnoredAny,
    #[serde(deserialize_with = "prepared_changes")]
    changes: Vec<PreparedChange>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Before {
    #[serde(rename = "root")]
    _root: RootId,
    #[serde(rename = "binding")]
    _binding: Revision,
    path: String,
    #[serde(rename = "native_identity")]
    _native_identity: serde::de::IgnoredAny,
    sha256: String,
    bytes: ByteCount,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreparedChange {
    path: String,
    before: Option<Before>,
    #[serde(default, deserialize_with = "optional_bytes")]
    before_bytes: Option<Vec<u8>>,
    #[serde(default, deserialize_with = "optional_bytes")]
    after: Option<Vec<u8>>,
    rename_to: Option<String>,
    #[serde(rename = "probes")]
    _probes: serde::de::IgnoredAny,
}
impl PreparedChange {
    fn project(self) -> Result<FileChange, QueryError> {
        let before = match (self.before, self.before_bytes) {
            (None, None) => None,
            (Some(proof), Some(bytes))
                if proof.path == self.path
                    && proof.bytes.get() == bytes.len() as u64
                    && proof.sha256 == vcp_protocol::digest_bytes(&bytes) =>
            {
                Some(Content {
                    sha256: proof.sha256,
                    bytes,
                })
            }
            _ => return Err(QueryError::InvalidData),
        };
        let result = FileChange {
            path: self.path,
            rename_to: self.rename_to,
            before,
            after: self.after.map(|bytes| Content {
                sha256: vcp_protocol::digest_bytes(&bytes),
                bytes,
            }),
        };
        result.validate().map_err(|_| QueryError::InvalidData)?;
        Ok(result)
    }
}

#[derive(Deserialize)]
#[serde(transparent)]
struct FileBytes(#[serde(deserialize_with = "public_diff::deserialize_file_bytes")] Vec<u8>);
fn optional_bytes<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> Result<Option<Vec<u8>>, D::Error> {
    Ok(Option::<FileBytes>::deserialize(decoder)?.map(|bytes| bytes.0))
}
fn prepared_changes<'de, D: serde::Deserializer<'de>>(
    decoder: D,
) -> Result<Vec<PreparedChange>, D::Error> {
    struct Changes;
    impl<'de> serde::de::Visitor<'de> for Changes {
        type Value = Vec<PreparedChange>;
        fn expecting(&self, out: &mut std::fmt::Formatter) -> std::fmt::Result {
            out.write_str("bounded prepared changes")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            let mut files = Vec::new();
            let mut after = 0usize;
            while let Some(file) = seq.next_element::<PreparedChange>()? {
                if files.len() == public_diff::MAX_FILES {
                    return Err(serde::de::Error::custom("file count ceiling"));
                }
                after += file.after.as_ref().map_or(0, Vec::len);
                if after > public_diff::MAX_AFTER_BYTES {
                    return Err(serde::de::Error::custom("proposed byte ceiling"));
                }
                files.push(file);
            }
            Ok(files)
        }
    }
    decoder.deserialize_seq(Changes)
}

// Spool has a push interface. A two-chunk bounded pipe allows serde's reader
// decoder to avoid retaining the much larger decimal-byte JSON representation.
// The full spool hash must pass before any public range is returned.
fn decode<T: DeserializeOwned>(
    store: &Store,
    descriptor: &ArtifactDescriptor,
) -> Result<T, QueryError> {
    if descriptor.length.get() > byte_limit(descriptor) {
        return Err(QueryError::Limit);
    }
    let spool = store.spool().clone();
    let expected = descriptor.clone();
    std::thread::scope(|scope| {
        let (sender, receiver) = std::sync::mpsc::sync_channel::<Vec<u8>>(2);
        let producer = std::thread::Builder::new()
            .spawn_scoped(scope, move || spool.read(&expected, PipeWriter(sender)))
            .map_err(|_| QueryError::Unavailable)?;
        let pipe = PipeReader {
            receiver,
            current: io::Cursor::new(Vec::new()),
        };
        let guarded = JsonStrings {
            inner: pipe,
            quoted: false,
            escaped: false,
            length: 0,
        };
        let mut reader = io::BufReader::with_capacity(vcp_store::artifact::CHUNK_BYTES, guarded);
        let decoded = serde_json::from_reader(&mut reader).map_err(|_| QueryError::InvalidData);
        drop(reader);
        producer
            .join()
            .map_err(|_| QueryError::Unavailable)?
            .map_err(|_| QueryError::Unavailable)?;
        decoded
    })
}
// Bound parser scratch allocation before serde sees string bytes, including
// ignored private fields. This conservative raw-token bound permits canonical
// public base64, escaped paths and the private operation's bounded arguments.
const MAX_STRING_TOKEN: usize = public_diff::MAX_FILE_BYTES.div_ceil(3) * 4;
struct JsonStrings<R> {
    inner: R,
    quoted: bool,
    escaped: bool,
    length: usize,
}
impl<R: Read> Read for JsonStrings<R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(bytes)?;
        for &byte in &bytes[..n] {
            if !self.quoted {
                if byte == b'"' {
                    self.quoted = true;
                    self.length = 0;
                }
            } else if !self.escaped && byte == b'"' {
                self.quoted = false;
            } else {
                self.length += 1;
                if self.length > MAX_STRING_TOKEN {
                    return Err(io::Error::other("diff JSON string ceiling"));
                }
                if self.escaped {
                    self.escaped = false;
                } else if byte == b'\\' {
                    self.escaped = true;
                }
            }
        }
        Ok(n)
    }
}
struct PipeWriter(std::sync::mpsc::SyncSender<Vec<u8>>);
impl Write for PipeWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        for chunk in bytes.chunks(vcp_store::artifact::CHUNK_BYTES) {
            self.0
                .send(chunk.to_vec())
                .map_err(|_| io::Error::other("diff decoder stopped"))?;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct PipeReader {
    receiver: std::sync::mpsc::Receiver<Vec<u8>>,
    current: io::Cursor<Vec<u8>>,
}
impl Read for PipeReader {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        loop {
            let n = self.current.read(bytes)?;
            if n != 0 {
                return Ok(n);
            }
            match self.receiver.recv() {
                Ok(bytes) => self.current = io::Cursor::new(bytes),
                Err(_) => return Ok(0),
            }
        }
    }
}

#[cfg(test)]
#[path = "public_diff/tests.rs"]
mod tests;
