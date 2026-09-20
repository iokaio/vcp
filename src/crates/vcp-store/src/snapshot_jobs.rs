// SPDX-License-Identifier: Apache-2.0
//! Explicit resumable snapshot operations. Canonical records precede lengthy
//! source/ciphertext I/O. The caller invokes CPU/copy phases outside its owner
//! thread and returns results for revision-checked stage commits; inspection
//! never schedules work. No process-global background worker is created.
use crate::{
    contract::{key, CanonicalStore, Collection, Mutation, Record, Transaction},
    keys::VerifiedKeys,
    portable_snapshot::Archive,
    private_paths::{self, Directory},
    vault_crypto::{
        Finalization, FinalizedCiphertext, Limits, Manifest, Object, PrivateStaging, FORMAT,
    },
    vault_publish::{LocalTrust, PublicationPermit, Published},
    Error, Result, Snapshot, Store,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use vcp_domain::{
    workspace::Workspace, CommandId, Revision, TransactionId, Watermark, WorkspaceId,
};
use vcp_protocol::{canonical_bytes, digest_bytes};

const TAG: &str = "vcp_snapshot_job_v1";
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Captured,
    ArchiveReady,
    CiphertextReady,
    Admitted,
    Published,
    Cancelled,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub inputs: crate::snapshot_inputs::Inputs,
    pub schema_version: u32,
    pub document_type: String,
    pub id: CommandId,
    pub workspace: WorkspaceId,
    pub revision: Revision,
    pub watermark: Watermark,
    pub state_digest: String,
    pub deletion: u64,
    pub authority: u64,
    pub source_root: String,
    pub key_ref: String,
    pub trust_revision: u64,
    pub stage: Stage,
    pub active: bool,
    pub inventory: Option<String>,
    pub archive_digest: Option<String>,
    finalization: Option<Finalization>,
    pub publication: Option<serde_json::Value>,
    pub pins: BTreeSet<String>,
}
pub struct Jobs {
    directory: Arc<Directory>,
}
pub struct Capture {
    pub job: Job,
    snapshot: Snapshot,
}
pub struct Prepared {
    job: CommandId,
    revision: Revision,
    inventory: String,
    digest: String,
}
pub struct Encrypted {
    job: CommandId,
    revision: Revision,
    finalization: Finalization,
}
impl Jobs {
    /// A developer-selected local directory outside workspace and declared sync
    /// roots. Files use generated job IDs only; job metadata never supplies paths.
    pub fn open(path: &Path, forbidden: &[PathBuf]) -> Result<Self> {
        Ok(Self {
            directory: Arc::new(Directory::open(path, forbidden)?),
        })
    }
    fn path(&self, id: &CommandId, suffix: &str) -> Result<PathBuf> {
        let id = id.as_str();
        if id.len() != 36
            || id.bytes().enumerate().any(|(i, b)| {
                if [8, 13, 18, 23].contains(&i) {
                    b != b'-'
                } else {
                    !b.is_ascii_hexdigit()
                }
            })
        {
            return Err(Error::Access);
        }
        Ok(self.directory.path.join(format!("{id}.{suffix}")))
    }
    pub fn inspect(store: &Store, id: &CommandId, workspace: &WorkspaceId) -> Result<Job> {
        let row = store
            .state()
            .record(Collection::SnapshotPin, id.as_str(), workspace)?;
        let job: Job = row.decode()?;
        if job.schema_version != 1
            || job.document_type != TAG
            || job.id != *id
            || job.workspace != *workspace
            || job.revision != row.revision
            || job.pins != row.references
        {
            return Err(Error::Corruption("snapshot job identity"));
        }
        Ok(job)
    }
    pub async fn begin(
        &self,
        store: &mut Store,
        id: CommandId,
        workspace: &WorkspaceId,
        trust: &LocalTrust,
    ) -> Result<Capture> {
        self.begin_inner(
            store,
            id,
            workspace,
            trust,
            crate::snapshot_inputs::Inputs::default(),
        )
        .await
    }
    /// User-facing backup creation requires a complete native checkpoint. The
    /// host captures and revalidates it before entering this canonical method.
    pub async fn begin_with_inputs(
        &self,
        store: &mut Store,
        id: CommandId,
        workspace: &WorkspaceId,
        trust: &LocalTrust,
        inputs: crate::snapshot_inputs::Inputs,
    ) -> Result<Capture> {
        if inputs.checkpoint.is_none() {
            return Err(Error::Unavailable("complete workspace checkpoint required"));
        }
        self.begin_inner(store, id, workspace, trust, inputs).await
    }
    async fn begin_inner(
        &self,
        store: &mut Store,
        id: CommandId,
        workspace: &WorkspaceId,
        trust: &LocalTrust,
        inputs: crate::snapshot_inputs::Inputs,
    ) -> Result<Capture> {
        self.path(&id, "archive")?;
        if store
            .state()
            .records
            .contains_key(&key(Collection::SnapshotPin, id.as_str()))
        {
            let job = Self::inspect(store, &id, workspace)?;
            if job.inputs != inputs || job.key_ref != trust.configuration().selected.key_ref {
                return Err(Error::Conflict("snapshot retry key changed"));
            }
            return self.resume_capture(store, &job);
        }
        let snapshot = store.snapshot()?;
        let state = snapshot.state();
        inputs.validate(store, state, workspace)?;
        let ws: Workspace = state
            .record(Collection::Workspace, workspace.as_str(), workspace)?
            .decode()?;
        if state.records.values().any(|r| r.workspace != *workspace)
            || state.events.iter().any(|e| e.event.workspace != *workspace)
            || state.commands.values().any(|r| r.workspace != *workspace)
        {
            return Err(Error::Access);
        }
        if trust.configuration().workspace != *workspace
            || trust.configuration().checkpoint.deletion > ws.deletion.get()
        {
            return Err(Error::Conflict("snapshot deletion/trust mismatch"));
        }
        let pins = state
            .records
            .values()
            .filter(|r| matches!(r.collection, Collection::Artifact | Collection::Generation))
            .map(Record::key)
            .collect();
        let job = Job {
            inputs,
            schema_version: 1,
            document_type: TAG.into(),
            id,
            workspace: workspace.clone(),
            revision: Revision::ZERO,
            watermark: state.watermark,
            state_digest: digest_bytes(&canonical_bytes(state)?),
            deletion: ws.deletion.get(),
            authority: ws.authority.get(),
            source_root: store.root().to_string_lossy().into_owned(),
            key_ref: trust.configuration().selected.key_ref.clone(),
            trust_revision: trust.configuration().revision,
            stage: Stage::Captured,
            active: true,
            inventory: None,
            archive_digest: None,
            finalization: None,
            publication: None,
            pins,
        };
        put(store, &job, None).await?;
        Ok(Capture { job, snapshot })
    }
    pub fn resume_capture(&self, store: &Store, job: &Job) -> Result<Capture> {
        if job.stage != Stage::Captured {
            return Err(Error::Conflict("snapshot already prepared; inspect stage"));
        }
        if store.root().to_string_lossy() != job.source_root {
            return Err(Error::Unavailable(
                "retained source root reconciliation required",
            ));
        }
        let snapshot = store.snapshot_at(job.watermark)?;
        if digest_bytes(&canonical_bytes(snapshot.state())?) != job.state_digest {
            return Err(Error::Corruption("snapshot source cut differs"));
        }
        Ok(Capture {
            job: job.clone(),
            snapshot,
        })
    }
    /// Bounded source preparation; no canonical mutation occurs here. If the
    /// caller drops the result, restart reconciles the exact deterministic bytes.
    pub fn prepare(
        &self,
        store: &Store,
        capture: Capture,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Prepared> {
        let archive = Archive::capture_with_inputs(
            store,
            &capture.snapshot,
            &capture.job.workspace,
            &capture.job.inputs,
            cancelled,
        )?;
        let bytes = canonical_bytes(&archive.payloads()?)?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(Error::Limit("private archive encoding"));
        }
        let digest = digest_bytes(&bytes);
        persist(&self.path(&capture.job.id, "archive")?, &bytes, &digest)?;
        Ok(Prepared {
            job: capture.job.id,
            revision: capture.job.revision,
            inventory: archive.inventory_digest()?,
            digest,
        })
    }
    pub async fn accept_prepared(
        &self,
        store: &mut Store,
        workspace: &WorkspaceId,
        prepared: Prepared,
    ) -> Result<Job> {
        let mut job = Self::inspect(store, &prepared.job, workspace)?;
        if job.revision != prepared.revision || job.stage != Stage::Captured {
            return Err(Error::Conflict("snapshot preparation stale"));
        }
        job.inventory = Some(prepared.inventory);
        job.archive_digest = Some(prepared.digest);
        job.stage = Stage::ArchiveReady;
        advance(store, job).await
    }
    fn archive(&self, job: &Job) -> Result<Archive> {
        let bytes =
            crate::artifact::read_bounded(&self.path(&job.id, "archive")?, 16 * 1024 * 1024)?;
        if Some(digest_bytes(&bytes)).as_ref() != job.archive_digest.as_ref() {
            return Err(Error::Corruption("snapshot private archive differs"));
        }
        Archive::decode(
            serde_json::from_slice(&bytes)?,
            job.inventory
                .as_deref()
                .ok_or(Error::Corruption("snapshot inventory missing"))?,
        )
    }
    pub fn encrypt(
        &self,
        job: &Job,
        trust: &LocalTrust,
        keys: &VerifiedKeys,
        staging: &PrivateStaging,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Encrypted> {
        if job.stage != Stage::ArchiveReady || cancelled() {
            return Err(Error::Conflict(
                "snapshot encryption not ready or cancelled",
            ));
        }
        let archive = self.archive(job)?;
        let payloads = archive.payloads()?;
        let checkpoint = &trust.configuration().checkpoint;
        let manifest = Manifest {
            format: FORMAT.into(),
            workspace: job.workspace.clone(),
            lineage: trust.configuration().lineage.clone(),
            sequence: checkpoint
                .sequence
                .checked_add(1)
                .ok_or(Error::Limit("snapshot sequence"))?,
            deletion: job.deletion,
            parent: checkpoint.parent.clone(),
            objects: payloads
                .iter()
                .map(|(id, b)| {
                    (
                        id.clone(),
                        Object {
                            sha256: id.clone(),
                            bytes: b.len() as u64,
                        },
                    )
                })
                .collect(),
        };
        let directory = self.path(&job.id, "encrypted")?;
        match fs::create_dir(&directory) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        crate::artifact::reject_link(&directory)?;
        let owner = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("owner.lock"))?;
        owner
            .try_lock()
            .map_err(|_| Error::Conflict("snapshot encryption already owned"))?;
        let marker = directory.join("finalization.json");
        if marker.exists() {
            let finalization: Finalization =
                serde_json::from_slice(&crate::artifact::read_bounded(&marker, 128 * 1024)?)?;
            if finalization.manifest != manifest
                || finalization.writer != keys.public().writer
                || finalization.recipient != keys.public().recipient
            {
                return Err(Error::Conflict(
                    "persisted finalization differs from snapshot",
                ));
            }
            FinalizedCiphertext::reopen(
                &directory.join("object.age"),
                &finalization,
                self.directory.clone(),
            )?;
            return Ok(Encrypted {
                job: job.id.clone(),
                revision: job.revision,
                finalization,
            });
        }
        // A killed encoder/copy with no finalization marker is not a committed
        // object. The exclusive job lease owns precisely this generated path.
        match fs::remove_file(directory.join("object.age")) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let mut encrypted = trust.encrypt(
            keys,
            staging,
            manifest,
            payloads,
            job.trust_revision,
            Limits::default(),
        )?;
        if cancelled() {
            return Err(Error::Unavailable("snapshot encryption cancelled"));
        }
        let finalization = encrypted.finalization();
        let mut bytes = Vec::new();
        encrypted.copy_ciphertext(&mut bytes)?;
        persist(&directory.join("object.age"), &bytes, &finalization.sha256)?;
        let metadata = canonical_bytes(&finalization)?;
        persist(&marker, &metadata, &digest_bytes(&metadata))?;

        Ok(Encrypted {
            job: job.id.clone(),
            revision: job.revision,
            finalization,
        })
    }
    pub async fn accept_encrypted(
        &self,
        store: &mut Store,
        workspace: &WorkspaceId,
        encrypted: Encrypted,
    ) -> Result<Job> {
        let mut job = Self::inspect(store, &encrypted.job, workspace)?;
        if job.revision != encrypted.revision || job.stage != Stage::ArchiveReady {
            return Err(Error::Conflict("snapshot encryption stale"));
        }
        job.finalization = Some(encrypted.finalization);
        job.stage = Stage::CiphertextReady;
        advance(store, job).await
    }
    pub fn reopen_ciphertext(&self, job: &Job) -> Result<FinalizedCiphertext> {
        FinalizedCiphertext::reopen(
            &self.path(&job.id, "encrypted")?.join("object.age"),
            job.finalization
                .as_ref()
                .ok_or(Error::Corruption("snapshot finalization missing"))?,
            self.directory.clone(),
        )
    }
    /// Canonical owner must call this immediately before dispatching the copy.
    /// Old admitted copies remain explicit obligations if a later purge occurs.
    pub async fn admit(
        &self,
        store: &mut Store,
        id: &CommandId,
        workspace: &WorkspaceId,
        trust: &LocalTrust,
    ) -> Result<(Job, FinalizedCiphertext, PublicationPermit)> {
        let mut job = Self::inspect(store, id, workspace)?;
        if !matches!(job.stage, Stage::CiphertextReady | Stage::Admitted) {
            return Err(Error::Conflict("snapshot publication not ready"));
        }
        let ws: Workspace = store
            .state()
            .record(Collection::Workspace, workspace.as_str(), workspace)?
            .decode()?;
        if ws.deletion.get() != job.deletion || ws.authority.get() != job.authority {
            return Err(Error::Conflict("snapshot policy changed; rebuild required"));
        }
        let ciphertext = self.reopen_ciphertext(&job)?;
        let permit = trust.admit(&ciphertext, job.id.clone(), job.trust_revision)?;
        if job.stage != Stage::Admitted {
            job.stage = Stage::Admitted;
            job = advance(store, job).await?;
        }
        Ok((job, ciphertext, permit))
    }
    pub async fn complete(
        &self,
        store: &mut Store,
        workspace: &WorkspaceId,
        receipt: &Published,
    ) -> Result<Job> {
        let mut job = Self::inspect(store, &receipt.operation, workspace)?;
        if job.stage == Stage::Published {
            return Ok(job);
        }
        let finalization = job
            .finalization
            .as_ref()
            .ok_or(Error::Corruption("snapshot finalization missing"))?;
        if job.stage != Stage::Admitted
            || !receipt.matches_job(&job.id, finalization, job.trust_revision)
        {
            return Err(Error::Conflict("snapshot publication receipt differs"));
        }
        job.publication = Some(serde_json::to_value(receipt)?);
        job.stage = Stage::Published;
        advance(store, job).await
    }
    /// Release only local source obligations, after copying has stopped. Vault
    /// object deletion/remote transfer are separate explicit retention work.
    pub async fn release(
        &self,
        store: &mut Store,
        id: &CommandId,
        workspace: &WorkspaceId,
        cancel: bool,
    ) -> Result<Job> {
        let mut job = Self::inspect(store, id, workspace)?;
        if !job.active {
            return Ok(job);
        }
        if job.stage == Stage::Admitted && cancel {
            return Err(Error::Conflict(
                "reconcile admitted vault copy before cancellation",
            ));
        }
        if !cancel && job.stage != Stage::Published {
            return Err(Error::Conflict("unfinished snapshot obligation"));
        }
        let encrypted = self.path(id, "encrypted")?;
        if encrypted.exists() {
            crate::artifact::reject_link(&encrypted)?;
            for name in ["object.age", "finalization.json", "owner.lock"] {
                match fs::remove_file(encrypted.join(name)) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.into()),
                }
            }
            // Unknown or unfinished staging names keep the job visibly active;
            // no recursive deletion guesses which files are owned.
            fs::remove_dir(encrypted)?;
        }
        match fs::remove_file(self.path(id, "archive")?) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        job.active = false;
        job.pins.clear();
        if cancel && job.stage != Stage::Published {
            job.stage = Stage::Cancelled;
        }
        advance(store, job).await
    }
}
fn persist(path: &Path, bytes: &[u8], digest: &str) -> Result<()> {
    if path.exists() {
        let prior = crate::artifact::read_bounded(path, 65 * 1024 * 1024)?;
        if digest_bytes(&prior) == digest {
            return Ok(());
        }
        return Err(Error::Conflict("owned snapshot object differs"));
    }
    let temporary = path.with_extension(format!("{}.partial", TransactionId::new()));
    private_paths::write_private(&temporary, bytes)?;
    let result = fs::hard_link(&temporary, path);
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
        return Err(Error::Conflict("snapshot object publication failed"));
    }
    // Publication succeeded; cleanup residue is a visible local obligation.
    let _ = fs::remove_file(&temporary);
    Ok(())
}
async fn advance(store: &mut Store, mut job: Job) -> Result<Job> {
    let expected = job.revision;
    job.revision = expected.next()?;
    put(store, &job, Some(expected)).await?;
    Ok(job)
}
async fn put(store: &mut Store, job: &Job, expected: Option<Revision>) -> Result<()> {
    let record = Record {
        collection: Collection::SnapshotPin,
        id: job.id.to_string(),
        workspace: job.workspace.clone(),
        revision: job.revision,
        value: serde_json::to_value(job)?,
        references: job.pins.clone(),
    };
    record.validate_shape()?;
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![Mutation::Put { expected, record }],
            events: vec![],
            command: None,
        })
        .await?;
    Ok(())
}

