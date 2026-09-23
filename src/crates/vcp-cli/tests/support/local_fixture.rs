// SPDX-License-Identifier: Apache-2.0
//! Shared real-process local attachment fixtures; no provider is configured.
#![allow(dead_code)]
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command as Process, ExitStatus, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};
use vcp_cli::settings::WorkspaceEntry;
use vcp_domain::{
    accounting::*, ids::*, revision::*, task::*, verification::Fingerprint, workspace::*,
};
use vcp_lifecycle::foundation::{CanonicalHost, Config};
use vcp_protocol::command::Command;
use vcp_store::{contract::Collection, BackendKind, Store};

pub(crate) struct Fixture {
    _temporary: tempfile::TempDir,
    pub(crate) workspace: PathBuf,
    pub(crate) data: PathBuf,
    pub(crate) config: Config,
}
impl Fixture {
    pub(crate) async fn new(backend: BackendKind) -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let base = temporary.path().canonicalize().unwrap();
        let workspace = base.join("workspace");
        let data = base.join("private-data");
        let directory = data.join("workspaces").join("local-fixture");
        fs::create_dir(&workspace).unwrap();
        fs::create_dir_all(&directory).unwrap();
        let currency: Currency = "USD".to_owned().try_into().unwrap();
        let config = Config {
            canonical_root: directory.join("canonical"),
            backend,
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
            binding: Binding {
                host: HostId::new(),
                root: workspace.to_string_lossy().into_owned(),
                repository: "local-process-fixture".into(),
                worktree: "main".into(),
                revision: Revision::ZERO,
            },
            actor: ActorId::new(),
            root_task: TaskId::new(),
            cap: Money {
                currency: currency.clone(),
                micros: Micros::ZERO,
            },
            protected: Micros::ZERO,
            price: PriceSnapshot {
                id: "a".repeat(64),
                provider: "offline-fixture".into(),
                model: "never-dispatched".into(),
                currency,
                capability: "b".repeat(64),
                valid_until: Timestamp::new(u64::MAX),
                rates: [
                    ChargeCategory::Input,
                    ChargeCategory::Output,
                    ChargeCategory::CacheRead,
                    ChargeCategory::CacheWrite,
                    ChargeCategory::Request,
                    ChargeCategory::ProviderTool,
                ]
                .into_iter()
                .map(|kind| {
                    (
                        kind,
                        Rate {
                            micros: Micros::ZERO,
                            per_units: Units::new(1),
                        },
                    )
                })
                .collect(),
            },
            input_ceiling: Units::new(1024),
            output_ceiling: Units::new(1024),
            max_transport_retries: 0,
            artifact_limit: ByteCount::new(vcp_store::artifact::DEFAULT_ARTIFACT_LIMIT),
            host_tool_denials: vec![],
        };
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        host.command(
            Command::CreateTask {
                root: config.root_task.clone(),
                parent: None,
                fork_origin: None,
                objective: Objective {
                    text: "offline attachment history".into(),
                    constraints: vec![],
                    acceptance: vec![],
                    source: EventId::new(),
                    steering: SteeringRevision::ZERO,
                },
                fingerprint: Fingerprint {
                    repository: "a".repeat(64),
                    buffers: "b".repeat(64),
                    environment: "c".repeat(64),
                },
                editing: false,
                required_checks: vec![],
            },
            Some(config.root_task.clone()),
            Revision::ZERO,
        )
        .unwrap();
        host.command(
            Command::Transition {
                next: TaskState::Paused,
                reason: "attachment must leave work paused".into(),
                verification: None,
            },
            Some(config.root_task.clone()),
            Revision::ZERO,
        )
        .unwrap();
        let root = vcp_repository::Root::open(
            vcp_repository::RootIdentity {
                workspace: config.workspace.clone(),
                root: RootId::parse(config.workspace.as_str()).unwrap(),
                repository: config.binding.repository.clone(),
                worktree: config.binding.worktree.clone(),
                binding: config.binding.revision,
            },
            &workspace,
        )
        .unwrap();
        vcp_cli::settings::save(
            &directory.join("workspace.json"),
            &WorkspaceEntry {
                version: 1,
                rebind_pending: false,
                config: config.clone(),
                identity: Some(vcp_cli::binding::capture(&root).unwrap()),
            },
        )
        .unwrap();
        owner.close().await.unwrap();
        drop(host);
        Self {
            _temporary: temporary,
            workspace,
            data,
            config,
        }
    }
    pub(crate) fn bootstrap(&self, role: &str) -> Value {
        json!({"schema":"vcp-local-bootstrap/1","workspace":self.workspace,"data":self.data,"role":role})
    }
    pub(crate) fn scope(&self) -> Value {
        json!({"workspace":self.config.workspace,"session":self.config.session})
    }
    pub(crate) async fn reopen(&self) -> Store {
        self.reopen_within(Duration::from_secs(15)).await
    }
    pub(crate) async fn reopen_within(&self, timeout: Duration) -> Store {
        let deadline = Instant::now() + timeout;
        loop {
            match Store::open(&self.config.canonical_root, self.config.backend, &[]).await {
                Ok(store) => return store,
                Err(error) if Instant::now() < deadline => {
                    let _ = error;
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Err(error) => panic!("server did not release canonical writer: {error}"),
            }
        }
    }
}

