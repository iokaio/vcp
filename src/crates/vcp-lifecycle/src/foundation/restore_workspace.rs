// SPDX-License-Identifier: Apache-2.0
//! Explicit checkpoint recovery into a new workspace, never over existing work.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use vcp_domain::{
    artifact::{ArtifactDescriptor, CaptureState},
    workspace::Workspace,
    CommandId, RootId, WorkspaceId,
};
use vcp_protocol::{canonical_bytes, digest_bytes};
use vcp_repository::{path::HeldPath, FileVersion, Root, RootIdentity};
use vcp_store::{contract::Collection, restore_import::Imported};
type Result<T> = std::result::Result<T, String>;

/// Derive a local descriptor without mutating the authenticated import. Publish
/// it as rebind-pending, then use the ordinary canonical Rebind command. This
/// ordering preserves the exact import proof across a pre-activation crash.
pub fn restored_configuration(
    store: &vcp_store::Store,
    imported: &Imported,
    materialized: &Materialized,
    actor: vcp_domain::ActorId,
    host: vcp_domain::HostId,
    prior: Option<&super::Config>,
) -> Result<super::Config> {
    use vcp_domain::{
        accounting::{Ledger, Money, PriceSnapshot},
        task::Task,
        workspace::Trust,
        ByteCount, Micros, Timestamp, Units,
    };
    materialized.revalidate()?;
    if materialized.workspace() != imported.workspace()
        || materialized.state_digest() != imported.state_digest()
        || materialized.source_manifest() != imported.source_manifest()
        || digest_bytes(&canonical_bytes(store.state()).map_err(|e| e.to_string())?)
            != imported.state_digest()
        || store
            .canonical_anchor()
            .canonicalize()
            .map_err(|_| "import root unavailable")?
            != imported
                .root()
                .canonicalize()
                .map_err(|_| "import root unavailable")?
        || prior.is_some_and(|config| &config.workspace != imported.workspace())
    {
        return Err("restored configuration proof or source changed".into());
    }
    let checkpoint = imported
        .checkpoint()
        .ok_or("restored workspace checkpoint missing")?;
    let artifact: ArtifactDescriptor = store
        .state()
        .record(
            Collection::Artifact,
            checkpoint.manifest.as_str(),
            imported.workspace(),
        )
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;
    let scope = &artifact.spec.scope;
    let task: Task = store
        .state()
        .record(Collection::Task, scope.task.as_str(), imported.workspace())
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;
    if task.parent.is_some() || task.scope != *scope {
        return Err("restored checkpoint does not identify a root task".into());
    }
    let workspace: Workspace = store
        .state()
        .record(
            Collection::Workspace,
            imported.workspace().as_str(),
            imported.workspace(),
        )
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;
    if workspace.trust != Trust::Untrusted {
        return Err("restored execution authority was not sanitized".into());
    }
    let ledger = store
        .state()
        .records
        .get(&vcp_store::contract::key(
            Collection::Ledger,
            scope.task.as_str(),
        ))
        .map(|row| row.decode::<Ledger>())
        .transpose()
        .map_err(|e| e.to_string())?;
    if ledger.as_ref().is_some_and(|ledger| ledger.scope != *scope) {
        return Err("restored ledger scope differs".into());
    }
    // USD/zero is an unconfigured local budget ceiling when no ledger exists,
    // not a price estimate or a newly admitted canonical monetary fact.
    let cap = ledger
        .as_ref()
        .map(|ledger| Money {
            currency: ledger.currency.clone(),
            micros: ledger.cap,
        })
        .or_else(|| prior.map(|config| config.cap.clone()))
        .unwrap_or(Money {
            currency: "USD"
                .to_owned()
                .try_into()
                .map_err(|_| "default currency invalid")?,
            micros: Micros::ZERO,
        });
    let protected = ledger
        .as_ref()
        .map(|ledger| ledger.protected)
        .or_else(|| prior.map(|config| config.protected))
        .unwrap_or(Micros::ZERO);
    let native = materialized
        .root()
        .hold(None, true)
        .map_err(|e| e.to_string())?;
    let mut binding = workspace.binding;
    binding.host = host;
    binding.root = materialized.root().path().to_string_lossy().into_owned();
    binding.repository = digest_bytes(native.native_identity.as_bytes());
    binding.worktree = binding.repository.clone();
    let config = super::Config {
        canonical_root: imported.root().to_owned(),
        backend: imported.backend(),
        workspace: imported.workspace().clone(),
        session: scope.session.clone(),
        root_task: scope.task.clone(),
        actor,
        binding,
        cap: cap.clone(),
        protected,
        price: prior
            .map(|config| config.price.clone())
            .unwrap_or(PriceSnapshot {
                id: digest_bytes(b"vcp-restored-unconfigured-price/1"),
                provider: "unconfigured".into(),
                model: "unconfigured".into(),
                currency: cap.currency,
                capability: digest_bytes(b"vcp-restored-unconfigured-capability/1"),
                valid_until: Timestamp::ZERO,
                rates: BTreeMap::new(),
            }),
        input_ceiling: prior
            .map(|config| config.input_ceiling)
            .unwrap_or(Units::new(1)),
        output_ceiling: prior
            .map(|config| config.output_ceiling)
            .unwrap_or(Units::new(1)),
        artifact_limit: prior
            .map(|config| config.artifact_limit)
            .unwrap_or(ByteCount::new(vcp_store::artifact::DEFAULT_ARTIFACT_LIMIT)),
        host_tool_denials: prior
            .map(|config| config.host_tool_denials.clone())
            .unwrap_or_default(),
    };
    materialized.revalidate()?;
    Ok(config)
}

