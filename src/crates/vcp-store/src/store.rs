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
#[path = "snapshot_pin.rs"]
pub(crate) mod snapshot_pin;

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
    base: State,
    prefixes: Vec<crate::replay_base::PrefixCommitment>,
    anchor: PathBuf,
    anchors: Vec<File>,
    commits: Vec<Commit>,
    root: PathBuf,
    kind: BackendKind,
    spool: Spool,
    artifact_limit: u64,
    poisoned: bool,
    #[cfg(feature = "qualification")]
    observer: Option<crate::backend::Observer>,
    _owner: File,
}
pub struct Snapshot {
    state: State,
    _pins: Vec<ArtifactPin>,
    _root_pin: File,
}
impl Snapshot {
    pub fn state(&self) -> &State {
        &self.state
    }
}
impl Store {
    /// Finish backend shutdown before releasing the canonical owner lock.
    /// Callers requiring an immediate reopen must await this method: dropping a
    /// SQLite connection alone does not wait for its native worker to terminate.
    pub async fn close(self) -> Result<()> {
        let Self {
            backend, _owner, ..
        } = self;
        let result = backend.close().await;
        drop(_owner);
        result
    }

    pub async fn open(root: &Path, kind: BackendKind, forbidden_roots: &[PathBuf]) -> Result<Self> {
        Self::open_with_artifact_limit(root, kind, forbidden_roots, DEFAULT_ARTIFACT_LIMIT).await
    }

