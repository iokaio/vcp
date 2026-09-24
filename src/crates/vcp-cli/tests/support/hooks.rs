// SPDX-License-Identifier: Apache-2.0
//! Pending native hooks must not monopolize the terminal's control loop.
use super::*;
use std::{sync::Mutex, time::Duration};

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
    fn WaitForSingleObject(handle: isize, millis: u32) -> u32;
    fn CloseHandle(handle: isize) -> i32;
}
struct NativeProcess(isize);
impl NativeProcess {
    fn open(pid: u32) -> Self {
        // SAFETY: SYNCHRONIZE only; retaining this handle prevents PID reuse ambiguity.
        let handle = unsafe { OpenProcess(0x0010_0000, 0, pid) };
        assert_ne!(handle, 0);
        Self(handle)
    }
    fn stopped(&self) -> bool {
        // SAFETY: this instance retains a valid process handle.
        unsafe { WaitForSingleObject(self.0, 0) == 0 }
    }
}
impl Drop for NativeProcess {
    fn drop(&mut self) {
        // SAFETY: this instance exclusively owns the handle.
        unsafe {
            CloseHandle(self.0);
        }
    }
}
async fn output_contains(captured: &Arc<Mutex<Vec<u8>>>, from: usize, needle: &str) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if String::from_utf8_lossy(&captured.lock().unwrap()[from..]).contains(needle) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "terminal did not serve {needle} while hook pending: {}",
            String::from_utf8_lossy(&captured.lock().unwrap())
        )
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_terminal_status_and_pause_remain_responsive_during_lifecycle_hooks() {
    for event in ["session_start", "task_completion"] {
        let server = MockServer::start().await;
        let requests = Arc::new(AtomicUsize::new(0));
        let count = requests.clone();
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .respond_with(move |_: &wiremock::Request| {
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(response(count.fetch_add(1, Ordering::SeqCst), "complete"))
            })
            .mount(&server)
            .await;
        let fixture = Fixture::new(&server.uri(), "complete");
        let script=br#"const fs=require('node:fs');const readline=require('node:readline');
readline.createInterface({input:process.stdin}).once('line',line=>{
 const plan=JSON.parse(line);if(!plan.input||!plan.definition)process.exit(2);
 fs.writeFileSync('hook-started.json',JSON.stringify({pid:process.pid,event:plan.input.event}));
 setInterval(()=>fs.appendFileSync('hook-heartbeat.txt','tick\n'),100);
 setTimeout(()=>{console.log(JSON.stringify({schema_version:1,findings:[],context:null,rewrite:null,block:false}));process.exit(0)},55000);
});"#;
        fs::write(fixture.workspace.join("hook.cjs"), script).unwrap();
        let mut profile: Value =
            serde_json::from_slice(&fs::read(&fixture.profile).unwrap()).unwrap();
        profile["processes"][0]["inputs"] = json!(["hook.cjs"]);
        profile["hooks"] = json!([{"id":"slow-terminal-hook","version":1,"source_hash":vcp_protocol::digest_bytes(script),"event":event,"priority":0,"before":[],"after":[],"command":{"profile":"node","arguments":["hook.cjs"],"working_directory":""},"effect_scope":"broker_profile","timeout_ms":60000,"max_output_bytes":65536,"failure_policy":"block"}]);
        fs::write(&fixture.profile, serde_json::to_vec(&profile).unwrap()).unwrap();
        let mut child = fixture
            .terminal("Change value to 42 and verify the result")
            .await;
        let writer = child.session.writer_sender();
        let captured = Arc::new(Mutex::new(Vec::new()));
        let output = captured.clone();
        let reader = tokio::spawn(async move {
            while let Some(bytes) = child.stdout_rx.recv().await {
                let mut output = output.lock().unwrap();
                assert!(output.len() + bytes.len() <= 2 * 1024 * 1024);
                output.extend(bytes);
            }
        });
        let exercise = async {
            tokio::time::timeout(Duration::from_secs(35), async {
                while !fixture.workspace.join("hook-started.json").exists() {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            })
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "{event}: hook did not start: {}",
                    String::from_utf8_lossy(&captured.lock().unwrap())
                )
            });
            let started: Value = serde_json::from_slice(
                &fs::read(fixture.workspace.join("hook-started.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(started["event"], event);
            let native = NativeProcess::open(started["pid"].as_u64().unwrap() as u32);
            assert!(!native.stopped());
            let request_count = requests.load(Ordering::SeqCst);
            if event == "session_start" {
                assert_eq!(request_count, 0, "start hook must gate model transport");
            }
            let offset = captured.lock().unwrap().len();
            writer.send(b"/status\r".to_vec()).await.unwrap();
            output_contains(&captured, offset, "\"cost\"").await;
            assert!(
                !native.stopped(),
                "status must be served during execution, not after hook timeout"
            );
            let offset = captured.lock().unwrap().len();
            writer.send(b"/pause\r".to_vec()).await.unwrap();
            output_contains(&captured, offset, "Pause requested").await;
            tokio::time::timeout(Duration::from_secs(10), async {
                while !native.stopped() {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            })
            .await
            .expect("pause must stop native hook promptly");
            assert_eq!(
                requests.load(Ordering::SeqCst),
                request_count,
                "pause must not release gated model work"
            );
            let task = fixture.task_id();
            let status = records(&fixture.run(&["tasks", "status", &task]).await);
            assert_eq!(status[0]["data"]["records"][0]["state"], "paused");
            writer.send(b"/exit\r".to_vec()).await.unwrap();
            let code = (&mut child.exit_rx).await.unwrap();
            assert!(
                matches!(code, 8 | 7),
                "paused or unresolved-effects exit expected: {code}"
            );
        };
        if tokio::time::timeout(Duration::from_secs(75), exercise)
            .await
            .is_err()
        {
            child.session.terminate();
            panic!("{event}: terminal pending hook controls timed out");
        }
        reader.await.unwrap();
    }
}
