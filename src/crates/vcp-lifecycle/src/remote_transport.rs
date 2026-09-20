// SPDX-License-Identifier: Apache-2.0
//! Private one-shot HTTP/1 transport prerequisite. No MCP service, retries,
//! redirects, DNS, ambient proxies, cookies, or credential acquisition.
use crate::{foundation::mcp::remote_authority::CredentialLease, Lifecycle};
use bytes::Bytes;
use codex_protocol::ThreadId;
use http::{header, HeaderMap, Method, Request, StatusCode};
use hyper::body::Body;
use hyper_util::rt::TokioIo;
use rustls::pki_types::ServerName;
use std::{
    future::poll_fn,
    io,
    net::SocketAddr,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpStream,
    time::Instant,
};
use vcp_domain::revision::Timestamp;
mod streaming;
pub(crate) mod trust;
pub(crate) use streaming::{Event, Exchange};

pub(crate) struct Tls {
    pub name: ServerName<'static>,
    /// Trusted host construction only. Never accept caller-configured verifiers.
    pub trust: Arc<trust::TrustSnapshot>,
}
pub(crate) struct Outbound {
    pub address: SocketAddr,
    pub authority: String,
    pub path: String,
    pub tls: Option<Tls>,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
    pub limits: Limits,
}
#[derive(Clone, Copy)]
pub(crate) struct Limits {
    pub request_bytes: usize,
    pub response_bytes: usize,
    /// Independent cap for each direction, including TLS wire overhead.
    pub wire_bytes: u64,
    pub header_bytes: usize,
    pub header_count: usize,
    pub deadline: Instant,
}
pub(crate) struct Response {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
    pub sent_bytes: u64,
    pub received_bytes: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FailureKind {
    Invalid,
    Revoked,
    Limit,
    Deadline,
    Transport,
    Protocol,
}
#[derive(Debug)]
pub(crate) struct Failure {
    pub kind: FailureKind,
    /// Physical TCP bytes, including TLS handshake records. Never a receipt.
    pub sent_bytes: u64,
    /// Once HTTP is enqueued, failure is conservatively outcome-unknown.
    pub request_submitted: bool,
}
struct SocketState {
    socket: Option<TcpStream>,
    closed: bool,
    sent: u64,
    received: u64,
    failure: Option<FailureKind>,
    waker: Option<Waker>,
    #[cfg(test)]
    allowance: Option<usize>,
    #[cfg(test)]
    blocked: bool,
}
pub(crate) struct SocketControl(Mutex<SocketState>);
impl SocketControl {
    fn new() -> Self {
        Self(Mutex::new(SocketState {
            socket: None,
            closed: false,
            sent: 0,
            received: 0,
            failure: None,
            waker: None,
            #[cfg(test)]
            allowance: None,
            #[cfg(test)]
            blocked: false,
        }))
    }
    fn close(&self) -> Option<Waker> {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        state.closed = true;
        state.failure.get_or_insert(FailureKind::Revoked);
        state.socket.take();
        state.waker.take()
    }
    fn failure(&self, fallback: FailureKind, submitted: bool) -> Failure {
        match self.0.lock() {
            Ok(state) => Failure {
                kind: state.failure.unwrap_or(fallback),
                sent_bytes: state.sent,
                request_submitted: submitted,
            },
            Err(_) => Failure {
                kind: FailureKind::Revoked,
                sent_bytes: 0,
                request_submitted: submitted,
            },
        }
    }
}
impl Lifecycle {
    pub(crate) fn close_all_remote_sockets(&self) {
        let selected = match self.0.remote_sockets.lock() {
            Ok(sockets) => sockets.keys().copied().collect::<Vec<_>>(),
            Err(error) => error.into_inner().keys().copied().collect::<Vec<_>>(),
        };
        for waker in self.close_remote_sockets(&selected) {
            waker.wake();
        }
    }
    // Caller holds lifecycle state. Lock order is state -> credential -> socket;
    // closing uses state -> registry -> socket and never enters credentials.
    pub(crate) fn close_remote_sockets(&self, selected: &[ThreadId]) -> Vec<Waker> {
        let mut wakers = vec![];
        {
            let mut sockets = self
                .0
                .remote_sockets
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            for thread in selected {
                if let Some(active) = sockets.remove(thread) {
                    for control in active.into_iter().filter_map(|entry| entry.upgrade()) {
                        if let Some(waker) = control.close() {
                            wakers.push(waker);
                        }
                    }
                }
            }
        }
        wakers
    }
}
struct FencedSocket {
    control: Arc<SocketControl>,
    runtime: Lifecycle,
    thread: ThreadId,
    generation: u64,
    credential: Option<CredentialLease>,
    limit: u64,
    deadline: Instant,
}
impl FencedSocket {
    fn gate<T>(
        &self,
        cx: &mut Context<'_>,
        action: impl FnOnce(&mut SocketState, &mut Context<'_>) -> Poll<io::Result<T>>,
    ) -> Poll<io::Result<T>> {
        let runtime = match self.runtime.0.state.lock() {
            Ok(state) => state,
            Err(_) => return Poll::Ready(Err(io::Error::other("remote owner unavailable"))),
        };
        if !runtime.attached
            || runtime.held(self.thread)
            || !runtime.admission_current(self.thread, self.generation)
        {
            let wake = self.control.close();
            drop(runtime);
            if let Some(wake) = wake {
                wake.wake();
            }
            return Poll::Ready(Err(io::Error::other("remote admission sealed")));
        }
        let checked = || {
            let mut state = match self.control.0.lock() {
                Ok(state) => state,
                Err(_) => return Poll::Ready(Err(io::Error::other("remote socket unavailable"))),
            };
            state.waker = Some(cx.waker().clone());
            if Instant::now() >= self.deadline {
                state.failure = Some(FailureKind::Deadline);
                state.closed = true;
                state.socket.take();
            }
            if state.closed {
                return Poll::Ready(Err(io::Error::other("remote socket closed")));
            }
            action(&mut state, cx)
        };
        match &self.credential {
            Some(lease) => match lease.with_current(now(), checked) {
                Ok(result) => result,
                Err(_) => {
                    let wake = self.control.close();
                    drop(runtime);
                    if let Some(wake) = wake {
                        wake.wake();
                    }
                    Poll::Ready(Err(io::Error::other(
                        "remote credential revoked or expired",
                    )))
                }
            },
            None => checked(),
        }
    }
}
impl Drop for FencedSocket {
    fn drop(&mut self) {
        let wake = {
            let mut state = self
                .control
                .0
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            state.closed = true;
            state.socket.take();
            state.waker.take()
        };
        if let Some(wake) = wake {
            wake.wake();
        }
    }
}
impl AsyncRead for FencedSocket {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.gate(cx, |state, cx| {
            let available = self.limit.saturating_sub(state.received);
            if available == 0 {
                state.failure = Some(FailureKind::Limit);
                return Poll::Ready(Err(io::Error::other("remote wire read limit")));
            }
            let length = buf.remaining().min(available as usize).min(8192);
            let mut chunk = [0; 8192];
            let mut read = ReadBuf::new(&mut chunk[..length]);
            let Some(socket) = state.socket.as_mut() else {
                return Poll::Ready(Err(io::Error::other("remote socket absent")));
            };
            match Pin::new(socket).poll_read(cx, &mut read) {
                Poll::Ready(Ok(())) => {
                    state.received += read.filled().len() as u64;
                    buf.put_slice(read.filled());
                    Poll::Ready(Ok(()))
                }
                other => other,
            }
        })
    }
}
impl AsyncWrite for FencedSocket {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.gate(cx, |state, cx| {
            let available = self.limit.saturating_sub(state.sent);
            if available == 0 {
                state.failure = Some(FailureKind::Limit);
                return Poll::Ready(Err(io::Error::other("remote wire write limit")));
            }
            let count = bytes.len().min(available as usize);
            #[cfg(test)]
            let count = match state.allowance {
                Some(0) => {
                    state.blocked = true;
                    return Poll::Pending;
                }
                Some(left) => count.min(left),
                None => count,
            };
            let Some(socket) = state.socket.as_mut() else {
                return Poll::Ready(Err(io::Error::other("remote socket absent")));
            };
            match Pin::new(socket).poll_write(cx, &bytes[..count]) {
                Poll::Ready(Ok(written)) => {
                    state.sent += written as u64;
                    #[cfg(test)]
                    if let Some(left) = &mut state.allowance {
                        *left -= written;
                    }
                    Poll::Ready(Ok(written))
                }
                other => other,
            }
        })
    }
    // The default vectored implementation delegates to poll_write, preserving
    // the same physical fence. No unchecked vectored fast path is exposed.
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.gate(cx, |state, cx| match state.socket.as_mut() {
            Some(socket) => Pin::new(socket).poll_flush(cx),
            None => Poll::Ready(Err(io::Error::other("remote socket absent"))),
        })
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.gate(cx, |state, cx| match state.socket.as_mut() {
            Some(socket) => Pin::new(socket).poll_shutdown(cx),
            None => Poll::Ready(Err(io::Error::other("remote socket absent"))),
        })
    }
}
trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}
fn now() -> Timestamp {
    Timestamp::new(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| {
                duration.as_millis().min(u64::MAX as u128) as u64
            }),
    )
}
fn valid(out: &Outbound) -> bool {
    let l = out.limits;
    let Ok(authority) = out.authority.parse::<http::uri::Authority>() else {
        return false;
    };
    if out
        .tls
        .as_ref()
        .is_some_and(|tls| tls.name.to_str() != authority.host().trim_matches(['[', ']']))
    {
        return false;
    }
    let remaining = l.deadline.saturating_duration_since(Instant::now());
    !remaining.is_zero()
        && remaining <= Duration::from_secs(120)
        && (1..=8 * 1024 * 1024).contains(&l.request_bytes)
        && out.body.len() <= l.request_bytes
        && (1..=16 * 1024 * 1024).contains(&l.response_bytes)
        && (8192..=32 * 1024 * 1024).contains(&l.wire_bytes)
        && (8192..=64 * 1024).contains(&l.header_bytes)
        && (1..=128).contains(&l.header_count)
        && out.headers.len() <= 32
        && out
            .headers
            .iter()
            .map(|(name, value)| name.as_str().len() + value.len())
            .sum::<usize>()
            <= 16 * 1024
        && !out.headers.contains_key(header::HOST)
        && !out.headers.contains_key(header::CONTENT_LENGTH)
        && !out.headers.contains_key(header::TRANSFER_ENCODING)
        && !out.headers.contains_key(header::CONNECTION)
        && !out.headers.contains_key(header::UPGRADE)
        && !out.headers.contains_key(header::EXPECT)
        && out.authority.len() <= 1024
        && !out.authority.is_empty()
        && out.authority.parse::<http::uri::Authority>().is_ok()
        && !out.authority.contains('@')
        && out.path.len() <= 8192
        && out.path.starts_with('/')
        && !out.path.starts_with("//")
        && out.path.parse::<http::uri::PathAndQuery>().is_ok()
        && (out.tls.is_some()
            || cfg!(any(test, feature = "qualification")) && out.address.ip().is_loopback())
}