pub struct Materialized {
    root: Root,
    state_digest: String,
    source_manifest: String,
    files: Vec<FileVersion>,
    directories: BTreeSet<String>,
    _pins: Vec<HeldPath>,
    retained_staging: Vec<PathBuf>,
}
impl Materialized {
    pub fn root(&self) -> &Root {
        &self.root
    }
    pub fn workspace(&self) -> &WorkspaceId {
        &self.root.identity.workspace
    }
    pub fn state_digest(&self) -> &str {
        &self.state_digest
    }
    pub fn source_manifest(&self) -> &str {
        &self.source_manifest
    }
    /// Interrupted preparation roots are retained, never recursively deleted.
    pub fn retained_staging(&self) -> &[PathBuf] {
        &self.retained_staging
    }
    pub fn revalidate(&self) -> Result<()> {
        self.root.hold(None, true).map_err(|e| e.to_string())?;
        for file in &self.files {
            self.root.revalidate(file).map_err(|e| e.to_string())?;
        }
        inventory(
            &self.root,
            &self.files.iter().map(|f| f.path.clone()).collect(),
            &self.directories,
            &AtomicBool::new(false),
        )?;
        Ok(())
    }
}
#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Intent {
    version: u32,
    workspace: WorkspaceId,
    source_manifest: String,
    state_digest: String,
    checkpoint: String,
    destination: PathBuf,
    parent_identity: String,
    files: BTreeMap<String, String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Attempt {
    name: String,
    native_identity: Option<String>,
}
struct Prepared {
    identity: RootIdentity,
    intent: Intent,
    journal: PathBuf,
    bytes: BTreeMap<String, Vec<u8>>,
}
struct Cancel(Option<Arc<AtomicBool>>);
impl Drop for Cancel {
    fn drop(&mut self) {
        if let Some(flag) = &self.0 {
            flag.store(true, Ordering::Release);
        }
    }
}
fn check(cancelled: &AtomicBool) -> Result<()> {
    if cancelled.load(Ordering::Acquire) {
        Err("workspace materialization cancelled; prior roots preserved".into())
    } else {
        Ok(())
    }
}
fn safe_name(name: &str) -> Result<()> {
    if vcp_repository::path::relative(Path::new(name)).map_err(|e| e.to_string())? != name
        || name.split('/').any(|p| p.eq_ignore_ascii_case(".git"))
    {
        return Err("invalid checkpoint source path".into());
    }
    Ok(())
}
fn immutable(root: &Root, name: &str, bytes: &[u8]) -> Result<()> {
    if root.path().join(name).exists() {
        if root
            .read(Path::new(name), 4 * 1024 * 1024)
            .map_err(|e| e.to_string())?
            .bytes
            == bytes
        {
            return Ok(());
        }
        return Err("workspace materialization receipt changed".into());
    }
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("materialization receipt exceeds bound".into());
    }
    let temporary = root.path().join(format!("{}.partial", CommandId::new()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000).share_mode(0);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|_| "materialization receipt staging unavailable")?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| "materialization receipt write failed")?;
    drop(file);
    fs::hard_link(&temporary, root.path().join(name))
        .map_err(|_| "materialization receipt publication failed")?;
    // This path was created exclusively by this invocation under a pinned directory.
    // Retaining a temporary receipt on failure is safe; never remove other entries.
    let _ = fs::remove_file(&temporary);
    Ok(())
}

