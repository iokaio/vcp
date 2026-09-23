// SPDX-License-Identifier: Apache-2.0
#![cfg(windows)]
//! Crash after durable acceptance but before the client consumes its response.
//! This does not claim the server had not already sent/buffered that response.
#[path = "support/local_fixture.rs"]
mod wire;
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    os::windows::{
        ffi::OsStringExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};
use vcp_domain::task::{Task, TaskState};
use vcp_lifecycle::foundation::CanonicalHost;
use vcp_protocol::command::Command as CanonicalCommand;
use vcp_store::{contract::Collection, BackendKind};
use windows_sys::Win32::{
    Foundation::{FILETIME, WAIT_OBJECT_0},
    System::{
        Pipes::GetNamedPipeInfo,
        Threading::{
            GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, TerminateProcess,
            WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
            PROCESS_TERMINATE,
        },
    },
};

// No background stdout reader: after send_batch we cannot consume the reply.
struct Client {
    child: Child,
    input: ChildStdin,
    output: Option<BufReader<ChildStdout>>,
}
impl Client {
    fn launch(fixture: &wire::Fixture) -> (Self, Value) {
        let mut child = Command::new(env!("CARGO_BIN_EXE_vcp"))
            .arg("local-bridge")
            .env_remove("OPENROUTER_API_KEY")
            .env_remove("OPENAI_API_KEY")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut client = Self {
            child,
            input,
            output: Some(output),
        };
        let mut bootstrap = fixture.bootstrap("controller");
        bootstrap["transport"] = json!("windows_pipe");
        client.send(bootstrap);
        let ready = client.receive();
        assert_eq!(ready["schema"], "vcp-local-ready/1");
        (client, ready)
    }
    fn send(&mut self, value: Value) {
        let mut bytes = serde_json::to_vec(&value).unwrap();
        bytes.push(b'\n');
        self.input.write_all(&bytes).unwrap();
        self.input.flush().unwrap();
    }
    fn receive(&mut self) -> Value {
        let mut output = self.output.take().unwrap();
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        let reader = std::thread::spawn(move || {
            let mut frame = String::new();
            let result = output.read_line(&mut frame);
            let _ = send.send((output, frame, result));
        });
        let (output, frame, result) = receive
            .recv_timeout(Duration::from_secs(20))
            .expect("bounded initial controller response");
        reader.join().unwrap();
        self.output = Some(output);
        result.unwrap();
        serde_json::from_str(&frame).unwrap()
    }
    fn rpc(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(request(id, method, params));
        let reply = self.receive();
        assert_eq!(reply["id"], id);
        reply
    }
    fn capacity(&self) -> usize {
        let (mut outbound, mut inbound) = (0, 0);
        // SAFETY: live owned anonymous pipe; outputs are valid u32 pointers.
        assert_ne!(
            unsafe {
                GetNamedPipeInfo(
                    self.output.as_ref().unwrap().get_ref().as_raw_handle(),
                    std::ptr::null_mut(),
                    &mut outbound,
                    &mut inbound,
                    std::ptr::null_mut(),
                )
            },
            0
        );
        outbound.max(inbound) as usize
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
struct Server(OwnedHandle);
impl Drop for Server {
    fn drop(&mut self) {
        // This owner is constructed only after creation-time and image checks.
        // An assertion failure must not leave its dedicated server running.
        unsafe {
            if WaitForSingleObject(self.0.as_raw_handle(), 0) != WAIT_OBJECT_0 {
                let _ = TerminateProcess(self.0.as_raw_handle(), 73);
                let _ = WaitForSingleObject(self.0.as_raw_handle(), 10000);
            }
        }
    }
}
impl Server {
    fn pin(ready: &Value) -> Self {
        let pin = &ready["server"];
        let pid = u32::try_from(pin["pid"].as_u64().unwrap()).unwrap();
        // SAFETY: creates an owned handle; only the authenticated fixture PID.
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE | PROCESS_TERMINATE,
                0,
                pid,
            )
        };
        assert!(!handle.is_null());
        let owned = unsafe { OwnedHandle::from_raw_handle(handle) };
        let mut created = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut exit, mut kernel, mut user) = (created, created, created);
        // SAFETY: held process and valid FILETIME outputs.
        assert_ne!(
            unsafe { GetProcessTimes(handle, &mut created, &mut exit, &mut kernel, &mut user) },
            0
        );
        let timestamp =
            (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
        assert_eq!(timestamp.to_string(), pin["created"].as_str().unwrap());
        let mut image = vec![0u16; 32768];
        let mut length = image.len() as u32;
        // SAFETY: owned process and sized writable UTF-16 buffer.
        assert_ne!(
            unsafe { QueryFullProcessImageNameW(handle, 0, image.as_mut_ptr(), &mut length) },
            0
        );
        let image =
            std::path::PathBuf::from(std::ffi::OsString::from_wide(&image[..length as usize]))
                .canonicalize()
                .unwrap();
        assert_eq!(
            image,
            std::path::Path::new(env!("CARGO_BIN_EXE_vcp"))
                .canonicalize()
                .unwrap()
        );
        assert_eq!(
            image,
            std::path::Path::new(pin["image"].as_str().unwrap())
                .canonicalize()
                .unwrap()
        );
        Self(owned)
    }
    fn crash(&self) {
        // SAFETY: handle pinned to this fixture's authenticated server instance;
        // retaining it prevents PID reuse from changing the termination target.
        assert_ne!(unsafe { TerminateProcess(self.0.as_raw_handle(), 73) }, 0);
        assert_eq!(
            unsafe { WaitForSingleObject(self.0.as_raw_handle(), 10000) },
            WAIT_OBJECT_0
        );
    }
}
fn request(id: u64, method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
}
fn initialization() -> Value {
    let methods = [
        "task/read",
        "session/create",
        "command/read",
        "controller/read",
        "controller/acquire",
        "controller/recover",
    ];
    json!({"protocol_version":"1.0","client":{"name":"crash-before-client-receipt","version":"1"},"capabilities":methods,"required_capabilities":methods})
}
async fn bounded_projection(fixture: &wire::Fixture) {
    let (host, owner) = CanonicalHost::open(fixture.config.clone()).unwrap();
    let task: Task = host
        .snapshot()
        .unwrap()
        .record(
            Collection::Task,
            fixture.config.root_task.as_str(),
            &fixture.config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    host.command(
        CanonicalCommand::Transition {
            next: TaskState::Blocked,
            reason: "bounded crash fixture evidence; ".repeat(64),
            verification: None,
        },
        Some(fixture.config.root_task.clone()),
        task.revision,
    )
    .unwrap();
    owner.close().await.unwrap();
    drop(host);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn durable_acceptance_survives_crash_before_client_consumes_batch_response() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let fixture = wire::Fixture::new(backend).await;
        bounded_projection(&fixture).await;
        let (mut client, ready) = Client::launch(&fixture);
        let server = Server::pin(&ready);
        assert!(client
            .rpc(1, "initialize", initialization())
            .get("error")
            .is_none());
        wire::accepted(&client.rpc(
            2,
            "controller/acquire",
            json!({"scope":fixture.scope(),"command_id":"crash-owner","expected_revision":null}),
        ));
        let params = json!({"scope":fixture.scope(),"task":fixture.config.root_task});
        let sample = client.rpc(3, "task/read", params.clone());
        assert!(sample.get("error").is_none());
        let create = json!({"scope":fixture.scope(),"mutation":{"command_id":"crash-create-once","expected_revision":"0","steering_revision":"0"},"new_session":"crash-created-session","configuration_revision":"0"});
        let mut observer = wire::Client::spawn("local-bridge");
        observer.send(json!({"schema":"vcp-local-attach/1","attachment":ready["observer_attachment"],"role":"observer"}));
        assert_eq!(observer.receive()["schema"], "vcp-local-ready/1");
        assert!(observer
            .rpc(1, "initialize", initialization())
            .get("error")
            .is_none());
        // The profile permits 64 entries. The mutation is last, so serialized
        // successful query results precede its acceptance in the single reply.
        let mut batch: Vec<_> = (100..163)
            .map(|id| request(id, "task/read", params.clone()))
            .collect();
        batch.push(request(163, "session/create", create.clone()));
        let minimum_prefix = 63 * serde_json::to_vec(&sample["result"]).unwrap().len();
        let capacity = client.capacity();
        assert!(
            minimum_prefix > capacity,
            "fixture response prefix {minimum_prefix} must exceed stdout pipe capacity {capacity}"
        );
        assert!(
            63 * serde_json::to_vec(&sample).unwrap().len() + 4096 < 256 * 1024,
            "batch response stays within protocol frame limit"
        );
        assert!(client.output.as_ref().unwrap().buffer().is_empty());
        client.send(Value::Array(batch));
        // No reads from client.output occur after this point. The bridge may
        // buffer the reply; that is not an application receipt or acknowledgement.
        let until = Instant::now() + Duration::from_secs(4);
        let receipt = loop {
            let response = observer.rpc(
                2,
                "command/read",
                json!({"scope":fixture.scope(),"command_id":"crash-create-once"}),
            );
            if response.get("error").is_none() {
                break response["result"].clone();
            }
            assert!(
                Instant::now() < until,
                "durable acceptance before bounded slow-reader disconnect"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        };
        server.crash();
        drop(client);
        drop(observer);
        let store = fixture.reopen().await;
        assert_eq!(
            store
                .state()
                .commands
                .values()
                .filter(|receipt| receipt.command.as_str() == "crash-create-once")
                .count(),
            1
        );
        assert!(store
            .state()
            .record(
                Collection::Session,
                "crash-created-session",
                &fixture.config.workspace
            )
            .is_ok());
        store.close().await.unwrap();
        let mut reconnected = wire::Client::connect(&fixture, "controller");
        assert!(reconnected
            .rpc(1, "initialize", initialization())
            .get("error")
            .is_none());
        assert_eq!(
            reconnected.rpc(
                2,
                "command/read",
                json!({"scope":fixture.scope(),"command_id":"crash-create-once"})
            )["result"],
            receipt
        );
        let lease = reconnected.rpc(3, "controller/read", json!({"scope":fixture.scope()}));
        // Recovery remains explicit: a new process does not inherit ownership.
        wire::accepted(&reconnected.rpc(4,"controller/recover",json!({"scope":fixture.scope(),"command_id":"recover-crashed-owner","expected_revision":lease["result"]["value"]["revision"],"generation":lease["result"]["value"]["generation"]})));
        let released = reconnected.rpc(5, "controller/read", json!({"scope":fixture.scope()}));
        wire::accepted(&reconnected.rpc(6,"controller/acquire",json!({"scope":fixture.scope(),"command_id":"replacement-owner","expected_revision":released["result"]["value"]["revision"]})));
        assert_eq!(
            reconnected.rpc(7, "session/create", create)["result"],
            receipt
        );
        assert!(reconnected.finish().await.0.success());
        let store = fixture.reopen().await;
        assert_eq!(
            store
                .state()
                .commands
                .values()
                .filter(|receipt| receipt.command.as_str() == "crash-create-once")
                .count(),
            1
        );
        store.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn abandoned_reader_releases_controller_without_blocking_observer_or_replacement() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let fixture = wire::Fixture::new(backend).await;
        bounded_projection(&fixture).await;
        let (mut abandoned, ready) = Client::launch(&fixture);
        let server = Server::pin(&ready);
        assert!(abandoned
            .rpc(1, "initialize", initialization())
            .get("error")
            .is_none());
        wire::accepted(&abandoned.rpc(2,"controller/acquire",json!({"scope":fixture.scope(),"command_id":"slow-reader-owner","expected_revision":null})));
        let params = json!({"scope":fixture.scope(),"task":fixture.config.root_task});
        let sample = abandoned.rpc(3, "task/read", params.clone());
        assert!(sample.get("error").is_none());
        let prefix = 63 * serde_json::to_vec(&sample["result"]).unwrap().len();
        let capacity = abandoned.capacity();
        assert!(
            prefix > capacity,
            "response prefix {prefix} exceeds pipe capacity {capacity}"
        );
        assert!(63 * serde_json::to_vec(&sample).unwrap().len() + 4096 < 256 * 1024);
        let mut observer = wire::Client::spawn("local-bridge");
        observer.send(json!({"schema":"vcp-local-attach/1","attachment":ready["observer_attachment"],"role":"observer"}));
        assert_eq!(observer.receive()["schema"], "vcp-local-ready/1");
        assert!(observer
            .rpc(1, "initialize", initialization())
            .get("error")
            .is_none());
        let create = json!({"scope":fixture.scope(),"mutation":{"command_id":"slow-reader-create","expected_revision":"0","steering_revision":"0"},"new_session":"slow-reader-created","configuration_revision":"0"});
        let mut batch: Vec<_> = (100..163)
            .map(|id| request(id, "task/read", params.clone()))
            .collect();
        batch.push(request(163, "session/create", create));
        assert!(abandoned.output.as_ref().unwrap().buffer().is_empty());
        abandoned.send(Value::Array(batch));
        // Keep both unread stdout and stdin open. Only the production bounded
        // writer deadline, not a test EOF or kill, can free this controller.
        let until = Instant::now() + Duration::from_secs(15);
        let released = loop {
            let lease = observer.rpc(2, "controller/read", json!({"scope":fixture.scope()}));
            assert!(lease.get("error").is_none(), "observer remains serviceable");
            if lease["result"]["value"]["ownership"] == "released" {
                break lease;
            }
            assert!(
                Instant::now() < until,
                "abandoned reader must release controller within bound"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        };
        assert_eq!(
            unsafe { WaitForSingleObject(server.0.as_raw_handle(), 0) },
            windows_sys::Win32::Foundation::WAIT_TIMEOUT,
            "server remains live throughout timeout recovery"
        );
        let first = observer.rpc(
            3,
            "command/read",
            json!({"scope":fixture.scope(),"command_id":"slow-reader-create"}),
        );
        wire::accepted(&first);
        let mut replacement = wire::Client::spawn("local-bridge");
        replacement.send(json!({"schema":"vcp-local-attach/1","attachment":ready["attachment"],"role":"controller"}));
        assert_eq!(replacement.receive()["schema"], "vcp-local-ready/1");
        assert!(replacement
            .rpc(1, "initialize", initialization())
            .get("error")
            .is_none());
        let next = json!({"scope":fixture.scope(),"mutation":{"command_id":"after-slow-reader","expected_revision":"0","steering_revision":"0"},"new_session":"replacement-created","configuration_revision":"0"});
        assert!(
            replacement
                .rpc(2, "session/create", next.clone())
                .get("error")
                .is_some(),
            "attachment alone cannot take ownership"
        );
        wire::accepted(&replacement.rpc(3,"controller/acquire",json!({"scope":fixture.scope(),"command_id":"slow-reader-replacement","expected_revision":released["result"]["value"]["revision"]})));
        let second = replacement.rpc(4, "session/create", next);
        wire::accepted(&second);
        assert_eq!(
            observer.rpc(
                4,
                "command/read",
                json!({"scope":fixture.scope(),"command_id":"after-slow-reader"})
            )["result"],
            second["result"]
        );
        // We still have not consumed any of the abandoned response bytes.
        drop(abandoned);
        assert!(replacement.finish().await.0.success());
        assert!(observer.finish().await.0.success());
        // Only test cleanup terminates the now unused dedicated server.
        drop(server);
        let store = fixture.reopen().await;
        for command in ["slow-reader-create", "after-slow-reader"] {
            assert_eq!(
                store
                    .state()
                    .commands
                    .values()
                    .filter(|receipt| receipt.command.as_str() == command)
                    .count(),
                1
            );
        }
        store.close().await.unwrap();
    }
}
