// SPDX-License-Identifier: Apache-2.0
//! Dedicated developer key interaction. Only public references cross JSONL;
//! secret parsing, generation and round-trip verification stay in store handles.
use clap::Subcommand;
use std::path::{Path, PathBuf};
use vcp_store::{
    keys::{LocalKeys, RecoveryCopy, RecoveryDirectory, VerifiedKeys},
    trust_store::TrustStore,
    vault_publish::{Checkpoint, LocalTrust},
};

type Result<T> = std::result::Result<T, String>;
#[cfg(windows)]
pub(crate) mod publisher;

#[derive(Debug, Subcommand)]
pub enum Backup {
    Keys {
        /// Independently supplied stable identity, required before a fresh-host restore.
        #[arg(long)]
        workspace_id: Option<String>,
        #[command(subcommand)]
        command: Keys,
    },
    Configure {
        #[arg(long)]
        vault: PathBuf,
        #[arg(long)]
        staging: PathBuf,
        #[arg(long)]
        expected_revision: Option<u64>,
        #[arg(long)]
        sync_root: Vec<PathBuf>,
        #[arg(
            long,
            conflicts_with = "manual_only",
            required_unless_present = "manual_only"
        )]
        automatic: bool,
        #[arg(long)]
        manual_only: bool,
    },
    Status,
    Create {
        #[arg(long)]
        key: PathBuf,
        /// Explicit trusted native Git executable, never a workspace command.
        #[arg(long)]
        git: PathBuf,
        #[arg(long)]
        operation: Option<String>,
        #[arg(long, requires = "operation")]
        retry: bool,
    },
    Cancel {
        #[arg(long)]
        operation: String,
    },
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateRequest {
    pub key: PathBuf,
    pub git: PathBuf,
    pub operation: vcp_domain::CommandId,
    pub retry: bool,
}

#[cfg(windows)]
pub async fn cancel_on_host(
    host: &vcp_lifecycle::foundation::CanonicalHost,
    data: &Path,
    operation: vcp_domain::CommandId,
) -> Result<serde_json::Value> {
    if !crate::selection::operation_id(operation.as_str()) {
        return Err("opaque backup operation UUID required".into());
    }
    if host
        .backup_progress()?
        .is_some_and(|progress| progress.operation == operation)
    {
        return serde_json::to_value(host.cancel_backup(&operation)?).map_err(|e| e.to_string());
    }
    let config = host.backup_configuration()?;
    let data = data.to_owned();
    let (trust, jobs) = tokio::task::spawn_blocking(move || -> Result<_> {
        let workspace = Path::new(&config.binding.root);
        let roots = forbidden(workspace, &config.canonical_root, &[])?;
        let path = crate::settings::local_path(&trust_path(&data, &config.workspace), workspace)?;
        let trust = TrustStore::open(&path, &roots).map_err(|e| e.to_string())?;
        let setup =
            vcp_lifecycle::foundation::backup::setup(&trust)?.ok_or("backup is not configured")?;
        let mut exclusions = forbidden(workspace, &config.canonical_root, &setup.sync_roots)?;
        exclusions.extend([path, setup.vault]);
        let jobs = vcp_store::snapshot_jobs::Jobs::open(&setup.staging, &exclusions)
            .map_err(|e| e.to_string())?;
        Ok((trust, jobs))
    })
    .await
    .map_err(|_| "backup cancellation setup worker stopped")??;
    let job = host.release_reopened_backup(trust, jobs, operation)?;
    Ok(
        serde_json::json!({"operation":job.id,"stage":job.stage,"active":job.active,"source_pins_released":!job.active,"vault_copy_deleted":false}),
    )
}

#[cfg(windows)]
pub async fn create_on_host(
    host: &vcp_lifecycle::foundation::CanonicalHost,
    data: &Path,
    request: CreateRequest,
) -> Result<serde_json::Value> {
    if !crate::selection::operation_id(request.operation.as_str()) {
        return Err("opaque backup operation UUID required".into());
    }
    // Exact active retries do not attempt a second independent trust owner.
    if let Some(progress) = host.backup_progress()? {
        if progress.running() && progress.operation == request.operation {
            return serde_json::to_value(progress).map_err(|e| e.to_string());
        }
    }
    host.unload_backup()?;
    let loaded = publisher::load(
        host,
        data,
        publisher::Selection::Direct {
            key: request.key.clone(),
            git: request.git.clone(),
        },
    )
    .await?;
    host.load_backup_with_revision(
        loaded.capabilities,
        loaded.git,
        loaded.automatic,
        loaded.revision,
    )?;
    serde_json::to_value(host.start_backup(request.operation, request.retry)?)
        .map_err(|e| e.to_string())
}