pub(crate) fn kind(record: &Record) -> bool {
    record.value["document_type"] == TAG
}
pub(crate) fn shape(record: &Record) -> Result<()> {
    if !kind(record) {
        return Ok(());
    }
    let job: Job = record.decode()?;
    let hash = |value: &str| {
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    if record.collection != Collection::SnapshotPin
        || job.schema_version != 1
        || job.id.as_str() != record.id
        || job.workspace != record.workspace
        || job.revision != record.revision
        || !hash(&job.state_digest)
        || !hash(&job.key_ref)
        || job.pins != record.references
        || job.pins.len() > 4096
        || job.source_root.len() > 32768
        || !Path::new(&job.source_root).is_absolute()
        || (!job.active && !job.pins.is_empty())
        || (!job.active && !matches!(job.stage, Stage::Published | Stage::Cancelled))
        || job.inventory.as_ref().is_some_and(|v| !hash(v))
        || job.archive_digest.as_ref().is_some_and(|v| !hash(v))
    {
        return Err(Error::Corruption("snapshot job shape"));
    }
    let prepared = matches!(
        job.stage,
        Stage::ArchiveReady | Stage::CiphertextReady | Stage::Admitted | Stage::Published
    );
    if prepared && (job.inventory.is_none() || job.archive_digest.is_none()) {
        return Err(Error::Corruption("snapshot preparation receipt"));
    }
    if matches!(
        job.stage,
        Stage::CiphertextReady | Stage::Admitted | Stage::Published
    ) && job.finalization.is_none()
    {
        return Err(Error::Corruption("snapshot encryption receipt"));
    }
    if let Some(finalization) = &job.finalization {
        if !hash(&finalization.sha256)
            || finalization.bytes > 65 * 1024 * 1024
            || finalization.manifest.workspace != job.workspace
            || finalization.manifest.deletion != job.deletion
            || finalization.manifest.format != FORMAT
            || finalization.manifest.objects.len() > 4096
        {
            return Err(Error::Corruption("snapshot finalized identity"));
        }
    }
    if (job.stage == Stage::Published) != job.publication.is_some() {
        return Err(Error::Corruption("snapshot publication receipt"));
    }
    for pin in &job.pins {
        if !pin.starts_with("artifact:") && !pin.starts_with("generation:") {
            return Err(Error::Corruption("snapshot pin type"));
        }
    }
    Ok(())
}
pub(crate) fn insert(state: &crate::contract::State, record: &Record) -> Result<()> {
    if !kind(record) {
        return Ok(());
    }
    shape(record)?;
    let job: Job = record.decode()?;
    let expected: BTreeSet<_> = state
        .records
        .values()
        .filter(|r| {
            r.workspace == job.workspace
                && matches!(r.collection, Collection::Artifact | Collection::Generation)
        })
        .map(Record::key)
        .collect();
    if job.stage != Stage::Captured
        || !job.active
        || job.watermark != state.watermark
        || job.state_digest != digest_bytes(&canonical_bytes(state)?)
        || job.pins != expected
        || job.inventory.is_some()
        || job.finalization.is_some()
    {
        return Err(Error::Conflict(
            "snapshot capture differs from canonical cut",
        ));
    }
    Ok(())
}
pub(crate) fn transition(before: &Record, after: &Record) -> Result<()> {
    if !kind(before) && !kind(after) {
        return Ok(());
    }
    if !kind(before) || !kind(after) {
        return Err(Error::Conflict("snapshot job type changed"));
    }
    shape(after)?;
    let old: Job = before.decode()?;
    let new: Job = after.decode()?;
    let release = old.active
        && !new.active
        && (new.stage == Stage::Cancelled && old.stage != Stage::Admitted
            || old.stage == Stage::Published && new.stage == Stage::Published);
    let forward = old.active
        && new.active
        && matches!(
            (old.stage, new.stage),
            (Stage::Captured, Stage::ArchiveReady)
                | (Stage::ArchiveReady, Stage::CiphertextReady)
                | (Stage::CiphertextReady, Stage::Admitted)
                | (Stage::Admitted, Stage::Published)
        );
    if !release && !forward
        || old.id != new.id
        || old.workspace != new.workspace
        || old.inputs != new.inputs
        || old.watermark != new.watermark
        || old.state_digest != new.state_digest
        || old.source_root != new.source_root
        || old.key_ref != new.key_ref
        || old.trust_revision != new.trust_revision
        || old.deletion != new.deletion
        || old.authority != new.authority
        || (!release && old.pins != new.pins)
        || old.inventory.is_some() && old.inventory != new.inventory
        || old.archive_digest.is_some() && old.archive_digest != new.archive_digest
        || old.finalization.is_some()
            && canonical_bytes(&old.finalization)? != canonical_bytes(&new.finalization)?
        || old.publication.is_some() && old.publication != new.publication
    {
        return Err(Error::Conflict("invalid snapshot job transition"));
    }
    Ok(())
}
