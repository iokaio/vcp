// SPDX-License-Identifier: Apache-2.0
//! Owned incremental HTTP/1 driver and exact request-flush observation.
use super::*;
use hyper::body::{Frame, Incoming, SizeHint};
use std::{convert::Infallible, future::Future};

pub(crate) enum Event {
    Written(Written),
    Head {
        status: StatusCode,
        headers: HeaderMap,
    },
    Chunk(Bytes),
    End {
        sent_bytes: u64,
        received_bytes: u64,
    },
}
/// Constructed only by a completed flush of this exact one-shot exchange.
/// It proves local write completion, never remote execution or receipt.
pub(crate) struct Written {
    instance: Arc<()>,
    digest: String,
}
impl Written {
    pub(crate) fn body_digest(&self) -> &str {
        &self.digest
    }
}
#[derive(Default)]
struct WriteState {
    handed_off: bool,
    plaintext: u64,
    written: bool,
    failed: bool,
    #[cfg(test)]
    body_paused: bool,
    #[cfg(test)]
    body_waker: Option<Waker>,
}
struct TrackedBody {
    bytes: Option<Bytes>,
    state: Arc<Mutex<WriteState>>,
}
impl Body for TrackedBody {
    type Data = Bytes;
    type Error = Infallible;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        #[cfg(test)]
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            if state.body_paused {
                state.body_waker = Some(cx.waker().clone());
                return Poll::Pending;
            }
        }
        #[cfg(not(test))]
        let _ = cx;
        match self.bytes.take() {
            Some(bytes) => {
                self.state
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .handed_off = true;
                Poll::Ready(Some(Ok(Frame::data(bytes))))
            }
            None => Poll::Ready(None),
        }
    }
    fn is_end_stream(&self) -> bool {
        self.bytes.is_none()
    }
    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(self.bytes.as_ref().map_or(0, |bytes| bytes.len()) as u64)
    }
}
/// Above TLS: handshake bytes do not count as HTTP plaintext. Below this is the
/// unchanged physical lifecycle/credential gate. Hyper 1.8.1's flattened writer
/// drains its entire encoded headers/body before calling this flush; its final
/// body frame is encoded synchronously after TrackedBody hands it off. Rustls's
/// flush then drains all TLS writes through that physical gate. These locked
/// implementation properties are covered by early-head/partial/TLS tests.
struct FlushObserver {
    io: Box<dyn Io>,
    state: Arc<Mutex<WriteState>>,
}
impl AsyncRead for FlushObserver {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_read(cx, buf)
    }
}
impl AsyncWrite for FlushObserver {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(&mut self.io).poll_write(cx, bytes);
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        match &result {
            Poll::Ready(Ok(count)) => {
                state.plaintext = state.plaintext.saturating_add(*count as u64)
            }
            Poll::Ready(Err(_)) => state.failed = true,
            _ => (),
        }
        result
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let result = Pin::new(&mut self.io).poll_flush(cx);
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        match &result {
            Poll::Ready(Ok(())) if state.handed_off && state.plaintext > 0 && !state.failed => {
                state.written = true
            }
            Poll::Ready(Err(_)) => state.failed = true,
            _ => (),
        }
        result
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_shutdown(cx)
    }
}
type Driver = Pin<Box<dyn Future<Output = Result<(), hyper::Error>> + Send>>;
type ResponseFuture =
    Pin<Box<dyn Future<Output = Result<http::Response<Incoming>, hyper::Error>> + Send>>;
