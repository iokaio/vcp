// SPDX-License-Identifier: Apache-2.0
//! Explicit developer backup setup, kept outside canonical/restored state.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use vcp_domain::WorkspaceId;
use vcp_protocol::{canonical_bytes, digest_bytes};
use vcp_store::{snapshot_jobs::Jobs, trust_store::TrustStore, vault_publish::Vault};
type Result<T> = std::result::Result<T, String>;
const LIMIT: usize = 256 * 1024;

pub fn status(
    state: &vcp_store::contract::State,
    workspace: &WorkspaceId,
) -> Result<serde_json::Value> {
    use vcp_store::contract::Collection;
    state
        .record(Collection::Workspace, workspace.as_str(), workspace)
        .map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    let mut published = 0;
    let mut total = 0;
    for record in state.records.values().filter(|record| {
        record.workspace == *workspace
            && record.collection == Collection::SnapshotPin
            && record.value["document_type"] == "vcp_snapshot_job_v1"
    }) {
        let job: vcp_store::snapshot_jobs::Job = record.decode().map_err(|e| e.to_string())?;
        if job.stage == vcp_store::snapshot_jobs::Stage::Published {
            published = published.max(job.watermark.get());
        }
        rows.push(serde_json::json!({"id":job.id,"revision":job.revision,"stage":job.stage,"active":job.active,"watermark":job.watermark,
            "key_reference":job.key_ref,"locally_published":job.stage==vcp_store::snapshot_jobs::Stage::Published,"cloud_transfer":"unknown","restore_verified":false}));
        total += 1;
        rows.sort_by(|a, b| {
            b["watermark"]
                .as_u64()
                .cmp(&a["watermark"].as_u64())
                .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
        });
        rows.truncate(128);
    }
    rows.sort_by(|a, b| {
        b["watermark"]
            .as_u64()
            .cmp(&a["watermark"].as_u64())
            .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
    });
    Ok(
        serde_json::json!({"workspace":workspace,"canonical_watermark":state.watermark,"jobs":rows,"job_count":total,"truncated":total>128,
        "unsynced_sequence_range":(published<state.watermark.get()).then(||serde_json::json!({"first":published+1,"last":state.watermark})),"range_basis":"not included in a locally published snapshot; remote transfer is unknown"}),
    )
}
impl super::CanonicalHost {
    pub fn backup_status(&self) -> Result<serde_json::Value> {
        self.worker.run_cleanup(|context| {
            status(context.engine.store().state(), &context.config.workspace).map_err(Into::into)
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Setup {
    pub schema_version: u32,
    pub revision: u64,
    pub previous: String,
    pub workspace: WorkspaceId,
    pub vault: PathBuf,
    pub staging: PathBuf,
    pub sync_roots: Vec<PathBuf>,
    pub automatic: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigurationRequest {
    pub expected: Option<u64>,
    pub vault: PathBuf,
    pub staging: PathBuf,
    pub sync_roots: Vec<PathBuf>,
    pub automatic: bool,
}
fn name(revision: u64) -> String {
    format!("backup-config-{revision:020}.json")
}
fn read(path: &Path) -> Result<Vec<u8>> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000).share_mode(1 | 2);
    }
    let file = options
        .open(path)
        .map_err(|_| "backup configuration unavailable")?;
    let meta = file
        .metadata()
        .map_err(|_| "backup configuration metadata unavailable")?;
    if !meta.is_file() || meta.len() > LIMIT as u64 || meta.file_type().is_symlink() {
        return Err("invalid backup configuration file".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err("backup configuration redirected".into());
        }
    }
    let mut bytes = Vec::new();
    file.take(LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "backup configuration read failed")?;
    if bytes.len() > LIMIT {
        return Err("backup configuration exceeds bound".into());
    }
    Ok(bytes)
}
/// The independent trust owner serializes setup changes with key rotation and
/// checkpoint updates. Restored canonical records cannot replace these files.
pub fn setup(trust: &TrustStore) -> Result<Option<Setup>> {
    let mut paths = Vec::new();
    for (count, item) in fs::read_dir(trust.directory())
        .map_err(|_| "backup setup directory unavailable")?
        .enumerate()
    {
        if count >= 4096 {
            return Err("backup setup directory exceeds bound".into());
        }
        let item = item.map_err(|_| "backup setup entry unavailable")?;
        let filename = item.file_name();
        let filename = filename.to_str().ok_or("invalid backup setup filename")?;
        if filename.starts_with("backup-config-") && !filename.ends_with(".partial") {
            paths.push(item.path());
        }
    }
    paths.sort();
    if paths.len() > 1024 {
        return Err("backup configuration history exceeds bound".into());
    }
    let mut previous = "0".repeat(64);
    let mut latest = None;
    for (revision, path) in paths.into_iter().enumerate() {
        if path.file_name().and_then(|value| value.to_str()) != Some(name(revision as u64).as_str())
        {
            return Err("backup configuration revision gap".into());
        }
        let bytes = read(&path)?;
        let value: Setup =
            serde_json::from_slice(&bytes).map_err(|_| "invalid backup configuration")?;
        if value.schema_version != 1
            || value.workspace != trust.trust().configuration().workspace
            || value.revision != revision as u64
            || value.previous != previous
            || canonical_bytes(&value).map_err(|_| "backup configuration encoding")? != bytes
        {
            return Err("backup configuration chain mismatch".into());
        }
        previous = digest_bytes(&bytes);
        latest = Some(value);
    }
    Ok(latest)
}

pub fn configure(
    trust: &TrustStore,
    workspace: &Path,
    canonical: &Path,
    request: &ConfigurationRequest,
) -> Result<Setup> {
    let ConfigurationRequest {
        expected,
        vault,
        staging,
        sync_roots,
        automatic,
    } = request;
    let old = setup(trust)?;
    if old.as_ref().map(|value| value.revision) != *expected {
        return Err("backup configuration changed; inspect its current revision".into());
    }
    if sync_roots.len() > 64 {
        return Err("too many synchronization roots".into());
    }
    let canonicalize = |path: &Path| {
        path.canonicalize()
            .map_err(|_| "selected backup directory must already exist".to_owned())
    };
    let vault = canonicalize(vault)?;
    let staging = canonicalize(staging)?;
    let sync_roots = sync_roots
        .iter()
        .map(|path| canonicalize(path))
        .collect::<Result<Vec<_>>>()?;
    // Open the actual production capabilities now: invalid separation cannot
    // be saved as an apparently usable configuration.
    Vault::open(
        &vault,
        &[
            workspace.to_owned(),
            canonical.to_owned(),
            staging.clone(),
            trust.directory().to_owned(),
        ],
    )
    .map_err(|_| "vault_unavailable: vault overlaps private state or is redirected")?;
    let mut forbidden = sync_roots.clone();
    forbidden.extend([
        workspace.to_owned(),
        canonical.to_owned(),
        vault.clone(),
        trust.directory().to_owned(),
    ]);
    Jobs::open(&staging, &forbidden)
        .map_err(|_| "local staging overlaps workspace, vault, synchronization or trust roots")?;
    let value = Setup {
        schema_version: 1,
        revision: old.as_ref().map_or(Ok(0), |old| {
            old.revision
                .checked_add(1)
                .ok_or("backup configuration revision overflow")
        })?,
        previous: old
            .as_ref()
            .map(|old| canonical_bytes(old).map(|bytes| digest_bytes(&bytes)))
            .transpose()
            .map_err(|_| "backup configuration encoding")?
            .unwrap_or_else(|| "0".repeat(64)),
        workspace: trust.trust().configuration().workspace.clone(),
        vault,
        staging,
        sync_roots,
        automatic: *automatic,
    };
    let bytes = canonical_bytes(&value).map_err(|_| "backup configuration encoding")?;
    if bytes.len() > LIMIT {
        return Err("backup configuration exceeds bound".into());
    }
    let path = trust.directory().join(name(value.revision));
    let temporary = path.with_extension(format!("{}.partial", vcp_domain::CommandId::new()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| "backup configuration staging failed")?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| "backup configuration flush failed")?;
    drop(file);
    fs::hard_link(&temporary, &path).map_err(|_| "backup configuration publication conflict")?;
    fs::remove_file(temporary)
        .map_err(|_| "backup configuration published; temporary cleanup pending")?;
    #[cfg(unix)]
    {
        std::fs::File::open(trust.directory())
            .and_then(|directory| directory.sync_all())
            .map_err(|_| "backup setup directory flush failed")?;
    }
    Ok(value)
}
