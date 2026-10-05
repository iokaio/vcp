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
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};
use vcp_domain::{artifact::ArtifactDescriptor, *};
use vcp_protocol::{canonical_bytes, command::CommandReceipt, digest_bytes, event::EventEnvelope};
#[path = "store_current_migration.rs"]
pub(crate) mod current_migration;
#[cfg(test)]
#[path = "disk_exhaustion_tests.rs"]
mod disk_exhaustion_tests;
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
    core: crate::backend::current_publication::open::Opened,
    prefixes: Vec<crate::replay_base::PrefixCommitment>,
    anchor: PathBuf,
    anchors: Vec<crate::canonical_lock::CanonicalLock>,
    root: PathBuf,
    forbidden_roots: Vec<PathBuf>,
    kind: BackendKind,
    #[cfg(feature = "qualification")]
    observer: Option<crate::backend::Observer>,
}
pub struct Snapshot {
    inner: current_migration::snapshot::PinnedDurableSnapshot,
}
impl Snapshot {
    pub fn current(&self) -> crate::CurrentStateView<'_> {
        self.inner.current()
    }
    pub async fn archive_state(&self) -> Result<State> {
        self.inner.archive_state().await
    }
    pub async fn logical_digest(&self) -> Result<String> {
        self.inner.logical_digest().await
    }
    pub(crate) async fn capture_stream(
        &mut self,
        destination: &mut impl crate::history_index::Pages,
        workspace: &WorkspaceId,
        inputs: &crate::snapshot_inputs::Inputs,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<crate::portable_snapshot::complete::Archive> {
        self.inner
            .capture(destination, workspace, inputs, check)
            .await
    }
    pub(crate) async fn verify_workspace(&self, workspace: &WorkspaceId) -> Result<()> {
        self.inner.verify_workspace(workspace).await
    }
    pub async fn history_event_count(&self) -> Result<u64> {
        self.inner.history_event_count().await
    }
    pub async fn history_event_at(&self, ordinal: u64) -> Result<Option<EventEnvelope>> {
        self.inner.history_event_at(ordinal).await
    }
    pub async fn history_event(&self, id: &EventId) -> Result<Option<EventEnvelope>> {
        self.inner.history_event(id).await
    }
    pub async fn command_receipt_by_id(
        &self,
        workspace: &WorkspaceId,
        command: &CommandId,
    ) -> Result<Option<CommandReceipt>> {
        self.inner.command_receipt_by_id(workspace, command).await
    }
    pub async fn history_events(
        &self,
        after: Option<u64>,
        limit: usize,
    ) -> Result<Vec<EventEnvelope>> {
        self.inner.history_events(after, limit).await
    }
    pub async fn close(self) -> Result<()> {
        self.inner.close().await
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
        let Self { core, anchors, .. } = self;
        let result = core.close().await;
        drop(anchors);
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
            store.core.diagnostics = diagnostics.clone();
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
        let owner = crate::canonical_lock::CanonicalLock::acquire(root, forbidden_roots)?;
        let root = owner.root().to_path_buf();
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
        if !format_path.exists() {
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
        // Preserve the legacy admission fence before opening any native file;
        // the current factory also rechecks these names under its held pins.
        for name in [
            "canonical.sqlite",
            "canonical.sqlite-wal",
            "canonical.sqlite-shm",
            "canonical.frames",
        ] {
            match reject_link(&root.join(name)) {
                Ok(()) => {}
                Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        let selection = crate::store_format::read(&owner, kind)?;
        let forbidden_roots = owner.forbidden().to_vec();
        let mut core = match selection {
            crate::store_format::Selection::Current { origin } => {
                crate::backend::current_publication::open::Opened::open_with_artifact_limit(
                    owner,
                    kind,
                    &origin,
                    artifact_limit,
                    diagnostics,
                )
                .await?
            }
            crate::store_format::Selection::Legacy(legacy) => {
                let started = Instant::now();
                let base = crate::replay_base::ReplayBase::load(&root);
                diagnostics.replay_base.record(started, base.is_ok());
                let base = base?;
                let started = Instant::now();
                let opened = Backend::open_observed(&root, kind, base.as_ref(), diagnostics).await;
                diagnostics.backend_open.record(started, opened.is_ok());
                let (backend, state, _) = opened?;
                let core = crate::backend::current_publication::open::Opened::from_legacy(
                    owner,
                    backend,
                    state,
                    base,
                    artifact_limit,
                    diagnostics,
                )
                .await?;
                if let Err(error) = crate::store_format::publish(&core, &legacy) {
                    let _ = core.close().await;
                    return Err(error);
                }
                core
            }
        };
        let prefixes = std::mem::take(&mut core.prefixes);
        core.diagnostics = diagnostics.clone();
        Ok(Self {
            core,
            prefixes,
            anchor: root.clone(),
            anchors: Vec::new(),
            root,
            forbidden_roots,
            kind,
            #[cfg(feature = "qualification")]
            observer: None,
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// Only a live admitted Store can derive a canonical child capability.
    /// Recheck the held native owner identity rather than trusting a pathname.
    pub(crate) fn validate_canonical_child(&self, child: &Path) -> Result<()> {
        if self.core.poisoned {
            return Err(Error::Access);
        }
        self.core.canonical_lock().verify_child(child)
    }
    pub(crate) fn validate_canonical_owner(&self) -> Result<()> {
        if self.core.poisoned {
            return Err(Error::Access);
        }
        self.core.canonical_lock().verify()
    }
    pub(crate) fn canonical_lock(&self) -> &crate::canonical_lock::CanonicalLock {
        &self.core.canonical_lock()
    }
    pub fn kind(&self) -> BackendKind {
        self.kind
    }
    pub fn artifact_limit(&self) -> u64 {
        self.core.artifact_limit
    }
    pub fn spool(&self) -> &Spool {
        &self.core.spool
    }
    pub fn healthy(&self) -> bool {
        !self.core.poisoned
    }
    /// Explicit complete archival DTO for bounded legacy export/rewrite APIs.
    pub async fn archive_state(&self) -> Result<State> {
        self.core.archive_state().await
    }
    /// Exact complete legacy encoding size, with no archival DTO or byte buffer.
    /// Stops at the caller's byte budget; it does not grant archival authority.
    pub async fn archive_size(&self, limit: usize) -> Result<usize> {
        self.archive_size_with_check(limit, &|| Ok(())).await
    }
    pub async fn archive_size_with_check(
        &self,
        limit: usize,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<usize> {
        self.core.archive_size(limit, check).await
    }
    /// Process-local diagnostics without reading payloads or changing durable state.
    pub fn diagnostics(&self) -> &crate::StoreDiagnostics {
        &self.core.diagnostics
    }
    /// Share a current-record snapshot without cloning historical payloads.
    /// Successful commits invalidate this cache; existing readers retain their
    /// immutable watermark. Admission must still check the canonical owner.
    pub fn current_state(&self) -> Arc<crate::CurrentState> {
        self.core.current_state()
    }
    /// Full native-byte replay at an exact retained cut.
    pub async fn prefix_digest(&self, watermark: Watermark) -> Result<String> {
        self.core.prefix_digest(watermark).await
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
        self.core.backend.configuration().await
    }
    pub fn snapshot(&self) -> Result<Snapshot> {
        Ok(Snapshot {
            inner: self.core.snapshot()?,
        })
    }
    pub(crate) async fn snapshot_at(&self, watermark: Watermark) -> Result<Snapshot> {
        Ok(Snapshot {
            inner: current_migration::snapshot::PinnedDurableSnapshot::at(&self.core, watermark)
                .await?,
        })
    }
    /// Excludes every snapshot of this physical root, including snapshots held
    /// after their original owner closed. Keep the guard alive through deletion.
    /// None means a live snapshot/cleanup holds the lock; other I/O errors remain
    /// errors. New snapshots cannot start until the returned guard is dropped.
    pub fn try_snapshot_cleanup_guard(&self) -> Result<Option<File>> {
        if self.core.poisoned {
            return Err(Error::Unavailable("reopen after indeterminate commit"));
        }
        snapshot_pin::cleanup(&self.root)
    }
    pub fn checkpoint(&mut self) -> Result<()> {
        self.core.checkpoint()
    }
    #[cfg(feature = "qualification")]
    pub fn observe(&mut self, observer: crate::backend::Observer) {
        self.observer = Some(observer);
    }
    /// Deletes only objects absent from every canonical reference and durable pin.
    /// Historical descriptors stay canonical until an explicit retention migration;
    /// this method intentionally cannot turn retention into silent data loss.
    pub async fn collect_orphan(&self, id: &ArtifactId) -> Result<bool> {
        if self.core.poisoned {
            return Err(Error::Unavailable("canonical state uncertain"));
        }
        let reference = key(Collection::Artifact, id.as_str());
        for record in self.current().records.values() {
            if record.key() == reference || record.required_references()?.contains(&reference) {
                return Ok(false);
            }
        }
        for workspace in self
            .current()
            .records
            .values()
            .filter(|row| row.collection == Collection::Workspace)
        {
            if !self
                .history_artifact_events(&workspace.workspace, id, None, 1)
                .await?
                .is_empty()
            {
                return Ok(false);
            }
        }
        self.core.spool.collect_unreferenced(id)
    }
    /// Replay one bounded original body through the existing complete semantic
    /// preparation and native publication path. No import authority is granted.
    pub(crate) async fn transact_original(&mut self, payload: &[u8]) -> Result<Receipt> {
        #[cfg(feature = "qualification")]
        let observer = self.observer.clone();
        let observe = move |barrier| {
            #[cfg(feature = "qualification")]
            if let Some(observer) = &observer {
                observer(barrier);
            }
            let _ = barrier;
        };
        self.core.transact_original(payload, &observe).await
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
        if self.core.poisoned {
            return Err(Error::Unavailable("canonical state uncertain"));
        }
        if destination.exists() {
            return Err(Error::Conflict("conversion requires a new root"));
        }
        validate_private_location(destination, forbidden)?;
        let snapshot = self.snapshot()?;
        let base = self.core.verified_base().await?;
        if let Some(base) = &base {
            fs::create_dir_all(destination)?;
            // The admitted legacy base is canonical. Preserve its original
            // source commitment and exact encoding, not a newly invented base.
            let bytes = canonical_bytes(base)?;
            immutable_file(&destination.join("replay-base.json"), &bytes)?;
            immutable_file(
                &destination.join("replay-base.seal"),
                digest_bytes(&bytes).as_bytes(),
            )?;
            immutable_file(
                &destination.join("format.json"),
                &canonical_bytes(&Format {
                    version: 2,
                    backend: kind,
                })?,
            )?;
            self.copy_retained_artifacts(destination, &base.state)?;
        }
        let mut target =
            Store::open_with_artifact_limit(destination, kind, forbidden, self.core.artifact_limit)
                .await?;
        let mut copied = BTreeSet::new();
        for record in snapshot
            .current()
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
            let current = self.core.spool.inspect(&descriptor.spec.id)?;
            // The original metadata is preserved by copying immutable files. This
            // includes interrupted captures and exact partial extents, not a new
            // successful capture invented by a conversion writer.
            let source = self.core.spool.root().join(current.spec.id.as_str());
            let capture = OpenOptions::new()
                .read(true)
                .open(source.join("owner.lock"))?;
            capture
                .try_lock_shared()
                .map_err(|_| Error::Conflict("quiesce artifact writers before conversion"))?;
            let destination = target.core.spool.root().join(current.spec.id.as_str());
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
            target.core.spool.verify(&descriptor)?;
        }
        let mut history = self.core.history().await?;
        while let Some(original) = history.next_original().await? {
            let receipt = target.transact_original(&original.bytes).await?;
            if receipt != original.commit.receipt {
                return Err(Error::Corruption("conversion changed receipt"));
            }
        }
        history.close().await?;
        let source_watermark = snapshot.current().watermark;
        let source_digest = self.prefix_digest(source_watermark).await?;
        if target.current().watermark != source_watermark
            || target.prefix_digest(source_watermark).await? != source_digest
        {
            return Err(Error::Corruption("conversion changed logical view"));
        }
        target.checkpoint()?;
        let evidence = serde_json::json!({"version":1,"watermark":source_watermark,"logical_sha256":source_digest,"source_backend":self.kind,"target_backend":kind});
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
            let source = self.core.spool.root().join(descriptor.spec.id.as_str());
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
    pub async fn retention_candidate(
        &self,
        records: &BTreeSet<String>,
        events: &BTreeSet<EventId>,
        tasks: &BTreeSet<TaskId>,
    ) -> Result<State> {
        let source = self.archive_state().await?;
        let mut next = source.clone();
        for key in records {
            let row = source
                .records
                .get(key)
                .ok_or(Error::Conflict("retention record missing"))?;
            let workspace: vcp_domain::workspace::Workspace = source
                .record(
                    Collection::Workspace,
                    row.workspace.as_str(),
                    &row.workspace,
                )?
                .decode()?;
            next.records.insert(
                key.clone(),
                crate::redaction_contract::redact_record(&source, row, workspace.deletion)?,
            );
        }
        for event in &mut next.events {
            if events.contains(&event.event.id) {
                let workspace: vcp_domain::workspace::Workspace = source
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
                    let workspace: vcp_domain::workspace::Workspace = source
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
        crate::redaction_contract::validate_rewrite(&source, &next)?;
        Ok(next)
    }
    pub fn retention_protection(
        &self,
        workspace: &WorkspaceId,
        task: Option<&TaskId>,
    ) -> Result<()> {
        crate::redaction_contract::unprotected(self.current(), workspace, task)
    }
    /// Remove only sealed-chain retired payloads under live reader/writer leases.
    /// The active canonical root and routing/lock records are never deleted.
    pub fn cleanup_rewrites(&self) -> Result<crate::rewrite::Cleanup> {
        if self.core.poisoned {
            return Err(Error::Unavailable("reopen before cleanup"));
        }
        if self.current().records.values().any(snapshot_pin_active) {
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
        if self.core.poisoned {
            return Err(Error::Unavailable("reopen before rewrite"));
        }
        let _snapshot = self.snapshot()?;
        let source = self.archive_state().await?;
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
                &serde_json::json!({"version":1,"root":id,"source":crate::legacy_state_stream::digest(&source)?}),
            )?,
        )?;
        crate::replay_base::ReplayBase::write(&destination, &source, &baseline, &self.prefixes)?;
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
            self.core.artifact_limit,
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
            self.core.artifact_limit,
        )
        .await?;
        if replacement.archive_state().await? != baseline {
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
            source_digest: crate::legacy_state_stream::digest(&source)?,
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
        self.core.poisoned = true;
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
            core, mut anchors, ..
        } = previous;
        self.anchors.append(&mut anchors);
        let (lock, closed) = core.close_retaining_lock().await;
        self.anchors.push(lock);
        closed?;
        Ok(receipt)
    }
}
impl CanonicalStore for Store {
    fn current(&self) -> crate::CurrentStateView<'_> {
        self.core.current()
    }
    async fn history_event_count(&self) -> Result<u64> {
        self.core.history_event_count().await
    }
    async fn history_events(&self, after: Option<u64>, limit: usize) -> Result<Vec<EventEnvelope>> {
        self.core.history_events(after, limit).await
    }
    async fn history_commands(
        &self,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<(String, CommandReceipt)>> {
        self.core.history_commands(after, limit).await
    }
    async fn history_artifact_events(
        &self,
        workspace: &WorkspaceId,
        artifact: &ArtifactId,
        after: Option<u64>,
        limit: usize,
    ) -> Result<Vec<(u64, EventEnvelope)>> {
        self.core
            .history_artifact_events(workspace, artifact, after, limit)
            .await
    }
    async fn history_event_at(&self, ordinal: u64) -> Result<Option<EventEnvelope>> {
        self.core.history_event_at(ordinal).await
    }
    async fn history_event(&self, id: &EventId) -> Result<Option<EventEnvelope>> {
        self.core.history_event(id).await
    }
    async fn command_receipt_by_id(
        &self,
        workspace: &WorkspaceId,
        command: &CommandId,
    ) -> Result<Option<CommandReceipt>> {
        self.core.command_receipt_by_id(workspace, command).await
    }
    async fn command_receipt(
        &self,
        workspace: &WorkspaceId,
        command: &CommandId,
        digest: &str,
    ) -> Result<Option<CommandReceipt>> {
        self.core.command_receipt(workspace, command, digest).await
    }
    async fn scoped_command_receipt(
        &self,
        workspace: &WorkspaceId,
        session: &SessionId,
        command: &CommandId,
    ) -> Result<Option<CommandReceipt>> {
        self.core
            .scoped_command_receipt(workspace, session, command)
            .await
    }
    async fn transaction_receipt(&self, id: &TransactionId) -> Result<Option<Receipt>> {
        self.core.transaction_receipt(id).await
    }
    async fn archive_state(&self) -> Result<State> {
        self.core.archive_state().await
    }
    async fn transact(&mut self, transaction: Transaction) -> Result<Receipt> {
        #[cfg(feature = "qualification")]
        let observer = self.observer.clone();
        let observe = move |barrier| {
            #[cfg(feature = "qualification")]
            if let Some(observer) = &observer {
                observer(barrier);
            }
            let _ = barrier;
        };
        self.core.transact(transaction, &observe).await
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

#[cfg(test)]
#[path = "store_original_append_tests.rs"]
mod original_append_tests;