pub(crate) struct Exchange {
    instance: Arc<()>,
    digest: String,
    write: Arc<Mutex<WriteState>>,
    written_yielded: bool,
    control: Arc<SocketControl>,
    driver: Option<Driver>,
    driver_error: Option<FailureKind>,
    response: Option<ResponseFuture>,
    sender: Option<hyper::client::conn::http1::SendRequest<TrackedBody>>,
    body: Option<Incoming>,
    head: Option<(StatusCode, HeaderMap)>,
    received: usize,
    limits: Limits,
    ended: bool,
}
impl Exchange {
    pub(crate) async fn start(
        runtime: Lifecycle,
        thread: ThreadId,
        generation: u64,
        out: Outbound,
        credential: Option<CredentialFence>,
    ) -> Result<Self, Failure> {
        Self::start_control(
            runtime,
            thread,
            generation,
            out,
            credential,
            Arc::new(SocketControl::new()),
        )
        .await
    }
    pub(super) async fn start_control(
        runtime: Lifecycle,
        thread: ThreadId,
        generation: u64,
        out: Outbound,
        credential: Option<CredentialFence>,
        control: Arc<SocketControl>,
    ) -> Result<Self, Failure> {
        // All supported MCP POST payloads are nonempty JSON. Empty bodies need
        // an independently qualified headers-only write observation if added.
        if !valid(&out) || out.body.is_empty() {
            return Err(control.failure(FailureKind::Invalid, false));
        }
        {
            let state = runtime
                .0
                .state
                .lock()
                .map_err(|_| control.failure(FailureKind::Revoked, false))?;
            if !state.attached || state.held(thread) || !state.admission_current(thread, generation)
            {
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
        let limits = out.limits;
        let connect = async {
            let fenced = FencedSocket {
                control: control.clone(),
                runtime,
                thread,
                generation,
                credential,
                limit: limits.wire_bytes,
                deadline: limits.deadline,
            };
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
                let stream = tokio_rustls::TlsConnector::from(tls.trust.config())
                    .connect_with(tls.name, fenced, |connection| {
                        connection.set_buffer_limit(Some(64 * 1024))
                    })
                    .await
                    .map_err(|_| FailureKind::Transport)?;
                Box::new(stream)
            } else {
                Box::new(fenced)
            };
            let write = Arc::new(Mutex::new(WriteState::default()));
            let io = FlushObserver {
                io,
                state: write.clone(),
            };
            let mut builder = hyper::client::conn::http1::Builder::new();
            builder
                .max_headers(limits.header_count)
                .max_buf_size(limits.header_bytes)
                .writev(false);
            let (mut sender, connection) = builder
                .handshake(TokioIo::new(io))
                .await
                .map_err(|_| FailureKind::Protocol)?;
            let digest = vcp_protocol::digest_bytes(&out.body);
            let mut request = Request::builder()
                .method(Method::POST)
                .uri(out.path)
                .header(header::HOST, out.authority)
                .header(header::CONNECTION, "close")
                .body(TrackedBody {
                    bytes: Some(Bytes::from(out.body)),
                    state: write.clone(),
                })
                .map_err(|_| FailureKind::Invalid)?;
            request.headers_mut().extend(out.headers);
            let response = Box::pin(sender.send_request(request));
            Ok(Self {
                instance: Arc::new(()),
                digest,
                write,
                written_yielded: false,
                control: control.clone(),
                driver: Some(Box::pin(connection)),
                driver_error: None,
                response: Some(response),
                sender: Some(sender),
                body: None,
                head: None,
                received: 0,
                limits,
                ended: false,
            })
        };
        match tokio::time::timeout_at(limits.deadline, connect).await {
            Ok(Ok(exchange)) => Ok(exchange),
            Ok(Err(kind)) => Err(control.failure(kind, false)),
            Err(_) => Err(control.failure(FailureKind::Deadline, false)),
        }
    }
    pub(crate) fn owns(&self, written: &Written) -> bool {
        Arc::ptr_eq(&self.instance, &written.instance) && written.digest == self.digest
    }
    pub(super) fn failure(&self, kind: FailureKind) -> Failure {
        self.control.failure(kind, true)
    }
    fn stop(&mut self) {
        self.ended = true;
        self.body.take();
        self.response.take();
        self.sender.take();
        self.driver.take();
    }
    pub(crate) async fn next(&mut self) -> Result<Event, Failure> {
        if self.ended {
            return Err(self.failure(FailureKind::Invalid));
        }
        match tokio::time::timeout_at(self.limits.deadline, poll_fn(|cx| self.poll_next(cx))).await
        {
            Ok(Ok(event)) => Ok(event),
            Ok(Err(kind)) => {
                let failure = self.failure(kind);
                self.stop();
                Err(failure)
            }
            Err(_) => {
                let failure = self.failure(FailureKind::Deadline);
                self.stop();
                Err(failure)
            }
        }
    }
    fn written(&mut self) -> bool {
        if !self.written_yielded
            && self
                .write
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .written
        {
            self.written_yielded = true;
            true
        } else {
            false
        }
    }
    fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Result<Event, FailureKind>> {
        for turn in 0..2 {
            if self.written() {
                return Poll::Ready(Ok(Event::Written(Written {
                    instance: self.instance.clone(),
                    digest: self.digest.clone(),
                })));
            }
            if let Some(response) = &mut self.response {
                match response.as_mut().poll(cx) {
                    Poll::Ready(Ok(response)) => {
                        self.response.take();
                        let (parts, body) = response.into_parts();
                        if parts
                            .headers
                            .iter()
                            .map(|(name, value)| name.as_str().len() + value.len())
                            .sum::<usize>()
                            > self.limits.header_bytes
                            || parts
                                .headers
                                .get(header::CONTENT_ENCODING)
                                .is_some_and(|value| value.as_bytes() != b"identity")
                        {
                            return Poll::Ready(Err(FailureKind::Protocol));
                        }
                        if body.size_hint().lower() > self.limits.response_bytes as u64 {
                            return Poll::Ready(Err(FailureKind::Limit));
                        }
                        self.head = Some((parts.status, parts.headers));
                        self.body = Some(body);
                    }
                    Poll::Ready(Err(_)) => {
                        return Poll::Ready(Err(self
                            .driver_error
                            .unwrap_or(FailureKind::Protocol)));
                    }
                    Poll::Pending => (),
                }
            }
            if let Some((status, headers)) = self.head.take() {
                return Poll::Ready(Ok(Event::Head { status, headers }));
            }
            if let Some(body) = &mut self.body {
                match Pin::new(body).poll_frame(cx) {
                    Poll::Ready(Some(Ok(frame))) => match frame.into_data() {
                        Ok(bytes) => {
                            if bytes.len()
                                > self.limits.response_bytes.saturating_sub(self.received)
                            {
                                return Poll::Ready(Err(FailureKind::Limit));
                            }
                            self.received += bytes.len();
                            return Poll::Ready(Ok(Event::Chunk(bytes)));
                        }
                        Err(_) => return Poll::Ready(Err(FailureKind::Protocol)),
                    },
                    Poll::Ready(Some(Err(_))) => {
                        return Poll::Ready(Err(self
                            .driver_error
                            .unwrap_or(FailureKind::Protocol)));
                    }
                    Poll::Ready(None) => {
                        let counts = self
                            .control
                            .0
                            .lock()
                            .map(|state| (state.sent, state.received))
                            .map_err(|_| FailureKind::Revoked);
                        let (sent_bytes, received_bytes) = match counts {
                            Ok(counts) => counts,
                            Err(error) => return Poll::Ready(Err(error)),
                        };
                        self.stop();
                        return Poll::Ready(Ok(Event::End {
                            sent_bytes,
                            received_bytes,
                        }));
                    }
                    Poll::Pending => (),
                }
            }
            if turn == 0 {
                if let Some(driver) = &mut self.driver {
                    match driver.as_mut().poll(cx) {
                        Poll::Ready(result) => {
                            if result.is_err() {
                                self.driver_error = Some(FailureKind::Protocol);
                            }
                            self.driver.take();
                        }
                        Poll::Pending => (),
                    }
                }
            }
        }
        if self.driver.is_none() {
            Poll::Ready(Err(self.driver_error.unwrap_or(FailureKind::Protocol)))
        } else {
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod tests;
