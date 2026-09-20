// SPDX-License-Identifier: Apache-2.0
//! Explicit local reconciliation; never copies state or resumes execution.
use crate::{
    binding,
    settings::{self, WorkspaceEntry},
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use vcp_domain::{ids::*, revision::*, workspace::Workspace};
use vcp_engine::{Access, Engine, HostFacts};
use vcp_protocol::{
    command::{Command, CommandEnvelope},
    digest_bytes,
};
use vcp_repository::{Root, RootIdentity};
use vcp_store::{contract::Collection, Store};

fn open_root(path: &Path, workspace: &WorkspaceId) -> Result<Root, String> {
    Root::open(
        RootIdentity {
            workspace: workspace.clone(),
            root: RootId::parse(workspace.as_str()).map_err(|e| e.to_string())?,
            repository: "explicit-local-reconciliation".into(),
            worktree: "explicit-local-reconciliation".into(),
            binding: Revision::ZERO,
        },
        path,
    )
    .map_err(|_| {
        "rebind requires an accessible local root without redirected path components".into()
    })
}

/// Select one existing descriptor. Its original directory remains the owner
/// location after rebind; callers discover it by its updated binding root.
fn selected(
    data: &Root,
    workspace: &WorkspaceId,
    destination: &Path,
) -> Result<(PathBuf, WorkspaceEntry), String> {
    let _directory = data
        .hold(Some(Path::new("workspaces")), true)
        .map_err(|_| "no accessible durable workspace registry")?;
    let mut selected = None;
    for (index, row) in std::fs::read_dir(data.path().join("workspaces"))
        .map_err(|_| "workspace registry unavailable")?
        .enumerate()
    {
        if index >= 4096 {
            return Err("workspace registry exceeds reconciliation scan limit".into());
        }
        let row = row.map_err(|_| "workspace registry entry unavailable")?;
        if !row
            .file_type()
            .map_err(|_| "workspace registry entry type unavailable")?
            .is_dir()
        {
            continue;
        }
        let relative = Path::new("workspaces")
            .join(row.file_name())
            .join("workspace.json");
        let source = match data.read(&relative, 256 * 1024) {
            Ok(source) => source,
            Err(vcp_repository::Error::Io(error))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                continue
            }
            Err(_) => return Err("workspace descriptor unavailable or redirected".into()),
        };
        let entry: WorkspaceEntry = serde_json::from_slice(&source.bytes)
            .map_err(|_| "invalid workspace descriptor in registry")?;
        if entry.config.workspace != *workspace {
            let bound = Path::new(&entry.config.binding.root);
            if settings::within(bound, destination) && settings::within(destination, bound) {
                return Err("destination is already bound to another durable workspace".into());
            }
            continue;
        }
        if entry.version != 1 || entry.config.canonical_root != row.path().join("canonical") {
            return Err("workspace descriptor canonical location mismatch".into());
        }
        if selected.is_some() {
            return Err(
                "ambiguous durable workspace identity; reconcile duplicate descriptors first"
                    .into(),
            );
        }
        selected = Some((data.path().join(relative), entry));
    }
    selected.ok_or("durable workspace identity was not found in this data root".into())
}

