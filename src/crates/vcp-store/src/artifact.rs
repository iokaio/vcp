// SPDX-License-Identifier: Apache-2.0
//! Immutable, bounded chunks retain binary bytes independently of rendered tails.
//! Published chunks have been synced; partial temporary chunks are never complete.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use vcp_domain::{artifact::*, *};
use vcp_protocol::{canonical_bytes, digest_bytes};

pub const CHUNK_BYTES: usize = 64 * 1024;
pub const DEFAULT_ARTIFACT_LIMIT: u64 = 1024 * 1024 * 1024;
pub const MAX_CHUNKS: u64 = 65_536;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteReceipt {
    pub retained: Range,
    pub total: ByteCount,
}

pub trait ArtifactWriter {
    fn write_chunk(&mut self, bytes: &[u8]) -> Result<WriteReceipt>;
    fn finalize(&mut self) -> Result<ArtifactDescriptor>;
    fn abort(&mut self) -> Result<ArtifactDescriptor>;
}

#[derive(Clone, Debug)]
pub struct Spool {
    root: PathBuf,
    limit: u64,
}

pub(crate) fn reject_link(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(Error::Corruption("artifact link"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(Error::Corruption("artifact reparse point"));
        }
    }
    Ok(())
}

pub(crate) fn immutable_file(path: &Path, bytes: &[u8]) -> Result<()> {
    immutable_file_observed(path, bytes, &|_| Ok(()))
}

fn immutable_file_observed(
    path: &Path,
    bytes: &[u8],
    observe: &dyn Fn(&str) -> Result<()>,
) -> Result<()> {
    let temporary = path.with_extension(format!("{}.partial", TransactionId::new()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    // Publication is create-only: no existing immutable file can be replaced.
    // A hard link is atomic and fails if the destination already exists.
    observe("before_publication")?;
    fs::hard_link(&temporary, path)?;
    observe("after_publication")?;
    fs::remove_file(&temporary)?;
    sync_directory(path.parent().ok_or(Error::Corruption("missing parent"))?)?;
    Ok(())
}

pub(crate) fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        File::open(path)?.sync_all()?;
    }
    // Windows qualification covers forced-process failure, not hardware power loss.
    // File contents use FlushFileBuffers; directory durability is not overstated.
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

pub(crate) fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    reject_link(path)?;
    let mut bytes = Vec::new();
    File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(Error::Limit("serialized metadata"));
    }
    Ok(bytes)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sealed {
    version: u32,
    descriptor: ArtifactDescriptor,
}

