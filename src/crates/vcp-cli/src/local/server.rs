// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{collections::BTreeSet, path::Path};
use vcp_domain::{ids::RootId, workspace::Workspace};
use vcp_engine::{
    rpc::{RpcHost, RpcSession, ESSENTIAL_CAPABILITIES},
    Access,
};
use vcp_lifecycle::foundation::{CanonicalHost, Config};
use vcp_protocol::{
    handshake::{ConnectionLimits, ExecutionHost, ServerInfo},
    jsonrpc,
};
use vcp_store::contract::Collection;

fn selection(request: &LaunchRequest) -> Result<(crate::selection::Lease, Config), String> {
    let workspace = request
        .workspace
        .canonicalize()
        .map_err(|_| "local workspace unavailable")?;
    let data = crate::settings::local_path(
        &request
            .data
            .clone()
            .map(Ok)
            .unwrap_or_else(crate::settings::default_data)?,
        &workspace,
    )?;
    let directory = crate::settings::workspace_directory(&data, &workspace)?
        .ok_or("local attachment requires an existing durable workspace")?;
    let lease = crate::selection::Lease::shared(&data, &directory)?;
    let (entry, _) = lease
        .descriptor()?
        .ok_or("workspace descriptor unavailable")?;
    if entry.rebind_pending || Path::new(&entry.config.binding.root) != workspace {
        return Err("workspace requires explicit rebind".into());
    }
    crate::selection::validate_location(&directory, &entry)?;
    let root = vcp_repository::Root::open(
        vcp_repository::RootIdentity {
            workspace: entry.config.workspace.clone(),
            root: RootId::parse(entry.config.workspace.as_str())
                .map_err(|_| "invalid root identity")?,
            repository: entry.config.binding.repository.clone(),
            worktree: entry.config.binding.worktree.clone(),
            binding: entry.config.binding.revision,
        },
        &workspace,
    )
    .map_err(|_| "workspace binding unavailable")?;
    crate::binding::verify(
        &root,
        entry
            .identity
            .as_ref()
            .ok_or("workspace identity requires rebind")?,
    )?;
    Ok((lease, entry.config))
}

fn access(host: &CanonicalHost, config: &Config, role: Role) -> Result<Access, String> {
    let workspace: Workspace = host
        .snapshot()?
        .record(
            Collection::Workspace,
            config.workspace.as_str(),
            &config.workspace,
        )
        .and_then(|record| record.decode())
        .map_err(|_| "current workspace authority unavailable")?;
    Ok(Access {
        actor: config.actor.clone(),
        workspace: config.workspace.clone(),
        session: config.session.clone(),
        authority: workspace.authority,
        read: true,
        write: role == Role::Controller,
        bootstrap: false,
    })
}

pub(super) async fn run(mut io: Framed) -> Result<(), String> {
    let boot: Bootstrap = bootstrap(&mut io).await?;
    let parent = parent_proof(&boot)?;
    let (_selection, config) = selection(&boot.request)?;
    // This acquires the real canonical writer and performs crash recovery. No
    // provider credentials, request, root thread or task is created by attachment.
    let (host, owner) = CanonicalHost::open(config.clone())?;
    let result = async {
        let mut connection = host.public_connection(access(&host, &config, boot.request.role)?)?;
        let signal = connection.loss_signal();
        io.on_loss(move || signal.invalidate());
        let methods: Vec<String> = connection.supported_methods().iter().map(|value| (*value).into()).collect();
        let capabilities: BTreeSet<String> = methods.iter().cloned()
            .chain(ESSENTIAL_CAPABILITIES.iter().map(|value| (*value).into())).collect();
        let mut session = RpcSession::new(ServerInfo {
            engine_build: concat!("vcp/", env!("CARGO_PKG_VERSION")).into(), methods, capabilities,
            limits: ConnectionLimits { maximum_frame_bytes: framed::LIMIT as u32,
                maximum_pending_requests: 1, maximum_subscriptions: 1,
                maximum_subscriber_queue_bytes: framed::LIMIT as u32 },
            execution_host: ExecutionHost { id: "local-windows".into(), platform: "windows".into() },
            sandbox_capabilities: vec![],
        }).map_err(|_| "local RPC configuration unavailable")?;
        io.send_value(&BootstrapReply { schema: BOOTSTRAP.into(), challenge: boot.challenge,
            ready: Ready { schema: READY.into(), server: HeldProcess::current().and_then(|process| process.pin())
                .map_err(|_| "server identity unavailable")?,
                scope: vcp_protocol::methods::Scope { workspace: config.workspace.as_str().to_owned().try_into().map_err(|_| "invalid workspace")?,
                    session: config.session.as_str().to_owned().try_into().map_err(|_| "invalid session")? }, role: boot.request.role } }).await?;
        let operation = async {
            loop {
                let bytes = match io.receive().await { Ok(frame) => frame, Err(_) => break };
                if !parent.is_alive().map_err(|_| "parent liveness unavailable")? { break; }
                let current = access(&host, &config, boot.request.role)?;
                let frame = std::str::from_utf8(&bytes).map_err(|_| "invalid local UTF-8")?;
                let mut lost = io.loss();
                let response = tokio::select! {
                    biased;
                    _ = lost.wait_for(|lost| *lost) => break,
                    response = session.dispatch_host(&mut connection, &current, jsonrpc::parse_frame(frame)) => response,
                };
                if let Some(response) = response { io.send_value(&response).await?; }
                if session.is_closed() { break; }
            }
            Ok::<(), String>(())
        }.await;
        let disconnected = connection.disconnect()?.wait().await;
        operation.and(disconnected.map(|_| ()))
    }.await;
    let closed = owner.close().await;
    result.and(closed)
}
