// SPDX-License-Identifier: Apache-2.0
//! Explicit local trust enrollment without provider setup or task dispatch.
use std::path::Path;
use vcp_domain::{
    workspace::{Trust, Workspace},
    Revision, WorkspaceId,
};
use vcp_store::{contract::Collection, Store};

pub async fn execute(
    data: &Path,
    destination: &Path,
    workspace: &WorkspaceId,
    expected: Revision,
) -> Result<serde_json::Value, String> {
    let directory = crate::settings::workspace_directory(data, destination)?
        .ok_or("workspace is not registered; restore and rebind first")?;
    let lease = crate::selection::Lease::shared(data, &directory)?;
    let (entry, _) = lease.descriptor()?.ok_or("workspace descriptor missing")?;
    if entry.rebind_pending || &entry.config.workspace != workspace {
        return Err("workspace identity differs or restore rebind is pending".into());
    }
    let root = crate::settings::registry_root(destination)?;
    let pin = root.hold(None, true).map_err(|e| e.to_string())?;
    let identity = entry
        .identity
        .as_ref()
        .ok_or("workspace requires physical rebind")?;
    crate::binding::verify(&root, identity)?;
    let store = Store::open(
        &entry.config.canonical_root,
        entry.config.backend,
        &[root.path().to_owned()],
    )
    .await
    .map_err(|e| e.to_string())?;
    let mut engine = vcp_engine::Engine::new(store).map_err(|e| e.to_string())?;
    let current: Workspace = engine
        .store()
        .state()
        .record(Collection::Workspace, workspace.as_str(), workspace)
        .and_then(|row| row.decode())
        .map_err(|e| e.to_string())?;
    if current.binding != entry.config.binding || current.revision != expected {
        return Err("workspace binding or revision changed; inspect before trusting".into());
    }
    // Existing policy and denials remain authoritative. This command never
    // installs execution profiles, automatic effects, or a new policy mode.
    let policy = vcp_engine::policy::current(engine.store().state(), workspace)
        .map_err(|e| e.to_string())?;
    let policy_revision = policy.revision;
    let policy_mode = policy.mode;
    crate::binding::verify(&root, identity)?;
    if root
        .hold(None, true)
        .map_err(|e| e.to_string())?
        .native_identity
        != pin.native_identity
    {
        return Err("workspace directory changed before trust enrollment".into());
    }
    if current.trust != Trust::Trusted {
        let envelope = vcp_protocol::command::CommandEnvelope {
            version: vcp_protocol::version::VERSION,
            id: vcp_domain::CommandId::new(),
            workspace: workspace.clone(),
            session: entry.config.session.clone(),
            task: None,
            caller: entry.config.actor.clone(),
            controller: engine.controller().clone(),
            owner_epoch: engine.owner_epoch(),
            expected,
            steering: vcp_domain::SteeringRevision::ZERO,
            payload: vcp_protocol::command::Command::SetWorkspaceTrust {
                trust: Trust::Trusted,
            },
        };
        let access = vcp_engine::Access {
            actor: entry.config.actor.clone(),
            workspace: workspace.clone(),
            session: entry.config.session.clone(),
            authority: current.authority,
            read: true,
            write: true,
            bootstrap: false,
        };
        engine
            .handle(
                envelope,
                &access,
                &vcp_engine::HostFacts::inspect(crate::settings::now()),
            )
            .await
            .map_err(|e| e.to_string())?;
    }
    let next: Workspace = engine
        .store()
        .state()
        .record(Collection::Workspace, workspace.as_str(), workspace)
        .and_then(|row| row.decode())
        .map_err(|e| e.to_string())?;
    engine
        .into_store()
        .close()
        .await
        .map_err(|e| e.to_string())?;
    Ok(
        serde_json::json!({"workspace":workspace,"trust":next.trust,"revision":next.revision,
        "authority":next.authority,"policy_revision":policy_revision,"policy_mode":policy_mode,
        "tasks_resumed":false,"provider_requests":0,"execution_profiles_loaded":false,
        "next_action":"existing policy denials still apply; explicitly capture or back up with the selected local Git executable"}),
    )
}