impl Spool {
    /// The caller supplies explicitly configured sync destinations; no platform
    /// folder-name heuristic is represented as a privacy boundary.
    pub fn open(root: &Path, forbidden_roots: &[PathBuf], limit: u64) -> Result<Self> {
        if limit == 0 {
            return Err(Error::Limit("zero capture capacity"));
        }
        // Resolve the existing ancestor before creating anything under a forbidden
        // destination. Canonicalization also catches junctions in parent paths.
        let absolute = std::path::absolute(root)?;
        let ancestor = absolute
            .ancestors()
            .find(|p| p.exists())
            .ok_or(Error::Access)?
            .canonicalize()?;
        for forbidden in forbidden_roots {
            if ancestor.starts_with(forbidden.canonicalize()?) {
                return Err(Error::Access);
            }
        }
        fs::create_dir_all(root)?;
        reject_link(root)?;
        let root = root.canonicalize()?;
        for forbidden in forbidden_roots {
            let forbidden = forbidden.canonicalize()?;
            if root.starts_with(forbidden) {
                return Err(Error::Access);
            }
        }
        Ok(Self { root, limit })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    fn directory(&self, id: &ArtifactId) -> PathBuf {
        self.root.join(id.as_str())
    }
    pub fn create(&self, spec: ArtifactSpec) -> Result<LocalWriter> {
        spec.validate()?;
        let directory = self.directory(&spec.id);
        fs::create_dir(&directory)?;
        let lease = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(directory.join("owner.lock"))?;
        lease
            .try_lock()
            .map_err(|_| Error::Conflict("artifact writer"))?;
        OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(directory.join("snapshot.lock"))?
            .sync_all()?;
        immutable_file(&directory.join("spec.json"), &canonical_bytes(&spec)?)?;
        Ok(LocalWriter {
            spec,
            directory,
            limit: self.limit,
            length: 0,
            chunks: 0,
            digest: Sha256::new(),
            terminal: None,
            failed: false,
            _lease: lease,
        })
    }
    fn spec(&self, id: &ArtifactId) -> Result<ArtifactSpec> {
        let directory = self.directory(id);
        reject_link(&directory)?;
        let spec: ArtifactSpec =
            serde_json::from_slice(&read_bounded(&directory.join("spec.json"), 16384)?)?;
        spec.validate()?;
        if &spec.id != id {
            return Err(Error::Corruption("artifact identity"));
        }
        Ok(spec)
    }
    fn chunks(&self, id: &ArtifactId) -> Result<Vec<(u64, String, PathBuf)>> {
        let directory = self.directory(id);
        reject_link(&directory)?;
        let mut chunks = Vec::new();
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| Error::Corruption("artifact filename"))?;
            if !name.ends_with(".chunk") {
                continue;
            }
            if chunks.len() as u64 >= MAX_CHUNKS {
                return Err(Error::Limit("artifact chunk count"));
            }
            let (index, digest) = name
                .trim_end_matches(".chunk")
                .split_once('-')
                .ok_or(Error::Corruption("chunk identity"))?;
            let index = index
                .parse::<u64>()
                .map_err(|_| Error::Corruption("chunk index"))?;
            if digest.len() != 64
                || !digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(Error::Corruption("chunk digest"));
            }
            chunks.push((index, digest.to_owned(), entry.path()));
        }
        chunks.sort_by_key(|row| row.0);
        for (index, row) in chunks.iter().enumerate() {
            if row.0 != index as u64 {
                return Err(Error::Corruption("missing or duplicate artifact chunk"));
            }
        }
        Ok(chunks)
    }
    fn scan(
        &self,
        id: &ArtifactId,
        mut visit: impl FnMut(&[u8]) -> Result<()>,
    ) -> Result<(u64, String)> {
        let mut length = 0u64;
        let mut digest = Sha256::new();
        for (_, expected, path) in self.chunks(id)? {
            let bytes = read_bounded(&path, CHUNK_BYTES)?;
            if bytes.is_empty() || digest_bytes(&bytes) != expected {
                return Err(Error::Corruption("artifact chunk bytes"));
            }
            length = length
                .checked_add(bytes.len() as u64)
                .ok_or(Error::Limit("artifact length"))?;
            if length > self.limit {
                return Err(Error::Limit("artifact length"));
            }
            digest.update(&bytes);
            visit(&bytes)?;
        }
        Ok((length, format!("{:x}", digest.finalize())))
    }
    pub fn inspect(&self, id: &ArtifactId) -> Result<ArtifactDescriptor> {
        let mut spec = self.spec(id)?;
        let (length, sha256) = self.scan(id, |_| Ok(()))?;
        let seal = self.directory(id).join("seal.json");
        if seal.exists() {
            let sealed: Sealed = serde_json::from_slice(&read_bounded(&seal, 32768)?)?;
            sealed.descriptor.validate()?;
            let mut expected_spec = spec.clone();
            if sealed.descriptor.state == CaptureState::Aborted {
                match sealed.descriptor.spec.omissions.last() {
                    Some(omission @ (Omission::ExplicitAbort | Omission::CaptureFailure)) => {
                        expected_spec.omissions.push(omission.clone())
                    }
                    _ => return Err(Error::Corruption("aborted capture omission")),
                }
            }
            if expected_spec != sealed.descriptor.spec {
                return Err(Error::Corruption("immutable capture metadata"));
            }
            if sealed.version != 1
                || sealed.descriptor.length.get() != length
                || sealed.descriptor.sha256 != sha256
                || sealed.descriptor.spec.id != spec.id
                || sealed.descriptor.spec.scope != spec.scope
                || sealed.descriptor.spec.channel != spec.channel
                || sealed.descriptor.spec.media_type != spec.media_type
                || sealed.descriptor.spec.schema != spec.schema
                || sealed.descriptor.spec.retention != spec.retention
                || sealed.descriptor.spec.source != spec.source
                || sealed.descriptor.state == CaptureState::Pending
            {
                return Err(Error::Corruption("artifact seal"));
            }
            Ok(sealed.descriptor)
        } else {
            spec.omissions.push(Omission::UnobservedTail);
            Ok(descriptor(spec, CaptureState::Pending, length, sha256))
        }
    }
    pub fn verify(&self, expected: &ArtifactDescriptor) -> Result<()> {
        expected.validate()?;
        if expected.state == CaptureState::Purged {
            return Err(Error::Unavailable("artifact content purged"));
        }
        if expected.state != CaptureState::Pending {
            if &self.inspect(&expected.spec.id)? != expected {
                return Err(Error::Corruption("artifact descriptor changed"));
            }
        } else {
            let mut spec = self.spec(&expected.spec.id)?;
            spec.omissions.push(Omission::UnobservedTail);
            if spec != expected.spec {
                return Err(Error::Corruption("capture metadata changed"));
            }
            let mut remaining = expected.length.get();
            let mut hash = Sha256::new();
            self.scan(&expected.spec.id, |bytes| {
                let take = remaining.min(bytes.len() as u64) as usize;
                hash.update(&bytes[..take]);
                remaining -= take as u64;
                Ok(())
            })?;
            if remaining != 0 || format!("{:x}", hash.finalize()) != expected.sha256 {
                return Err(Error::Corruption("retained prefix changed"));
            }
        }
        Ok(())
    }
    /// Streams full retained bytes, including invalid UTF-8. Rendering limits do
    /// not alter the spool. Access must be checked against current canonical scope.
    pub fn read(&self, expected: &ArtifactDescriptor, mut sink: impl Write) -> Result<()> {
        self.verify(expected)?;
        let mut remaining = expected.length.get();
        self.scan(&expected.spec.id, |chunk| {
            let take = remaining.min(chunk.len() as u64) as usize;
            sink.write_all(&chunk[..take])?;
            remaining -= take as u64;
            Ok(())
        })?;
        Ok(())
    }
    pub fn unfinished(&self) -> Result<Vec<ArtifactDescriptor>> {
        let mut result = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                return Err(Error::Corruption("unexpected spool entry"));
            }
            let id = ArtifactId::parse(entry.file_name().to_string_lossy().into_owned())?;
            let descriptor = self.inspect(&id)?;
            if descriptor.state != CaptureState::Complete {
                result.push(descriptor);
            }
        }
        Ok(result)
    }
    pub fn pin(&self, id: &ArtifactId) -> Result<ArtifactPin> {
        self.spec(id)?;
        let file = OpenOptions::new()
            .read(true)
            .open(self.directory(id).join("snapshot.lock"))?;
        file.try_lock_shared()
            .map_err(|_| Error::Conflict("artifact garbage collection in progress"))?;
        Ok(ArtifactPin { _file: file })
    }
    /// Caller must hold the canonical owner lock and prove no current record or
    /// durable snapshot pin references this object. Both capture and reader locks
    /// are acquired before deletion. IDs cannot contain path separators.
    pub(crate) fn collect_unreferenced(&self, id: &ArtifactId) -> Result<bool> {
        self.spec(id)?;
        let directory = self.directory(id);
        let owner = OpenOptions::new()
            .read(true)
            .write(true)
            .open(directory.join("owner.lock"))?;
        if owner.try_lock().is_err() {
            return Ok(false);
        }
        let readers = OpenOptions::new()
            .read(true)
            .write(true)
            .open(directory.join("snapshot.lock"))?;
        if readers.try_lock().is_err() {
            return Ok(false);
        }
        let resolved = directory.canonicalize()?;
        if resolved.parent() != Some(self.root.as_path()) {
            return Err(Error::Access);
        }
        // No recursive traversal: a spool object has only bounded ordinary files.
        let entries = fs::read_dir(&resolved)?.collect::<std::io::Result<Vec<_>>>()?;
        for entry in &entries {
            reject_link(&entry.path())?;
            if !entry.file_type()?.is_file() {
                return Err(Error::Corruption("artifact directory contents"));
            }
        }
        for entry in entries {
            fs::remove_file(entry.path())?;
        }
        fs::remove_dir(&resolved)?;
        sync_directory(&self.root)?;
        Ok(true)
    }
}

