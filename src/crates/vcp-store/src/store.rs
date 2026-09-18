// SPDX-License-Identifier: Apache-2.0
use crate::{
    artifact::{
        immutable_file, read_bounded, reject_link, ArtifactPin, Spool, DEFAULT_ARTIFACT_LIMIT,
    },
    backend::Backend,
    contract::*,
    BackendKind, Barrier, Error, Result,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
use vcp_domain::{artifact::ArtifactDescriptor, *};
use vcp_protocol::{canonical_bytes, digest_bytes};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Format {
    version: u32,
    backend: BackendKind,
}

/// Owns a real OS lock for the entire canonical root, including accounting and
/// activation. A PID/nonce is diagnostic only; stale file contents confer no lock.
pub struct Store {
    backend: Backend,
    state: State,
    commits: Vec<Commit>,
    root: PathBuf,
    kind: BackendKind,
    spool: Spool,
    poisoned: bool,
    #[cfg(feature = "qualification")]
    observer: Option<crate::backend::Observer>,
    _owner: File,
}
pub struct Snapshot {
    state: State,
    _pins: Vec<ArtifactPin>,
}
impl Snapshot {
    pub fn state(&self) -> &State {
        &self.state
    }
}
impl Store {
    pub async fn open(root: &Path, kind: BackendKind, forbidden_roots: &[PathBuf]) -> Result<Self> {
        // Check the enclosing location before creating plaintext canonical bytes.
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
        let owner_path = root.join("owner.lock");
        if owner_path.exists() {
            reject_link(&owner_path)?;
        }
        let mut owner = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&owner_path)?;
        owner
            .try_lock()
            .map_err(|_| Error::Conflict("canonical root already has an owner"))?;
        let diagnostic = canonical_bytes(
            &serde_json::json!({"pid":std::process::id(),"nonce":ControllerId::new(),"format":1}),
        )?;
        owner.seek(SeekFrom::Start(0))?;
        owner.write_all(&diagnostic)?;
        owner.set_len(diagnostic.len() as u64)?;
        owner.sync_all()?;
        let format_path = root.join("format.json");
        if format_path.exists() {
            let format: Format = serde_json::from_slice(&read_bounded(&format_path, 1024)?)?;
            if format.version != FORMAT_VERSION || format.backend != kind {
                return Err(Error::Incompatible);
            }
        } else {
            // Never reinterpret an existing backend whose format marker is missing.
            for name in ["canonical.sqlite", "canonical.frames", "spool"] {
                if root.join(name).exists() {
                    return Err(Error::Corruption("canonical format marker missing"));
                }
            }
            immutable_file(
                &format_path,
                &canonical_bytes(&Format {
                    version: FORMAT_VERSION,
                    backend: kind,
                })?,
            )?;
        }
        for name in [
            "canonical.sqlite",
            "canonical.sqlite-wal",
            "canonical.sqlite-shm",
            "canonical.frames",
        ] {
            let path = root.join(name);
            match reject_link(&path) {
                Ok(()) => {}
                // SQLite's closing worker may remove WAL/SHM between metadata
                // observations. Absence is normal; an observed link never is.
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        let (backend, state, commits) = Backend::open(&root, kind).await?;
        let spool = Spool::open(&root.join("spool"), forbidden_roots, DEFAULT_ARTIFACT_LIMIT)?;
        for record in state
            .records
            .values()
            .filter(|record| record.collection == Collection::Artifact)
        {
            spool.verify(&record.decode()?)?;
        }
        Ok(Self {
            backend,
            state,
            commits,
            root,
            kind,
            spool,
            poisoned: false,
            #[cfg(feature = "qualification")]
            observer: None,
            _owner: owner,
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn kind(&self) -> BackendKind {
        self.kind
    }
    pub fn spool(&self) -> &Spool {
        &self.spool
    }
    pub fn healthy(&self) -> bool {
        !self.poisoned
    }
    pub fn state(&self) -> &State {
        &self.state
    }
    pub(crate) fn prefix_digest(&self, watermark: Watermark) -> Result<String> {
        if watermark > self.state.watermark {
            return Err(Error::Corruption("snapshot beyond canonical watermark"));
        }
        let mut state = State::default();
        for commit in self.commits.iter().take(watermark.get() as usize) {
            state.replay(commit)?;
        }
        if state.watermark != watermark {
            return Err(Error::Corruption("snapshot prefix missing"));
        }
        Ok(digest_bytes(&canonical_bytes(&state)?))
    }
    pub async fn configuration(&mut self) -> Result<serde_json::Value> {
        self.backend.configuration().await
    }
    pub fn snapshot(&self) -> Result<Snapshot> {
        if self.poisoned {
            return Err(Error::Unavailable("reopen after indeterminate commit"));
        }
        let mut pins = Vec::new();
        for record in self
            .state
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
        {
            let descriptor: ArtifactDescriptor = record.decode()?;
            pins.push(self.spool.pin(&descriptor.spec.id)?);
        }
        Ok(Snapshot {
            state: self.state.clone(),
            _pins: pins,
        })
    }
    pub fn checkpoint(&mut self) -> Result<()> {
        if self.poisoned {
            return Err(Error::Unavailable("reopen after indeterminate commit"));
        }
        self.backend.checkpoint(&self.state)
    }
    #[cfg(feature = "qualification")]
    pub fn observe(&mut self, observer: crate::backend::Observer) {
        self.observer = Some(observer);
    }
    /// Deletes only objects absent from every canonical reference and durable pin.
    /// Historical descriptors stay canonical until an explicit retention migration;
    /// this method intentionally cannot turn retention into silent data loss.
    pub fn collect_orphan(&self, id: &ArtifactId) -> Result<bool> {
        if self.poisoned {
            return Err(Error::Unavailable("canonical state uncertain"));
        }
        let reference = key(Collection::Artifact, id.as_str());
        for record in self.state.records.values() {
            if record.key() == reference || record.required_references()?.contains(&reference) {
                return Ok(false);
            }
        }
        if self
            .state
            .events
            .iter()
            .any(|event| event.event.artifacts.contains(id))
        {
            return Ok(false);
        }
        self.spool.collect_unreferenced(id)
    }
    /// Logical conversion replays immutable transactions in a newly created root.
    /// The source owner remains held and source bytes are never edited by conversion.
    /// Historical artifact prefixes are verified after copying current retained data.
    pub async fn convert(
        &self,
        destination: &Path,
        kind: BackendKind,
        forbidden: &[PathBuf],
    ) -> Result<Store> {
        if self.poisoned {
            return Err(Error::Unavailable("canonical state uncertain"));
        }
        if destination.exists() {
            return Err(Error::Conflict("conversion requires a new root"));
        }
        let snapshot = self.snapshot()?;
        let mut target = Store::open(destination, kind, forbidden).await?;
        let mut copied = BTreeSet::new();
        for record in snapshot
            .state
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
        {
            let descriptor: ArtifactDescriptor = record.decode()?;
            if !copied.insert(descriptor.spec.id.clone()) {
                continue;
            }
            let current = self.spool.inspect(&descriptor.spec.id)?;
            // The original metadata is preserved by copying immutable files. This
            // includes interrupted captures and exact partial extents, not a new
            // successful capture invented by a conversion writer.
            let source = self.spool.root().join(current.spec.id.as_str());
            let capture = OpenOptions::new()
                .read(true)
                .open(source.join("owner.lock"))?;
            capture
                .try_lock_shared()
                .map_err(|_| Error::Conflict("quiesce artifact writers before conversion"))?;
            let destination = target.spool.root().join(current.spec.id.as_str());
            fs::create_dir(&destination)?;
            for entry in fs::read_dir(&source)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if name == "spec.json" || name == "seal.json" || name.ends_with(".chunk") {
                    immutable_file(
                        &destination.join(&name),
                        &read_bounded(&entry.path(), crate::artifact::CHUNK_BYTES)?,
                    )?;
                }
            }
            for name in ["owner.lock", "snapshot.lock"] {
                File::create(destination.join(name))?.sync_all()?;
            }
            target.spool.verify(&descriptor)?;
        }
        for commit in &self.commits {
            let receipt = target.transact(commit.transaction.clone()).await?;
            if receipt != commit.receipt {
                return Err(Error::Corruption("conversion changed receipt"));
            }
        }
        if target.state != snapshot.state {
            return Err(Error::Corruption("conversion changed logical view"));
        }
        target.checkpoint()?;
        let evidence = serde_json::json!({"version":1,"watermark":self.state.watermark,"logical_sha256":digest_bytes(&canonical_bytes(&self.state)?),"source_backend":self.kind,"target_backend":kind});
        immutable_file(
            &target.root.join("conversion.json"),
            &canonical_bytes(&evidence)?,
        )?;
        Ok(target)
    }
}
impl CanonicalStore for Store {
    fn state(&self) -> &State {
        &self.state
    }
    async fn transact(&mut self, transaction: Transaction) -> Result<Receipt> {
        if self.poisoned {
            return Err(Error::Unavailable(
                "reopen to reconcile an indeterminate commit",
            ));
        }
        let (next, commit) = self.state.prepare(&transaction)?;
        if self.state.transactions.contains_key(&transaction.id) {
            return Ok(commit.receipt);
        }
        for mutation in &transaction.mutations {
            if let Mutation::Put { record, .. } = mutation {
                if record.collection == Collection::Artifact {
                    self.spool.verify(&record.decode()?)?;
                }
            }
        }
        #[cfg(feature = "qualification")]
        let observer = self.observer.clone();
        let observe = |barrier| {
            #[cfg(feature = "qualification")]
            if let Some(observer) = &observer {
                observer(barrier);
            }
            #[cfg(not(feature = "qualification"))]
            let _ = barrier;
        };
        observe(Barrier::Prepared);
        // Poison before awaiting: cancellation after a write may have committed.
        // Reopening replays durable receipts and is the only way to clear it.
        self.poisoned = true;
        self.backend.append(&commit, &next, &observe).await?;
        self.state = next;
        self.commits.push(commit.clone());
        self.poisoned = false;
        observe(Barrier::BeforeReply);
        Ok(commit.receipt)
    }
}
