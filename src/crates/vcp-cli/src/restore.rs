// SPDX-License-Identifier: Apache-2.0
//! Local restore orchestration. Archive fields never enroll writer authority or
//! choose canonical paths; descriptor activation uses a held selection lease.
use crate::{selection::Lease, storage::Backend};
use clap::Args;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use vcp_domain::{CommandId, WorkspaceId};
type Result<T> = std::result::Result<T, String>;
fn barrier(stage: &str) {
    #[cfg(feature = "qualification")]
    if std::env::var("VCP_TEST_RESTORE_BARRIER").ok().as_deref() == Some(stage) {
        if let Some(path) = std::env::var_os("VCP_TEST_RESTORE_MARKER") {
            use std::io::Write;
            if let Ok(mut marker) = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
            {
                if marker
                    .write_all(stage.as_bytes())
                    .and_then(|()| marker.sync_all())
                    .is_ok()
                {
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            }
        }
    }
    let _ = stage;
}
struct StopSignal(tokio::task::JoinHandle<()>);
impl Drop for StopSignal {
    fn drop(&mut self) {
        self.0.abort();
    }
}
#[derive(Debug, Args)]
pub struct Restore {
    #[arg(long)]
    pub workspace_id: String,
    #[arg(long)]
    pub source: PathBuf,
    #[arg(long)]
    pub key: PathBuf,
    #[arg(long)]
    pub staging: PathBuf,
    #[arg(long, value_enum, default_value = "sqlite")]
    pub backend: Backend,
    #[arg(long)]
    pub preview: bool,
    #[arg(long)]
    pub operation: Option<String>,
    #[arg(long)]
    pub expected_descriptor: Option<String>,
    #[arg(long)]
    pub ciphertext_sha256: Option<String>,
    #[arg(long)]
    pub bytes: Option<u64>,
    #[arg(long)]
    pub sync_root: Vec<PathBuf>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    version: u32,
    operation: CommandId,
    workspace: WorkspaceId,
    expected_descriptor: Option<String>,
    ciphertext: String,
    bytes: u64,
    backend: Backend,
    destination: PathBuf,
    canonical: PathBuf,
    actor: vcp_domain::ActorId,
    timestamp: vcp_domain::Timestamp,
    trust_digest: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Activation {
    version: u32,
    intent: String,
    manifest: String,
    manifest_body: vcp_store::vault_crypto::Manifest,
    sequence: u64,
    deletion: u64,
    state_digest: String,
    prefix_digest: String,
    watermark: vcp_domain::Watermark,
    entry: crate::settings::WorkspaceEntry,
}
fn read<T: serde::de::DeserializeOwned>(directory: &Path, name: &str) -> Result<T> {
    let bytes = crate::settings::registry_root(directory)?
        .read(Path::new(name), 1024 * 1024)
        .map_err(|_| "restore receipt unavailable or redirected")?
        .bytes;
    serde_json::from_slice(&bytes).map_err(|_| "invalid restore receipt".into())
}
fn limits() -> vcp_store::vault_crypto::Limits {
    vcp_store::vault_crypto::Limits {
        plaintext_bytes: 64 * 1024 * 1024,
        payload_bytes: 64 * 1024 * 1024,
        ciphertext_bytes: 65 * 1024 * 1024,
        objects: 4096,
    }
}
fn preview_sizes(
    trust: &vcp_store::vault_publish::LocalTrust,
    recovery: &vcp_store::keys::RecoveryCopy,
    source: &Path,
) -> Result<(u64, u64)> {
    let verified = trust
        .verify_restore(source, recovery, limits())
        .map_err(|e| e.to_string())?;
    let restored = verified.restored();
    let inventory: Vec<_> = restored
        .payloads
        .iter()
        .filter_map(|(id, bytes)| {
            serde_json::from_slice::<serde_json::Value>(bytes)
                .ok()
                .filter(|value| value["format"] == "vcp-neutral-history/1")
                .map(|_| id.clone())
        })
        .collect();
    if inventory.len() != 1 {
        return Err("restore inventory is missing or ambiguous".into());
    }
    let archive =
        vcp_store::portable_snapshot::Archive::decode(restored.payloads.clone(), &inventory[0])
            .map_err(|e| e.to_string())?;
    let checkpoint = archive
        .inputs()
        .checkpoint
        .as_ref()
        .ok_or("snapshot has no materializable native checkpoint")?;
    let mut source_bytes = 0u64;
    for id in checkpoint.sources.values() {
        let descriptor: vcp_domain::artifact::ArtifactDescriptor = archive
            .state()
            .record(
                vcp_store::contract::Collection::Artifact,
                id.as_str(),
                archive.workspace(),
            )
            .and_then(|row| row.decode())
            .map_err(|e| e.to_string())?;
        source_bytes = source_bytes
            .checked_add(descriptor.length.get())
            .ok_or("checkpoint size overflow")?;
    }
    let canonical_bytes = restored.payloads.values().try_fold(0u64, |total, bytes| {
        total
            .checked_add(bytes.len() as u64)
            .ok_or("archive size overflow")
    })?;
    Ok((source_bytes, canonical_bytes))
}
async fn rebuild(
    data: &Path,
    directory: &Path,
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> serde_json::Value {
    let result = async {
        let lease = Lease::shared(data, directory)?;
        let (entry, _) = lease
            .descriptor()?
            .ok_or("restored descriptor unavailable")?;
        if entry.rebind_pending {
            return Err("rebind pending".to_owned());
        }
        let readiness = vcp_lifecycle::foundation::restore_search::rebuild_after_restore(
            &entry.config,
            cancelled,
        )
        .await?;
        serde_json::to_value(readiness).map_err(|e| e.to_string())
    }
    .await;
    result.unwrap_or_else(
        |error| serde_json::json!({"lexical_ready":false,"rebuild_pending":true,"reason":error}),
    )
}
pub async fn execute(
    request: &Restore,
    data: &Path,
    destination: &Path,
) -> Result<serde_json::Value> {
    use std::sync::{atomic::AtomicBool, Arc};
    use vcp_store::{
        restore_stage::{Restore as Staged, Stage},
        trust_store::TrustStore,
        vault_crypto::Limits,
    };
    let workspace = WorkspaceId::parse(&request.workspace_id)
        .map_err(|_| "invalid independent workspace identity")?;
    let destination =
        std::path::absolute(destination).map_err(|_| "invalid restore destination")?;
    let parent = destination
        .parent()
        .ok_or("restore destination parent missing")?
        .canonicalize()
        .map_err(|_| "restore destination parent must exist")?;
    let destination = parent.join(
        destination
            .file_name()
            .ok_or("restore destination name required")?,
    );
    let data = crate::settings::local_path(data, &destination)?;
    let directory = registry(&data, &workspace)?;
    if !directory.exists() {
        std::fs::create_dir(&directory).map_err(|_| "restore registry directory unavailable")?;
    }
    let mut lease = Lease::exclusive(&data, &directory)?;
    let selected = lease.descriptor()?;
    let id = request
        .operation
        .as_deref()
        .map(CommandId::parse)
        .transpose()
        .map_err(|_| "invalid restore operation")?
        .or_else(|| request.preview.then(CommandId::new))
        .ok_or("restore apply requires the preview operation UUID")?;
    if !crate::selection::operation_id(id.as_str()) {
        return Err("opaque restore operation UUID required".into());
    }
    let source = crate::settings::read_bounded(&request.source, 65 * 1024 * 1024)?;
    let ciphertext = vcp_protocol::digest_bytes(&source);
    let bytes = source.len() as u64;
    drop(source);
    let mut forbidden = vec![data.clone()];
    if destination.exists() {
        forbidden.push(destination.clone());
    }
    for root in &request.sync_root {
        forbidden.push(
            root.canonicalize()
                .map_err(|_| "declared sync root unavailable")?,
        );
    }
    for name in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
        if let Some(root) = std::env::var_os(name) {
            let root = PathBuf::from(root);
            if root.exists() {
                forbidden.push(root.canonicalize().map_err(|_| "sync root unavailable")?);
            }
        }
    }
    let mut trust_roots = forbidden.clone();
    trust_roots.retain(|root| root != &data);
    let mut trust = TrustStore::open(&crate::backup::trust_path(&data, &workspace), &trust_roots)
        .map_err(|e| e.to_string())?;
    let trust_digest = vcp_protocol::digest_bytes(
        &vcp_protocol::canonical_bytes(trust.trust().configuration()).map_err(|e| e.to_string())?,
    );
    let mut key_roots = forbidden.clone();
    key_roots.push(
        request
            .staging
            .canonicalize()
            .map_err(|_| "private restore staging must exist")?,
    );
    let recovery = crate::backup::recovery(&request.key, &key_roots)?;
    let verified = vcp_store::keys::LocalKeys::import(&recovery)
        .and_then(|keys| keys.verify_recovery(&recovery))
        .map_err(|_| "independent recovery key verification failed")?;
    if verified.public() != &trust.trust().configuration().selected {
        return Err("recovery key is not independently enrolled".into());
    }
    let expected = selected.as_ref().map(|(_, digest)| digest.clone());
    if request.preview {
        let (source_bytes, canonical_bytes) =
            preview_sizes(trust.trust(), &recovery, &request.source)?;
        let staging_space = crate::disk_space::observe(&request.staging, bytes)?;
        let canonical_space = crate::disk_space::observe(&directory, canonical_bytes)?;
        let destination_space = crate::disk_space::observe(&parent, source_bytes)?;
        return Ok(
            serde_json::json!({"preview":true,"operation":id,"workspace":workspace,"ciphertext_sha256":ciphertext,"bytes":bytes,"expected_descriptor":expected,"destination":destination,"workspace_collision":destination.exists(),"staging_space":staging_space,"canonical_space":canonical_space,"destination_space":destination_space,"retained_recovery_root":selected.as_ref().map(|(entry,_)|&entry.config.canonical_root),"authentication":"verified signed archive; repeated independently on apply","recovery_verified":true,"backend":request.backend,"search":"reopen compatible generation or rebuild required","git":"source bytes restored; Git index and diffs retained as evidence, executable Git metadata is not recreated","missing_secrets":"execution profiles and providers must be explicitly configured locally","global_newest_known":false}),
        );
    }
    if request.ciphertext_sha256.as_ref() != Some(&ciphertext) || request.bytes != Some(bytes) {
        return Err("restore requires unchanged preview ciphertext digest and byte length".into());
    }
    let target = lease.target(&id)?;
    // Also exclude canonical owners that were opened outside the CLI registry.
    // The old root remains retained, and is never rewritten by this restore.
    let previous_owner = if let Some((entry, _)) = &selected {
        if entry.config.canonical_root != target {
            Some(
                vcp_store::Store::open(
                    &entry.config.canonical_root,
                    entry.config.backend,
                    &trust_roots,
                )
                .await
                .map_err(|e| e.to_string())?,
            )
        } else {
            None
        }
    } else {
        None
    };
    let intent_name = format!("restore-{id}.json");
    let activation_name = format!("restore-{id}.activation.json");
    let intent = if directory.join(&intent_name).exists() {
        let prior: Intent = read(&directory, &intent_name)?;
        if prior.version != 1
            || prior.operation != id
            || prior.workspace != workspace
            || prior.expected_descriptor != request.expected_descriptor
            || prior.ciphertext != ciphertext
            || prior.bytes != bytes
            || prior.backend.kind() != request.backend.kind()
            || prior.destination != destination
            || prior.canonical != target
        {
            return Err("restore operation payload mismatch".into());
        }
        prior
    } else {
        if expected != request.expected_descriptor {
            return Err("restore descriptor changed; inspect a new preview".into());
        }
        let value = Intent {
            version: 1,
            operation: id.clone(),
            workspace: workspace.clone(),
            expected_descriptor: expected.clone(),
            ciphertext: ciphertext.clone(),
            bytes,
            backend: request.backend,
            destination: destination.clone(),
            canonical: target.clone(),
            actor: vcp_domain::ActorId::new(),
            timestamp: crate::settings::now(),
            trust_digest: trust_digest.clone(),
        };
        immutable(
            &directory.join(&intent_name),
            &vcp_protocol::canonical_bytes(&value).map_err(|e| e.to_string())?,
        )?;
        value
    };
    let intent_digest = vcp_protocol::digest_bytes(
        &vcp_protocol::canonical_bytes(&intent).map_err(|e| e.to_string())?,
    );
    barrier("intent");
    let staging_parent = crate::settings::local_path(&request.staging, &destination)?;
    let _staging_pin = crate::settings::registry_root(&staging_parent)?
        .hold(None, true)
        .map_err(|_| "restore staging redirected")?;
    let stage_path = staging_parent.join(format!("restore-{id}"));
    if !stage_path.exists() {
        std::fs::create_dir(&stage_path).map_err(|_| "restore staging unavailable")?;
    }
    let mut staged = if std::fs::read_dir(&stage_path)
        .map_err(|_| "restore staging unavailable")?
        .next()
        .is_none()
    {
        Staged::begin(
            &stage_path,
            &forbidden,
            id.clone(),
            trust.trust(),
            ciphertext,
            bytes,
        )
    } else {
        Staged::open(&stage_path, &forbidden)
    }
    .map_err(|e| e.to_string())?;
    if staged.status().operation != id
        || staged.status().workspace != workspace
        || staged.status().ciphertext != intent.ciphertext
        || staged.status().bytes != bytes
    {
        return Err("restore staging identity mismatch".into());
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = cancelled.clone();
    let _signal = StopSignal(tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            signal.store(true, std::sync::atomic::Ordering::Release);
        }
    }));
    let stopped = || cancelled.load(std::sync::atomic::Ordering::Acquire);
    let limits = Limits {
        plaintext_bytes: 64 * 1024 * 1024,
        payload_bytes: 64 * 1024 * 1024,
        ciphertext_bytes: 65 * 1024 * 1024,
        objects: 4096,
    };
    if directory.join(&activation_name).exists() {
        let activation: Activation = read(&directory, &activation_name)?;
        if activation.version != 1
            || activation.intent != intent_digest
            || activation.entry.config.canonical_root != target
            || activation.entry.config.workspace != workspace
            || activation.manifest_body.workspace != workspace
            || activation.sequence != activation.manifest_body.sequence
            || activation.deletion != activation.manifest_body.deletion
            || vcp_protocol::digest_bytes(
                &vcp_protocol::canonical_bytes(&activation.manifest_body)
                    .map_err(|e| e.to_string())?,
            ) != activation.manifest
        {
            return Err("restore activation receipt mismatch".into());
        }
        let already_selected = selected.as_ref().is_some_and(|(entry, _)| {
            entry.config.canonical_root == target && entry.config.workspace == workspace
        });
        let needs_rebind = !already_selected
            || selected
                .as_ref()
                .is_some_and(|(entry, _)| entry.rebind_pending);
        // An activation intent is not evidence that selection happened. Before
        // the first CAS, repeat every original authority and native proof.
        let prepared = if !already_selected {
            if expected != intent.expected_descriptor || trust_digest != intent.trust_digest {
                return Err("restore trust or selection changed before activation".into());
            }
            let validated = staged
                .authenticate(trust.trust(), &recovery, limits, &stopped)
                .map_err(|e| e.to_string())?;
            let imported = staged
                .import(
                    &validated,
                    trust.trust(),
                    request.backend.kind(),
                    &target,
                    intent.actor.clone(),
                    intent.timestamp,
                    &trust_roots,
                    &stopped,
                )
                .await
                .map_err(|e| e.to_string())?;
            if imported.state_digest() != activation.state_digest
                || imported.source_manifest() != activation.manifest
            {
                return Err("prepared restore identity differs from activation receipt".into());
            }
            let materialization_forbidden: Vec<_> = forbidden
                .iter()
                .filter(|root| *root != &destination)
                .cloned()
                .chain(std::iter::once(staging_parent.clone()))
                .collect();
            let materialized = vcp_lifecycle::foundation::restore_workspace::materialize(
                &imported,
                &destination,
                &id,
                &materialization_forbidden,
                cancelled.clone(),
            )
            .await?;
            if activation.entry.identity.as_ref()
                != Some(&crate::binding::capture(materialized.root())?)
            {
                return Err("materialized restore identity differs from activation receipt".into());
            }
            Some((imported, materialized, validated))
        } else {
            None
        };
        let store = if let Some((imported, _, _)) = &prepared {
            imported.reopen_verified().await
        } else {
            vcp_store::Store::open(&target, request.backend.kind(), &trust_roots).await
        }
        .map_err(|e| e.to_string())?;
        if store
            .prefix_digest(activation.watermark)
            .map_err(|e| e.to_string())?
            != activation.prefix_digest
        {
            return Err("restore activation historical prefix differs".into());
        }
        if let Some((_, materialized, _)) = &prepared {
            materialized.revalidate()?;
            if stopped() {
                return Err("restore cancelled before activation".into());
            }
            lease.publish(intent.expected_descriptor.as_deref(), &activation.entry)?;
            barrier("descriptor");
        }
        let checkpoint = &trust.trust().configuration().checkpoint;
        if checkpoint.sequence < activation.sequence
            || (checkpoint.sequence == activation.sequence
                && checkpoint.parent.as_deref() != Some(activation.manifest.as_str()))
        {
            if trust_digest != intent.trust_digest {
                return Err(
                    "independent restore checkpoint changed before activation completion".into(),
                );
            }
            let validated = staged
                .authenticate(trust.trust(), &recovery, limits, &stopped)
                .map_err(|e| e.to_string())?;
            let revision = trust.trust().configuration().revision;
            trust
                .update(revision, |local| {
                    local.advance_after_restore(validated.proof(), revision)
                })
                .map_err(|e| e.to_string())?;
            barrier("trust");
        }
        store.close().await.map_err(|e| e.to_string())?;
        drop(prepared);
        if let Some(owner) = previous_owner {
            owner.close().await.map_err(|e| e.to_string())?;
        }
        drop(lease);
        drop(trust);
        let rebound = if needs_rebind {
            crate::rebind::rebind(&data, &destination, &workspace).await?
        } else {
            serde_json::json!({"already_completed":true,"canonical_state_unchanged":true})
        };
        barrier("rebind");
        let search = rebuild(&data, &directory, cancelled.clone()).await;
        return Ok(
            serde_json::json!({"operation":id,"activated":true,"reconciled":true,"rebind":rebound,"search":search,"tasks_resumed":false,"execution_grants_restored":false}),
        );
    }
    if expected != intent.expected_descriptor || trust_digest != intent.trust_digest {
        return Err("restore authority or descriptor changed; preparation retained".into());
    }
    if staged.status().stage == Stage::Pending {
        staged
            .acquire(&request.source, &stopped)
            .map_err(|e| e.to_string())?;
    }
    let validated = staged
        .authenticate(trust.trust(), &recovery, limits, &stopped)
        .map_err(|e| e.to_string())?;
    let imported = staged
        .import(
            &validated,
            trust.trust(),
            request.backend.kind(),
            &target,
            intent.actor.clone(),
            intent.timestamp,
            &trust_roots,
            &stopped,
        )
        .await
        .map_err(|e| e.to_string())?;
    barrier("import");
    let materialization_forbidden: Vec<_> = forbidden
        .iter()
        .filter(|root| *root != &destination)
        .cloned()
        .chain(std::iter::once(staging_parent))
        .collect();
    let materialized = vcp_lifecycle::foundation::restore_workspace::materialize(
        &imported,
        &destination,
        &id,
        &materialization_forbidden,
        cancelled.clone(),
    )
    .await?;
    let store = imported
        .reopen_verified()
        .await
        .map_err(|e| e.to_string())?;
    let config = vcp_lifecycle::foundation::restore_workspace::restored_configuration(
        &store,
        &imported,
        &materialized,
        intent.actor.clone(),
        vcp_domain::HostId::new(),
        selected.as_ref().map(|(entry, _)| &entry.config),
    )?;
    let identity = crate::binding::capture(materialized.root())?;
    let next = crate::settings::WorkspaceEntry {
        version: 2,
        rebind_pending: true,
        config,
        identity: Some(identity),
    };
    let manifest = &validated.proof().restored().manifest;
    let activation = Activation {
        version: 1,
        intent: intent_digest,
        manifest: imported.source_manifest().into(),
        manifest_body: manifest.clone(),
        sequence: manifest.sequence,
        deletion: manifest.deletion,
        state_digest: imported.state_digest().into(),
        prefix_digest: store
            .prefix_digest(store.state().watermark)
            .map_err(|e| e.to_string())?,
        watermark: store.state().watermark,
        entry: next,
    };
    immutable(
        &directory.join(&activation_name),
        &vcp_protocol::canonical_bytes(&activation).map_err(|e| e.to_string())?,
    )?;
    barrier("activation_receipt");
    materialized.revalidate()?;
    if stopped() {
        return Err("restore cancelled before descriptor activation; preparation retained".into());
    }
    lease.publish(intent.expected_descriptor.as_deref(), &activation.entry)?;
    barrier("descriptor");
    let revision = trust.trust().configuration().revision;
    trust
        .update(revision, |local| {
            local.advance_after_restore(validated.proof(), revision)
        })
        .map_err(|e| e.to_string())?;
    barrier("trust");
    store.close().await.map_err(|e| e.to_string())?;
    if let Some(owner) = previous_owner {
        owner.close().await.map_err(|e| e.to_string())?;
    }
    let retained_staging = materialized.retained_staging().to_vec();
    drop(materialized);
    drop(imported);
    drop(lease);
    drop(trust);
    let rebound = crate::rebind::rebind(&data, &destination, &workspace).await?;
    barrier("rebind");
    let search = rebuild(&data, &directory, cancelled.clone()).await;
    Ok(
        serde_json::json!({"operation":id,"activated":true,"rebind":rebound,"retained_staging":retained_staging,"search":search,"git":"source bytes restored; index/diffs retained as evidence, Git repository metadata not recreated","tasks_resumed":false,"execution_grants_restored":false}),
    )
}
fn registry(data: &Path, workspace: &WorkspaceId) -> Result<PathBuf> {
    let base = data.join("workspaces");
    if !base.exists() {
        std::fs::create_dir_all(&base).map_err(|_| "registry unavailable")?;
    }
    let mut selected = None;
    for (index, row) in std::fs::read_dir(&base)
        .map_err(|_| "registry unavailable")?
        .enumerate()
    {
        if index >= 1024 {
            return Err("workspace registry exceeds bound".into());
        }
        let directory = row.map_err(|_| "registry entry unavailable")?.path();
        if !directory.join("workspace.json").exists() {
            continue;
        }
        let lease = Lease::shared(data, &directory)?;
        if lease
            .descriptor()?
            .is_some_and(|(entry, _)| entry.config.workspace == *workspace)
        {
            if selected.replace(directory).is_some() {
                return Err("ambiguous workspace registry identity".into());
            }
        }
    }
    Ok(selected
        .unwrap_or_else(|| base.join(vcp_protocol::digest_bytes(workspace.as_str().as_bytes()))))
}
fn immutable(path: &Path, bytes: &[u8]) -> Result<()> {
    if bytes.len() > 1024 * 1024 {
        return Err("restore receipt exceeds bound".into());
    }
    use std::io::Write;
    if path.exists() {
        let root =
            crate::settings::registry_root(path.parent().ok_or("intent directory missing")?)?;
        let prior = root
            .read(
                Path::new(path.file_name().ok_or("intent name missing")?),
                1024 * 1024,
            )
            .map_err(|_| "intent unavailable or redirected")?
            .bytes;
        return if prior == bytes {
            Ok(())
        } else {
            Err("restore operation payload mismatch".into())
        };
    }
    let temporary = path.with_extension(format!("{}.partial", CommandId::new()));
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|_| "restore intent unavailable")?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| "restore intent flush failed")?;
    drop(file);
    std::fs::hard_link(&temporary, path).map_err(|_| "restore intent publication failed")?;
    std::fs::remove_file(temporary).map_err(|_| "restore intent cleanup pending")?;
    Ok(())
}