    pub async fn open_with_artifact_limit(
        root: &Path,
        kind: BackendKind,
        forbidden_roots: &[PathBuf],
        artifact_limit: u64,
    ) -> Result<Self> {
        if artifact_limit == 0 || artifact_limit > DEFAULT_ARTIFACT_LIMIT {
            return Err(Error::Limit("artifact capacity"));
        }
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
        if let Some((destination, activation)) = crate::rewrite::resolve(&root)? {
            if activation.backend != kind {
                return Err(Error::Incompatible);
            }
            let mut active = Box::pin(Self::open_with_artifact_limit(
                &destination,
                kind,
                forbidden_roots,
                artifact_limit,
            ))
            .await?;
            let activated_base = crate::replay_base::ReplayBase::load(&destination)?
                .ok_or(Error::Corruption("activated replay base missing"))?;
            if activated_base.source_digest != activation.source_digest
                || activated_base.state.watermark != activation.watermark
                || active.prefix_digest(activation.watermark)? != activation.state_digest
            {
                return Err(Error::Corruption("rewrite activation state"));
            }
            active.anchor = root;
            active.anchors.push(owner);
            return Ok(active);
        }
        let format_path = root.join("format.json");
        if format_path.exists() {
            let format: Format = serde_json::from_slice(&read_bounded(&format_path, 1024)?)?;
            let expected_version = if root.join("replay-base.json").exists() {
                2
            } else {
                FORMAT_VERSION
            };
            if format.version != expected_version || format.backend != kind {
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
                    version: if root.join("replay-base.json").exists() {
                        2
                    } else {
                        FORMAT_VERSION
                    },
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
        let loaded_base = crate::replay_base::ReplayBase::load(&root)?;
        let prefixes = loaded_base
            .as_ref()
            .map(|b| b.prefixes.clone())
            .unwrap_or_default();
        let base = loaded_base.map(|b| b.state).unwrap_or_default();
        let (backend, state, commits) = Backend::open(&root, kind).await?;
        let spool = Spool::open(&root.join("spool"), forbidden_roots, artifact_limit)?;
        for record in state
            .records
            .values()
            .filter(|record| record.collection == Collection::Artifact)
        {
            let descriptor: ArtifactDescriptor = record.decode()?;
            if descriptor.state != vcp_domain::artifact::CaptureState::Purged {
                spool.verify(&descriptor)?;
            }
        }
        Ok(Self {
            backend,
            state,
            base,
            prefixes,
            anchor: root.clone(),
            anchors: Vec::new(),
            commits,
            root,
            kind,
            spool,
            artifact_limit,
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
        if watermark < self.base.watermark {
            return Err(Error::Unavailable("history precedes retained replay base"));
        }
        let mut state = self.base.clone();
        for commit in self
            .commits
            .iter()
            .take_while(|c| c.receipt.watermark <= watermark)
        {
            state.replay(commit)?;
        }
        if state.watermark != watermark {
            return Err(Error::Corruption("snapshot prefix missing"));
        }
        Ok(digest_bytes(&canonical_bytes(&state)?))
    }
    /// Outer migration anchors retain opaque boundary commitments across an
    /// explicitly validated content rewrite. This never reconstructs old state.
    pub(crate) fn remember_prefix(&mut self, watermark: Watermark, digest: &str) -> Result<()> {
        let prefix = crate::replay_base::PrefixCommitment {
            watermark,
            digest: digest.to_owned(),
        };
        if self.prefixes.contains(&prefix) {
            return Ok(());
        }
        if self.prefix_digest(watermark)? != digest {
            return Err(Error::Corruption("migration prefix differs"));
        }
        if self.prefixes.len() >= 4096 {
            return Err(Error::Limit("retained prefix commitments"));
        }
        self.prefixes.push(prefix);
        Ok(())
    }
    pub async fn configuration(&mut self) -> Result<serde_json::Value> {
        self.backend.configuration().await
    }
    pub fn snapshot(&self) -> Result<Snapshot> {
        if self.poisoned {
            return Err(Error::Unavailable("reopen after indeterminate commit"));
        }
        let root_pin = snapshot_pin::acquire(&self.root)?;
        let mut pins = Vec::new();
        for record in self
            .state
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
        {
            let descriptor: ArtifactDescriptor = record.decode()?;
            if descriptor.state != vcp_domain::artifact::CaptureState::Purged {
                pins.push(self.spool.pin(&descriptor.spec.id)?);
            }
        }
        Ok(Snapshot {
            state: self.state.clone(),
            _pins: pins,
            _root_pin: root_pin,
        })
    }
    /// Excludes every snapshot of this physical root, including snapshots held
    /// after their original owner closed. Keep the guard alive through deletion.
    /// None means a live snapshot/cleanup holds the lock; other I/O errors remain
    /// errors. New snapshots cannot start until the returned guard is dropped.
    pub fn try_snapshot_cleanup_guard(&self) -> Result<Option<File>> {
        if self.poisoned {
            return Err(Error::Unavailable("reopen after indeterminate commit"));
        }
        snapshot_pin::cleanup(&self.root)
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
        validate_private_location(destination, forbidden)?;
        let snapshot = self.snapshot()?;
        if self.base.watermark != Watermark::ZERO {
            fs::create_dir_all(destination)?;
            crate::replay_base::ReplayBase::write(
                destination,
                &self.base,
                &self.base,
                &self.prefixes,
            )?;
            immutable_file(
                &destination.join("format.json"),
                &canonical_bytes(&Format {
                    version: 2,
                    backend: kind,
                })?,
            )?;
            self.copy_retained_artifacts(destination, &self.base)?;
        }
        let mut target =
            Store::open_with_artifact_limit(destination, kind, forbidden, self.artifact_limit)
                .await?;
        let mut copied = BTreeSet::new();
        for record in snapshot
            .state
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
        {
            let descriptor: ArtifactDescriptor = record.decode()?;
            if descriptor.state == vcp_domain::artifact::CaptureState::Purged {
                continue;
            }
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
            if destination.exists() {
                continue;
            }
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
    fn copy_retained_artifacts(&self, destination: &Path, state: &State) -> Result<()> {
        let spool = destination.join("spool");
        fs::create_dir_all(&spool)?;
        for record in state
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
        {
            let descriptor: ArtifactDescriptor = record.decode()?;
            if descriptor.state == vcp_domain::artifact::CaptureState::Purged {
                continue;
            }
            let source = self.spool.root().join(descriptor.spec.id.as_str());
            let capture = OpenOptions::new()
                .read(true)
                .open(source.join("owner.lock"))?;
            capture
                .try_lock_shared()
                .map_err(|_| Error::Conflict("quiesce artifact writers before rewrite"))?;
            let target = spool.join(descriptor.spec.id.as_str());
            fs::create_dir(&target)?;
            for entry in fs::read_dir(&source)? {
                let entry = entry?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if name == "spec.json" || name == "seal.json" || name.ends_with(".chunk") {
                    immutable_file(
                        &target.join(&name),
                        &read_bounded(&entry.path(), crate::artifact::CHUNK_BYTES)?,
                    )?;
                }
            }
            for name in ["owner.lock", "snapshot.lock"] {
                File::create(target.join(name))?.sync_all()?;
            }
        }
        Ok(())
    }
    /// Rewrite into a sealed replay base without copying historical commit
    /// payloads. Caller supplies an authorized, validated retention transform.
    /// Existing root content remains pending cleanup; this is not purge completion.
    pub async fn rewrite_base(
        &mut self,
        baseline: State,
        forbidden: &[PathBuf],
    ) -> Result<crate::rewrite::RewriteReceipt> {
        if self.poisoned {
            return Err(Error::Unavailable("reopen before rewrite"));
        }
        let _snapshot = self.snapshot()?;
        let history = crate::rewrite::receipts(&self.anchor)?;
        if history.len() >= crate::rewrite::MAX_REWRITES {
            return Err(Error::Limit("rewrite activation count"));
        }
        let children = self.anchor.join(".replay-roots");
        validate_private_location(&children, forbidden)?;
        fs::create_dir_all(&children)?;
        reject_link(&children)?;
        let id = TransactionId::new();
        let destination = children.join(id.as_str());
        fs::create_dir(&destination)?;
        crate::replay_base::ReplayBase::write(
            &destination,
            &self.state,
            &baseline,
            &self.prefixes,
        )?;
        immutable_file(
            &destination.join("format.json"),
            &canonical_bytes(&Format {
                version: 2,
                backend: self.kind,
            })?,
        )?;
        self.copy_retained_artifacts(&destination, &baseline)?;
        let candidate = Store::open_with_artifact_limit(
            &destination,
            self.kind,
            forbidden,
            self.artifact_limit,
        )
        .await?;
        candidate.close().await?;
        #[cfg(feature = "qualification")]
        if let Some(observer) = &self.observer {
            observer(Barrier::BeforeValidation);
        }
        let mut replacement = Store::open_with_artifact_limit(
            &destination,
            self.kind,
            forbidden,
            self.artifact_limit,
        )
        .await?;
        if replacement.state != baseline {
            return Err(Error::Corruption("rewrite reopened state differs"));
        }
        let mut pending = history
            .last()
            .map(|r| r.pending_roots.clone())
            .unwrap_or_default();
        pending.push(if self.root == self.anchor {
            "anchor".to_owned()
        } else {
            self.root
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or(Error::Access)?
                .to_owned()
        });
        let receipt = crate::rewrite::RewriteReceipt {
            version: 1,
            generation: history.len() as u64,
            root: id,
            backend: self.kind,
            watermark: baseline.watermark,
            source_digest: digest_bytes(&canonical_bytes(&self.state)?),
            state_digest: digest_bytes(&canonical_bytes(&baseline)?),
            previous: history
                .last()
                .map(canonical_bytes)
                .transpose()?
                .map(|b| digest_bytes(&b))
                .unwrap_or_else(|| "0".repeat(64)),
            pending_roots: pending,
        };
        #[cfg(feature = "qualification")]
        if let Some(observer) = &self.observer {
            observer(Barrier::BeforeActivation);
        }
        self.poisoned = true;
        immutable_file(
            &self
                .anchor
                .join(format!("rewrite-{:020}.json", receipt.generation)),
            &canonical_bytes(&receipt)?,
        )?;
        #[cfg(feature = "qualification")]
        if let Some(observer) = &self.observer {
            observer(Barrier::AfterActivation);
        }
        replacement.anchor = self.anchor.clone();
        #[cfg(feature = "qualification")]
        {
            replacement.observer = self.observer.clone();
        }
        let previous = std::mem::replace(self, replacement);
        let Store {
            backend,
            _owner,
            mut anchors,
            ..
        } = previous;
        // Retain ownership across activation; opening the descriptor's original
        // root cannot acquire a second writer while the new root is active.
        self.anchors.append(&mut anchors);
        self.anchors.push(_owner);
        backend.close().await?;
        Ok(receipt)
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

fn validate_private_location(path: &Path, forbidden: &[PathBuf]) -> Result<()> {
    let absolute = std::path::absolute(path)?;
    let ancestor = absolute
        .ancestors()
        .find(|p| p.exists())
        .ok_or(Error::Access)?
        .canonicalize()?;
    for root in forbidden {
        if ancestor.starts_with(root.canonicalize()?) {
            return Err(Error::Access);
        }
    }
    Ok(())
}
