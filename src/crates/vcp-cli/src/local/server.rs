// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::{path::Path, sync::Arc};
type Execution = Option<Arc<execution::Supervisor>>;
use vcp_domain::{ids::RootId, workspace::Workspace};
use vcp_engine::{
    rpc::{capabilities_for_methods, RpcHost, RpcSession},
    Access,
};
use vcp_lifecycle::foundation::{CanonicalHost, Config};
use vcp_protocol::{
    handshake::{ConnectionLimits, ExecutionHost, ServerInfo},
    jsonrpc,
};
use vcp_store::contract::Collection;

fn selection(
    request: &LaunchRequest,
) -> Result<(crate::selection::Lease, Config, PathBuf), String> {
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
    let mut config = entry.config;
    if let Some(task) = &request.root_task {
        if request.role != Role::Controller {
            return Err("explicit root selection requires controller bootstrap".into());
        }
        // This is a selection for this new host lifetime, never a mutation of
        // the durable descriptor or of an already running host's bindings.
        config.root_task = task.clone();
    }
    Ok((lease, config, data))
}

fn selected_root(host: &CanonicalHost, config: &Config) -> Result<(), String> {
    let state = host.snapshot()?;
    let Some(row) = state.records.get(&vcp_store::contract::key(
        Collection::Task,
        config.root_task.as_str(),
    )) else {
        return Ok(());
    };
    let task: vcp_domain::task::Task = row.decode().map_err(|_| "selected root is unavailable")?;
    if row.workspace != config.workspace
        || task.scope.workspace != config.workspace
        || task.scope.session != config.session
        || task.scope.task != config.root_task
        || task.root != config.root_task
        || task.parent.is_some()
        || task.redaction.is_some()
    {
        return Err("selected task is not a root in the bound session".into());
    }
    Ok(())
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
    let mut boot: Bootstrap = bootstrap(&mut io).await?;
    let parent = parent_proof(&boot)?;
    if let Some(execution) = &boot.request.execution {
        execution.validate(boot.request.role)?;
    }
    let (_selection, config, data) = selection(&boot.request)?;
    // This acquires the real canonical writer and performs crash recovery. No
    // provider request, root thread or task is created by attachment.
    let (host, owner) = CanonicalHost::open(config.clone())?;
    if boot.request.root_task.is_some() {
        if let Err(error) = selected_root(&host, &config) {
            owner.close().await?;
            return Err(error);
        }
    }
    let execution = boot
        .request
        .execution
        .take()
        .map(|execution| execution::Supervisor::new(host.clone(), config.clone(), data, execution))
        .transpose()?;
    let result = match boot.request.transport {
        Transport::Stdio => {
            let (_stop, stopped) = tokio::sync::watch::channel(false);
            connection(
                host,
                config,
                execution.clone(),
                boot.request.role,
                io,
                Opening::Stdio {
                    challenge: boot.challenge,
                    parent,
                },
                stopped,
            )
            .await
        }
        Transport::WindowsPipe => {
            pipe_service(
                host,
                config,
                execution.clone(),
                boot.request.role,
                io,
                boot.challenge,
                parent,
            )
            .await
        }
    };
    let closed = if let Some(execution) = execution {
        execution.shutdown(owner).await
    } else {
        owner.close().await
    };
    result.and(closed)
}

enum Opening {
    Stdio {
        challenge: String,
        parent: HeldProcess,
    },
    Pipe(PipeGrants),
}

#[derive(Clone)]
struct PipeGrants {
    primary: Attachment,
    observer: Attachment,
    maximum_role: Role,
}

fn ready(config: &Config, role: Role, grants: Option<&PipeGrants>) -> Result<Ready, String> {
    let scope = vcp_protocol::methods::Scope {
        workspace: config
            .workspace
            .as_str()
            .to_owned()
            .try_into()
            .map_err(|_| "invalid workspace")?,
        session: config
            .session
            .as_str()
            .to_owned()
            .try_into()
            .map_err(|_| "invalid session")?,
    };
    Ok(Ready {
        schema: READY.into(),
        server: HeldProcess::current()
            .and_then(|process| process.pin())
            .map_err(|_| "server identity unavailable")?,
        scope: scope.clone(),
        role,
        attachment: grants.map(|grants| {
            if role == Role::Observer {
                grants.observer.clone()
            } else {
                grants.primary.clone()
            }
        }),
        observer_attachment: grants
            .filter(|_| role == Role::Controller)
            .map(|grants| grants.observer.clone()),
        observer_reconnect: grants.map(|grants| ObserverReconnect {
            endpoint: grants.primary.endpoint.clone(),
            server: grants.primary.server.clone(),
            scope,
        }),
    })
}

