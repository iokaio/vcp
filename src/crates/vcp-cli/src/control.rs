// SPDX-License-Identifier: Apache-2.0
//! Private, current-user-only Windows owner connection. No independent writer.
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::windows::named_pipe::{ClientOptions, ServerOptions},
};
use vcp_domain::{
    ids::*,
    task::{Task, TaskState},
};
use vcp_lifecycle::foundation::CanonicalHost;
use vcp_protocol::command::{Command, CommandEnvelope};

#[derive(Serialize, Deserialize)]
#[serde(tag = "operation", deny_unknown_fields)]
pub enum Request {
    Query {
        workspace: WorkspaceId,
        query: crate::app::Query,
    },
    Prepare {
        workspace: WorkspaceId,
        task: TaskId,
        cancel: bool,
    },
    Stop {
        command: Box<CommandEnvelope>,
    },
}
const LIMIT: u64 = 1024 * 1024;
pub fn pipe(key: &str) -> String {
    format!(r"\\.\pipe\vcp-owner-{key}")
}

pub async fn request(name: &str, request: &Request) -> Result<serde_json::Value, String> {
    let operation = async {
        let mut stream = loop {
            match ClientOptions::new().open(name) {
                Ok(stream) => break stream,
                Err(error) if error.raw_os_error() == Some(231) => {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await
                }
                Err(_) => {
                    return Err(
                        "owning controller is unreachable; no second writer was opened".into(),
                    )
                }
            }
        };
        let mut bytes = serde_json::to_vec(request).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        stream
            .write_all(&bytes)
            .await
            .map_err(|_| "owner control write failed")?;
        let mut frame = Vec::new();
        BufReader::new((&mut stream).take(LIMIT + 1))
            .read_until(b'\n', &mut frame)
            .await
            .map_err(|_| "owner control read failed")?;
        if frame.len() > LIMIT as usize || frame.last() != Some(&b'\n') {
            return Err("invalid owner response frame".into());
        }
        let value: serde_json::Value =
            serde_json::from_slice(&frame).map_err(|_| "invalid owner response")?;
        stream
            .write_all(b"\n")
            .await
            .map_err(|_| "owner acknowledgement failed")?;
        if let Some(error) = value.get("error").and_then(|e| e.as_str()) {
            return Err(error.to_owned());
        }
        Ok(value["result"].clone())
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), operation)
        .await
        .map_err(|_| "owner control timed out".to_owned())?
}

/// Bind before billable admission. Explicit DACL denies other users and remote
/// clients; first-instance prevents silently joining a pre-existing endpoint.
pub fn serve(
    name: &str,
    host: CanonicalHost,
    workspace: WorkspaceId,
) -> Result<tokio::task::JoinHandle<()>, String> {
    let mut server = bind(name, true)?;
    let name = name.to_owned();
    Ok(tokio::spawn(async move {
        loop {
            if server.connect().await.is_err() {
                break;
            }
            // Keep a fresh listening instance alive before releasing the current
            // connection. Reusing a disconnected overlapped handle can retain a
            // previous client's EOF readiness on Windows.
            let Ok(next) = bind(&name, false) else {
                break;
            };
            let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
                handle(&mut server, &host, &workspace).await
            })
            .await
            .unwrap_or_else(|_| Err("owner input timed out".into()));
            let value = match result {
                Ok(value) => serde_json::json!({"result":value}),
                Err(error) => serde_json::json!({"error":error}),
            };
            if let Ok(mut bytes) = serde_json::to_vec(&value) {
                if bytes.len() <= LIMIT as usize {
                    bytes.push(b'\n');
                    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                        server.write_all(&bytes).await?;
                        // Do not discard kernel-buffered output on disconnect.
                        server.read_u8().await.map(|_| ())
                    })
                    .await;
                }
            }
            server = next;
        }
    }))
}

