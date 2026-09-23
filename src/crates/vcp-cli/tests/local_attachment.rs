// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
//! Compiled controlled-launch bootstrap and canonical RPC process evidence.
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
    accounting::*,
    controller::{Lease, Reason},
    ids::*,
    revision::*,
    task::*,
    verification::Fingerprint,
    workspace::*,
};
use vcp_lifecycle::foundation::{CanonicalHost, Config};
use vcp_protocol::command::Command;
use vcp_store::{contract::Collection, BackendKind, Store};

struct Fixture {
    _temporary: tempfile::TempDir,
    workspace: PathBuf,
    data: PathBuf,
    config: Config,
}
impl Fixture {
    async fn new(backend: BackendKind) -> Self {
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
    fn bootstrap(&self, role: &str) -> Value {
        json!({"schema":"vcp-local-bootstrap/1","workspace":self.workspace,"data":self.data,"role":role})
    }
    fn scope(&self) -> Value {
        json!({"workspace":self.config.workspace,"session":self.config.session})
    }
    async fn reopen(&self) -> Store {
        let deadline = Instant::now() + Duration::from_secs(15);
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

struct Client {
    child: Child,
    input: Option<ChildStdin>,
    frames: mpsc::Receiver<Result<Value, String>>,
    diagnostics: mpsc::Receiver<String>,
}
impl Client {
    fn spawn(mode: &str) -> Self {
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
    fn send(&mut self, value: Value) {
        let mut bytes = serde_json::to_vec(&value).unwrap();
        bytes.push(b'\n');
        self.raw(&bytes);
    }
    fn raw(&mut self, bytes: &[u8]) {
        self.input.as_mut().unwrap().write_all(bytes).unwrap();
        self.input.as_mut().unwrap().flush().unwrap();
    }
    fn receive(&self) -> Value {
        self.frames
            .recv_timeout(Duration::from_secs(20))
            .expect("bounded compiled process response")
            .expect("pure protocol stdout")
    }
    fn connect(fixture: &Fixture, role: &str) -> Self {
        let mut client = Self::spawn("local-bridge");
        client.send(fixture.bootstrap(role));
        let ready = client.receive();
        assert_eq!(ready["schema"], "vcp-local-ready/1");
        assert_eq!(ready["scope"], fixture.scope());
        assert_eq!(ready["role"], role);
        assert!(ready["server"].is_object());
        client
    }
    fn rpc(&mut self, request: u64, method: &str, params: Value) -> Value {
        self.send(json!({"jsonrpc":"2.0","id":request,"method":method,"params":params}));
        let response = self.receive();
        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], request);
        response
    }
    fn initialize(&mut self) {
        let response = self.rpc(1, "initialize", json!({"protocol_version":"1.0","client":{"name":"compiled-fixture","version":"1"},"capabilities":["session/read","session/create","command/read","controller/read","controller/acquire","controller/release","controller/recover"],"required_capabilities":["controller/read","controller/acquire","session/read"]}));
        assert!(response.get("error").is_none(), "{response}");
        assert!(response["result"]["methods"]
            .as_array()
            .unwrap()
            .contains(&json!("controller/acquire")));
        assert_eq!(response["result"]["execution_host"]["platform"], "windows");
    }
    async fn finish(&mut self) -> (ExitStatus, String) {
        self.finish_inner(true).await
    }
    async fn rejected(&mut self) -> (ExitStatus, String) {
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
fn accepted(response: &Value) -> &Value {
    assert!(response.get("error").is_none(), "{response}");
    assert_eq!(response["result"]["kind"], "acceptance");
    &response["result"]
}
fn assert_offline_paused(store: &Store, config: &Config) {
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_controller_retries_preserve_identity_and_release_keeps_reads_open() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let fixture = Fixture::new(backend).await;
        let mut client = Client::connect(&fixture, "controller");
        client.initialize();
        let scope = fixture.scope();
        let acquire = json!({"scope":scope,"command_id":"acquire","expected_revision":null});
        let receipt = client.rpc(2, "controller/acquire", acquire.clone());
        accepted(&receipt);
        assert_eq!(
            client.rpc(3, "controller/acquire", acquire)["result"],
            receipt["result"]
        );
        let create = json!({"scope":scope,"mutation":{"command_id":"create-once","expected_revision":"0","steering_revision":"0"},"new_session":"created","configuration_revision":"0"});
        let created = client.rpc(4, "session/create", create.clone());
        accepted(&created);
        assert_eq!(
            client.rpc(5, "session/create", create.clone())["result"],
            created["result"]
        );
        assert_eq!(
            client.rpc(
                6,
                "command/read",
                json!({"scope":scope,"command_id":"create-once"})
            )["result"],
            created["result"]
        );
        let mut conflict = create;
        conflict["new_session"] = json!("different");
        assert!(client
            .rpc(7, "session/create", conflict)
            .get("error")
            .is_some());
        let mut other = scope.clone();
        other["session"] = json!("created");
        assert!(client
            .rpc(8, "session/read", json!({"scope":other}))
            .get("error")
            .is_some());
        let lease = client.rpc(9, "controller/read", json!({"scope":scope}));
        assert_eq!(lease["result"]["value"]["ownership"], "this_connection");
        let release = json!({"scope":scope,"command_id":"release","expected_revision":lease["result"]["value"]["revision"],"generation":lease["result"]["value"]["generation"]});
        let released = client.rpc(10, "controller/release", release.clone());
        accepted(&released);
        assert_eq!(
            client.rpc(11, "controller/release", release)["result"],
            released["result"]
        );
        assert_eq!(
            client.rpc(12, "session/read", json!({"scope":scope}))["result"]["kind"],
            "session"
        );
        assert_eq!(
            client.rpc(13, "controller/read", json!({"scope":scope}))["result"]["value"]
                ["ownership"],
            "released"
        );
        assert!(client.finish().await.0.success());
        let store = fixture.reopen().await;
        assert_offline_paused(&store, &fixture.config);
        assert_eq!(
            store
                .state()
                .commands
                .values()
                .filter(|receipt| receipt.command.as_str() == "create-once")
                .count(),
            1
        );
        store.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_observer_reads_without_controller_authority_or_provider_configuration() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let fixture = Fixture::new(backend).await;
        let mut client = Client::connect(&fixture, "observer");
        client.initialize();
        let scope = fixture.scope();
        assert_eq!(
            client.rpc(2, "session/read", json!({"scope":scope}))["result"]["kind"],
            "session"
        );
        assert!(client
            .rpc(
                3,
                "controller/acquire",
                json!({"scope":scope,"command_id":"observer-claim","expected_revision":null})
            )
            .get("error")
            .is_some());
        assert!(client.rpc(4, "session/create", json!({"scope":scope,"mutation":{"command_id":"observer-create","expected_revision":"0","steering_revision":"0"},"new_session":"denied","configuration_revision":"0"})).get("error").is_some());
        assert_eq!(
            client.rpc(5, "controller/read", json!({"scope":scope}))["result"]["value"]
                ["ownership"],
            "unclaimed"
        );
        assert!(client.finish().await.0.success());
        let store = fixture.reopen().await;
        assert_offline_paused(&store, &fixture.config);
        assert!(!store
            .state()
            .commands
            .values()
            .any(|receipt| receipt.command.as_str().starts_with("observer-")));
        store.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_attachment_refuses_competing_writer_and_eof_releases_owner_paused() {
    let fixture = Fixture::new(BackendKind::Files).await;
    let mut owner = Client::connect(&fixture, "controller");
    owner.initialize();
    accepted(&owner.rpc(
        2,
        "controller/acquire",
        json!({"scope":fixture.scope(),"command_id":"owner-acquire","expected_revision":null}),
    ));
    assert!(
        Store::open(&fixture.config.canonical_root, fixture.config.backend, &[])
            .await
            .is_err()
    );
    let mut competing = Client::spawn("local-bridge");
    competing.send(fixture.bootstrap("controller"));
    let (status, diagnostics) = competing.rejected().await;
    assert!(!status.success());
    assert!(!diagnostics.is_empty());
    assert_eq!(
        owner.rpc(3, "session/read", json!({"scope":fixture.scope()}))["result"]["kind"],
        "session"
    );
    assert!(owner.finish().await.0.success());
    let store = fixture.reopen().await;
    assert_offline_paused(&store, &fixture.config);
    let lease: Lease = store
        .state()
        .records
        .values()
        .find(|row| {
            row.collection == Collection::Access
                && row.value["document_type"] == "vcp_controller_lease_v1"
        })
        .unwrap()
        .decode()
        .unwrap();
    assert!(lease.holder.is_none());
    assert_eq!(lease.reason, Reason::ConnectionLost);
    store.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_bootstrap_rejects_malformed_oversized_and_unproved_server_launch_without_stdout()
{
    for (mode, bytes) in [
        ("local-bridge", b"{invalid}\n".to_vec()),
        ("local-bridge", { let mut frame = vec![b' '; 16 * 1024 + 1]; frame.push(b'\n'); frame }),
        ("local-bridge", b"{\"schema\":\"vcp-local-bootstrap/1\",\"workspace\":\"relative\",\"data\":null,\"role\":\"controller\"}\n".to_vec()),
        ("local-server", b"{\"schema\":\"vcp-local-bootstrap/1\",\"parent_handle\":0}\n".to_vec()),
    ] {
        let mut client = Client::spawn(mode); client.raw(&bytes);
        let (status, diagnostics) = client.rejected().await;
        assert!(!status.success()); assert!(!diagnostics.is_empty());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_oversized_protocol_frame_closes_live_controller_and_releases_writer() {
    let fixture = Fixture::new(BackendKind::Sqlite).await;
    let mut client = Client::connect(&fixture, "controller");
    client.initialize();
    accepted(&client.rpc(
        2,
        "controller/acquire",
        json!({"scope":fixture.scope(),"command_id":"oversize-owner","expected_revision":null}),
    ));
    assert!(
        Store::open(&fixture.config.canonical_root, fixture.config.backend, &[])
            .await
            .is_err()
    );

    let mut input = client.input.take().unwrap();
    let (written, result) = mpsc::sync_channel(1);
    let (release, hold) = mpsc::sync_channel(1);
    let writer = std::thread::spawn(move || {
        // This exceeds the negotiated 1 MiB lexical frame ceiling before LF.
        // Keep the input handle open afterward: the helper must close on its
        // bounded decoder failure, independently of EOF or a valid request.
        let outcome = input.write_all(&vec![b' '; 1024 * 1024 + 2]);
        let _ = written.send(outcome.map_err(|error| error.kind()));
        let _ = hold.recv_timeout(Duration::from_secs(30));
        drop(input);
    });
    let _ = client.rejected().await;
    // A failed write is expected when the peer rejects before consuming all bytes.
    let _ = result
        .recv_timeout(Duration::from_secs(3))
        .expect("bounded writer must unblock after peer rejection");
    let _ = release.send(());
    writer.join().unwrap();

    let store = fixture.reopen().await;
    assert_offline_paused(&store, &fixture.config);
    let lease: Lease = store
        .state()
        .records
        .values()
        .find(|row| {
            row.collection == Collection::Access
                && row.value["document_type"] == "vcp_controller_lease_v1"
        })
        .unwrap()
        .decode()
        .unwrap();
    assert!(lease.holder.is_none());
    assert_eq!(lease.reason, Reason::ConnectionLost);
    assert_eq!(lease.generation, Revision::new(1));
    assert_eq!(
        store
            .state()
            .commands
            .values()
            .filter(|receipt| receipt.command.as_str() == "oversize-owner")
            .count(),
        1
    );
    assert_eq!(
        store
            .state()
            .records
            .values()
            .filter(|row| row.collection == Collection::Session)
            .count(),
        1
    );
    store.close().await.unwrap();
}
