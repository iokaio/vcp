// SPDX-License-Identifier: Apache-2.0
//! Private one-shot HTTP/1 transport prerequisite. No MCP service, retries,
//! redirects, DNS, ambient proxies, cookies, or credential acquisition.
use crate::{foundation::mcp::remote_authority::CredentialLease, Lifecycle};
use bytes::Bytes;
use codex_protocol::ThreadId;
use http::{header, HeaderMap, Method, Request, StatusCode};
use http_body_util::{BodyExt, Full};
use hyper::body::Body;
use hyper_util::rt::TokioIo;
use rustls::{pki_types::ServerName, ClientConfig};
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

pub(crate) struct Tls {
    pub name: ServerName<'static>,
    /// Trusted host construction only. Never accept caller-configured verifiers.
    pub config: Arc<ClientConfig>,
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
/// Complete bounded responses only: this prerequisite is not an incremental SSE
/// transport. It cannot yet service callbacks while a response remains open.
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
    if !valid(&out) {
        return Err(control.failure(FailureKind::Invalid, false));
    }
    {
        let state = runtime
            .0
            .state
            .lock()
            .map_err(|_| control.failure(FailureKind::Revoked, false))?;
        if !state.attached || state.held(thread) || !state.admission_current(thread, generation) {
            return Err(control.failure(FailureKind::Revoked, false));
        }
        let mut active = runtime
            .0
            .remote_sockets
            .lock()
            .map_err(|_| control.failure(FailureKind::Revoked, false))?;
        let slots = active.entry(thread).or_default();
        slots.retain(|slot| slot.strong_count() > 0);
        if slots.len() >= 16 {
            return Err(control.failure(FailureKind::Limit, false));
        }
        slots.push(Arc::downgrade(&control));
    }
    let deadline = out.limits.deadline;
    let mut submitted = false;
    let action = async {
        let fenced = FencedSocket {
            control: control.clone(),
            runtime,
            thread,
            generation,
            credential,
            limit: out.limits.wire_bytes,
            deadline,
        };
        // Register the connecting socket before yielding. Owner loss can close
        // it even if this caller never polls its future again. No DNS/fallback.
        poll_fn(|cx| {
            fenced.gate(cx, |state, _| {
                let create = || -> io::Result<TcpStream> {
                    let socket = socket2::Socket::new(
                        socket2::Domain::for_address(out.address),
                        socket2::Type::STREAM,
                        Some(socket2::Protocol::TCP),
                    )?;
                    socket.set_nonblocking(true)?;
                    match socket.connect(&out.address.into()) {
                        Ok(()) => (),
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => (),
                        Err(error) => return Err(error),
                    }
                    TcpStream::from_std(socket.into())
                };
                match create() {
                    Ok(socket) => {
                        state.socket = Some(socket);
                        Poll::Ready(Ok(()))
                    }
                    Err(error) => Poll::Ready(Err(error)),
                }
            })
        })
        .await
        .map_err(|_| FailureKind::Transport)?;
        poll_fn(|cx| {
            fenced.gate(cx, |state, cx| {
                let Some(socket) = state.socket.as_ref() else {
                    return Poll::Ready(Err(io::Error::other("remote socket absent")));
                };
                match socket.poll_write_ready(cx) {
                    Poll::Ready(Ok(())) => Poll::Ready(
                        socket
                            .take_error()
                            .and_then(|error| error.map_or(Ok(()), Err)),
                    ),
                    other => other,
                }
            })
        })
        .await
        .map_err(|_| FailureKind::Transport)?;
        let io: Box<dyn Io> = if let Some(tls) = out.tls {
            let mut config = (*tls.config).clone();
            config.enable_early_data = false;
            config.alpn_protocols = vec![b"http/1.1".to_vec()];
            let stream = tokio_rustls::TlsConnector::from(Arc::new(config))
                .connect_with(tls.name, fenced, |connection| {
                    connection.set_buffer_limit(Some(64 * 1024))
                })
                .await
                .map_err(|_| FailureKind::Transport)?;
            Box::new(stream)
        } else {
            Box::new(fenced)
        };
        let mut builder = hyper::client::conn::http1::Builder::new();
        builder
            .max_headers(out.limits.header_count)
            .max_buf_size(out.limits.header_bytes)
            .writev(false);
        let (mut sender, connection) = builder
            .handshake(TokioIo::new(io))
            .await
            .map_err(|_| FailureKind::Protocol)?;
        let mut request = Request::builder()
            .method(Method::POST)
            .uri(out.path)
            .header(header::HOST, out.authority)
            .header(header::CONNECTION, "close")
            .body(Full::new(Bytes::from(out.body)))
            .map_err(|_| FailureKind::Invalid)?;
        request.headers_mut().extend(out.headers);
        submitted = true;
        let response = async {
            let response = sender
                .send_request(request)
                .await
                .map_err(|_| FailureKind::Protocol)?;
            let (parts, mut body) = response.into_parts();
            if parts
                .headers
                .iter()
                .map(|(name, value)| name.as_str().len() + value.len())
                .sum::<usize>()
                > out.limits.header_bytes
                || parts
                    .headers
                    .get(header::CONTENT_ENCODING)
                    .is_some_and(|value| value.as_bytes() != b"identity")
            {
                return Err(FailureKind::Protocol);
            }
            if body.size_hint().lower() > out.limits.response_bytes as u64 {
                return Err(FailureKind::Limit);
            }
            let mut bytes = Vec::new();
            while let Some(frame) = body.frame().await {
                let frame = frame.map_err(|_| FailureKind::Protocol)?;
                if let Some(data) = frame.data_ref() {
                    if data.len() > out.limits.response_bytes.saturating_sub(bytes.len()) {
                        return Err(FailureKind::Limit);
                    }
                    bytes.extend_from_slice(data);
                } else {
                    return Err(FailureKind::Protocol);
                }
            }
            Ok((parts.status, parts.headers, bytes))
        };
        tokio::pin!(response);
        tokio::pin!(connection);
        // Both futures are owned here. No task or client pool survives Drop.
        tokio::select! {
            result=&mut response=>result,
            result=&mut connection=>{result.map_err(|_|FailureKind::Protocol)?;response.await},
        }
    };
    let result = tokio::time::timeout_at(deadline, action).await;
    match result {
        Ok(Ok((status, headers, body))) => {
            let state = control.0.lock().map_err(|_| Failure {
                kind: FailureKind::Revoked,
                sent_bytes: 0,
                request_submitted: submitted,
            })?;
            Ok(Response {
                status,
                headers,
                body,
                sent_bytes: state.sent,
                received_bytes: state.received,
            })
        }
        Ok(Err(kind)) => Err(control.failure(kind, submitted)),
        Err(_) => Err(control.failure(FailureKind::Deadline, submitted)),
    }
}

#[cfg(test)]
mod tests;
