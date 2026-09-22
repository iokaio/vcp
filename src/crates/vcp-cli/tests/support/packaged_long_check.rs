// SPDX-License-Identifier: Apache-2.0
//! P8-01/02: relocated package, declared long checks and live owner pause.
use super::*;
use std::{process::Stdio, time::Duration};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "exact extracted package and native Node; real >120-second check"]
async fn packaged_declared_long_check_completes_and_reopens_on_both_stores() {
    tokio::join!(long_check("files", false), long_check("sqlite", false));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "exact extracted package and native Node; pause a declared long check"]
async fn packaged_declared_long_check_pause_stops_execution_on_both_stores() {
    tokio::join!(long_check("files", true), long_check("sqlite", true));
}

// Fixture::run intentionally has a 90-second ceiling. This owned child instead
// has an explicit bounded wait and is killed/reaped if an assertion unwinds.
struct OwnedRun(std::process::Child);
impl Drop for OwnedRun {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
impl OwnedRun {
    async fn wait(&mut self, seconds: u64) -> std::process::ExitStatus {
        tokio::time::timeout(Duration::from_secs(seconds), async {
            loop {
                if let Some(status) = self.0.try_wait().unwrap() {
                    break status;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("packaged long-check CLI deadline")
    }
}

// Hold the actual check process handle before pause, so a later PID reuse
// cannot make process-exit evidence ambiguous.
struct ObservedProcess(HANDLE);
impl Drop for ObservedProcess {
    fn drop(&mut self) {
        // SAFETY: this handle was returned by OpenProcess and is owned here.
        unsafe { CloseHandle(self.0) };
    }
}
impl ObservedProcess {
    fn open(pid: u32) -> Self {
        // SAFETY: query/synchronize only; no borrowed memory or process mutation.
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        assert!(!handle.is_null(), "open active native check process");
        Self(handle)
    }
    fn state(&self) -> u32 {
        // SAFETY: the owned process handle remains valid until Drop.
        unsafe { WaitForSingleObject(self.0, 0) }
    }
}

async fn data(fixture: &Fixture, args: &[&str]) -> Value {
    let output = fixture.run(args).await;
    assert!(
        output.status.success(),
        "{args:?}: {} {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    records(&output)
        .into_iter()
        .find(|row| row["type"] == "result")
        .unwrap()["data"]
        .clone()
}

async fn chain(fixture: &Fixture, task: &str) -> Vec<Value> {
    let mut items = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..16 {
        let mut args = vec!["inspect", task, "--view", "chain", "--limit", "128"];
        if let Some(cursor) = &cursor {
            args.extend(["--cursor", cursor]);
        }
        let page = data(fixture, &args).await;
        assert!(page["gaps"].as_array().unwrap().is_empty());
        items.extend(page["items"].as_array().unwrap().iter().cloned());
        if page["next_cursor"].is_null() {
            return items;
        }
        cursor = Some(page["next_cursor"].to_string());
    }
    panic!("long-check inspection exceeded bounded pagination")
}

async fn long_check(backend: &str, pause: bool) {
    std::env::var_os("VCP_TEST_SKILL_PACKAGE")
        .expect("P8 long checks require the exact extracted package");
    let server = MockServer::start().await;
    let mut fixture = Fixture::new(&server.uri(), "complete");
    fixture.package(true); // Enforces compiled/archive identity and relocation.
    data(&fixture, &["storage", "configure", "--backend", backend]).await;
    let started = fixture._temp.path().join("check-started.json");
    let finished = fixture._temp.path().join("check-finished.json");
    let executions = fixture._temp.path().join("check-executions.txt");
    // Independent markers live outside the workspace; observing test progress
    // must not change the repository fingerprint being verified. Embed their
    // JSON path literals instead of extending the public environment allowlist.
    let source = format!(
        "const marker={},finished={},executions={};\n{}",
        serde_json::to_string(&started).unwrap(),
        serde_json::to_string(&finished).unwrap(),
        serde_json::to_string(&executions).unwrap(),
        r#"const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs');
test('changed_value',async()=>{
  assert.equal(fs.readFileSync('value.txt','utf8').trim(),'42');
  const start=Date.now();
  fs.appendFileSync(executions,'executed\n');
  fs.writeFileSync(marker+'.tmp',JSON.stringify({pid:process.pid,start}));
  fs.renameSync(marker+'.tmp',marker);
  await new Promise(resolve=>setTimeout(resolve,121000));
  const elapsed=Date.now()-start;
  assert.ok(elapsed>=120000);
  fs.writeFileSync(finished,JSON.stringify({elapsed}));
});
"#
    );
    fs::write(fixture.workspace.join("acceptance.cjs"), &source).unwrap();
    let mut profile: Value = serde_json::from_slice(&fs::read(&fixture.profile).unwrap()).unwrap();
    profile["processes"][0]["max_timeout_ms"] = json!(180_000);
    profile["checks"][0]["timeout_ms"] = json!(150_000);
    fs::write(&fixture.profile, serde_json::to_vec(&profile).unwrap()).unwrap();

    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let completed = finished.clone();
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(move |_: &wiremock::Request| {
            let index = observed.fetch_add(1, Ordering::SeqCst);
            // Verification may first refresh instruction scope. Retry only
            // until the actual native check finishes; never run it twice.
            let body = if index == 0 || completed.exists() {
                response(index, "complete")
            } else {
                response(1, "complete")
                    .replace("item-1", &format!("item-{index}"))
                    .replace("call-1", &format!("call-{index}"))
                    .replace("response-1", &format!("response-{index}"))
            };
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(body)
        })
        .mount(&server)
        .await;
    let stdout = fixture._temp.path().join("owner.stdout");
    let stderr = fixture._temp.path().join("owner.stderr");
    let mut owner = OwnedRun(
        fixture
            .command(&[
                "run",
                "Change value to 42 and run the declared long acceptance check",
                "--autonomy",
                "autonomous",
            ])
            .stdout(Stdio::from(fs::File::create(&stdout).unwrap()))
            .stderr(Stdio::from(fs::File::create(&stderr).unwrap()))
            .spawn()
            .unwrap(),
    );
    tokio::time::timeout(Duration::from_secs(45), async {
        while !started.exists() {
            assert!(
                owner.0.try_wait().unwrap().is_none(),
                "owner exited before native execution: {} {}",
                fs::read_to_string(&stdout).unwrap(),
                fs::read_to_string(&stderr).unwrap()
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("observe actual long check startup");
    let marker: Value = serde_json::from_slice(&fs::read(&started).unwrap()).unwrap();
    let process = ObservedProcess::open(marker["pid"].as_u64().unwrap().try_into().unwrap());
    assert_eq!(process.state(), WAIT_TIMEOUT, "check is still executing");
    let task = fixture.task_id();
    let before_pause = calls.load(Ordering::SeqCst);
    if pause {
        data(&fixture, &["tasks", "pause", &task]).await;
    }
    let output = Output {
        status: owner.wait(if pause { 30 } else { 180 }).await,
        stdout: fs::read(&stdout).unwrap(),
        stderr: fs::read(&stderr).unwrap(),
    };
    let rows = records(&output);
    let conditions = &rows.last().unwrap()["conditions"];
    assert_eq!(conditions["completed"], !pause, "{backend}: {rows:?}");
    assert_eq!(conditions["durably_paused"], pause, "{backend}: {rows:?}");
    assert_eq!(conditions["internal_failure"], false, "{backend}: {rows:?}");
    assert_eq!(
        conditions["invalid_configuration"], false,
        "{backend}: {rows:?}"
    );
    assert_eq!(output.status.success(), !pause, "{backend}: {rows:?}");
    if pause {
        assert!(
            matches!(output.status.code(), Some(7 | 8)),
            "{backend}: {rows:?}"
        );
    }
    assert_eq!(
        process.state(),
        WAIT_OBJECT_0,
        "native check must have exited"
    );
    if pause {
        assert!(
            !finished.exists(),
            "interrupted check cannot report success"
        );
        assert_eq!(calls.load(Ordering::SeqCst), before_pause);
    } else {
        let completed: Value = serde_json::from_slice(&fs::read(&finished).unwrap()).unwrap();
        assert!(completed["elapsed"].as_u64().unwrap() >= 120_000);
    }
    assert_eq!(fs::read(&executions).unwrap(), b"executed\n");
    assert_eq!(
        fs::read_to_string(fixture.workspace.join("value.txt")).unwrap(),
        "42\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.workspace.join("acceptance.cjs")).unwrap(),
        source
    );

    // Every read below starts the relocated executable again, preserving the
    // result through a fresh owner rather than reading its in-memory state.
    let status = data(&fixture, &["tasks", "status", &task]).await;
    assert_eq!(
        status["records"][0]["state"],
        if pause { "paused" } else { "completed" }
    );
    let snapshot = chain(&fixture, &task).await;
    let checks: Vec<_> = snapshot
        .iter()
        .filter(|row| row["collection"] == "verification")
        .flat_map(|row| row["record"]["checks"].as_array().unwrap())
        .filter(|check| check["specification"] == "package.json#test")
        .collect();
    assert_eq!(
        checks
            .iter()
            .any(|check| check["outcome"]["status"] == "passed"),
        !pause
    );
    if !pause {
        let check = checks
            .iter()
            .find(|check| check["outcome"]["status"] == "passed")
            .unwrap();
        assert_eq!(check["exit_code"], 0);
        let artifact = data(
            &fixture,
            &[
                "inspect",
                check["output"].as_str().unwrap(),
                "--view",
                "verification",
                "--offset",
                "0",
                "--length",
                "65536",
            ],
        )
        .await;
        let item = &artifact["items"][0];
        let bytes: Vec<u8> = serde_json::from_value(item["bytes"].clone()).unwrap();
        assert!(item["next_offset"].is_null());
        assert_eq!(
            item["descriptor"]["sha256"],
            vcp_protocol::digest_bytes(&bytes)
        );
        assert_eq!(item["descriptor"]["spec"]["scope"]["task"], task);
        let receipt: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(receipt["outcome"]["status"], "passed");
        assert_eq!(receipt["plan"]["request"]["timeout_ms"], 150_000);
    }
    let attempts: Vec<vcp_domain::accounting::Attempt> = snapshot
        .iter()
        .filter(|row| row["collection"] == "attempt")
        .map(|row| serde_json::from_value(row["record"].clone()).unwrap())
        .collect();
    let count = calls.load(Ordering::SeqCst);
    assert!((2..=8).contains(&count));
    assert_eq!(attempts.len(), count);
    for attempt in &attempts {
        assert_eq!(
            attempt.phase,
            vcp_domain::accounting::ReservationState::Settled
        );
        assert_eq!(attempt.charged.get(), 100);
        assert!(attempt.uncertain.is_none());
    }
    let retained = |items: Vec<Value>| {
        items
            .into_iter()
            .filter(|row| {
                matches!(
                    row["collection"].as_str(),
                    Some("attempt" | "effect" | "verification")
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        retained(chain(&fixture, &task).await),
        retained(snapshot),
        "reopen cannot rewrite evidence"
    );
    assert_eq!(server.received_requests().await.unwrap().len(), count);
    assert_eq!(
        fs::read(&executions).unwrap(),
        b"executed\n",
        "inspection cannot replay execution"
    );
}