pub(crate) struct Client {
    child: Child,
    pub(crate) input: Option<ChildStdin>,
    frames: mpsc::Receiver<Result<Value, String>>,
    diagnostics: mpsc::Receiver<String>,
}
impl Client {
    pub(crate) fn spawn(mode: &str) -> Self {
        let mut child = Process::new(env!("CARGO_BIN_EXE_vcp"))
            .arg(mode)
            .env_remove("OPENROUTER_API_KEY")
            .env_remove("OPENAI_API_KEY")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (send, frames) = mpsc::sync_channel(16);
        std::thread::spawn(move || {
            let mut output = BufReader::new(stdout);
            loop {
                let mut frame = Vec::new();
                match output
                    .by_ref()
                    .take(1024 * 1024 + 1)
                    .read_until(b'\n', &mut frame)
                {
                    Ok(0) => break,
                    Ok(_) => {
                        let result = if frame.len() > 1024 * 1024 || frame.last() != Some(&b'\n') {
                            Err("unbounded or partial protocol stdout".into())
                        } else {
                            serde_json::from_slice(&frame)
                                .map_err(|_| "non-JSON diagnostics on protocol stdout".into())
                        };
                        if send.send(result).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = send.send(Err(error.to_string()));
                        break;
                    }
                }
            }
        });
        let (send, diagnostics) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stderr.take(128 * 1024).read_to_end(&mut bytes);
            let _ = send.send(String::from_utf8_lossy(&bytes).into_owned());
        });
        Self {
            child,
            input,
            frames,
            diagnostics,
        }
    }
    pub(crate) fn send(&mut self, value: Value) {
        let mut bytes = serde_json::to_vec(&value).unwrap();
        bytes.push(b'\n');
        self.raw(&bytes);
    }
    pub(crate) fn raw(&mut self, bytes: &[u8]) {
        self.input.as_mut().unwrap().write_all(bytes).unwrap();
        self.input.as_mut().unwrap().flush().unwrap();
    }
    #[track_caller]
    pub(crate) fn receive(&self) -> Value {
        match self.frames.recv_timeout(Duration::from_secs(20)) {
            Ok(frame) => frame.expect("pure protocol stdout"),
            Err(error) => panic!(
                "bounded compiled process response: {error:?}; diagnostics: {}",
                self.diagnostics.try_recv().unwrap_or_default()
            ),
        }
    }
    pub(crate) fn connect(fixture: &Fixture, role: &str) -> Self {
        let mut client = Self::spawn("local-bridge");
        client.send(fixture.bootstrap(role));
        let ready = client.receive();
        assert_eq!(ready["schema"], "vcp-local-ready/1");
        assert_eq!(ready["scope"], fixture.scope());
        assert_eq!(ready["role"], role);
        assert!(ready["server"].is_object());
        client
    }
    #[track_caller]
    pub(crate) fn rpc(&mut self, request: u64, method: &str, params: Value) -> Value {
        self.send(json!({"jsonrpc":"2.0","id":request,"method":method,"params":params}));
        let response = self.receive();
        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], request);
        response
    }
    pub(crate) fn initialize(&mut self) {
        let response = self.rpc(1, "initialize", json!({"protocol_version":"1.0","client":{"name":"compiled-fixture","version":"1"},"capabilities":["session/read","session/create","command/read","controller/read","controller/acquire","controller/release","controller/recover"],"required_capabilities":["controller/read","controller/acquire","session/read"]}));
        assert!(response.get("error").is_none(), "{response}");
        assert!(response["result"]["methods"]
            .as_array()
            .unwrap()
            .contains(&json!("controller/acquire")));
        assert_eq!(response["result"]["execution_host"]["platform"], "windows");
    }
    pub(crate) async fn finish(&mut self) -> (ExitStatus, String) {
        self.finish_inner(true).await
    }
    pub(crate) async fn rejected(&mut self) -> (ExitStatus, String) {
        // Keep input open until rejection, so EOF cannot mask the tested boundary.
        self.finish_inner(false).await
    }
    async fn finish_inner(&mut self, close_input: bool) -> (ExitStatus, String) {
        if close_input {
            drop(self.input.take());
        }
        let deadline = Instant::now() + Duration::from_secs(20);
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "local helper failed to exit after EOF"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        drop(self.input.take());
        match self.frames.recv_timeout(Duration::from_secs(3)) {
            Err(mpsc::RecvTimeoutError::Disconnected) => (),
            other => panic!("unexpected trailing stdout or held output handle: {other:?}"),
        }
        let diagnostics = self
            .diagnostics
            .recv_timeout(Duration::from_secs(3))
            .unwrap();
        (status, diagnostics)
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        drop(self.input.take());
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
pub(crate) fn accepted(response: &Value) -> &Value {
    assert!(response.get("error").is_none(), "{response}");
    assert_eq!(response["result"]["kind"], "acceptance");
    &response["result"]
}
pub(crate) fn assert_offline_paused(store: &Store, config: &Config) {
    let task: Task = store
        .state()
        .record(
            Collection::Task,
            config.root_task.as_str(),
            &config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(task.state, TaskState::Paused);
    assert!(!store
        .state()
        .records
        .values()
        .any(|row| matches!(row.collection, Collection::Attempt | Collection::Effect)));
}