#[derive(Debug, Subcommand)]
pub enum Keys {
    /// Create, export, and independently verify new recovery material.
    Create {
        #[arg(long)]
        recovery_dir: PathBuf,
        #[arg(long)]
        sync_root: Vec<PathBuf>,
    },
    /// Explicitly enroll an independent recovery copy and trusted checkpoint.
    Import {
        #[arg(long)]
        key: PathBuf,
        #[arg(long)]
        lineage: String,
        #[arg(long)]
        checkpoint: PathBuf,
        #[arg(long)]
        sync_root: Vec<PathBuf>,
    },
    /// Verify a saved recovery copy against the independently pinned recipient.
    Verify {
        #[arg(long)]
        key: PathBuf,
        #[arg(long)]
        sync_root: Vec<PathBuf>,
    },
    /// Select a new verified recipient; old recovery copies remain necessary.
    Rotate {
        #[arg(long)]
        recovery_dir: PathBuf,
        #[arg(long)]
        expected_revision: u64,
        #[arg(long)]
        sync_root: Vec<PathBuf>,
    },
}

pub fn trust_path(data: &Path, workspace: &vcp_domain::WorkspaceId) -> PathBuf {
    data.join("developer-trust")
        .join(vcp_protocol::digest_bytes(workspace.as_str().as_bytes()))
}

/// Declared sync roots supplement the known Windows provider roots. Unknown
/// third-party synchronizers cannot be discovered universally.
pub fn forbidden(workspace: &Path, canonical: &Path, declared: &[PathBuf]) -> Result<Vec<PathBuf>> {
    if declared.len() > 64 {
        return Err("too many declared synchronization roots".into());
    }
    let mut paths = vec![workspace.to_path_buf(), canonical.to_path_buf()];
    paths.extend(declared.iter().cloned());
    for name in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
        if let Some(path) = std::env::var_os(name) {
            if Path::new(&path).exists() {
                paths.push(path.into());
            }
        }
    }
    let mut paths = paths
        .into_iter()
        .map(|path| {
            path.canonicalize()
                .map_err(|_| "declared private/synchronization root unavailable".to_owned())
        })
        .collect::<Result<Vec<_>>>()?;
    paths.sort();
    paths.dedup();
    Ok(paths)
}

pub fn configure(
    command: &Backup,
    data: &Path,
    workspace: &Path,
    entry: &crate::settings::WorkspaceEntry,
) -> Result<serde_json::Value> {
    let Backup::Configure {
        vault,
        staging,
        expected_revision,
        sync_root,
        automatic,
        ..
    } = command
    else {
        return Err("backup configuration command required".into());
    };
    let roots = forbidden(workspace, &entry.config.canonical_root, sync_root)?;
    let trust = TrustStore::open(&trust_path(data, &entry.config.workspace), &roots)
        .map_err(|e| e.to_string())?;
    let sync: Vec<_> = roots
        .iter()
        .filter(|path| *path != workspace && **path != entry.config.canonical_root)
        .cloned()
        .collect();
    let setup = vcp_lifecycle::foundation::backup::configure(
        &trust,
        workspace,
        &entry.config.canonical_root,
        &vcp_lifecycle::foundation::backup::ConfigurationRequest {
            expected: *expected_revision,
            vault: vault.clone(),
            staging: staging.clone(),
            sync_roots: sync,
            automatic: *automatic,
        },
    )?;
    Ok(
        serde_json::json!({"kind":"backup_configuration","configuration":setup,
        "key_reference":trust.trust().configuration().selected.key_ref,"independent_recovery_verified":true,
        "signing_material":"explicit key import required in each owner process","cloud_transfer":"unknown"}),
    )
}

pub fn configuration_status(
    data: &Path,
    workspace: &Path,
    entry: &crate::settings::WorkspaceEntry,
) -> Result<serde_json::Value> {
    let roots = forbidden(workspace, &entry.config.canonical_root, &[])?;
    let trust = TrustStore::open(&trust_path(data, &entry.config.workspace), &roots)
        .map_err(|e| e.to_string())?;
    let setup = vcp_lifecycle::foundation::backup::setup(&trust)?;
    Ok(
        serde_json::json!({"kind":"backup_status","configuration":setup,
        "public_keys":trust.trust().configuration().selected,"trust_revision":trust.trust().configuration().revision,
        "independent_recovery_verified":trust.trust().configuration().recovery_verified,
        "global_newest_known":false,"cloud_transfer":"unknown"}),
    )
}

pub(crate) fn recovery(path: &Path, forbidden: &[PathBuf]) -> Result<RecoveryCopy> {
    if path.extension().and_then(|value| value.to_str()) != Some("recovery") {
        return Err("key must name an independently saved .recovery file".into());
    }
    let id = path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or("invalid recovery reference")?;
    let parent = std::path::absolute(path)
        .map_err(|_| "invalid recovery path")?
        .parent()
        .ok_or("recovery directory required")?
        .to_path_buf();
    RecoveryDirectory::open(&parent, forbidden)
        .and_then(|directory| directory.open_copy(id))
        .map_err(|_| {
            "recovery material is unavailable or outside the permitted private directory".into()
        })
}

pub fn verified_key(path: &Path, forbidden: &[PathBuf]) -> Result<VerifiedKeys> {
    let copy = recovery(path, forbidden)?;
    LocalKeys::import(&copy)
        .and_then(|keys| keys.verify_recovery(&copy))
        .map_err(|_| {
            "recovery_not_verified: independently saved material failed verification".into()
        })
}