pub struct ArtifactPin {
    _file: File,
}
pub struct LocalWriter {
    spec: ArtifactSpec,
    directory: PathBuf,
    limit: u64,
    length: u64,
    chunks: u64,
    digest: Sha256,
    terminal: Option<ArtifactDescriptor>,
    failed: bool,
    _lease: File,
}
fn descriptor(
    spec: ArtifactSpec,
    state: CaptureState,
    length: u64,
    sha256: String,
) -> ArtifactDescriptor {
    ArtifactDescriptor {
        spec,
        state,
        length: ByteCount::new(length),
        sha256,
        retained: vec![Range {
            start: ByteCount::ZERO,
            end: ByteCount::new(length),
        }],
    }
}
impl LocalWriter {
    fn finish(&mut self, state: CaptureState) -> Result<ArtifactDescriptor> {
        self.finish_observed(state, &|_| Ok(()))
    }

    /// Qualification-only fault observer at the immutable seal publication boundary.
    /// An injected I/O error follows the same writer fencing path as a write failure.
    #[cfg(feature = "qualification")]
    pub fn finalize_observed(
        &mut self,
        observe: &dyn Fn(&str) -> Result<()>,
    ) -> Result<ArtifactDescriptor> {
        self.finish_observed(CaptureState::Complete, observe)
    }

