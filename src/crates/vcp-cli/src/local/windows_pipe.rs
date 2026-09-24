// SPDX-License-Identifier: Apache-2.0
//! Private Windows pipe bootstrap. Authenticate before starting protocol pumps.
use super::{
    windows_identity::{self, HeldProcess, Principal},
    Attachment, ObserverReconnect, Role,
};
use serde::{Deserialize, Serialize};
use std::{mem::size_of, ptr::null_mut, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::windows::named_pipe::{ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions},
};
use windows_sys::Win32::{
    Foundation::{LocalFree, ERROR_PIPE_BUSY},
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        },
        SECURITY_ATTRIBUTES,
    },
    Storage::FileSystem::SECURITY_IDENTIFICATION,
};

const SCHEMA: &str = "vcp-local-pipe-auth/1";
const RECONNECT: &str = "vcp-local-pipe-observer-reconnect/1";
const AUTH_LIMIT: usize = 16 * 1024;
const DEADLINE: Duration = Duration::from_secs(10);
const PREFIX: &str = r"\\.\pipe\vcp-local-";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Hello {
    schema: String,
    ticket: String,
    role: Role,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObserverHello {
    schema: String,
    scope: vcp_protocol::methods::Scope,
}

fn endpoint(value: &str) -> Result<(), String> {
    let suffix = value.strip_prefix(PREFIX).ok_or("invalid local endpoint")?;
    if suffix.len() != 64 || !suffix.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid local endpoint".into());
    }
    Ok(())
}

struct Security(*mut std::ffi::c_void);
impl Drop for Security {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0) };
    }
}
impl Security {
    fn current() -> Result<Self, String> {
        let sid = HeldProcess::current()
            .and_then(|process| process.pin())
            .map_err(|_| "local endpoint principal unavailable")?
            .principal
            .sid;
        let mut storage = vec![0usize; sid.len().div_ceil(size_of::<usize>())];
        // Aligned copy of the SID obtained from the current kernel token.
        unsafe {
            std::ptr::copy_nonoverlapping(
                sid.as_ptr(),
                storage.as_mut_ptr().cast::<u8>(),
                sid.len(),
            )
        };
        let mut text = null_mut();
        if unsafe { ConvertSidToStringSidW(storage.as_mut_ptr().cast(), &mut text) } == 0 {
            return Err("local endpoint principal unavailable".into());
        }
        let mut length = 0;
        // Windows returns an allocated, terminated SID string. Bound its scan.
        while length < 1024 && unsafe { *text.add(length) } != 0 {
            length += 1;
        }
        let sid = if length < 1024 {
            String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) }).ok()
        } else {
            None
        };
        unsafe { LocalFree(text.cast()) };
        let sid = sid.ok_or("local endpoint principal unavailable")?;
        let descriptor_text: Vec<u16> = format!("D:P(A;;GA;;;{sid})")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut descriptor = null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                descriptor_text.as_ptr(),
                1,
                &mut descriptor,
                null_mut(),
            )
        } == 0
        {
            return Err("private local endpoint ACL unavailable".into());
        }
        Ok(Self(descriptor))
    }
}

/// Keep at least one listening/connected instance alive throughout server life.
/// Bind the successor before dispatching an accepted instance to authentication.
pub(super) fn bind(name: &str, first: bool) -> Result<NamedPipeServer, String> {
    endpoint(name)?;
    let security = Security::current()?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: security.0,
        bInheritHandle: 0,
    };
    // CreateNamedPipe copies the descriptor before these allocations are dropped.
    unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(
                name,
                (&attributes as *const SECURITY_ATTRIBUTES)
                    .cast_mut()
                    .cast(),
            )
    }
    .map_err(|_| "private local endpoint unavailable".into())
}

