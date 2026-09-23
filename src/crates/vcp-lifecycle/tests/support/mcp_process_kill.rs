// SPDX-License-Identifier: Apache-2.0
//! Independent owner termination after a valid MCP reply, before result capture.
use super::*;
use super::{
    mcp::Fixture,
    mcp_http,
    mcp_http_peer::{Peer, Scenario},
};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::Path,
    process::{Child, Stdio},
    time::Instant,
};
use tokio::sync::Notify;
use vcp_domain::effect::{Effect, EffectState};
use vcp_lifecycle::foundation::mcp::{
    remote_authority::{RemoteProfile, RemoteProfileConfig},
    RemoteRegistration, Request,
};

fn record(path: &Path, value: &impl serde::Serialize) {
    let temporary = path.with_extension("tmp");
    let mut file = fs::File::create(&temporary).unwrap();
    file.write_all(&serde_json::to_vec_pretty(value).unwrap())
        .unwrap();
    file.sync_all().unwrap();
    drop(file);
    fs::rename(temporary, path).unwrap();
}

fn result_receipts(host: &CanonicalHost, id: &ToolRunId) -> usize {
    host.snapshot()
        .unwrap()
        .records
        .values()
        .filter(|row| row.collection == Collection::Artifact)
        .filter(|row| {
            let artifact: ArtifactDescriptor = row.decode().unwrap();
            if !matches!(
                artifact.spec.schema.as_str(),
                "vcp-mcp-call-result-v1" | "vcp-mcp-http-result-v1"
            ) {
                return false;
            }
            let bytes = host.read_artifact(artifact.spec.id).unwrap();
            let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            value["effect"] == serde_json::json!(id)
        })
        .count()
}

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(Some(_))) {
            return;
        }
        let _ = self.0.kill();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            match self.0.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => std::thread::sleep(Duration::from_millis(25)),
                Err(_) => break,
            }
        }
        eprintln!(
            "MCP supervisor could not confirm owner PID {} stopped",
            self.0.id()
        );
    }
}

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
    fn WaitForSingleObject(handle: isize, millis: u32) -> u32;
    fn CloseHandle(handle: isize) -> i32;
}
struct NativeProcess(isize);
impl NativeProcess {
    fn open(pid: u32) -> Self {
        // SAFETY: SYNCHRONIZE only; this held handle prevents PID-reuse ambiguity.
        let handle = unsafe { OpenProcess(0x0010_0000, 0, pid) };
        assert_ne!(handle, 0);
        let process = Self(handle);
        assert_eq!(
            process.poll(),
            258,
            "MCP fixture must be live before owner kill"
        );
        process
    }
    fn poll(&self) -> u32 {
        // SAFETY: this owned process handle stays valid until Drop.
        unsafe { WaitForSingleObject(self.0, 0) }
    }
}
impl Drop for NativeProcess {
    fn drop(&mut self) {
        // SAFETY: exactly this instance owns and closes the native handle.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "independent supervisor launches the parked MCP owner"]
async fn mcp_reply_receipt_fault_owner() {
    let root = std::path::PathBuf::from(std::env::var_os("VCP_MCP_KILL_ROOT").unwrap());
    let backend = if std::env::var("VCP_MCP_KILL_BACKEND").unwrap() == "files" {
        BackendKind::Files
    } else {
        BackendKind::Sqlite
    };
    let transport = std::env::var("VCP_MCP_KILL_TRANSPORT").unwrap();
    let mut f = Fixture::new(backend, "normal").await;
    f._temp.disable_cleanup(true);
    record(
        &root.join("fixture.json"),
        &serde_json::json!({"root":f._temp.path(),"workspace":f.workspace}),
    );
    let arrived = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let id;
    let dispatch;
    if transport == "stdio" {
        let list = f
            .host
            .mcp_control(
                f.thread,
                Request::List {
                    server: "fixture".into(),
                },
            )
            .await
            .unwrap();
        let entry = list["catalog"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["tool"] == "write_marker")
            .unwrap();
        let ticket = f
            .host
            .prepare_mcp_call(
                f.thread,
                Request::Call {
                    server: "fixture".into(),
                    tool: "write_marker".into(),
                    identity_digest: entry["identity_digest"].as_str().unwrap().into(),
                    arguments_json: r#"{"value":"actual-owner-kill-once"}"#.into(),
                },
            )
            .await
            .unwrap();
        id = ticket.effect().clone();
        let ticket = ticket.qualification_block_before_receipt(arrived.clone(), release.clone());
        let host = f.host.clone();
        dispatch = tokio::spawn(async move { host.dispatch_mcp_call(ticket).await });
    } else {
        let peer: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("peer.json")).unwrap()).unwrap();
        let profile = RemoteProfile::new(RemoteProfileConfig {
            workspace: f.config.workspace.clone(),
            server: "remote".into(),
            revision: Revision::ZERO,
            endpoint: peer["endpoint"].as_str().unwrap().into(),
            credential_ref: None,
        })
        .unwrap();
        f.host
            .configure_mcp_remote(
                RemoteRegistration::fixture_roots(
                    profile,
                    BTreeSet::from(["echo".into(), "read_marker".into(), "write_marker".into()]),
                    vcp_extensions::mcp::registration::Limits {
                        frame_bytes: 64 * 1024,
                        total_discovery_bytes: 1024 * 1024,
                        tools: 16,
                        pages: 8,
                        timeout_ms: 10_000,
                        stderr_bytes: 0,
                    },
                    vec![serde_json::from_value(peer["certificate"].clone()).unwrap()],
                )
                .unwrap(),
            )
            .unwrap();
        let list = mcp_http::list(&f).await;
        let ticket = f
            .host
            .prepare_remote_mcp_call(
                f.thread,
                mcp_http::call(&list, "write_marker", serde_json::json!({})),
            )
            .await
            .unwrap();
        mcp_http::approve_ticket(&f, &ticket);
        id = ticket.effect().clone();
        let ticket = ticket.qualification_block_before_receipt(arrived.clone(), release.clone());
        let host = f.host.clone();
        dispatch = tokio::spawn(async move { host.dispatch_remote_mcp_call(ticket).await });
    }
    tokio::time::timeout(Duration::from_secs(15), arrived.notified())
        .await
        .unwrap();
    assert_eq!(result_receipts(&f.host, &id), 0);
    let before = f.host.snapshot().unwrap();
    let effect: Effect = before
        .record(Collection::Effect, id.as_str(), &f.config.workspace)
        .unwrap()
        .decode()
        .unwrap();
    assert!(matches!(
        effect.state,
        EffectState::Running | EffectState::DispatchRecorded
    ));
    record(&root.join("config.json"), &f.config);
    record(&root.join("before.json"), &before);
    record(
        &root.join("barrier.json"),
        &serde_json::json!({
            "phase":"valid_reply_before_durable_receipt","transport":transport,"effect":id,
            "result_receipts":0,"fixture_root":f._temp.path(),"workspace":f.workspace,
            "child_pid": if transport == "stdio" { Some(fs::read_to_string(f.workspace.join("started")).unwrap().parse::<u32>().unwrap()) } else { None }
        }),
    );
    let completed = dispatch.await;
    record(
        &root.join("barrier-completed-before-kill.json"),
        &serde_json::json!({
            "unexpected_dispatch_completion":true, "joined":completed.is_ok(),
        }),
    );
    panic!("MCP dispatch completed before the supervisor kill");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mcp_owner_process_kill_after_reply_reopens_unknown_without_replay() {
    for backend in ["files", "sqlite"] {
        for transport in ["stdio", "https"] {
            let mut temp = tempfile::tempdir().unwrap();
            temp.disable_cleanup(true);
            let root = temp.path();
            println!(
                "retained MCP owner-kill fixture {backend}/{transport}: {}",
                root.display()
            );
            let peer = if transport == "https" {
                Some(Peer::start(Scenario::Normal).await)
            } else {
                None
            };
            if let Some(peer) = &peer {
                record(
                    &root.join("peer.json"),
                    &serde_json::json!({"endpoint":peer.endpoint(),"certificate":peer.root_certificate()}),
                );
            }
            let deadline = Instant::now() + Duration::from_secs(60);
            let declared = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
                + 60_000;
            record(
                &root.join("schedule.json"),
                &serde_json::json!({"backend":backend,"transport":transport,"deadline_unix_ms":declared,"repeat_seed":"mcp-owner-kill-1"}),
            );
            let mut process = OwnedChild(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "mcp_process_kill::mcp_reply_receipt_fault_owner",
                        "--ignored",
                        "--nocapture",
                    ])
                    .env("VCP_MCP_KILL_ROOT", root)
                    .env("VCP_MCP_KILL_BACKEND", backend)
                    .env("VCP_MCP_KILL_TRANSPORT", transport)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .unwrap(),
            );
            while !root.join("barrier.json").exists() {
                if let Some(status) = process.0.try_wait().unwrap() {
                    panic!("MCP child exited before barrier: {status}");
                }
                assert!(
                    Instant::now() < deadline,
                    "MCP owner barrier exceeded 60 second deadline"
                );
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            let marker: serde_json::Value =
                serde_json::from_slice(&fs::read(root.join("barrier.json")).unwrap()).unwrap();
            assert_eq!(marker["phase"], "valid_reply_before_durable_receipt");
            assert_eq!(marker["result_receipts"], 0);
            let config: Config =
                serde_json::from_slice(&fs::read(root.join("config.json")).unwrap()).unwrap();
            let before: vcp_store::contract::State =
                serde_json::from_slice(&fs::read(root.join("before.json")).unwrap()).unwrap();
            let effect_id = ToolRunId::parse(marker["effect"].as_str().unwrap()).unwrap();
            let effect_marker = peer
                .as_ref()
                .map(|peer| peer.marker().to_path_buf())
                .unwrap_or_else(|| {
                    std::path::PathBuf::from(marker["workspace"].as_str().unwrap())
                        .join("writes.jsonl")
                });
            let written = fs::read(&effect_marker).unwrap();
            assert_eq!(
                String::from_utf8(written.clone()).unwrap().lines().count(),
                1
            );
            let requests = peer.as_ref().map(|peer| peer.observations().len());
            if let Some(peer) = &peer {
                assert_eq!(peer.effect_count(), 1);
            }
            let native = marker["child_pid"]
                .as_u64()
                .map(|pid| NativeProcess::open(pid as u32));
            record(
                &root.join("supervisor-ack.json"),
                &serde_json::json!({"barrier":marker,"observed_effect_sha256":vcp_protocol::digest_bytes(&written),"owner_pid":process.0.id(),"remote_request_count":requests}),
            );
            assert!(
                process.0.try_wait().unwrap().is_none(),
                "MCP owner exited before supervisor kill"
            );
            process.0.kill().unwrap();
            let stopped_by = Instant::now() + Duration::from_secs(10);
            loop {
                if let Some(status) = process.0.try_wait().unwrap() {
                    assert!(!status.success());
                    break;
                }
                assert!(
                    Instant::now() < stopped_by,
                    "killed MCP owner did not terminate"
                );
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            if let Some(native) = &native {
                while native.poll() == 258 {
                    assert!(
                        Instant::now() < stopped_by,
                        "owned MCP process survived owner loss"
                    );
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
                assert_eq!(native.poll(), 0);
            }
            assert!(
                !root.join("barrier-completed-before-kill.json").exists(),
                "MCP receipt barrier expired/completed before process termination"
            );
            for _ in 0..2 {
                let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
                host.reconcile_effects().unwrap();
                let state = host.snapshot().unwrap();
                let effect: Effect = state
                    .record(Collection::Effect, effect_id.as_str(), &config.workspace)
                    .unwrap()
                    .decode()
                    .unwrap();
                assert_eq!(effect.state, EffectState::OutcomeUnknown);
                assert_eq!(result_receipts(&host, &effect_id), 0);
                let previous: Effect = before
                    .record(Collection::Effect, effect_id.as_str(), &config.workspace)
                    .unwrap()
                    .decode()
                    .unwrap();
                assert_eq!(effect.scope, previous.scope);
                assert_eq!(effect.operation_digest, previous.operation_digest);
                for (key, row) in before.records.iter().filter(|(_, row)| {
                    matches!(row.collection, Collection::Ledger | Collection::Attempt)
                }) {
                    assert_eq!(state.records.get(key), Some(row));
                }
                for (key, receipt) in &before.commands {
                    assert_eq!(state.commands.get(key), Some(receipt));
                }
                for event in &before.events {
                    assert!(state.events.contains(event));
                }
                owner.close().await.unwrap();
                drop(host);
                assert_eq!(fs::read(&effect_marker).unwrap(), written);
                if let Some(peer) = &peer {
                    assert_eq!(peer.effect_count(), 1);
                    assert_eq!(Some(peer.observations().len()), requests);
                }
            }
            fs::write(root.join("external-effect.txt"), &written).unwrap();
            record(
                &root.join("outcome.json"),
                &serde_json::json!({
                    "backend":backend,"transport":transport,"real_owner_kill":true,"reopens":2,
                    "effect":"outcome_unknown","result_receipts":0,"external_effects":1,
                    "native_child_termination_verified":native.is_some(),"independent_https_peer_remained_live":peer.is_some(),
                    "replayed_requests":0,"fixture_root":marker["fixture_root"]
                }),
            );
        }
    }
}