    fn finish_observed(
        &mut self,
        state: CaptureState,
        observe: &dyn Fn(&str) -> Result<()>,
    ) -> Result<ArtifactDescriptor> {
        if let Some(terminal) = &self.terminal {
            if terminal.state != state {
                return Err(Error::Conflict("artifact is already terminal"));
            }
            return Ok(terminal.clone());
        }
        if self.failed && state == CaptureState::Complete {
            return Err(Error::Unavailable(
                "capture failed; abort or reopen for inspection",
            ));
        }
        let mut spec = self.spec.clone();
        if state == CaptureState::Aborted {
            spec.omissions.push(if self.failed {
                Omission::CaptureFailure
            } else {
                Omission::ExplicitAbort
            });
        }
        // A publication may succeed before a later flush/cleanup reports failure.
        // Derive the abort extent from durable chunks, never stale in-memory length.
        let spool = Spool {
            root: self
                .directory
                .parent()
                .ok_or(Error::Corruption("spool parent"))?
                .to_owned(),
            limit: self.limit,
        };
        let (length, hash) = spool.scan(&self.spec.id, |_| Ok(()))?;
        if state == CaptureState::Complete
            && (length != self.length || hash != format!("{:x}", self.digest.clone().finalize()))
        {
            self.failed = true;
            return Err(Error::Corruption("writer extent"));
        }
        let result = descriptor(spec, state, length, hash);
        let bytes = canonical_bytes(&Sealed {
            version: 1,
            descriptor: result.clone(),
        })?;
        if let Err(error) =
            immutable_file_observed(&self.directory.join("seal.json"), &bytes, observe)
        {
            self.failed = true;
            return Err(error);
        }
        self.terminal = Some(result.clone());
        Ok(result)
    }
}
impl ArtifactWriter for LocalWriter {
    fn write_chunk(&mut self, bytes: &[u8]) -> Result<WriteReceipt> {
        if self.failed || self.terminal.is_some() {
            return Err(Error::Unavailable("capture is closed or failed"));
        }
        if bytes.len() > CHUNK_BYTES {
            return Err(Error::Limit("capture chunk exceeds 64 KiB"));
        }
        let end = self
            .length
            .checked_add(bytes.len() as u64)
            .ok_or(Error::Limit("capture length"))?;
        if end > self.limit || (!bytes.is_empty() && self.chunks >= MAX_CHUNKS) {
            self.failed = true;
            return Err(Error::Limit(
                "capture capacity reached; dependent work must pause",
            ));
        }
        let start = self.length;
        if !bytes.is_empty() {
            let path =
                self.directory
                    .join(format!("{:020}-{}.chunk", self.chunks, digest_bytes(bytes)));
            if let Err(error) = immutable_file(&path, bytes) {
                self.failed = true;
                return Err(error);
            }
            self.digest.update(bytes);
            self.length = end;
            self.chunks += 1;
        }
        Ok(WriteReceipt {
            retained: Range {
                start: ByteCount::new(start),
                end: ByteCount::new(end),
            },
            total: ByteCount::new(end),
        })
    }
    fn finalize(&mut self) -> Result<ArtifactDescriptor> {
        self.finish(CaptureState::Complete)
    }
    fn abort(&mut self) -> Result<ArtifactDescriptor> {
        self.finish(CaptureState::Aborted)
    }
}