fn ticket(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut bytes = [0; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

fn same_ticket(left: &[u8; 32], right: &[u8; 32]) -> bool {
    let mut difference = 0u8;
    for index in 0..32 {
        // Volatile reads retain every fixed-position comparison, including after
        // a mismatch. Neither control flow nor addresses depend on ticket bytes.
        difference |= unsafe {
            std::ptr::read_volatile(left.as_ptr().add(index))
                ^ std::ptr::read_volatile(right.as_ptr().add(index))
        };
    }
    difference == 0
}

/// Pin comes from the controlled bootstrap, held in client memory. This routine
/// authenticates the pipe server before disclosing the ticket; it reads no ack.
pub(super) async fn connect(
    attachment: &Attachment,
    role: Role,
) -> Result<BufReader<NamedPipeClient>, String> {
    ticket(&attachment.ticket).ok_or("invalid local attachment credential")?;
    connect_frame(
        attachment,
        &Hello {
            schema: SCHEMA.into(),
            ticket: attachment.ticket.clone(),
            role,
        },
    )
    .await
}

pub(super) async fn reconnect_observer(
    reference: &ObserverReconnect,
) -> Result<BufReader<NamedPipeClient>, String> {
    // Reuse kernel pin validation; no credential is read from the reference.
    let attachment = Attachment {
        endpoint: reference.endpoint.clone(),
        server: reference.server.clone(),
        ticket: String::new(),
    };
    connect_frame(
        &attachment,
        &ObserverHello {
            schema: RECONNECT.into(),
            scope: reference.scope.clone(),
        },
    )
    .await
}

async fn connect_frame(
    attachment: &Attachment,
    hello: &impl Serialize,
) -> Result<BufReader<NamedPipeClient>, String> {
    endpoint(&attachment.endpoint)?;
    let own = HeldProcess::current()
        .and_then(|process| process.pin())
        .map_err(|_| "local client identity unavailable")?;
    if attachment.server.image != own.image
        || attachment.server.file != own.file
        || attachment.server.principal != own.principal
    {
        return Err("trusted local server identity denied".into());
    }
    tokio::time::timeout(DEADLINE, async {
        let mut stream = loop {
            match ClientOptions::new()
                .security_qos_flags(SECURITY_IDENTIFICATION)
                .open(&attachment.endpoint)
            {
                Ok(stream) => break stream,
                Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                    tokio::time::sleep(Duration::from_millis(20)).await
                }
                Err(_) => return Err("local attachment endpoint unavailable".to_owned()),
            }
        };
        let process = windows_identity::verify_pipe_server(&stream, &attachment.server)
            .map_err(|_| "local server pin denied")?;
        let mut frame =
            serde_json::to_vec(hello).map_err(|_| "local authentication encoding failed")?;
        if frame.len() > AUTH_LIMIT {
            return Err("local authentication frame too large".into());
        }
        frame.push(b'\n');
        stream
            .write_all(&frame)
            .await
            .map_err(|_| "local authentication write failed")?;
        stream
            .flush()
            .await
            .map_err(|_| "local authentication write failed")?;
        process
            .validate(&attachment.server)
            .map_err(|_| "local server exited during authentication")?;
        Ok(BufReader::new(stream))
    })
    .await
    .map_err(|_| "local authentication timed out".to_owned())?
}

/// Call after server.connect(). Authentication is bounded and preserves any
/// prefetched protocol bytes. Caller emits Ready only after public connection setup.
#[cfg(test)]
async fn authenticate_server(
    stream: NamedPipeServer,
    principal: &Principal,
    expected_ticket: &str,
    maximum_role: Role,
) -> Result<(BufReader<NamedPipeServer>, Role), String> {
    authenticate_server_grants(stream, principal, &[(expected_ticket, maximum_role)]).await
}

/// Distinct observer/controller credentials retain their own role ceilings.
/// Evaluate every configured grant; a match never borrows another grant's role.
#[cfg(test)]
async fn authenticate_server_grants(
    stream: NamedPipeServer,
    principal: &Principal,
    grants: &[(&str, Role)],
) -> Result<(BufReader<NamedPipeServer>, Role), String> {
    authenticate_server_access(stream, principal, grants, None).await
}

/// Ticket grants retain their role ceilings. Discovery reconnect additionally
/// requires the same live executable identity and can only grant scoped reads.
pub(super) async fn authenticate_server_access(
    stream: NamedPipeServer,
    principal: &Principal,
    grants: &[(&str, Role)],
    observer_scope: Option<&vcp_protocol::methods::Scope>,
) -> Result<(BufReader<NamedPipeServer>, Role), String> {
    if grants.is_empty() || grants.len() > 2 {
        return Err("invalid server attachment grants".into());
    }
    let grants: Vec<_> = grants
        .iter()
        .map(|(value, role)| {
            ticket(value)
                .map(|value| (value, *role))
                .ok_or("invalid server attachment credential")
        })
        .collect::<Result<_, _>>()?;
    if grants.len() == 2 && same_ticket(&grants[0].0, &grants[1].0) {
        return Err("server attachment credentials must be distinct".into());
    }
    let mut stream = BufReader::new(stream);
    tokio::time::timeout(DEADLINE, async {
        let mut frame = Vec::new();
        let count = (&mut stream)
            .take((AUTH_LIMIT + 2) as u64)
            .read_until(b'\n', &mut frame)
            .await
            .map_err(|_| "local authentication read failed")?;
        if count == 0 || count > AUTH_LIMIT + 1 || frame.last() != Some(&b'\n') {
            return Err("invalid local authentication frame".to_owned());
        }
        // No await or pipe reader runs between the last read and impersonation.
        let peer = windows_identity::authenticate_pipe_client(stream.get_ref())
            .map_err(|_| "local client identity denied")?;
        if &peer.pin.principal != principal
            || !peer
                .process
                .is_alive()
                .map_err(|_| "local client identity denied")?
        {
            return Err("local client principal denied".into());
        }
        if let Ok(hello) = serde_json::from_slice::<ObserverHello>(&frame) {
            let own = HeldProcess::current()
                .and_then(|process| process.pin())
                .map_err(|_| "local server identity unavailable")?;
            if hello.schema != RECONNECT
                || observer_scope != Some(&hello.scope)
                || peer.pin.image != own.image
                || peer.pin.file != own.file
            {
                return Err("local observer identity or scope denied".into());
            }
            peer.process
                .validate(&peer.pin)
                .map_err(|_| "local observer exited during authentication")?;
            return Ok(Role::Observer);
        }
        let hello: Hello =
            serde_json::from_slice(&frame).map_err(|_| "invalid local authentication frame")?;
        let received = ticket(&hello.ticket).ok_or("local attachment credential denied")?;
        let mut authorized = 0u8;
        for (expected, maximum_role) in &grants {
            let matched = same_ticket(&received, expected) as u8;
            let allowed = (*maximum_role == Role::Controller || hello.role == Role::Observer) as u8;
            authorized |= matched & allowed;
        }
        if hello.schema != SCHEMA || authorized == 0 {
            return Err("local attachment credential or role denied".into());
        }
        Ok(hello.role)
    })
    .await
    .map_err(|_| "local authentication timed out".to_owned())?
    .map(|role| (stream, role))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn attachment() -> Attachment {
        let random = super::super::windows_launch::random_bytes::<32>().unwrap();
        let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        Attachment {
            endpoint: format!("{PREFIX}{suffix}"),
            server: HeldProcess::current().unwrap().pin().unwrap(),
            ticket: "42".repeat(32),
        }
    }
    #[tokio::test]
    async fn observer_ticket_cannot_borrow_another_grants_controller_role() {
        for reversed in [false, true] {
            for (credential, requested, allowed) in [
                ("24", Role::Observer, true),
                ("24", Role::Controller, false),
                ("42", Role::Controller, true),
                ("42", Role::Observer, true),
                ("66", Role::Observer, false),
            ] {
                let mut attachment = attachment();
                attachment.ticket = credential.repeat(32);
                let principal = attachment.server.principal.clone();
                let server = bind(&attachment.endpoint, true).unwrap();
                let accepted = tokio::spawn(async move {
                    server.connect().await.unwrap();
                    let controller = "42".repeat(32);
                    let observer = "24".repeat(32);
                    let mut grants = [
                        (controller.as_str(), Role::Controller),
                        (observer.as_str(), Role::Observer),
                    ];
                    if reversed {
                        grants.reverse();
                    }
                    authenticate_server_grants(server, &principal, &grants).await
                });
                let _client = connect(&attachment, requested).await.unwrap();
                let result = accepted.await.unwrap();
                assert_eq!(
                    result.is_ok(),
                    allowed,
                    "credential grant ceiling must be order-independent"
                );
                if let Ok((_, role)) = result {
                    assert!(role == requested);
                }
            }
        }
    }
    #[tokio::test]
    async fn real_pipe_authentication_preserves_prefetch_and_rejects_bad_credentials() {
        for case in ["valid", "observer", "ticket", "role", "principal", "sid"] {
            let mut attachment = attachment();
            let expected = attachment.ticket.clone();
            let mut principal = attachment.server.principal.clone();
            if case == "ticket" {
                attachment.ticket = "24".repeat(32);
            }
            if case == "principal" {
                principal.session ^= 1;
            }
            if case == "sid" {
                principal.sid.push(0);
            }
            let server = bind(&attachment.endpoint, true).unwrap();
            assert!(
                bind(&attachment.endpoint, true).is_err(),
                "first-instance reservation must reject collision"
            );
            let maximum = if case == "role" || case == "observer" {
                Role::Observer
            } else {
                Role::Controller
            };
            let accepted = tokio::spawn(async move {
                server.connect().await.unwrap();
                authenticate_server(server, &principal, &expected, maximum).await
            });
            let requested = if case == "observer" {
                Role::Observer
            } else {
                Role::Controller
            };
            let mut client = connect(&attachment, requested).await.unwrap();
            if case == "valid" || case == "observer" {
                client.write_all(b"{\"next\":true}\n").await.unwrap();
            }
            let result = accepted.await.unwrap();
            if case == "valid" || case == "observer" {
                let (mut stream, role) = result.unwrap();
                assert!(role == requested);
                let mut next = String::new();
                stream.read_line(&mut next).await.unwrap();
                assert_eq!(next, "{\"next\":true}\n");
            } else {
                assert!(result.is_err(), "{case}");
            }
        }
    }
    #[tokio::test]
    async fn authentication_rejects_oversized_and_truncated_first_frames() {
        for bytes in [vec![b'x'; AUTH_LIMIT + 2], b"truncated".to_vec()] {
            let attachment = attachment();
            let server = bind(&attachment.endpoint, true).unwrap();
            let principal = attachment.server.principal.clone();
            let expected = attachment.ticket.clone();
            let accepted = tokio::spawn(async move {
                server.connect().await.unwrap();
                authenticate_server(server, &principal, &expected, Role::Controller).await
            });
            let mut client = ClientOptions::new().open(&attachment.endpoint).unwrap();
            let _ = client.write_all(&bytes).await;
            drop(client);
            assert!(tokio::time::timeout(DEADLINE, accepted)
                .await
                .unwrap()
                .unwrap()
                .is_err());
        }
    }
    #[tokio::test]
    async fn wrong_process_pin_is_rejected_before_ticket_write() {
        let mut attachment = attachment();
        let server = bind(&attachment.endpoint, true).unwrap();
        attachment.server.created.push('0');
        let accepted = tokio::spawn(async move {
            server.connect().await.unwrap();
            let mut stream = BufReader::new(server);
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes).await.unwrap();
            bytes
        });
        assert!(connect(&attachment, Role::Observer).await.is_err());
        assert!(tokio::time::timeout(DEADLINE, accepted)
            .await
            .unwrap()
            .unwrap()
            .is_empty());
    }
    #[test]
    fn fixed_ticket_comparison_checks_each_position() {
        let original = [42u8; 32];
        assert!(same_ticket(&original, &original));
        for index in 0..32 {
            let mut changed = original;
            changed[index] ^= 1;
            assert!(!same_ticket(&original, &changed));
        }
        assert!(ticket(&"0".repeat(63)).is_none());
        assert!(ticket(&"z".repeat(64)).is_none());
    }
}
