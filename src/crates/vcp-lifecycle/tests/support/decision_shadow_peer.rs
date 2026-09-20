// SPDX-License-Identifier: Apache-2.0
//! Controlled local TLS peer; synthetic protocol evidence, never model quality.
use serde_json::Value;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Notify,
};

#[derive(Clone, Copy)]
pub(super) enum Behavior {
    Reply,
    UnknownCost,
    TruncatedEnvelope,
    Lost,
    Delayed,
}
#[derive(Clone)]
pub(super) struct Observation {
    pub path: String,
    pub body: Vec<u8>,
    pub authorization_matches: bool,
}
struct State {
    observations: Mutex<Vec<Observation>>,
    requests: Notify,
    release: Notify,
    handshakes: AtomicUsize,
    replies: AtomicUsize,
}
pub(super) struct Peer {
    port: u16,
    certificate: Vec<u8>,
    state: Arc<State>,
    task: tokio::task::JoinHandle<()>,
}
impl Peer {
    pub async fn start(
        behavior: Behavior,
        response: impl Fn(&Value) -> Value + Send + Sync + 'static,
    ) -> Self {
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let certificate = cert.der().to_vec();
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.der().clone()],
            rustls::pki_types::PrivateKeyDer::Pkcs8(rustls::pki_types::PrivatePkcs8KeyDer::from(
                signing_key.serialize_der(),
            )),
        )
        .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(State {
            observations: Mutex::new(vec![]),
            requests: Notify::new(),
            release: Notify::new(),
            handshakes: AtomicUsize::new(0),
            replies: AtomicUsize::new(0),
        });
        let owned = state.clone();
        let response = Arc::new(response);
        let task = tokio::spawn(async move {
            // No detached connection tasks; dropping Peer aborts the one owned driver.
            for _ in 0..8 {
                let Ok(Ok((socket, _))) =
                    tokio::time::timeout(Duration::from_secs(30), listener.accept()).await
                else {
                    break;
                };
                let Ok(Ok(mut stream)) =
                    tokio::time::timeout(Duration::from_secs(10), acceptor.accept(socket)).await
                else {
                    continue;
                };
                owned.handshakes.fetch_add(1, Ordering::SeqCst);
                let state = owned.clone();
                let response = response.clone();
                let _ = tokio::time::timeout(Duration::from_secs(25), async move {
                    let mut head = Vec::new();
                    loop {
                        if head.len() >= 16 * 1024 { return; }
                        let Ok(byte) = stream.read_u8().await else { return; };
                        head.push(byte);
                        if head.ends_with(b"\r\n\r\n") { break; }
                    }
                    let Ok(text) = std::str::from_utf8(&head) else { return; };
                    let mut lines = text.split("\r\n");
                    let Some(first) = lines.next() else { return; };
                    let words: Vec<_> = first.split(' ').collect();
                    if words.len() != 3 || words[0] != "POST" { return; }
                    let path = words[1].to_string();
                    let mut length = None;
                    let mut authorization_matches = false;
                    for line in lines {
                        let Some((name, value)) = line.split_once(':') else { continue; };
                        if name.eq_ignore_ascii_case("content-length") {
                            if length.is_some() { return; }
                            length = value.trim().parse::<usize>().ok();
                        }
                        if name.eq_ignore_ascii_case("authorization") {
                            authorization_matches = value.trim() == "Bearer synthetic-shadow-credential";
                        }
                        if name.eq_ignore_ascii_case("transfer-encoding") { return; }
                    }
                    let Some(length) = length.filter(|length| *length > 0 && *length <= 256 * 1024) else { return; };
                    let mut body = vec![0; length];
                    if stream.read_exact(&mut body).await.is_err() { return; }
                    let Ok(value) = serde_json::from_slice::<Value>(&body) else { return; };
                    state.observations.lock().unwrap().push(Observation { path, body, authorization_matches });
                    state.requests.notify_one();
                    if matches!(behavior, Behavior::Lost) { return; }
                    if matches!(behavior, Behavior::Delayed) { state.release.notified().await; }
                    let mut value = response(&value);
                    if matches!(behavior, Behavior::UnknownCost) {
                        value["usage"].as_object_mut().unwrap().remove("cost");
                    }
                    let body = serde_json::to_vec(&value).unwrap();
                    assert!(body.len() <= 64 * 1024);
                    let head = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len() + usize::from(matches!(behavior, Behavior::TruncatedEnvelope)));
                    if stream.write_all(head.as_bytes()).await.is_ok() && stream.write_all(&body).await.is_ok() && stream.flush().await.is_ok() {
                        state.replies.fetch_add(1, Ordering::SeqCst);
                    }
                    let _ = stream.shutdown().await;
                }).await;
            }
        });
        Self {
            port,
            certificate,
            state,
            task,
        }
    }
    pub fn endpoint(&self) -> String {
        format!("https://localhost:{}/", self.port)
    }
    pub fn root_certificate(&self) -> Vec<u8> {
        self.certificate.clone()
    }
    pub fn observations(&self) -> Vec<Observation> {
        self.state.observations.lock().unwrap().clone()
    }
    pub fn handshakes(&self) -> usize {
        self.state.handshakes.load(Ordering::SeqCst)
    }
    pub fn replies(&self) -> usize {
        self.state.replies.load(Ordering::SeqCst)
    }
    pub async fn wait_request(&self) {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let ready = self.state.requests.notified();
                if !self.observations().is_empty() {
                    break;
                }
                ready.await;
            }
        })
        .await
        .unwrap();
    }
    pub fn release(&self) {
        self.state.release.notify_one();
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