pub fn keys(
    command: &Keys,
    data: &Path,
    workspace: &Path,
    entry: Option<&crate::settings::WorkspaceEntry>,
    requested_workspace: Option<&str>,
) -> Result<serde_json::Value> {
    let requested = requested_workspace
        .map(vcp_domain::WorkspaceId::parse)
        .transpose()
        .map_err(|_| "invalid independent workspace identity")?;
    if entry.is_some_and(|entry| {
        requested
            .as_ref()
            .is_some_and(|id| id != &entry.config.workspace)
    }) {
        return Err(
            "independent workspace identity differs from the selected local workspace".into(),
        );
    }
    let workspace_id = requested
        .or_else(|| entry.map(|entry| entry.config.workspace.clone()))
        .ok_or(
            "fresh-host key enrollment requires --workspace-id from independent trusted metadata",
        )?;
    let declared = match command {
        Keys::Create { sync_root, .. }
        | Keys::Import { sync_root, .. }
        | Keys::Verify { sync_root, .. }
        | Keys::Rotate { sync_root, .. } => sync_root,
    };
    let roots = forbidden(
        workspace,
        entry.map_or(workspace, |entry| entry.config.canonical_root.as_path()),
        declared,
    )?;
    let trust_path = crate::settings::local_path(&trust_path(data, &workspace_id), workspace)?;
    if !trust_path.exists() {
        if matches!(command, Keys::Verify { .. } | Keys::Rotate { .. }) {
            return Err("writer_not_trusted: enroll developer keys first".into());
        }
        std::fs::create_dir_all(&trust_path).map_err(|_| "private trust directory unavailable")?;
    }
    let mut recovery_forbidden = roots.clone();
    recovery_forbidden.push(trust_path.clone());
    let mut created_copy = None;
    let keys = match command {
        Keys::Create { recovery_dir, .. } | Keys::Rotate { recovery_dir, .. } => {
            let directory =
                std::path::absolute(recovery_dir).map_err(|_| "invalid recovery directory")?;
            // The developer creates/selects this independent directory. Never
            // create a secret-bearing location inside a potentially synced tree.
            let directory =
                RecoveryDirectory::open(&directory, &recovery_forbidden).map_err(|_| {
                    "independent recovery directory unavailable or overlaps protected roots"
                })?;
            let keys = LocalKeys::generate().map_err(|_| "local key generation failed")?;
            let copy = keys
                .export_recovery(&directory)
                .map_err(|e| e.to_string())?;
            let verified = keys
                .verify_recovery(&copy)
                .map_err(|_| "recovery_not_verified: exported copy failed verification")?;
            created_copy = Some(copy.path());
            verified
        }
        Keys::Import { key, .. } | Keys::Verify { key, .. } => {
            verified_key(key, &recovery_forbidden)?
        }
    };
    let trust = match command {
        Keys::Create { .. } | Keys::Import { .. } => {
            let (lineage, checkpoint) = match command {
                Keys::Import {
                    lineage,
                    checkpoint,
                    ..
                } => {
                    let bytes = crate::settings::read_bounded(checkpoint, 4096)?;
                    let checkpoint: Checkpoint = serde_json::from_slice(&bytes)
                        .map_err(|_| "invalid independently supplied trusted checkpoint")?;
                    (lineage.clone(), checkpoint)
                }
                _ => (
                    vcp_protocol::digest_bytes(vcp_domain::CommandId::new().as_str().as_bytes()),
                    Checkpoint {
                        sequence: 0,
                        deletion: 0,
                        parent: None,
                    },
                ),
            };
            let trust = LocalTrust::enroll(&keys, workspace_id.clone(), lineage, checkpoint)
                .map_err(|_| "writer_not_trusted: invalid independent enrollment")?;
            TrustStore::enroll(&trust_path, &roots, trust).map_err(|e| e.to_string())?
        }
        Keys::Verify { .. } | Keys::Rotate { .. } => {
            let mut trust = TrustStore::open(&trust_path, &roots).map_err(|e| e.to_string())?;
            if trust.trust().configuration().workspace != workspace_id {
                return Err("writer_not_trusted: workspace binding differs".into());
            }
            if let Keys::Rotate {
                expected_revision, ..
            } = command
            {
                trust
                    .update(*expected_revision, |trust| {
                        trust.rotate(&keys, *expected_revision)
                    })
                    .map_err(|e| e.to_string())?;
            } else if &trust.trust().configuration().selected != keys.public() {
                return Err("recovery_not_verified: copy differs from selected public key".into());
            }
            trust
        }
    };
    Ok(
        serde_json::json!({"kind":"developer_keys","configuration":trust.trust().configuration(),
        "recovery_copy":created_copy,"independent_recovery_verified":true,
        "secret_cache":"none; publication requires an explicit local key import",
        "rotation_limit":"Old snapshots still require their old recovery copies; copied ciphertext and keys cannot be recalled."}),
    )
}
