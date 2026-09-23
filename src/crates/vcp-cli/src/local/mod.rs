// SPDX-License-Identifier: Apache-2.0
//! Controlled local helper entry points. All bootstrap input travels over pipes.
mod execution;
mod framed;
mod server;
mod windows_identity;
mod windows_launch;
mod windows_pipe;

use framed::Framed;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};
use windows_identity::{Executable, HeldProcess, ProcessPin};

const BOOTSTRAP: &str = "vcp-local-bootstrap/1";
const READY: &str = "vcp-local-ready/1";
const BOOTSTRAP_LIMIT: usize = 16 * 1024;
const BOOTSTRAP_DEADLINE: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Role {
    Observer,
    Controller,
}

#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Transport {
    #[default]
    Stdio,
    WindowsPipe,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Attachment {
    endpoint: String,
    server: ProcessPin,
    ticket: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LaunchRequest {
    schema: String,
    workspace: PathBuf,
    data: Option<PathBuf>,
    role: Role,
    #[serde(default)]
    transport: Transport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    execution: Option<execution::Configuration>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AttachRequest {
    schema: String,
    attachment: Attachment,
    role: Role,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum BridgeRequest {
    Launch(LaunchRequest),
    Attach(AttachRequest),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bootstrap {
    schema: String,
    parent: ProcessPin,
    parent_handle: u64,
    challenge: String,
    request: LaunchRequest,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ready {
    schema: String,
    server: ProcessPin,
    scope: vcp_protocol::methods::Scope,
    role: Role,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    attachment: Option<Attachment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    observer_attachment: Option<Attachment>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Handoff {
    schema: String,
    challenge: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BootstrapReply {
    schema: String,
    challenge: String,
    ready: Ready,
}

pub fn requested() -> bool {
    std::env::args_os()
        .nth(1)
        .is_some_and(|mode| mode == "local-bridge" || mode == "local-server")
}

/// Dedicated processes isolate blocking OS standard handles from CLI execution.
pub async fn run() -> Result<u8, String> {
    windows_identity::seal_inherited_stdio().map_err(|_| "local standard handles unavailable")?;
    let arguments: Vec<_> = std::env::args_os().collect();
    if arguments.len() != 2 {
        return Err("local helper accepts bootstrap only over stdin".into());
    }
    let io = Framed::new(std::io::stdin(), std::io::stdout())?;
    if arguments[1] == "local-bridge" {
        bridge(io).await?;
    } else if arguments[1] == "local-server" {
        server::run(io).await?;
    } else {
        return Err("invalid local helper mode".into());
    }
    Ok(0)
}

async fn bootstrap<T: serde::de::DeserializeOwned>(io: &mut Framed) -> Result<T, String> {
    let bytes = tokio::time::timeout(BOOTSTRAP_DEADLINE, io.receive())
        .await
        .map_err(|_| "local bootstrap timed out")??;
    if bytes.len() > BOOTSTRAP_LIMIT {
        return Err("local bootstrap too large".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "invalid local bootstrap".into())
}

fn parent_proof(boot: &Bootstrap) -> Result<HeldProcess, String> {
    if boot.schema != BOOTSTRAP
        || boot.request.schema != BOOTSTRAP
        || boot.challenge.len() != 64
        || !boot.challenge.bytes().all(|b| b.is_ascii_hexdigit())
        || boot.parent_handle == 0
        || boot.parent_handle > isize::MAX as u64
    {
        return Err("local bootstrap authentication denied".into());
    }
    // The only accepted bootstrap channel is inherited anonymous stdin. The
    // value denotes its real, inherited query-only parent proof, not a PID.
    let parent =
        HeldProcess::inherited(boot.parent_handle).map_err(|_| "local parent handle denied")?;
    parent
        .validate(&boot.parent)
        .map_err(|_| "local parent proof denied")?;
    let current = HeldProcess::current()
        .and_then(|process| process.pin())
        .map_err(|_| "local process identity unavailable")?;
    if boot.parent.principal != current.principal
        || boot.parent.image != current.image
        || boot.parent.file != current.file
        || boot.parent.pid == current.pid
    {
        return Err("local parent identity denied".into());
    }
    Ok(parent)
}

async fn bridge(mut client: Framed) -> Result<(), String> {
    match bootstrap::<BridgeRequest>(&mut client).await? {
        BridgeRequest::Launch(request) => launch_bridge(client, request).await,
        BridgeRequest::Attach(request) => attach_bridge(client, request).await,
    }
}

async fn attach_bridge(mut client: Framed, request: AttachRequest) -> Result<(), String> {
    if request.schema != "vcp-local-attach/1" {
        return Err("invalid attachment bootstrap".into());
    }
    let mut server =
        Framed::from_async(windows_pipe::connect(&request.attachment, request.role).await?);
    let ready: Ready = bootstrap(&mut server).await?;
    if ready.schema != READY
        || ready.server != request.attachment.server
        || ready.role != request.role
    {
        return Err("pipe attachment identity denied".into());
    }
    client.send_value(&ready).await?;
    forward(&mut client, &mut server).await
}

async fn launch_bridge(mut client: Framed, mut request: LaunchRequest) -> Result<(), String> {
    if let Some(execution) = &request.execution {
        execution.validate(request.role)?;
    }
    if request.data.is_none() {
        request.data = Some(crate::settings::default_data()?);
    }
    if request.schema != BOOTSTRAP
        || !request.workspace.is_absolute()
        || request
            .data
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
    {
        return Err("local launch requires explicit absolute workspace/data paths".into());
    }
    let executable =
        Executable::open(&std::env::current_exe().map_err(|_| "local executable unavailable")?)
            .map_err(|_| "trusted local executable unavailable")?;
    let launched =
        windows_launch::launch(&executable).map_err(|_| "controlled local launch failed")?;
    let expected = launched
        .process
        .pin()
        .map_err(|_| "launched identity unavailable")?;
    let parent = HeldProcess::current()
        .and_then(|process| process.pin())
        .map_err(|_| "parent identity unavailable")?;
    if expected.image != executable.path()
        || expected.file != *executable.identity()
        || expected.principal != parent.principal
    {
        return Err("launched identity denied".into());
    }
    let role = request.role;
    let transport = request.transport;
    let challenge: String = windows_launch::random_bytes::<32>()
        .map_err(|_| "local entropy unavailable")?
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let mut child = Framed::new(launched.stdout, launched.stdin)?;
    // Child diagnostics never join protocol output and never accumulate in memory.
    let mut diagnostics = launched.stderr;
    std::thread::Builder::new()
        .name("local-diagnostics".into())
        .spawn(move || {
            let _ = std::io::copy(&mut diagnostics, &mut std::io::sink());
        })
        .map_err(|_| "local diagnostic drain unavailable")?;
    let startup = async {
        child
            .send_value(&Bootstrap {
                schema: BOOTSTRAP.into(),
                parent,
                parent_handle: launched.parent_proof_value,
                challenge: challenge.clone(),
                request,
            })
            .await?;
        let reply: BootstrapReply = bootstrap(&mut child).await?;
        launched
            .process
            .validate(&expected)
            .map_err(|_| "local server exited during authentication")?;
        if reply.schema != BOOTSTRAP
            || reply.challenge != challenge
            || reply.ready.schema != READY
            || reply.ready.server != expected
            || reply.ready.role != role
        {
            return Err("local server authentication denied".into());
        }
        if transport == Transport::WindowsPipe {
            let attachment = reply
                .ready
                .attachment
                .as_ref()
                .ok_or("pipe attachment bootstrap missing")?;
            if attachment.server != expected {
                return Err("pipe bootstrap pin changed".into());
            }
            let mut pipe = Framed::from_async(windows_pipe::connect(attachment, role).await?);
            let pipe_ready: Ready = bootstrap(&mut pipe).await?;
            if pipe_ready.schema != READY
                || pipe_ready.server != expected
                || pipe_ready.scope != reply.ready.scope
                || pipe_ready.role != role
            {
                return Err("pipe bootstrap identity changed".into());
            }
            child
                .send_value(&Handoff {
                    schema: "vcp-local-handoff/1".into(),
                    challenge: challenge.clone(),
                })
                .await?;
            let ack: Handoff = bootstrap(&mut child).await?;
            if ack.schema != "vcp-local-handoff-accepted/1" || ack.challenge != challenge {
                return Err("local server handoff denied".into());
            }
            return Ok((reply.ready, Some(pipe)));
        }
        if reply.ready.attachment.is_some() {
            return Err("unexpected pipe bootstrap".into());
        }
        Ok::<_, String>((reply.ready, None))
    }
    .await;
    let result = match startup {
        Ok((ready, Some(mut pipe))) => {
            launched.guard.handoff();
            drop(child);
            client.send_value(&ready).await?;
            return forward(&mut client, &mut pipe).await;
        }
        Ok((ready, None)) => match client.send_value(&ready).await {
            Ok(()) => forward(&mut client, &mut child).await,
            Err(error) => Err(error),
        },
        Err(error) => Err(error),
    };
    drop(child); // Close stdin: server owns pause/drain before releasing its writer.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while !launched
        .guard
        .wait(0)
        .map_err(|_| "local child liveness unavailable")?
    {
        if tokio::time::Instant::now() >= deadline {
            launched
                .guard
                .terminate()
                .map_err(|_| "local child cleanup incomplete")?;
            return Err("local shutdown required process recovery".into());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let code = launched
        .guard
        .exit_code()
        .map_err(|_| "local child exit status unavailable")?;
    if code != 0 {
        return Err(format!(
            "local server exited with status 0x{code:08x}; reconcile pending commands"
        ));
    }
    result
}

async fn forward(client: &mut Framed, server: &mut Framed) -> Result<(), String> {
    loop {
        tokio::select! {
            frame = client.receive() => match frame {
                Ok(frame) => {
                    let mut lost = client.loss();
                    tokio::select! {
                        biased;
                        _ = lost.wait_for(|lost| *lost) => break,
                        sent = server.send(frame) => sent?,
                    }
                },
                Err(_) => break,
            },
            frame = server.receive() => match frame {
                Ok(frame) => client.send(frame).await?,
                Err(_) => return Err("local server connection lost; reconcile pending commands".into()),
            },
        }
    }
    Ok(())
}