fn bind(
    name: &str,
    first: bool,
) -> Result<tokio::net::windows::named_pipe::NamedPipeServer, String> {
    let security = Security::current_user()?;
    let mut attributes = windows_sys::Win32::Security::SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<windows_sys::Win32::Security::SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: security.0,
        bInheritHandle: 0,
    };
    // SAFETY: attributes and its self-relative descriptor are valid through
    // CreateNamedPipe. Windows copies the descriptor into the kernel object.
    unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(
                name,
                (&mut attributes as *mut windows_sys::Win32::Security::SECURITY_ATTRIBUTES).cast(),
            )
    }
    .map_err(|_| "private owner endpoint unavailable".into())
}

async fn handle(
    server: &mut tokio::net::windows::named_pipe::NamedPipeServer,
    host: &CanonicalHost,
    workspace: &WorkspaceId,
) -> Result<serde_json::Value, String> {
    let mut frame = Vec::new();
    BufReader::new(server.take(vcp_protocol::version::MAX_COMMAND_BYTES as u64 + 1))
        .read_until(b'\n', &mut frame)
        .await
        .map_err(|error| format!("control read failed: {error}"))?;
    if frame.len() > vcp_protocol::version::MAX_COMMAND_BYTES || frame.last() != Some(&b'\n') {
        return Err("invalid control frame".into());
    }
    let request: Request =
        serde_json::from_slice(&frame).map_err(|_| "invalid owner request".to_owned())?;
    match request {
        Request::Query {
            workspace: requested,
            query,
        } if requested == *workspace => match query {
            crate::app::Query::Inspect { request } => {
                serde_json::to_value(host.inspect(request)?).map_err(|e| e.to_string())
            }
            query => crate::app::query(&host.snapshot()?, workspace, &query),
        },
        Request::Prepare {
            workspace: requested,
            task,
            cancel,
        } if requested == *workspace => {
            let state = host.snapshot()?;
            let task: Task = state
                .record(
                    vcp_store::contract::Collection::Task,
                    task.as_str(),
                    workspace,
                )
                .and_then(|r| r.decode())
                .map_err(|e| e.to_string())?;
            let command = host.control_envelope(
                CommandId::new(),
                task.scope.task,
                task.revision,
                Command::Transition {
                    next: if cancel {
                        TaskState::Cancelled
                    } else {
                        TaskState::Paused
                    },
                    reason: "explicit CLI control".into(),
                    verification: None,
                },
            )?;
            serde_json::to_value(command).map_err(|e| e.to_string())
        }
        Request::Stop { command } => {
            serde_json::to_value(host.stop(*command)?).map_err(|e| e.to_string())
        }
        _ => Err("owner workspace denied".into()),
    }
}

struct Security(*mut std::ffi::c_void);
impl Drop for Security {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::LocalFree(self.0);
        }
    }
}
impl Security {
    fn current_user() -> Result<Self, String> {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, LocalFree},
            Security::{
                Authorization::{
                    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
                },
                GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER,
            },
            System::Threading::{GetCurrentProcess, OpenProcessToken},
        };
        // SAFETY: handles are closed on all paths; aligned token storage remains
        // alive until SID conversion; LocalAlloc strings/descriptors use LocalFree.
        unsafe {
            let mut token = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err("current user token unavailable".into());
            }
            let mut size = 0;
            GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut size);
            let mut storage = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
            let ok = GetTokenInformation(
                token,
                TokenUser,
                storage.as_mut_ptr().cast(),
                size,
                &mut size,
            );
            CloseHandle(token);
            if ok == 0 {
                return Err("current user SID unavailable".into());
            }
            let user = &*storage.as_ptr().cast::<TOKEN_USER>();
            let mut text = std::ptr::null_mut();
            if ConvertSidToStringSidW(user.User.Sid, &mut text) == 0 {
                return Err("current user SID conversion failed".into());
            }
            let mut len = 0;
            while *text.add(len) != 0 {
                len += 1;
            }
            let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
            LocalFree(text.cast());
            let sddl: Vec<u16> = format!("D:P(A;;GA;;;{sid})")
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let mut descriptor = std::ptr::null_mut();
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                std::ptr::null_mut(),
            ) == 0
            {
                return Err("private owner ACL unavailable".into());
            }
            Ok(Self(descriptor))
        }
    }
}