/// Host supplies already-authorized pinned address, exact generation, scoped
/// credential lease and explicit TLS roots. This primitive never grants them.
/// Convenience collector for bounded complete responses. Streaming consumers
/// use Exchange directly to handle callback POSTs while an SSE body stays open.
pub(crate) async fn exchange(
    runtime: Lifecycle,
    thread: ThreadId,
    generation: u64,
    out: Outbound,
    credential: Option<CredentialLease>,
) -> Result<Response, Failure> {
    exchange_control(
        runtime,
        thread,
        generation,
        out,
        credential,
        Arc::new(SocketControl::new()),
    )
    .await
}
async fn exchange_control(
    runtime: Lifecycle,
    thread: ThreadId,
    generation: u64,
    out: Outbound,
    credential: Option<CredentialLease>,
    control: Arc<SocketControl>,
) -> Result<Response, Failure> {
    let mut exchange =
        Exchange::start_control(runtime, thread, generation, out, credential, control).await?;
    let mut head = None;
    let mut body = Vec::new();
    loop {
        match exchange.next().await? {
            Event::Written(_) => (),
            Event::Head { status, headers } => head = Some((status, headers)),
            Event::Chunk(bytes) => body.extend_from_slice(&bytes),
            Event::End {
                sent_bytes,
                received_bytes,
            } => {
                let (status, headers) =
                    head.ok_or_else(|| exchange.failure(FailureKind::Protocol))?;
                return Ok(Response {
                    status,
                    headers,
                    body,
                    sent_bytes,
                    received_bytes,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests;
