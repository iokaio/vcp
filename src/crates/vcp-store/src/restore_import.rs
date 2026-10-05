// SPDX-License-Identifier: Apache-2.0
//! Import into an operation-owned private child. The public result is produced
//! only after fixed authority sanitization and a separate successful reopen.
use crate::{
    contract::CanonicalStore,
    private_paths::Directory,
    restore_stage::{immutable, Validated},
    BackendKind, Error, Result, Store,
};
use std::{
    fs,
    path::{Path, PathBuf},
};
use vcp_domain::{ActorId, CommandId, Timestamp, WorkspaceId};
use vcp_protocol::{canonical_bytes, digest_bytes};
#[path = "restore_import_stream.rs"]
mod stream;

pub struct Imported {
    root: PathBuf,
    workspace: WorkspaceId,
    state_digest: String,
    source_manifest: String,
    checkpoint: Option<crate::snapshot_inputs::Checkpoint>,
    backend: BackendKind,
    forbidden: Vec<PathBuf>,
    _directory: Directory,
}
impl Imported {
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn workspace(&self) -> &WorkspaceId {
        &self.workspace
    }
    pub fn state_digest(&self) -> &str {
        &self.state_digest
    }
    pub fn source_manifest(&self) -> &str {
        &self.source_manifest
    }
    /// The native checkpoint selected by the authenticated archive inventory.
    pub fn checkpoint(&self) -> Option<&crate::snapshot_inputs::Checkpoint> {
        self.checkpoint.as_ref()
    }
    pub fn backend(&self) -> BackendKind {
        self.backend
    }
    /// Destination activation retains this returned canonical owner while
    /// checking and publishing the expected workspace descriptor revision.
    pub async fn reopen_verified(&self) -> Result<Store> {
        let store = Store::open(&self.root, self.backend, &self.forbidden).await?;
        if store.prefix_digest(store.current().watermark).await? != self.state_digest {
            return Err(Error::Conflict("prepared restore root changed"));
        }
        for row in store
            .current()
            .records
            .values()
            .filter(|r| r.collection == crate::contract::Collection::Artifact)
        {
            let descriptor: vcp_domain::artifact::ArtifactDescriptor = row.decode()?;
            if descriptor.state != vcp_domain::artifact::CaptureState::Purged {
                store.spool().verify(&descriptor)?;
            }
        }
        Ok(store)
    }
}
#[allow(clippy::too_many_arguments)] // Mirrors the persisted import intent and its host-owned path constraints.
pub(crate) async fn prepare(
    validated: &Validated,
    root: &Path,
    forbidden: &[PathBuf],
    backend: BackendKind,
    operation: &CommandId,
    actor: &ActorId,
    timestamp: Timestamp,
    cancelled: &dyn Fn() -> bool,
) -> Result<Imported> {
    if cancelled() {
        return Err(Error::Unavailable("restore import cancelled"));
    }
    let (archive, proof) = match &validated.data {
        crate::restore_stage::Data::Stream(stream) => {
            return self::stream::prepare_stream(
                stream,
                &digest_bytes(&canonical_bytes(&validated.manifest())?),
                root,
                forbidden,
                backend,
                operation,
                actor,
                timestamp,
                cancelled,
            )
            .await;
        }
        crate::restore_stage::Data::Legacy { archive, proof } => (archive, proof),
    };
    let source = archive.state();
    let transaction = crate::restore_authority::transaction(
        source,
        archive.workspace(),
        operation,
        actor,
        timestamp,
    )?;
    let (expected, _) = source.prepare(&transaction)?;
    if !root.exists() {
        fs::create_dir(root)?;
    }
    let directory = Directory::open(root, forbidden)?;
    let source_digest = crate::legacy_state_stream::digest(source)?;
    let base = crate::replay_base::ReplayBase {
        version: 1,
        source_digest: source_digest.clone(),
        state: source.clone(),
        prefixes: vec![crate::replay_base::PrefixCommitment {
            watermark: source.watermark,
            digest: source_digest,
        }],
    };
    let bytes = canonical_bytes(&base)?;
    immutable(&root.join("replay-base.json"), &bytes)?;
    immutable(
        &root.join("replay-base.seal"),
        digest_bytes(&bytes).as_bytes(),
    )?;
    crate::replay_base::ReplayBase::load(root)?
        .ok_or(Error::Corruption("restore replay base missing"))?;
    // All generated names and contents come from the authenticated archive.
    // Retrying only accepts byte-identical already materialized objects.
    archive.write_spool_resumable(&root.join("spool"), forbidden, cancelled)?;
    if cancelled() {
        return Err(Error::Unavailable("restore import cancelled"));
    }
    if !root.join("format.json").exists() {
        immutable(
            &root.join("format.json"),
            &canonical_bytes(&serde_json::json!({"version":2,"backend":backend}))?,
        )?;
    }
    let mut store = Store::open(root, backend, forbidden).await?;
    if store.archive_state().await? == *source {
        store.transact(transaction).await?;
    }
    if store.archive_state().await? != expected {
        return Err(Error::Corruption("restore sanitized state differs"));
    }
    store.close().await?;
    if cancelled() {
        return Err(Error::Unavailable("restore import cancelled"));
    }
    let reopened = Store::open(root, backend, forbidden).await?;
    if reopened.archive_state().await? != expected {
        return Err(Error::Corruption("restore fresh reopen differs"));
    }
    reopened.close().await?;
    Ok(Imported {
        root: root.to_owned(),
        workspace: archive.workspace().clone(),
        state_digest: crate::legacy_state_stream::digest(&expected)?,
        source_manifest: digest_bytes(&canonical_bytes(&proof.restored().manifest)?),
        checkpoint: archive.inputs().checkpoint.clone(),
        backend,
        forbidden: forbidden.to_vec(),
        _directory: directory,
    })
}
