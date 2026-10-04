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
    sync::{Arc, OnceLock},
    time::Instant,
};
use vcp_domain::{artifact::ArtifactDescriptor, *};
use vcp_protocol::{canonical_bytes, digest_bytes};
#[cfg(test)]
#[path = "disk_exhaustion_tests.rs"]
mod disk_exhaustion_tests;
#[cfg(test)]
#[path = "store_current_migration.rs"]
pub(crate) mod current_migration;
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
    state_size: StateSize,
    current: OnceLock<Arc<crate::CurrentState>>,
    base: State,
    prefixes: Vec<crate::replay_base::PrefixCommitment>,
    anchor: PathBuf,
    anchors: Vec<File>,
    root: PathBuf,
    forbidden_roots: Vec<PathBuf>,
    kind: BackendKind,
    spool: Spool,
    artifact_limit: u64,
    poisoned: bool,
    diagnostics: crate::StoreDiagnostics,
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
    pub fn current(&self) -> crate::CurrentStateView<'_> {
        (&self.state).into()
    }
    pub fn state(&self) -> &State {
        &self.state
    }
}
/// Unknown/legacy durable pins fail closed. A vault job may release source
/// leases only with its explicit inactive marker and no retained source refs.
pub fn snapshot_pin_active(record: &Record) -> bool {
    record.collection == Collection::SnapshotPin
        && !(record.value["document_type"] == "vcp_snapshot_job_v1"
            && record.value["active"] == false
            && record.references.is_empty())
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
        Self::open_diagnosed_with_artifact_limit(root, kind, forbidden_roots, artifact_limit)
            .await
            .0
    }

    /// Return phase observations even when opening fails. Diagnostics contain
    /// no paths, payloads or error text; the original error remains unchanged.
    pub async fn open_with_diagnostics(
        root: &Path,
        kind: BackendKind,
        forbidden_roots: &[PathBuf],
    ) -> (Result<Self>, crate::StoreDiagnostics) {
        Self::open_diagnosed_with_artifact_limit(
            root,
            kind,
            forbidden_roots,
            DEFAULT_ARTIFACT_LIMIT,
        )
        .await
    }

    async fn open_diagnosed_with_artifact_limit(
        root: &Path,
        kind: BackendKind,
        forbidden_roots: &[PathBuf],
        artifact_limit: u64,
    ) -> (Result<Self>, crate::StoreDiagnostics) {
        let open_started = Instant::now();
        let mut diagnostics = crate::StoreDiagnostics::new(kind);
        let mut result = Self::open_observed(
            root,
            kind,
            forbidden_roots,
            artifact_limit,
            &mut diagnostics,
        )
        .await;
        diagnostics.open.record(open_started, result.is_ok());
        if let Ok(store) = &mut result {
            store.diagnostics = diagnostics.clone();
        }
        (result, diagnostics)
    }

    async fn open_observed(
        root: &Path,
        kind: BackendKind,
        forbidden_roots: &[PathBuf],
        artifact_limit: u64,
        diagnostics: &mut crate::StoreDiagnostics,
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
            let mut active = Box::pin(Self::open_observed(
                &destination,
                kind,
                forbidden_roots,
                artifact_limit,
                diagnostics,
            ))
            .await?;
            let activated_base = crate::replay_base::ReplayBase::load(&destination)?
                .ok_or(Error::Corruption("activated replay base missing"))?;
            if activated_base.source_digest != activation.source_digest
                || activated_base.state.watermark != activation.watermark
                || active.prefix_digest(activation.watermark).await? != activation.state_digest
            {
                return Err(Error::Corruption("rewrite activation state"));
            }
            active.anchor = root;
            active.anchors.push(owner);
            return Ok(active);
        }
        if root.join("retired.json").exists() {
            return Err(Error::Conflict(
                "retired replay root; open canonical anchor",
            ));
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
        let base_started = Instant::now();
        let loaded_base = crate::replay_base::ReplayBase::load(&root);
        diagnostics
            .replay_base
            .record(base_started, loaded_base.is_ok());
        let loaded_base = loaded_base?;
        let prefixes = loaded_base
            .as_ref()
            .map(|b| b.prefixes.clone())
            .unwrap_or_default();
        let backend_started = Instant::now();
        let opened = Backend::open_observed(&root, kind, loaded_base.as_ref(), diagnostics).await;
        diagnostics
            .backend_open
            .record(backend_started, opened.is_ok());
        let (backend, state, state_size) = opened?;
        let base = loaded_base.map(|b| b.state).unwrap_or_default();
        let artifacts_started = Instant::now();
        let artifacts = (|| {
            let spool = Spool::open(&root.join("spool"), forbidden_roots, artifact_limit)?;
            for record in state
                .records
                .values()
                .filter(|record| record.collection == Collection::Artifact)
            {
                let descriptor: ArtifactDescriptor = record.decode()?;
                if descriptor.state != vcp_domain::artifact::CaptureState::Purged {
                    spool.verify(&descriptor)?;
                    diagnostics.verified_artifacts =
                        diagnostics.verified_artifacts.saturating_add(1);
                }
            }
            Ok::<_, Error>(spool)
        })();
        diagnostics
            .artifact_verification
            .record(artifacts_started, artifacts.is_ok());
        let spool = artifacts?;
        diagnostics.current_watermark = state.watermark.get();
        Ok(Self {
            backend,
            state,
            state_size,
            current: OnceLock::new(),
            base,
            prefixes,
            anchor: root.clone(),
            anchors: Vec::new(),
            root,
            forbidden_roots: forbidden_roots
                .iter()
                .map(|root| root.canonicalize())
                .collect::<std::io::Result<Vec<_>>>()?,
            kind,
            spool,
            artifact_limit,
            poisoned: false,
            diagnostics: diagnostics.clone(),
            #[cfg(feature = "qualification")]
            observer: None,
            _owner: owner,
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// Only a live admitted Store can derive a canonical child capability.
    /// Recheck the held native owner identity rather than trusting a pathname.
    pub(crate) fn validate_canonical_child(&self, child: &Path) -> Result<()> {
        if self.poisoned || child.parent() != Some(self.root.as_path()) {
            return Err(Error::Access);
        }
        if self
            .forbidden_roots
            .iter()
            .any(|root| child.starts_with(root) || root.starts_with(child))
        {
            return Err(Error::Access);
        }
        self.validate_canonical_owner()
    }
    pub(crate) fn validate_canonical_owner(&self) -> Result<()> {
        if self.poisoned {
            return Err(Error::Access);
        }
        let path = self.root.join("owner.lock");
        reject_link(&path)?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x0020_0000).share_mode(1 | 2);
        }
        let owner = options.open(path)?;
        if !crate::private_paths::allowed_handle(&owner, false)?
            || crate::vault_publish::native_identity(&owner)?
                != crate::vault_publish::native_identity(&self._owner)?
        {
            return Err(Error::Corruption("canonical owner identity changed"));
        }
        Ok(())
    }
    pub fn kind(&self) -> BackendKind {
        self.kind
    }
    pub fn artifact_limit(&self) -> u64 {
        self.artifact_limit
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
    /// Process-local diagnostics without reading payloads or changing durable state.
    pub fn diagnostics(&self) -> &crate::StoreDiagnostics {
        &self.diagnostics
    }
    /// Share a current-record snapshot without cloning historical payloads.
    /// Successful commits invalidate this cache; existing readers retain their
    /// immutable watermark. Admission must still check the canonical owner.
    pub fn current_state(&self) -> Arc<crate::CurrentState> {
        Arc::clone(
            self.current
                .get_or_init(|| Arc::new(crate::CurrentState::from_state(&self.state))),
        )
    }
    /// Verify retained canonical history at an exact cut. This read-only digest
    /// cannot authorize an import or restore history before the retained base.
    pub async fn prefix_digest(&self, watermark: Watermark) -> Result<String> {
        let state = self.reconstruct_at(watermark).await?;
        crate::legacy_state_stream::digest(&state)
    }
    async fn reconstruct_at(&self, watermark: Watermark) -> Result<State> {
        if self.poisoned {
            return Err(Error::Unavailable("reopen after indeterminate commit"));
        }
        if watermark > self.state.watermark {
            return Err(Error::Corruption("snapshot beyond canonical watermark"));
        }
        if watermark < self.base.watermark {
            return Err(Error::Unavailable("history precedes retained replay base"));
        }
        let mut state = self.base.clone();
        let mut history = self
            .backend
            .history(&self.root, &self.state, self.base.watermark)
            .await?;
        while state.watermark < watermark {
            let commit = history
                .next()
                .await?
                .ok_or(Error::Corruption("snapshot prefix missing"))?;
            state = state.into_replayed(&commit)?;
        }
        history.close().await?;
        if state.watermark != watermark {
            return Err(Error::Corruption("snapshot prefix missing"));
        }
        Ok(state)
    }
    /// Outer migration anchors retain opaque boundary commitments across an
    /// explicitly validated content rewrite. This never reconstructs old state.
    pub(crate) async fn remember_prefix(
        &mut self,
        watermark: Watermark,
        digest: &str,
    ) -> Result<()> {
        let prefix = crate::replay_base::PrefixCommitment {
            watermark,
            digest: digest.to_owned(),
        };
        if self.prefixes.contains(&prefix) {
            return Ok(());
        }
        if self.prefix_digest(watermark).await? != digest {
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
        self.pin_snapshot(self.state.clone())
    }
    /// A durable job may reconstruct an exact retained cut after restarting.
    /// A cut predating a rewrite base is explicitly unavailable, never replaced
    /// with a newer state under the old job identity.
    pub(crate) async fn snapshot_at(&self, watermark: Watermark) -> Result<Snapshot> {
        if self.poisoned {
            return Err(Error::Unavailable("reopen after indeterminate commit"));
        }
        if watermark < self.base.watermark || watermark > self.state.watermark {
            return Err(Error::Unavailable(
                "snapshot cut outside retained replay history",
            ));
        }
        let state = if watermark == self.state.watermark {
            self.state.clone()
        } else {
            self.reconstruct_at(watermark).await?
        };
        if state.watermark != watermark {
            return Err(Error::Corruption("snapshot cut missing"));
        }
        self.pin_snapshot(state)
    }
    fn pin_snapshot(&self, state: State) -> Result<Snapshot> {
        let root_pin = snapshot_pin::acquire(&self.root)?;
        let mut pins = Vec::new();
        for record in state
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
            state,
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
        let started = Instant::now();
        let result = self.backend.checkpoint(&self.state);
        self.diagnostics.checkpoint.record(started, result.is_ok());
        result
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
        let mut history = self
            .backend
            .history(&self.root, &self.state, self.base.watermark)
            .await?;
        while let Some(commit) = history.next().await? {
            let receipt = target.transact(commit.transaction.clone()).await?;
            if receipt != commit.receipt {
                return Err(Error::Corruption("conversion changed receipt"));
            }
        }
        history.close().await?;
        if target.state != snapshot.state {
            return Err(Error::Corruption("conversion changed logical view"));
        }
        target.checkpoint()?;
        let evidence = serde_json::json!({"version":1,"watermark":self.state.watermark,"logical_sha256":crate::legacy_state_stream::digest(&self.state)?,"source_backend":self.kind,"target_backend":kind});
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
    /// Exact replay-base transform; canonical IDs and commitment digests remain.
    pub fn retention_candidate(
        &self,
        records: &BTreeSet<String>,
        events: &BTreeSet<EventId>,
        tasks: &BTreeSet<TaskId>,
    ) -> Result<State> {
        let mut next = self.state.clone();
        for key in records {
            let row = self
                .state
                .records
                .get(key)
                .ok_or(Error::Conflict("retention record missing"))?;
            let workspace: vcp_domain::workspace::Workspace = self
                .state
                .record(
                    Collection::Workspace,
                    row.workspace.as_str(),
                    &row.workspace,
                )?
                .decode()?;
            next.records.insert(
                key.clone(),
                crate::redaction_contract::redact_record(&self.state, row, workspace.deletion)?,
            );
        }
        for event in &mut next.events {
            if events.contains(&event.event.id) {
                let workspace: vcp_domain::workspace::Workspace = self
                    .state
                    .record(
                        Collection::Workspace,
                        event.event.workspace.as_str(),
                        &event.event.workspace,
                    )?
                    .decode()?;
                *event = vcp_protocol::redaction::event(event, workspace.deletion)
                    .map_err(|_| Error::Conflict("event redaction"))?;
            }
        }
        for receipt in next.commands.values_mut() {
            if let vcp_protocol::command::CommandResult::Inspection { task: Some(task) } =
                &receipt.result
            {
                if tasks.contains(&task.scope.task) {
                    let workspace: vcp_domain::workspace::Workspace = self
                        .state
                        .record(
                            Collection::Workspace,
                            receipt.workspace.as_str(),
                            &receipt.workspace,
                        )?
                        .decode()?;
                    receipt.result =
                        vcp_protocol::redaction::inspection(&receipt.result, workspace.deletion)
                            .map_err(|_| Error::Conflict("inspection redaction"))?;
                    let transaction = next
                        .transactions
                        .get_mut(&receipt.transaction)
                        .ok_or(Error::Corruption("inspection transaction"))?;
                    transaction.command = Some(receipt.clone());
                }
            }
        }
        crate::redaction_contract::validate_rewrite(&self.state, &next)?;
        Ok(next)
    }
    pub fn retention_protection(
        &self,
        workspace: &WorkspaceId,
        task: Option<&TaskId>,
    ) -> Result<()> {
        crate::redaction_contract::unprotected(&self.state, workspace, task)
    }
    /// Remove only sealed-chain retired payloads under live reader/writer leases.
    /// The active canonical root and routing/lock records are never deleted.
    pub fn cleanup_rewrites(&self) -> Result<crate::rewrite::Cleanup> {
        if self.poisoned {
            return Err(Error::Unavailable("reopen before cleanup"));
        }
        if self.state.records.values().any(snapshot_pin_active) {
            let latest = crate::rewrite::receipts(&self.anchor)?.pop();
            return Ok(crate::rewrite::Cleanup {
                pinned: latest.map(|r| r.pending_roots).unwrap_or_default(),
                ..Default::default()
            });
        }
        crate::rewrite::cleanup(&self.anchor, &self.root, &|_phase| {
            #[cfg(feature = "qualification")]
            if let Some(observer) = &self.observer {
                observer(_phase);
            }
        })
    }
    pub fn canonical_anchor(&self) -> &Path {
        &self.anchor
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
        immutable_file(
            &destination.join("private-rewrite.json"),
            &canonical_bytes(
                &serde_json::json!({"version":1,"root":id,"source":crate::legacy_state_stream::digest(&self.state)?}),
            )?,
        )?;
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
            source_digest: crate::legacy_state_stream::digest(&self.state)?,
            state_digest: crate::legacy_state_stream::digest(&baseline)?,
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
        let mut next_size = self.state_size;
        let prepared = PreparedTransition::prepare(
            &self.state,
            &transaction,
            &mut self.diagnostics,
            &mut next_size,
        )?;
        if self.state.transactions.contains_key(&transaction.id) {
            self.diagnostics.duplicate_transactions =
                self.diagnostics.duplicate_transactions.saturating_add(1);
            return Ok(prepared.into_parts().1.receipt);
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
        let append_started = Instant::now();
        let append = self
            .backend
            .append(prepared.commit(), prepared.state(), &observe)
            .await;
        self.diagnostics
            .append
            .record(append_started, append.is_ok());
        append?;
        let (next, commit) = prepared.into_parts();
        self.state = next;
        self.state_size = next_size;
        self.current.take();
        self.diagnostics.current_watermark = self.state.watermark.get();
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