/// Reference writer for shared capture conformance. It has the same explicit
/// completion/abort semantics; production capture always uses the local spool.
pub struct MemoryWriter {
    spec: ArtifactSpec,
    bytes: Vec<u8>,
    limit: u64,
    chunks: u64,
    terminal: Option<ArtifactDescriptor>,
    failed: bool,
}
impl MemoryWriter {
    pub fn new(spec: ArtifactSpec, limit: u64) -> Result<Self> {
        spec.validate()?;
        if limit == 0 {
            return Err(Error::Limit("zero capture capacity"));
        }
        Ok(Self {
            spec,
            bytes: Vec::new(),
            limit,
            chunks: 0,
            terminal: None,
            failed: false,
        })
    }
    pub fn retained(&self) -> &[u8] {
        &self.bytes
    }
    fn finish(&mut self, state: CaptureState) -> Result<ArtifactDescriptor> {
        if let Some(terminal) = &self.terminal {
            return if terminal.state == state {
                Ok(terminal.clone())
            } else {
                Err(Error::Conflict("artifact terminal"))
            };
        }
        if self.failed && state == CaptureState::Complete {
            return Err(Error::Unavailable("capture failed"));
        }
        let mut spec = self.spec.clone();
        if state == CaptureState::Aborted {
            spec.omissions.push(if self.failed {
                Omission::CaptureFailure
            } else {
                Omission::ExplicitAbort
            });
        }
        let result = descriptor(
            spec,
            state,
            self.bytes.len() as u64,
            digest_bytes(&self.bytes),
        );
        self.terminal = Some(result.clone());
        Ok(result)
    }
}
impl ArtifactWriter for MemoryWriter {
    fn write_chunk(&mut self, bytes: &[u8]) -> Result<WriteReceipt> {
        if self.failed || self.terminal.is_some() {
            return Err(Error::Unavailable("capture closed"));
        }
        if bytes.len() > CHUNK_BYTES {
            return Err(Error::Limit("chunk"));
        }
        let start = self.bytes.len() as u64;
        let end = start
            .checked_add(bytes.len() as u64)
            .ok_or(Error::Limit("capture length"))?;
        if end > self.limit || (!bytes.is_empty() && self.chunks >= MAX_CHUNKS) {
            self.failed = true;
            return Err(Error::Limit("capture capacity"));
        }
        self.bytes.extend_from_slice(bytes);
        if !bytes.is_empty() {
            self.chunks += 1;
        }
        Ok(WriteReceipt {
            retained: Range {
                start: ByteCount::new(start),
                end: ByteCount::new(end),
            },
            total: ByteCount::new(end),
        })
    }
    fn finalize(&mut self) -> Result<ArtifactDescriptor> {
        self.finish(CaptureState::Complete)
    }
    fn abort(&mut self) -> Result<ArtifactDescriptor> {
        self.finish(CaptureState::Aborted)
    }
}