/// Requires a new destination outside local canonical/vault/sync roots. On retry,
/// an existing destination is accepted only by recorded native identity and bytes.
/// Git metadata is retained audit evidence, not installed executable repository state.
pub async fn materialize(
    imported: &Imported,
    destination: &Path,
    operation: &CommandId,
    forbidden: &[PathBuf],
    cancelled: Arc<AtomicBool>,
) -> Result<Materialized> {
    let mut cancel_guard = Cancel(Some(cancelled.clone()));
    check(&cancelled)?;
    if !destination.is_absolute() || forbidden.is_empty() {
        return Err("explicit local destination and exclusion roots required".into());
    }
    let operation = operation.as_str();
    if operation.len() != 36
        || !operation.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
    {
        return Err("opaque materialization operation required".into());
    }
    let checkpoint = imported
        .checkpoint()
        .ok_or("authenticated workspace checkpoint missing")?;
    let store = imported
        .reopen_verified()
        .await
        .map_err(|e| e.to_string())?;
    let workspace: Workspace = store
        .state()
        .record(
            Collection::Workspace,
            imported.workspace().as_str(),
            imported.workspace(),
        )
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;
    let identity = RootIdentity {
        workspace: workspace.id.clone(),
        root: RootId::parse(workspace.id.as_str()).map_err(|e| e.to_string())?,
        repository: workspace.binding.repository,
        worktree: workspace.binding.worktree,
        binding: workspace.binding.revision,
    };
    let parent = Root::open(
        identity.clone(),
        destination.parent().ok_or("destination parent missing")?,
    )
    .map_err(|e| e.to_string())?;
    let parent_pin = parent.hold(None, true).map_err(|e| e.to_string())?;
    let name = destination
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or("destination name invalid")?;
    safe_name(name)?;
    let destination = parent.path().join(name);
    let canonical_root = imported.root().to_owned();
    for root in forbidden.iter().chain(std::iter::once(&canonical_root)) {
        let root = root
            .canonicalize()
            .map_err(|_| "materialization exclusion root unavailable")?;
        if destination.starts_with(&root) || root.starts_with(&destination) {
            return Err("workspace materialization overlaps a protected root".into());
        }
    }
    if checkpoint.sources.len() > 4096 {
        return Err("workspace source count exceeds bound".into());
    }
    let mut bytes = BTreeMap::new();
    let mut names = BTreeSet::new();
    let mut total = 0usize;
    for (path, id) in &checkpoint.sources {
        check(&cancelled)?;
        safe_name(path)?;
        if !names.insert(path.to_lowercase()) {
            return Err("checkpoint source names collide on Windows".into());
        }
        let descriptor: ArtifactDescriptor = store
            .state()
            .record(Collection::Artifact, id.as_str(), imported.workspace())
            .map_err(|e| e.to_string())?
            .decode()
            .map_err(|e| e.to_string())?;
        if descriptor.state != CaptureState::Complete || descriptor.length.get() > 4 * 1024 * 1024 {
            return Err("checkpoint source unavailable or oversized".into());
        }
        let mut content = Vec::new();
        store
            .spool()
            .read(&descriptor, &mut content)
            .map_err(|e| e.to_string())?;
        total = total
            .checked_add(content.len())
            .ok_or("checkpoint size overflow")?;
        if total > 8 * 1024 * 1024 {
            return Err("checkpoint source bytes exceed bound".into());
        }
        bytes.insert(path.clone(), content);
    }
    for path in &names {
        let mut parent = Path::new(path).parent();
        while let Some(part) = parent {
            if names.contains(&part.to_string_lossy().replace('\\', "/")) {
                return Err("checkpoint file and directory collide".into());
            }
            parent = part.parent();
        }
    }
    let prepared = Prepared {
        identity,
        intent: Intent {
            version: 1,
            workspace: workspace.id,
            source_manifest: imported.source_manifest().into(),
            state_digest: imported.state_digest().into(),
            checkpoint: checkpoint.manifest.to_string(),
            destination,
            parent_identity: parent_pin.native_identity.clone(),
            files: bytes
                .iter()
                .map(|(p, b)| (p.clone(), digest_bytes(b)))
                .collect(),
        },
        journal: imported
            .root()
            .join(format!("workspace-materialization-{operation}")),
        bytes,
    };
    let stopped = cancelled.clone();
    let proof = tokio::task::spawn_blocking(move || execute(prepared, stopped))
        .await
        .map_err(|_| "materialization worker failed")??;
    store.close().await.map_err(|e| e.to_string())?;
    check(&cancelled)?;
    cancel_guard.0 = None;
    Ok(proof)
}

