// SPDX-License-Identifier: Apache-2.0
//! Bounded local TLS MCP peer. Observations contain no authorization values.
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Notify,
};
#[path = "../../src/bin/fixtures/mcp_content.rs"]
mod content;
pub(super) use content::{Mode as ContentMode, URIS as CONTENT_URIS};

#[derive(Clone, Copy)]
pub(super) enum Scenario {
    Normal,
    Callbacks,
    LostResponse,
    BlockAfterEffect,
    SessionChanged,
    Redirect,
    ReplyThenBadSuffix,
    ReplyThenInvalidJson,
    SecretEcho,
    SecretEchoEscaped,
    CumulativeBody,
    ReplyThenControl,
    CallbackSecret,
    CallbackSecretEscaped,
    Content(ContentMode),
}
#[derive(Clone, Debug)]
pub(super) struct Observation {
    pub method: String,
    pub tool: Option<String>,
    pub authorization_matches: bool,
    pub body_digest: String,
}
struct State {
    scenario: Scenario,
    expected_authorization: Option<String>,
    observations: Mutex<Vec<Observation>>,
    response_sizes: Mutex<Vec<usize>>,
    content_changed: AtomicBool,
    session: String,
    effects: AtomicUsize,
    marker: std::path::PathBuf,
    redirect: String,
    callbacks: AtomicUsize,
    callback: Notify,
    release: Notify,
    effect: Notify,
}
pub(super) struct Peer {
    address: std::net::SocketAddr,
    certificate: Vec<u8>,
    state: Arc<State>,
    task: tokio::task::JoinHandle<()>,
    _directory: tempfile::TempDir,
}
impl Peer {
    pub async fn start(scenario: Scenario) -> Self {
        Self::with_authorization(scenario, None).await
    }
    pub async fn with_authorization(
        scenario: Scenario,
        expected_authorization: Option<String>,
    ) -> Self {
        Self::configured(scenario, expected_authorization, "fixture-session".into()).await
    }
    pub async fn with_session(scenario: Scenario, session: &str) -> Self {
        assert!(
            !session.is_empty()
                && session.len() <= 1024
                && session
                    .bytes()
                    .all(|byte| byte.is_ascii() && !byte.is_ascii_control())
        );
        Self::configured(scenario, None, session.into()).await
    }
    async fn configured(
        scenario: Scenario,
        expected_authorization: Option<String>,
        session: String,
    ) -> Self {
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let certificate = cert.der().to_vec();
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let server = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![cert.der().clone()],
                rustls::pki_types::PrivateKeyDer::Pkcs8(
                    rustls::pki_types::PrivatePkcs8KeyDer::from(signing_key.serialize_der()),
                ),
            )
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let state = Arc::new(State {
            scenario,
            expected_authorization,
            observations: Mutex::new(Vec::new()),
            response_sizes: Mutex::new(Vec::new()),
            content_changed: AtomicBool::new(false),
            session,
            effects: AtomicUsize::new(0),
            marker: directory.path().join("effects.txt"),
            redirect: format!("https://localhost:{}/redirect-target", address.port()),
            callbacks: AtomicUsize::new(0),
            callback: Notify::new(),
            release: Notify::new(),
            effect: Notify::new(),
        });
        let owned = state.clone();
        let task = tokio::spawn(async move {
            let mut children = tokio::task::JoinSet::new();
            for _ in 0..64 {
                let accepted =
                    tokio::time::timeout(Duration::from_secs(60), listener.accept()).await;
                let Ok(Ok((socket, _))) = accepted else {
                    break;
                };
                while children.len() >= 4 {
                    let _ = children.join_next().await;
                }
                let acceptor = acceptor.clone();
                let state = owned.clone();
                children.spawn(async move {
                    let _ = tokio::time::timeout(Duration::from_secs(30), async move {
                        let Ok(mut io) = acceptor.accept(socket).await else {
                            return;
                        };
                        serve(&mut io, state).await;
                        let _ = io.shutdown().await;
                    })
                    .await;
                });
            }
            children.abort_all();
            while children.join_next().await.is_some() {}
        });
        Self {
            address,
            certificate,
            state,
            task,
            _directory: directory,
        }
    }
    pub fn endpoint(&self) -> String {
        format!("https://localhost:{}/mcp", self.address.port())
    }
    pub fn address(&self) -> std::net::SocketAddr {
        self.address
    }
    pub fn root_certificate(&self) -> Vec<u8> {
        self.certificate.clone()
    }
    pub fn observations(&self) -> Vec<Observation> {
        self.state.observations.lock().unwrap().clone()
    }
    pub fn response_sizes(&self) -> Vec<usize> {
        self.state.response_sizes.lock().unwrap().clone()
    }
    pub fn change_content_descriptors(&self) {
        self.state.content_changed.store(true, Ordering::SeqCst);
    }
    pub fn effect_count(&self) -> usize {
        self.state.effects.load(Ordering::SeqCst)
    }
    pub fn marker(&self) -> &std::path::Path {
        &self.state.marker
    }
    pub fn callback_count(&self) -> usize {
        self.state.callbacks.load(Ordering::SeqCst)
    }
    pub async fn wait_effect(&self) {
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let notified = self.state.effect.notified();
                if self.effect_count() > 0 {
                    break;
                }
                notified.await;
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

async fn serve(
    io: &mut (impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin),
    state: Arc<State>,
) {
    let mut header = Vec::new();
    loop {
        let Ok(byte) = io.read_u8().await else {
            return;
        };
        header.push(byte);
        if header.len() > 16384 {
            return;
        }
        if header.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let Ok(header) = std::str::from_utf8(&header) else {
        return;
    };
    let field = |name: &str| {
        header
            .lines()
            .skip(1)
            .filter_map(|line| line.split_once(':'))
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.trim())
    };
    let Some(length) = field("content-length").and_then(|v| v.parse::<usize>().ok()) else {
        return;
    };
    if length > 1024 * 1024 {
        return;
    }
    let authorization_matches = match &state.expected_authorization {
        Some(expected) => field("authorization") == Some(expected.as_str()),
        None => field("authorization").is_none(),
    };
    let mut body = vec![0; length];
    if io.read_exact(&mut body).await.is_err() {
        return;
    }
    let Ok(request) = serde_json::from_slice::<Value>(&body) else {
        return;
    };
    let method = request["method"].as_str().unwrap_or("callback-reply");
    state.observations.lock().unwrap().push(Observation {
        method: method.into(),
        tool: request["params"]["name"].as_str().map(str::to_owned),
        authorization_matches,
        body_digest: vcp_protocol::digest_bytes(&body),
    });
    if !authorization_matches {
        respond(io, 401, "Unauthorized", None, b"").await;
        return;
    }
    if matches!(state.scenario, Scenario::Redirect) {
        let response = format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", state.redirect);
        let _ = io.write_all(response.as_bytes()).await;
        return;
    }
    if method == "notifications/initialized" {
        respond(io, 202, "Accepted", None, b"").await;
        return;
    }
    if method == "callback-reply" {
        let current = state.callbacks.load(Ordering::SeqCst);
        let valid = request["jsonrpc"] == "2.0"
            && if matches!(
                state.scenario,
                Scenario::CallbackSecret
                    | Scenario::CallbackSecretEscaped
                    | Scenario::Content(
                        ContentMode::CallbackSecret | ContentMode::CallbackSecretEscaped
                    )
            ) {
                (if matches!(state.scenario, Scenario::Content(_)) {
                    current < 64
                        && request["id"]
                            == format!("synthetic-http-bearer fixture-session {current}")
                } else {
                    current == 0 && request["id"] == "synthetic-http-bearer fixture-session"
                }) && request["result"] == json!({})
                    && request.get("error").is_none()
            } else {
                match current {
                    0 => {
                        request["id"] == "peer-ping"
                            && request["result"] == json!({})
                            && request.get("error").is_none()
                    }
                    1 => {
                        request["id"] == "peer-sampling"
                            && request["error"]["code"] == -32601
                            && request.get("result").is_none()
                    }
                    _ => false,
                }
            };
        if !valid {
            respond(io, 400, "Bad Request", None, b"").await;
            return;
        }
        state.callbacks.fetch_add(1, Ordering::SeqCst);
        state.callback.notify_one();
        respond(io, 202, "Accepted", None, b"").await;
        return;
    }
    let id = request["id"].clone();
    let mut result = match method {
        "initialize" => {
            json!({"protocolVersion":"2025-11-25","capabilities":match state.scenario {Scenario::Content(mode)=>mode.capabilities(),_=>json!({"tools":{}})},"serverInfo":{"name":"vcp-public-http-fixture","version":"1.0"}})
        }
        "tools/list" => json!({"tools":[
            {"name":"echo","description":"Return public text","inputSchema":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"],"additionalProperties":false}},
            {"name":"write_marker","description":"Increment independent fixture marker","annotations":{"readOnlyHint":true},"inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
            {"name":"read_marker","description":"Read fixture marker count","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}
        ]}),
        "tools/call" => {
            let name = request["params"]["name"].as_str().unwrap_or("");
            if name == "write_marker" {
                use std::io::Write;
                let mut marker = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&state.marker)
                    .unwrap();
                marker.write_all(b"effect\n").unwrap();
                marker.sync_all().unwrap();
                state.effects.fetch_add(1, Ordering::SeqCst);
                state.effect.notify_one();
            }
            if matches!(state.scenario, Scenario::LostResponse) {
                return;
            }
            if matches!(state.scenario, Scenario::BlockAfterEffect) {
                state.release.notified().await;
            }
            let text = if matches!(
                state.scenario,
                Scenario::SecretEcho | Scenario::SecretEchoEscaped
            ) {
                format!("{} fixture-session", field("authorization").unwrap_or(""))
            } else if name == "echo" {
                request["params"]["arguments"]["text"]
                    .as_str()
                    .unwrap_or("")
                    .to_owned()
            } else {
                state.effects.load(Ordering::SeqCst).to_string()
            };
            json!({"content":[{"type":"text","text":text}],"isError":false})
        }
        method @ ("resources/list" | "resources/read" | "prompts/list" | "prompts/get") => {
            let Scenario::Content(mode) = state.scenario else {
                return;
            };
            let mode = if state.content_changed.load(Ordering::SeqCst) {
                ContentMode::Changed
            } else {
                mode
            };
            let Some(result) = content::result(mode, method, &request["params"]) else {
                return;
            };
            if content::is_read(method) {
                use std::io::Write;
                let mut marker = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&state.marker)
                    .unwrap();
                marker.write_all(b"content\n").unwrap();
                marker.sync_all().unwrap();
                state.effects.fetch_add(1, Ordering::SeqCst);
                state.effect.notify_one();
                if mode == ContentMode::LostResponse {
                    return;
                }
                if mode == ContentMode::BlockAfterEffect {
                    state.release.notified().await;
                }
            }
            result
        }
        _ => return,
    };
    if matches!(state.scenario, Scenario::CumulativeBody) {
        if method == "initialize" {
            result["serverInfo"]["name"] = json!("n".repeat(256));
            result["serverInfo"]["version"] = json!("v".repeat(128));
        }
        if method == "tools/list" {
            result = json!({"tools":[{"name":"echo","description":"d".repeat(900),"inputSchema":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"],"additionalProperties":false}}]});
        }
    }
    let result = json!({"jsonrpc":"2.0","id":id,"result":result});
    if (method == "tools/call" || content::is_read(method))
        && matches!(
            state.scenario,
            Scenario::Callbacks
                | Scenario::ReplyThenBadSuffix
                | Scenario::ReplyThenInvalidJson
                | Scenario::ReplyThenControl
                | Scenario::CallbackSecret
                | Scenario::CallbackSecretEscaped
                | Scenario::Content(
                    ContentMode::CallbackSecret | ContentMode::CallbackSecretEscaped
                )
                | Scenario::Content(
                    ContentMode::UpdatedDuringRead | ContentMode::ListChangedDuringRead
                )
        )
    {
        let _=io.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nMcp-Session-Id: fixture-session\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").await;
        let callback_base = state.callbacks.load(Ordering::SeqCst);
        let controls = if matches!(state.scenario, Scenario::Callbacks) {
            vec![
                json!({"jsonrpc":"2.0","id":"peer-ping","method":"ping"}),
                json!({"jsonrpc":"2.0","id":"peer-sampling","method":"sampling/createMessage","params":{}}),
            ]
        } else if matches!(
            state.scenario,
            Scenario::CallbackSecret
                | Scenario::CallbackSecretEscaped
                | Scenario::Content(
                    ContentMode::CallbackSecret | ContentMode::CallbackSecretEscaped
                )
        ) {
            vec![
                json!({"jsonrpc":"2.0","id":if matches!(state.scenario,Scenario::Content(_)){format!("synthetic-http-bearer fixture-session {callback_base}")}else{"synthetic-http-bearer fixture-session".into()},"method":"ping"}),
            ]
        } else {
            Vec::new()
        };
        for (index, callback) in controls.iter().enumerate() {
            if matches!(
                state.scenario,
                Scenario::CallbackSecretEscaped
                    | Scenario::Content(ContentMode::CallbackSecretEscaped)
            ) {
                let encoded = serde_json::to_string(callback)
                    .unwrap()
                    .replace("synthetic-http-bearer", "\\u0073ynthetic-http-bearer")
                    .replace("fixture-session", "\\u0066ixture-session");
                event_json(io, &encoded).await;
            } else {
                event(io, callback).await;
            }
            loop {
                let notified = state.callback.notified();
                if state.callbacks.load(Ordering::SeqCst) > callback_base + index {
                    break;
                }
                notified.await;
            }
        }
        if let Scenario::Content(mode) = state.scenario {
            if method == "resources/read" && mode == ContentMode::UpdatedDuringRead {
                event(io,&json!({"jsonrpc":"2.0","method":"notifications/resources/updated","params":{"uri":request["params"]["uri"]}})).await;
            }
            if method == "resources/read" && mode == ContentMode::ListChangedDuringRead {
                event(
                    io,
                    &json!({"jsonrpc":"2.0","method":"notifications/resources/list_changed"}),
                )
                .await;
            }
        }
        event(io, &result).await;
        if matches!(state.scenario, Scenario::ReplyThenControl) {
            event(
                io,
                &json!({"jsonrpc":"2.0","id":"late-control","method":"ping"}),
            )
            .await;
        }
        if matches!(state.scenario, Scenario::ReplyThenBadSuffix) {
            let suffix = "data: {\"unfinished\":";
            let wire = format!("{:x}\r\n{}\r\n", suffix.len(), suffix);
            let _ = io.write_all(wire.as_bytes()).await;
        }
        if matches!(state.scenario, Scenario::ReplyThenInvalidJson) {
            let suffix = "data: {invalid json}\n\n";
            let wire = format!("{:x}\r\n{}\r\n", suffix.len(), suffix);
            let _ = io.write_all(wire.as_bytes()).await;
        }
        let _ = io.write_all(b"0\r\n\r\n").await;
        return;
    }
    let session = if method != "initialize" && matches!(state.scenario, Scenario::SessionChanged) {
        "changed-session"
    } else {
        &state.session
    };
    let mut bytes = serde_json::to_vec(&result).unwrap();
    state.response_sizes.lock().unwrap().push(bytes.len());
    if (method == "tools/call" || content::is_read(method))
        && matches!(
            state.scenario,
            Scenario::SecretEchoEscaped | Scenario::Content(ContentMode::SecretEchoEscaped)
        )
    {
        bytes = String::from_utf8(bytes)
            .unwrap()
            .replace("synthetic-http-bearer", "\\u0073ynthetic-http-bearer")
            .replace("fixture-session", "\\u0066ixture-session")
            .into_bytes();
    }
    respond(io, 200, "OK", Some(session), &bytes).await;
}
async fn respond(
    io: &mut (impl tokio::io::AsyncWrite + Unpin),
    status: u16,
    reason: &str,
    session: Option<&str>,
    body: &[u8],
) {
    let session = session
        .map(|value| format!("Mcp-Session-Id: {value}\r\n"))
        .unwrap_or_default();
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\n{session}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = io.write_all(head.as_bytes()).await;
    let _ = io.write_all(body).await;
}
async fn event(io: &mut (impl tokio::io::AsyncWrite + Unpin), value: &Value) {
    event_json(io, &serde_json::to_string(value).unwrap()).await;
}
async fn event_json(io: &mut (impl tokio::io::AsyncWrite + Unpin), value: &str) {
    let data = format!("data: {value}\n\n");
    let frame = format!("{:x}\r\n{}\r\n", data.len(), data);
    let _ = io.write_all(frame.as_bytes()).await;
    let _ = io.flush().await;
}