pub async fn rebind(
    data: &Path,
    workspace: &Path,
    workspace_id: &WorkspaceId,
) -> Result<Value, String> {
    let data_root = open_root(data, workspace_id)?;
    let _data_pin = data_root.hold(None, true).map_err(|e| e.to_string())?;
    let destination = open_root(workspace, workspace_id)?;
    let _destination_pin = destination.hold(None, true).map_err(|e| e.to_string())?;
    let identity = binding::capture(&destination)?;
    let (entry_path, mut entry) = selected(&data_root, workspace_id, destination.path())?;
    let canonical = settings::local_path(&entry.config.canonical_root, destination.path())?;
    if canonical != entry.config.canonical_root || !canonical.join("format.json").is_file() {
        return Err(
            "existing canonical store is missing or redirected; restore it before rebind".into(),
        );
    }
    let store = Store::open(
        &canonical,
        entry.config.backend,
        &[destination.path().to_owned()],
    )
    .await
    .map_err(|error| match error {
        vcp_store::Error::Conflict("canonical root already has an owner") => {
            "workspace has an active owner; close it before rebind".into()
        }
        _ => error.to_string(),
    })?;
    let mut engine = Engine::new(store).map_err(|e| e.to_string())?;
    let current: Workspace = engine
        .store()
        .state()
        .record(Collection::Workspace, workspace_id.as_str(), workspace_id)
        .and_then(|row| row.decode())
        .map_err(|e| e.to_string())?;
    let mut next_binding = current.binding.clone();
    next_binding.root = destination.path().to_string_lossy().into_owned();
    next_binding.repository = digest_bytes(
        identity
            .git_directory_identity
            .as_deref()
            .unwrap_or(&identity.directory_identity)
            .as_bytes(),
    );
    next_binding.worktree = digest_bytes(identity.directory_identity.as_bytes());
    let changed = next_binding != current.binding;
    if changed {
        let command = CommandEnvelope {
            version: vcp_protocol::version::VERSION,
            id: CommandId::new(),
            workspace: workspace_id.clone(),
            session: entry.config.session.clone(),
            task: None,
            caller: entry.config.actor.clone(),
            controller: engine.controller().clone(),
            owner_epoch: engine.owner_epoch(),
            expected: current.revision,
            steering: SteeringRevision::ZERO,
            payload: Command::Rebind {
                binding: next_binding,
            },
        };
        let access = Access {
            actor: entry.config.actor.clone(),
            workspace: workspace_id.clone(),
            session: entry.config.session.clone(),
            authority: current.authority,
            read: true,
            write: true,
            bootstrap: false,
        };
        engine
            .handle(command, &access, &HostFacts::inspect(settings::now()))
            .await
            .map_err(|e| e.to_string())?;
    }
    let bound: Workspace = engine
        .store()
        .state()
        .record(Collection::Workspace, workspace_id.as_str(), workspace_id)
        .and_then(|row| row.decode())
        .map_err(|e| e.to_string())?;
    // Commit precedes hint publication. A retry reads the canonical binding and
    // repairs the descriptor without repeating the authority-changing command.
    binding::verify(&destination, &identity)?;
    entry.config.binding = bound.binding.clone();
    entry.identity = Some(identity);
    settings::save(&entry_path, &entry)?;
    let result = json!({
        "workspace":workspace_id, "root":bound.binding.root,
        "binding_revision":bound.binding.revision, "authority":bound.authority,
        "trust":bound.trust, "rebound":changed, "history_preserved":true,
        "tasks_resumed":false, "descriptor":entry_path,
        "next_action":"inspect unfinished tasks and reconcile inputs/effects before explicit resume"
    });
    engine
        .into_store()
        .close()
        .await
        .map_err(|e| e.to_string())?;
    Ok(result)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use vcp_domain::{
        accounting::*,
        workspace::{Binding, Trust},
    };
    use vcp_lifecycle::foundation::{CanonicalHost, Config};
    use vcp_store::BackendKind;

    fn config(canonical_root: PathBuf, root: &Path, backend: BackendKind) -> Config {
        let currency: Currency = "USD".to_owned().try_into().unwrap();
        Config {
            canonical_root,
            backend,
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
            binding: Binding {
                host: HostId::new(),
                root: root.to_string_lossy().into_owned(),
                repository: "original".into(),
                worktree: "original".into(),
                revision: Revision::ZERO,
            },
            actor: ActorId::new(),
            root_task: TaskId::new(),
            cap: Money {
                currency: currency.clone(),
                micros: Micros::new(1000),
            },
            protected: Micros::ZERO,
            price: PriceSnapshot {
                id: "a".repeat(64),
                provider: "fixture".into(),
                model: "fixture/model".into(),
                currency,
                capability: "b".repeat(64),
                valid_until: Timestamp::new(u64::MAX),
                rates: [
                    ChargeCategory::Input,
                    ChargeCategory::Output,
                    ChargeCategory::CacheRead,
                    ChargeCategory::CacheWrite,
                    ChargeCategory::Request,
                    ChargeCategory::ProviderTool,
                ]
                .into_iter()
                .map(|kind| {
                    (
                        kind,
                        Rate {
                            micros: Micros::ZERO,
                            per_units: Units::new(1),
                        },
                    )
                })
                .collect(),
            },
            input_ceiling: Units::new(4096),
            output_ceiling: Units::new(1024),
            artifact_limit: ByteCount::new(vcp_store::artifact::DEFAULT_ARTIFACT_LIMIT),
            host_tool_denials: vec![],
        }
    }

    #[tokio::test]
    async fn moved_root_rebind_preserves_store_and_repairs_descriptor_without_repeating_command() {
        for backend in [BackendKind::Files, BackendKind::Sqlite] {
            let temp = tempfile::tempdir().unwrap();
            let base = temp.path().canonicalize().unwrap();
            let data = base.join("data");
            let directory = data.join("workspaces/original-key");
            let original = base.join("original");
            let moved = base.join("moved");
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::create_dir(&original).unwrap();
            let config = config(directory.join("canonical"), &original, backend);
            let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
            let entry_path = directory.join("workspace.json");
            settings::save(
                &entry_path,
                &WorkspaceEntry {
                    version: 1,
                    config: config.clone(),
                    identity: None,
                },
            )
            .unwrap();
            let before = settings::read_bounded(&entry_path, 256 * 1024).unwrap();
            assert!(rebind(&data, &original, &config.workspace)
                .await
                .unwrap_err()
                .contains("active owner"));
            owner.close().await.unwrap();
            drop(host);
            std::fs::rename(&original, &moved).unwrap();
            let result = rebind(&data, &moved, &config.workspace).await.unwrap();
            assert_eq!(result["rebound"], true);
            assert_eq!(result["trust"], "untrusted");
            assert_eq!(result["tasks_resumed"], false);
            let store = Store::open(&config.canonical_root, backend, &[])
                .await
                .unwrap();
            let bound: Workspace = store
                .state()
                .record(
                    Collection::Workspace,
                    config.workspace.as_str(),
                    &config.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(bound.trust, Trust::Untrusted);
            assert!(bound.authority > AuthorityRevision::ZERO);
            assert!(store
                .state()
                .record(
                    Collection::Session,
                    config.session.as_str(),
                    &config.workspace
                )
                .is_ok());
            let watermark = store.state().watermark;
            store.close().await.unwrap();
            // Simulate a crash after canonical commit but before descriptor save.
            std::fs::write(&entry_path, before).unwrap();
            let repaired = rebind(&data, &moved, &config.workspace).await.unwrap();
            assert_eq!(repaired["rebound"], false);
            let store = Store::open(&config.canonical_root, backend, &[])
                .await
                .unwrap();
            assert_eq!(store.state().watermark, watermark);
            store.close().await.unwrap();
            let entry: WorkspaceEntry =
                serde_json::from_slice(&settings::read_bounded(&entry_path, 256 * 1024).unwrap())
                    .unwrap();
            assert_eq!(entry.config.canonical_root, config.canonical_root);
            assert_eq!(Path::new(&entry.config.binding.root), moved);
            assert!(entry.identity.is_some());
            let duplicate = data.join("workspaces/duplicate");
            std::fs::create_dir(&duplicate).unwrap();
            let mut duplicate_entry = entry;
            duplicate_entry.config.canonical_root = duplicate.join("canonical");
            settings::save(&duplicate.join("workspace.json"), &duplicate_entry).unwrap();
            assert!(rebind(&data, &moved, &config.workspace)
                .await
                .unwrap_err()
                .contains("ambiguous"));
        }
    }
}
