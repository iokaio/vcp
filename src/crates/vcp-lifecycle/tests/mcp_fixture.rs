// SPDX-License-Identifier: Apache-2.0
//! Independent controlled-peer observations; no VCP model or network invocation.
use serde_json::{json, Value};
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};

struct Peer {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    directory: tempfile::TempDir,
}
impl Peer {
    fn new(scenario: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_vcp-mcp-fixture"))
            .args([directory.path().as_os_str(), std::ffi::OsStr::new(scenario)])
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        Self {
            input: child.stdin.take().unwrap(),
            output: BufReader::new(child.stdout.take().unwrap()),
            child,
            directory,
        }
    }
    async fn send(&mut self, value: Value) {
        let mut bytes = serde_json::to_vec(&value).unwrap();
        bytes.push(b'\n');
        tokio::time::timeout(Duration::from_secs(3), self.input.write_all(&bytes))
            .await
            .unwrap()
            .unwrap();
        self.input.flush().await.unwrap();
    }
    async fn next(&mut self) -> Option<Value> {
        let mut bytes = Vec::new();
        let count = tokio::time::timeout(
            Duration::from_secs(3),
            self.output.read_until(b'\n', &mut bytes),
        )
        .await
        .unwrap()
        .unwrap();
        if count == 0 {
            return None;
        }
        assert!(count <= 64 * 1024);
        Some(serde_json::from_slice(&bytes).unwrap())
    }
    async fn initialize(&mut self) {
        self.send(json!({"jsonrpc":"2.0","id":"init","method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"fixture-observer","version":"1"}}})).await;
        assert_eq!(
            self.next().await.unwrap()["result"]["protocolVersion"],
            "2025-11-25"
        );
        self.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .await;
    }
    async fn stop(mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}

#[tokio::test]
async fn native_peer_exposes_exact_handshake_pages_and_independent_write_evidence() {
    let mut peer = Peer::new("pagination");
    peer.initialize().await;
    peer.send(json!({"jsonrpc":"2.0","id":"list1","method":"tools/list","params":{}}))
        .await;
    let first = peer.next().await.unwrap();
    assert_eq!(first["result"]["tools"].as_array().unwrap().len(), 1);
    peer.send(json!({"jsonrpc":"2.0","id":"list2","method":"tools/list","params":{"cursor":first["result"]["nextCursor"]}})).await;
    let second = peer.next().await.unwrap();
    assert_eq!(second["result"]["tools"].as_array().unwrap().len(), 2);
    assert_eq!(
        second["result"]["tools"][1]["annotations"]["readOnlyHint"],
        true
    );
    peer.send(json!({"jsonrpc":"2.0","id":"write1","method":"tools/call","params":{"name":"write_marker","arguments":{"value":"observed marker"}}})).await;
    assert_eq!(peer.next().await.unwrap()["result"]["isError"], false);
    assert_eq!(
        std::fs::read_to_string(peer.directory.path().join("value.txt")).unwrap(),
        "observed marker"
    );
    let writes = std::fs::read_to_string(peer.directory.path().join("writes.jsonl")).unwrap();
    assert_eq!(writes.lines().count(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(&writes).unwrap()["request_id"],
        "write1"
    );
    peer.send(json!({"jsonrpc":"2.0","id":"read1","method":"tools/call","params":{"name":"read_marker","arguments":{}}})).await;
    assert_eq!(
        peer.next().await.unwrap()["result"]["content"][0]["text"],
        "observed marker"
    );
    peer.stop().await;
}

#[tokio::test]
async fn callbacks_require_explicit_rejection_and_disconnect_does_not_erase_effects() {
    let mut peer = Peer::new("callback");
    peer.initialize().await;
    peer.send(json!({"jsonrpc":"2.0","id":"echo1","method":"tools/call","params":{"name":"echo","arguments":{"text":"public fixture"}}})).await;
    assert_eq!(
        peer.next().await.unwrap()["method"],
        "sampling/createMessage"
    );
    peer.send(json!({"jsonrpc":"2.0","id":"fixture-callback","error":{"code":-32601,"message":"not negotiated"}})).await;
    assert_eq!(
        peer.next().await.unwrap()["result"]["content"][0]["text"],
        "public fixture"
    );
    assert!(peer.directory.path().join("callback-handled").is_file());
    peer.stop().await;

    let mut peer = Peer::new("write-then-exit");
    peer.initialize().await;
    peer.send(json!({"jsonrpc":"2.0","id":"lost1","method":"tools/call","params":{"name":"write_marker","arguments":{"value":"effect without response"}}})).await;
    assert!(peer.next().await.is_none());
    assert_eq!(
        std::fs::read_to_string(peer.directory.path().join("value.txt")).unwrap(),
        "effect without response"
    );
    assert_eq!(
        std::fs::read_to_string(peer.directory.path().join("writes.jsonl"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    peer.stop().await;
}