async fn connection(
    host: CanonicalHost,
    config: Config,
    execution: Execution,
    role: Role,
    mut io: Framed,
    opening: Opening,
    mut stopped: tokio::sync::watch::Receiver<bool>,
) -> Result<(), String> {
    let connection = host.public_connection(access(&host, &config, role)?)?;
    let signal = connection.loss_signal();
    let transport_signal = signal.clone();
    io.on_loss(move || transport_signal.invalidate());
    let mut connection = execution::Rpc::new(connection, execution);
    let result = async {
        let methods: Vec<String> = connection.supported_methods().iter().map(|value| (*value).into()).collect();
        let capabilities = capabilities_for_methods(&methods);
        let mut session = RpcSession::new(ServerInfo {
            engine_build: concat!("vcp/", env!("CARGO_PKG_VERSION")).into(), methods, capabilities,
            limits: ConnectionLimits { maximum_frame_bytes: framed::LIMIT as u32,
                maximum_pending_requests: 1, maximum_subscriptions: 8,
                maximum_subscriber_queue_bytes: framed::LIMIT as u32 },
            execution_host: ExecutionHost { id: config.binding.host.as_str().into(), platform: "windows".into() },
            sandbox_capabilities: vec![],
        }).map_err(|_| "local RPC configuration unavailable")?;
        let parent = match opening {
            Opening::Stdio { challenge, parent } => {
                io.send_value(&BootstrapReply { schema: BOOTSTRAP.into(), challenge, ready: ready(&config, role, None)? }).await?;
                Some(parent)
            }
            Opening::Pipe(grants) => {
                io.send_value(&ready(&config, role, Some(&grants))?).await?;
                None
            }
        };
            loop {
                let bytes = tokio::select! {
                    biased;
                    _ = stopped.wait_for(|stopped| *stopped) => break,
                    frame = io.receive() => match frame { Ok(frame) => frame, Err(_) => break },
                };
                if let Some(parent) = &parent {
                    if !parent.is_alive().map_err(|_| "parent liveness unavailable")? { break; }
                }
                let current = access(&host, &config, role)?;
                let frame = std::str::from_utf8(&bytes).map_err(|_| "invalid local UTF-8")?;
                let mut lost = io.loss();
                let response = tokio::select! {
                    biased;
                    _ = stopped.wait_for(|stopped| *stopped) => break,
                    _ = lost.wait_for(|lost| *lost) => break,
                    response = session.dispatch_host(&mut connection, &current, jsonrpc::parse_frame(frame)) => response,
                };
                if let Some(response) = response { io.send_value(&response).await?; }
                if session.is_closed() { break; }
            }
            Ok::<(), String>(())
    }.await;
    signal.invalidate();
    let disconnected = connection.disconnect().await;
    result.and(disconnected.map(|_| ()))
}

const PIPE_IDLE: Duration = Duration::from_secs(30);
const PIPE_CLIENTS: usize = 16;
const PIPE_AUTHENTICATING: usize = 16;