fn execute(prepared: Prepared, cancelled: Arc<AtomicBool>) -> Result<Materialized> {
    execute_with(prepared, cancelled, &|_| {})
}
fn execute_with(
    prepared: Prepared,
    cancelled: Arc<AtomicBool>,
    barrier: &dyn Fn(&str),
) -> Result<Materialized> {
    check(&cancelled)?;
    let parent_path = prepared
        .intent
        .destination
        .parent()
        .ok_or("destination parent missing")?;
    let parent = Root::open(prepared.identity.clone(), parent_path).map_err(|e| e.to_string())?;
    let parent_pin = parent.hold(None, true).map_err(|e| e.to_string())?;
    if parent_pin.native_identity != prepared.intent.parent_identity {
        return Err("destination parent changed".into());
    }
    if !prepared.journal.exists() {
        fs::create_dir(&prepared.journal).map_err(|_| "materialization journal unavailable")?;
    }
    let journal =
        Root::open(prepared.identity.clone(), &prepared.journal).map_err(|e| e.to_string())?;
    let _journal_pin = journal.hold(None, true).map_err(|e| e.to_string())?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000).share_mode(1 | 2);
    }
    let owner = options
        .open(journal.path().join("owner.lock"))
        .map_err(|_| "materialization owner unavailable")?;
    let metadata = owner
        .metadata()
        .map_err(|_| "materialization owner metadata unavailable")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("materialization owner redirected".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("materialization owner redirected".into());
        }
    }
    owner
        .try_lock()
        .map_err(|_| "workspace materialization already in progress")?;
    immutable(
        &journal,
        "intent.json",
        &canonical_bytes(&prepared.intent).map_err(|e| e.to_string())?,
    )?;
    let mut attempts = Vec::new();
    for n in 0..4 {
        let name = format!("allocated-{n}.json");
        if journal.path().join(&name).exists() {
            let bytes = journal
                .read(Path::new(&name), 65536)
                .map_err(|e| e.to_string())?
                .bytes;
            let mut row: Attempt = serde_json::from_slice(&bytes)
                .map_err(|_| "invalid materialization attempt receipt")?;
            if row.name.len() != 36
                || !row.name.bytes().enumerate().all(|(i, b)| {
                    if [8, 13, 18, 23].contains(&i) {
                        b == b'-'
                    } else {
                        b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
                    }
                })
            {
                return Err("invalid materialization stage name".into());
            }
            let completed = format!("attempt-{n}.json");
            if journal.path().join(&completed).exists() {
                let saved: Attempt = serde_json::from_slice(
                    &journal
                        .read(Path::new(&completed), 65536)
                        .map_err(|e| e.to_string())?
                        .bytes,
                )
                .map_err(|_| "invalid materialization identity receipt")?;
                if saved.name != row.name || saved.native_identity.is_none() {
                    return Err("materialization allocation identity differs".into());
                }
                row = saved;
            }
            attempts.push(row);
        } else {
            break;
        }
    }
    if prepared.intent.destination.exists() {
        let root = Root::open(prepared.identity.clone(), &prepared.intent.destination)
            .map_err(|e| e.to_string())?;
        let pin = root.hold(None, true).map_err(|e| e.to_string())?;
        if !attempts
            .iter()
            .any(|a| a.native_identity.as_deref() == Some(pin.native_identity.as_str()))
        {
            return Err("destination already exists; divergent work preserved".into());
        }
        return finish(root, &prepared, &attempts, &parent, &cancelled);
    }
    if attempts.len() >= 4 {
        return Err(
            "materialization retry bound reached; retained preparation roots require review".into(),
        );
    }
    let name = CommandId::new().to_string();
    immutable(
        &journal,
        &format!("allocated-{}.json", attempts.len()),
        &canonical_bytes(&Attempt {
            name: name.clone(),
            native_identity: None,
        })
        .map_err(|e| e.to_string())?,
    )?;
    barrier("allocated");
    check(&cancelled)?;
    let stage =
        vcp_repository::restore::Staging::create(&parent, &name).map_err(|e| e.to_string())?;
    barrier("created");
    check(&cancelled)?;
    let attempt = Attempt {
        name,
        native_identity: Some(stage.native_identity().into()),
    };
    immutable(
        &journal,
        &format!("attempt-{}.json", attempts.len()),
        &canonical_bytes(&attempt).map_err(|e| e.to_string())?,
    )?;
    attempts.push(attempt);
    barrier("attempt");
    let stage_root =
        Root::open(prepared.identity.clone(), stage.path()).map_err(|e| e.to_string())?;
    for (path, bytes) in &prepared.bytes {
        check(&cancelled)?;
        let parts: Vec<_> = path.split('/').collect();
        let mut relative = PathBuf::new();
        for part in &parts[..parts.len() - 1] {
            let _pin = stage_root
                .hold(
                    if relative.as_os_str().is_empty() {
                        None
                    } else {
                        Some(&relative)
                    },
                    true,
                )
                .map_err(|e| e.to_string())?;
            relative.push(part);
            if !stage_root.path().join(&relative).exists() {
                fs::create_dir(stage_root.path().join(&relative))
                    .map_err(|_| "checkpoint directory creation failed")?;
            }
            stage_root
                .hold(Some(&relative), true)
                .map_err(|e| e.to_string())?;
        }
        let _parent = stage_root
            .hold(
                if relative.as_os_str().is_empty() {
                    None
                } else {
                    Some(&relative)
                },
                true,
            )
            .map_err(|e| e.to_string())?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x0020_0000).share_mode(0);
        }
        let mut file = options
            .open(stage_root.path().join(path))
            .map_err(|_| "checkpoint source creation failed; preparation retained")?;
        for chunk in bytes.chunks(64 * 1024) {
            check(&cancelled)?;
            file.write_all(chunk)
                .map_err(|_| "checkpoint source write failed; preparation retained")?;
        }
        file.sync_all()
            .map_err(|_| "checkpoint source synchronization failed")?;
        barrier("file");
    }
    check(&cancelled)?;
    // Validate the complete staged tree before publishing its directory name.
    drop(finish(
        stage_root, &prepared, &attempts, &parent, &cancelled,
    )?);
    barrier("before_publish");
    check(&cancelled)?;
    let name = prepared
        .intent
        .destination
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("destination name invalid")?;
    let published = stage.publish(name).map_err(|e| e.to_string())?;
    let (root, pin) = published.into_parts();
    barrier("after_publish");
    let mut proof = finish(root, &prepared, &attempts, &parent, &cancelled)?;
    proof._pins.push(pin);
    Ok(proof)
}
fn finish(
    root: Root,
    prepared: &Prepared,
    attempts: &[Attempt],
    parent: &Root,
    cancelled: &AtomicBool,
) -> Result<Materialized> {
    check(cancelled)?;
    let mut pins = vec![root.hold(None, true).map_err(|e| e.to_string())?];
    let mut files = Vec::new();
    let mut directories = BTreeSet::new();
    for (path, expected) in &prepared.intent.files {
        check(cancelled)?;
        let observed = root
            .version(Path::new(path), 4 * 1024 * 1024)
            .map_err(|e| e.to_string())?;
        if observed.sha256 != *expected {
            return Err("materialized checkpoint source changed".into());
        }
        pins.push(root.pin_version(&observed).map_err(|e| e.to_string())?);
        files.push(observed);
        let mut parent = Path::new(path).parent();
        while let Some(part) = parent {
            if !part.as_os_str().is_empty() {
                directories.insert(part.to_string_lossy().replace('\\', "/"));
            }
            parent = part.parent();
        }
    }
    pins.extend(inventory(
        &root,
        &prepared.intent.files.keys().cloned().collect(),
        &directories,
        cancelled,
    )?);
    check(cancelled)?;
    Ok(Materialized {
        root,
        state_digest: prepared.intent.state_digest.clone(),
        source_manifest: prepared.intent.source_manifest.clone(),
        files,
        directories,
        _pins: pins,
        retained_staging: attempts
            .iter()
            .map(|a| parent.path().join(&a.name))
            .filter(|p| p.exists())
            .collect(),
    })
}
fn inventory(
    root: &Root,
    files: &BTreeSet<String>,
    directories: &BTreeSet<String>,
    cancelled: &AtomicBool,
) -> Result<Vec<HeldPath>> {
    let mut pins = Vec::new();
    let mut queue = vec![PathBuf::new()];
    let mut count = 0;
    while let Some(relative) = queue.pop() {
        check(cancelled)?;
        for entry in fs::read_dir(root.path().join(&relative))
            .map_err(|_| "materialized directory unavailable")?
        {
            check(cancelled)?;
            let entry = entry.map_err(|_| "materialized directory entry unavailable")?;
            count += 1;
            if count > 8192 {
                return Err("materialized directory inventory exceeds bound".into());
            }
            let path = relative.join(entry.file_name());
            let name = path.to_string_lossy().replace('\\', "/");
            if directories.contains(&name) {
                pins.push(root.hold(Some(&path), true).map_err(|e| e.to_string())?);
                queue.push(path);
            } else if !files.contains(&name) {
                return Err(
                    "materialized workspace contains unexpected files; activation refused".into(),
                );
            }
        }
    }
    Ok(pins)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn fixture(base: &Path) -> Prepared {
        let parent = base.join("workspaces");
        let canonical = base.join("canonical");
        fs::create_dir_all(&parent).unwrap();
        fs::create_dir_all(&canonical).unwrap();
        let identity = RootIdentity {
            workspace: WorkspaceId::parse("workspace").unwrap(),
            root: RootId::parse("workspace").unwrap(),
            repository: "retained-repository".into(),
            worktree: "retained-worktree".into(),
            binding: vcp_domain::Revision::ZERO,
        };
        let root = Root::open(identity.clone(), &parent).unwrap();
        let pin = root.hold(None, true).unwrap();
        let bytes: BTreeMap<String, Vec<u8>> = BTreeMap::from([
            ("src/tracked.txt".into(), b"dirty retained bytes\n".to_vec()),
            ("untracked.bin".into(), vec![0, 1, 255, 3]),
        ]);
        Prepared {
            identity,
            intent: Intent {
                version: 1,
                workspace: WorkspaceId::parse("workspace").unwrap(),
                source_manifest: "a".repeat(64),
                state_digest: "b".repeat(64),
                checkpoint: "checkpoint".into(),
                destination: root.path().join("recovered"),
                parent_identity: pin.native_identity,
                files: bytes
                    .iter()
                    .map(|(name, bytes)| (name.clone(), digest_bytes(bytes)))
                    .collect(),
            },
            journal: canonical.join("materialization"),
            bytes,
        }
    }
    #[test]
    fn checkpoint_materialization_is_exact_pinned_and_identity_bound_on_retry() {
        let temp = tempfile::tempdir().unwrap();
        let first = execute(fixture(temp.path()), Arc::new(AtomicBool::new(false))).unwrap();
        first.revalidate().unwrap();
        assert_eq!(
            fs::read(first.root().path().join("src/tracked.txt")).unwrap(),
            b"dirty retained bytes\n"
        );
        assert_eq!(
            fs::read(first.root().path().join("untracked.bin")).unwrap(),
            [0, 1, 255, 3]
        );
        assert!(fs::write(first.root().path().join("src/tracked.txt"), "overwrite").is_err());
        assert!(fs::rename(first.root().path(), temp.path().join("moved")).is_err());
        assert!(first.retained_staging().is_empty());
        drop(first);
        let retry = execute(fixture(temp.path()), Arc::new(AtomicBool::new(false))).unwrap();
        retry.revalidate().unwrap();
        drop(retry);
        let destination = fixture(temp.path()).intent.destination;
        fs::rename(&destination, temp.path().join("original")).unwrap();
        fs::create_dir(&destination).unwrap();
        fs::create_dir(destination.join("src")).unwrap();
        fs::write(
            destination.join("src/tracked.txt"),
            b"dirty retained bytes\n",
        )
        .unwrap();
        fs::write(destination.join("untracked.bin"), [0, 1, 255, 3]).unwrap();
        assert!(execute(fixture(temp.path()), Arc::new(AtomicBool::new(false))).is_err());
        assert!(temp.path().join("original/src/tracked.txt").exists());
    }
    #[test]
    fn checkpoint_materialization_preserves_existing_targets_and_changed_sources() {
        for kind in ["empty", "directory", "file"] {
            let temp = tempfile::tempdir().unwrap();
            let prepared = fixture(temp.path());
            if kind == "file" {
                fs::write(&prepared.intent.destination, b"unrelated").unwrap();
            } else {
                fs::create_dir(&prepared.intent.destination).unwrap();
                if kind == "directory" {
                    fs::write(prepared.intent.destination.join("user.txt"), b"unrelated").unwrap();
                }
            }
            assert!(execute(prepared, Arc::new(AtomicBool::new(false))).is_err());
            assert!(fixture(temp.path()).intent.destination.exists());
        }
        let temp = tempfile::tempdir().unwrap();
        drop(execute(fixture(temp.path()), Arc::new(AtomicBool::new(false))).unwrap());
        let prepared = fixture(temp.path());
        fs::write(
            prepared.intent.destination.join("src/tracked.txt"),
            b"new human edit",
        )
        .unwrap();
        assert!(execute(prepared, Arc::new(AtomicBool::new(false))).is_err());
        assert_eq!(
            fs::read(
                fixture(temp.path())
                    .intent
                    .destination
                    .join("src/tracked.txt")
            )
            .unwrap(),
            b"new human edit"
        );
    }
    #[test]
    fn checkpoint_materialization_final_revalidation_rejects_new_inventory() {
        for directory in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let proof = execute(fixture(temp.path()), Arc::new(AtomicBool::new(false))).unwrap();
            let added = proof.root().path().join(if directory {
                "new-directory"
            } else {
                "AGENTS.md"
            });
            if directory {
                fs::create_dir(&added).unwrap();
            } else {
                fs::write(&added, b"new user instructions").unwrap();
            }
            assert!(proof.revalidate().is_err());
            assert!(added.exists());
        }
    }
    #[test]
    fn checkpoint_materialization_cancellation_keeps_old_preparations_and_retries() {
        for phase in [
            "allocated",
            "created",
            "attempt",
            "file",
            "before_publish",
            "after_publish",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let cancelled = Arc::new(AtomicBool::new(false));
            assert!(
                execute_with(fixture(temp.path()), cancelled.clone(), &|at| {
                    if at == phase {
                        cancelled.store(true, Ordering::Release);
                    }
                })
                .is_err()
            );
            let proof = execute(fixture(temp.path()), Arc::new(AtomicBool::new(false))).unwrap();
            proof.revalidate().unwrap();
            assert_eq!(
                proof.retained_staging().len(),
                usize::from(!["allocated", "after_publish"].contains(&phase))
            );
        }
    }
    #[test]
    fn materialization_child() {
        let Some(base) = std::env::var_os("VCP_WORKSPACE_MATERIALIZATION_CHILD") else {
            return;
        };
        let phase = std::env::var("VCP_WORKSPACE_MATERIALIZATION_PHASE").unwrap();
        let base = PathBuf::from(base);
        execute_with(fixture(&base), Arc::new(AtomicBool::new(false)), &|at| {
            if at == phase {
                fs::write(base.join("ready"), at).unwrap();
                loop {
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        })
        .unwrap();
    }
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    #[test]
    fn checkpoint_materialization_survives_native_process_kill_at_six_boundaries() {
        for phase in [
            "allocated",
            "created",
            "attempt",
            "file",
            "before_publish",
            "after_publish",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let log = fs::File::create(temp.path().join("child.log")).unwrap();
            let mut child = Child(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "foundation::restore_workspace::tests::materialization_child",
                        "--nocapture",
                    ])
                    .env("VCP_WORKSPACE_MATERIALIZATION_CHILD", temp.path())
                    .env("VCP_WORKSPACE_MATERIALIZATION_PHASE", phase)
                    .stdout(log.try_clone().unwrap())
                    .stderr(log)
                    .spawn()
                    .unwrap(),
            );
            let until = Instant::now() + Duration::from_secs(30);
            while !temp.path().join("ready").exists() {
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "child failed: {}",
                    fs::read_to_string(temp.path().join("child.log")).unwrap()
                );
                assert!(Instant::now() < until, "materialization barrier timed out");
                std::thread::sleep(Duration::from_millis(20));
            }
            child.0.kill().unwrap();
            child.0.wait().unwrap();
            let proof = execute(fixture(temp.path()), Arc::new(AtomicBool::new(false))).unwrap();
            proof.revalidate().unwrap();
            assert_eq!(
                proof.retained_staging().len(),
                usize::from(!["allocated", "after_publish"].contains(&phase))
            );
        }
    }
}
