// SPDX-License-Identifier: Apache-2.0
//! Explicit backend selection and verified local conversion. Existing direct
//! Store roots are preserved; this never seeds an ActiveRoot over user data.
use crate::{selection::Lease, settings::WorkspaceEntry};
use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::path::Path;
use vcp_store::{BackendKind, Store};
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy, Debug, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    Files,
    Sqlite,
}
impl Backend {
    pub fn kind(self) -> BackendKind {
        match self {
            Self::Files => BackendKind::Files,
            Self::Sqlite => BackendKind::Sqlite,
        }
    }
}
#[derive(Debug, Subcommand)]
pub enum Storage {
    Configure {
        #[arg(long, value_enum)]
        backend: Backend,
        #[arg(long)]
        preview: bool,
        #[arg(long)]
        expected_revision: Option<u64>,
    },
    Migrate {
        #[arg(long, value_enum)]
        backend: Backend,
        #[arg(long)]
        preview: bool,
        /// Required for apply, using the exact descriptor digest from preview.
        #[arg(long)]
        expected_descriptor: Option<String>,
        /// Generated preview identity, reused verbatim for apply.
        #[arg(long)]
        operation: Option<String>,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preference {
    version: u32,
    revision: u64,
    backend: Backend,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Migration {
    version: u32,
    operation: vcp_domain::CommandId,
    expected_descriptor: String,
    source: WorkspaceEntry,
    target: std::path::PathBuf,
    backend: Backend,
    state_digest: String,
}
pub fn preference(directory: &Path) -> Result<Option<BackendKind>> {
    Ok(read_preference(directory)?.map(|value| value.backend.kind()))
}
fn read_preference(directory: &Path) -> Result<Option<Preference>> {
    let path = directory.join("storage-preference.json");
    if !path.exists() {
        return Ok(None);
    }
    let root = crate::settings::registry_root(directory)?;
    let bytes = root
        .read(Path::new("storage-preference.json"), 4096)
        .map_err(|_| "storage preference unavailable or redirected")?
        .bytes;
    let value: Preference =
        serde_json::from_slice(&bytes).map_err(|_| "invalid storage preference")?;
    if value.version != 1 {
        return Err("unsupported storage preference".into());
    }
    Ok(Some(value))
}
pub async fn execute(
    command: &Storage,
    data: &Path,
    directory: &Path,
    workspace: &Path,
) -> Result<serde_json::Value> {
    if !directory.exists() {
        if !matches!(command, Storage::Configure { preview: false, .. }) {
            return Err("workspace has no storage selection".into());
        }
        std::fs::create_dir_all(directory)
            .map_err(|_| "storage selection directory unavailable")?;
    }
    let mut lease = Lease::exclusive(data, directory)?;
    let selected = lease.descriptor()?;
    match command {
        Storage::Configure {
            backend,
            preview,
            expected_revision,
        } => {
            let old = read_preference(directory)?;
            if *preview {
                return Ok(
                    serde_json::json!({"preview":true,"requested_backend":backend,"preference_revision":old.as_ref().map(|value|value.revision),"active_backend":selected.as_ref().map(|(entry,_)|entry.config.backend),"effect":"future workspace creation only; existing state requires explicit migration"}),
                );
            }
            if old.as_ref().map(|value| value.revision) != *expected_revision {
                return Err("storage preference changed; inspect a new preview".into());
            }
            let value = Preference {
                version: 1,
                revision: old.map_or(Ok(0), |value| {
                    value
                        .revision
                        .checked_add(1)
                        .ok_or("preference revision overflow")
                })?,
                backend: *backend,
            };
            // Selection is exclusively held; this public local preference is
            // never imported from a portable archive or used as activation.
            let bytes = serde_json::to_vec(&value).map_err(|_| "storage preference encoding")?;
            let target = directory.join("storage-preference.json");
            let temporary = directory.join(format!(
                "storage-preference-{}.new",
                vcp_domain::CommandId::new()
            ));
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)
                .map_err(|_| "storage preference staging failed")?;
            file.write_all(&bytes)
                .and_then(|()| file.sync_all())
                .map_err(|_| "storage preference flush failed")?;
            drop(file);
            replace_public(&temporary, &target)?;
            Ok(
                serde_json::json!({"configuration":value,"active_backend_unchanged":true,"migration_required":selected.as_ref().is_some_and(|(entry,_)|entry.config.backend!=backend.kind())}),
            )
        }
        Storage::Migrate {
            backend,
            preview,
            expected_descriptor,
            operation,
        } => {
            let (entry, digest) = selected.ok_or("workspace has no canonical root to migrate")?;
            let id = operation
                .as_deref()
                .map(vcp_domain::CommandId::parse)
                .transpose()
                .map_err(|_| "invalid migration operation")?
                .or_else(|| preview.then(vcp_domain::CommandId::new))
                .ok_or("migration apply requires the preview operation UUID")?;
            // Validate before using the caller-controlled identity in a path.
            if !crate::selection::operation_id(id.as_str()) {
                return Err("opaque migration UUID required".into());
            }
            let target = directory.join("canonical-roots").join(id.as_str());
            let journal_name = format!("migration-{id}.json");
            let journal = directory.join(&journal_name);
            if !*preview && journal.exists() {
                let bytes = crate::settings::registry_root(directory)?
                    .read(Path::new(&journal_name), 256 * 1024)
                    .map_err(|_| "migration journal unavailable or redirected")?
                    .bytes;
                let prior: Migration =
                    serde_json::from_slice(&bytes).map_err(|_| "invalid migration journal")?;
                if prior.version != 1
                    || prior.operation != id
                    || prior.target != target
                    || prior.backend.kind() != backend.kind()
                    || Some(prior.expected_descriptor.as_str()) != expected_descriptor.as_deref()
                {
                    return Err("migration operation payload mismatch".into());
                }
                crate::selection::validate_location(directory, &prior.source)?;
                if entry.config.canonical_root == target && entry.config.backend == backend.kind() {
                    let reopened = Store::open(&target, backend.kind(), &[workspace.to_owned()])
                        .await
                        .map_err(|e| e.to_string())?;
                    let watermark = reopened.state().watermark;
                    reopened.close().await.map_err(|e| e.to_string())?;
                    return Ok(
                        serde_json::json!({"activated":true,"reconciled":true,"operation":id,"canonical_root":target,"backend":backend,"retained_recovery_root":prior.source.config.canonical_root,"canonical_watermark":watermark,"tasks_resumed":false}),
                    );
                }
            }
            if !*preview && expected_descriptor.as_deref() != Some(digest.as_str()) {
                return Err("migration requires the unchanged preview descriptor digest".into());
            }
            if entry.config.backend == backend.kind() {
                return Err("requested backend is already active".into());
            }
            let source = Store::open(
                &entry.config.canonical_root,
                entry.config.backend,
                &[workspace.to_owned()],
            )
            .await
            .map_err(|e| e.to_string())?;
            let bytes = vcp_protocol::canonical_bytes(source.state())
                .map_err(|e| e.to_string())?
                .len();
            if *preview {
                let value = serde_json::json!({"preview":true,"operation":id,"expected_descriptor":digest,"source_root":entry.config.canonical_root,"target_root":target,"backend":backend,
                    "canonical_watermark":source.state().watermark,"canonical_serialized_bytes":bytes,"staging_space":"canonical bytes plus retained artifact payloads and backend overhead; free-space check occurs during materialization",
                    "index_compatibility":"rebuild from retained canonical sources; derived files are not silently trusted","retained_recovery_root":entry.config.canonical_root,"workspace_collision":false,"missing_secrets":[]});
                source.close().await.map_err(|e| e.to_string())?;
                return Ok(value);
            }
            let target = lease.target(&id)?;
            let state_digest = vcp_protocol::digest_bytes(
                &vcp_protocol::canonical_bytes(source.state()).map_err(|e| e.to_string())?,
            );
            if journal.exists() {
                let bytes = crate::settings::registry_root(directory)?
                    .read(Path::new(&journal_name), 256 * 1024)
                    .map_err(|_| "migration journal unavailable")?
                    .bytes;
                let prior: Migration =
                    serde_json::from_slice(&bytes).map_err(|_| "invalid migration journal")?;
                if prior.state_digest != state_digest {
                    return Err("source changed after migration began; preserved candidate requires a new operation".into());
                }
            } else {
                if target.exists() {
                    return Err("unowned migration target already exists".into());
                }
                let intent = Migration {
                    version: 1,
                    operation: id.clone(),
                    expected_descriptor: digest.clone(),
                    source: entry.clone(),
                    target: target.clone(),
                    backend: *backend,
                    state_digest,
                };
                let bytes = vcp_protocol::canonical_bytes(&intent).map_err(|e| e.to_string())?;
                use std::io::Write;
                let temporary = directory.join(format!(
                    "migration-{}.partial",
                    vcp_domain::CommandId::new()
                ));
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&temporary)
                    .map_err(|_| "migration intent staging failed")?;
                file.write_all(&bytes)
                    .and_then(|()| file.sync_all())
                    .map_err(|_| "migration intent flush failed")?;
                drop(file);
                std::fs::hard_link(&temporary, &journal)
                    .map_err(|_| "migration intent publication failed")?;
                std::fs::remove_file(&temporary)
                    .map_err(|_| "migration intent temporary cleanup failed")?;
            }
            if !target.exists() {
                let converted = source
                    .convert(&target, backend.kind(), &[workspace.to_owned()])
                    .await
                    .map_err(|e| e.to_string())?;
                converted.close().await.map_err(|e| e.to_string())?;
            }
            let converted = Store::open(&target, backend.kind(), &[workspace.to_owned()])
                .await
                .map_err(|e| e.to_string())?;
            if converted.state() != source.state() {
                return Err("converted state failed independent reopen comparison".into());
            }
            let mut config = entry.config.clone();
            config.canonical_root = target.clone();
            config.backend = backend.kind();
            let next = WorkspaceEntry {
                version: 2,
                config,
                identity: entry.identity,
            };
            lease.publish(Some(&digest), &next)?;
            let result = serde_json::json!({"activated":true,"operation":id,"canonical_root":target,"backend":backend,"retained_recovery_root":entry.config.canonical_root,"canonical_watermark":converted.state().watermark,"search":"rebuild_required","tasks_resumed":false});
            converted.close().await.map_err(|e| e.to_string())?;
            source.close().await.map_err(|e| e.to_string())?;
            Ok(result)
        }
    }
}
fn replace_public(source: &Path, target: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: both UTF-16 buffers live across the call; the selection lease
        // pins the containing directory and excludes descriptor users.
        if unsafe {
            MoveFileExW(
                source.as_ptr(),
                target.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err("storage preference publication failed".into());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(source, target).map_err(|_| "storage preference publication failed".into())
    }
}