fn random_hex() -> Result<String, String> {
    Ok(windows_launch::random_bytes::<32>()
        .map_err(|_| "local entropy unavailable")?
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

async fn pipe_service(
    host: CanonicalHost,
    config: Config,
    execution: Execution,
    role: Role,
    mut bootstrap_io: Framed,
    challenge: String,
    parent: HeldProcess,
) -> Result<(), String> {
    let server_pin = HeldProcess::current()
        .and_then(|process| process.pin())
        .map_err(|_| "server identity unavailable")?;
    let attachment = Attachment {
        endpoint: format!(r"\\.\pipe\vcp-local-{}", random_hex()?),
        server: server_pin,
        ticket: random_hex()?,
    };
    let grants = PipeGrants {
        observer: Attachment {
            ticket: random_hex()?,
            ..attachment.clone()
        },
        primary: attachment,
        maximum_role: role,
    };
    let listener = windows_pipe::bind(&grants.primary.endpoint, true)?;
    let (stop, stopped) = tokio::sync::watch::channel(false);
    let supervisor = tokio::spawn(supervise(
        host,
        config.clone(),
        execution,
        grants.clone(),
        listener,
        stop.clone(),
        stopped,
    ));
    let handoff = async {
        bootstrap_io
            .send_value(&BootstrapReply {
                schema: BOOTSTRAP.into(),
                challenge: challenge.clone(),
                ready: ready(&config, role, Some(&grants))?,
            })
            .await?;
        let offered: Handoff = bootstrap(&mut bootstrap_io).await?;
        if offered.schema != "vcp-local-handoff/1"
            || offered.challenge != challenge
            || !parent
                .is_alive()
                .map_err(|_| "bootstrap parent unavailable")?
        {
            return Err("local handoff authentication denied".into());
        }
        bootstrap_io
            .send_value(&Handoff {
                schema: "vcp-local-handoff-accepted/1".into(),
                challenge,
            })
            .await
    }
    .await;
    if handoff.is_err() {
        let _ = stop.send(true);
    }
    drop(bootstrap_io);
    let result = supervisor
        .await
        .map_err(|_| "local connection supervisor interrupted")?;
    handoff.and(result)
}

struct Activity {
    count: usize,
    last_zero: tokio::time::Instant,
    closing: bool,
}
struct Active(std::sync::Arc<std::sync::Mutex<Activity>>);
impl Active {
    fn admit(activity: &std::sync::Arc<std::sync::Mutex<Activity>>) -> Option<Self> {
        let mut state = activity.lock().unwrap_or_else(|error| error.into_inner());
        if state.closing {
            return None;
        }
        state.count += 1;
        Some(Self(activity.clone()))
    }
}
impl Drop for Active {
    fn drop(&mut self) {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        state.count -= 1;
        if state.count == 0 {
            state.last_zero = tokio::time::Instant::now();
        }
    }
}

async fn supervise(
    host: CanonicalHost,
    config: Config,
    execution: Execution,
    grants: PipeGrants,
    mut listener: tokio::net::windows::named_pipe::NamedPipeServer,
    stop: tokio::sync::watch::Sender<bool>,
    mut stopped: tokio::sync::watch::Receiver<bool>,
) -> Result<(), String> {
    use std::sync::{Arc, Mutex};
    let authentication = Arc::new(tokio::sync::Semaphore::new(PIPE_AUTHENTICATING));
    let clients = Arc::new(tokio::sync::Semaphore::new(PIPE_CLIENTS));
    let active = Arc::new(Mutex::new(Activity {
        count: 0,
        last_zero: tokio::time::Instant::now(),
        closing: false,
    }));
    let mut tasks = tokio::task::JoinSet::new();
    let mut timer = tokio::time::interval(Duration::from_millis(100));
    let mut outcome = Ok(());
    let client_stop = stopped.clone();
    loop {
        tokio::select! {
            biased;
            _ = stopped.wait_for(|stopped| *stopped) => break,
            _ = timer.tick() => {
                let mut state = active.lock().unwrap_or_else(|error| error.into_inner());
                if state.count == 0 && state.last_zero.elapsed() >= PIPE_IDLE {
                    state.closing = true;
                    break;
                }
            }
            _ = tasks.join_next(), if !tasks.is_empty() => {}
            connected = listener.connect() => {
                if connected.is_err() { outcome = Err("local pipe listener failed".into()); break; }
                // Reserve the namespace continuously while authenticated clients run.
                let next = match windows_pipe::bind(&grants.primary.endpoint, false) {
                    Ok(next) => next,
                    Err(error) => { outcome = Err(error); break; },
                };
                let stream = std::mem::replace(&mut listener, next);
                let permit = match authentication.clone().try_acquire_owned() {
                    Ok(permit) => permit,
                    Err(_) => { drop(stream); continue; },
                };
                let clients = clients.clone();
                let active = active.clone();
                let host = host.clone();
                let config = config.clone();
                let execution = execution.clone();
                let grants = grants.clone();
                let mut stopped = client_stop.clone();
                tasks.spawn(async move {
                    let Ok(ready) = ready(&config, Role::Observer, Some(&grants)) else { return; };
                    let tickets = [(grants.primary.ticket.as_str(), grants.maximum_role),
                        (grants.observer.ticket.as_str(), Role::Observer)];
                    let authenticated = tokio::select! {
                        biased;
                        _ = stopped.wait_for(|stopped| *stopped) => return,
                        authenticated = windows_pipe::authenticate_server_access(stream, &grants.primary.server.principal, &tickets, Some(&ready.scope)) => authenticated,
                    };
                    let Ok((stream, role)) = authenticated else { return; };
                    let Ok(_client) = clients.try_acquire_owned() else { return; };
                    let Some(_active) = Active::admit(&active) else { return; };
                    drop(permit);
                    let _ = connection(host, config, execution, role, Framed::from_async(stream), Opening::Pipe(grants), stopped).await;
                    // Active count and client capacity include canonical disconnect/drain.
                });
            }
        }
    }
    active
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .closing = true;
    let _ = stop.send(true);
    drop(listener);
    while tasks.join_next().await.is_some() {}
    outcome
}
