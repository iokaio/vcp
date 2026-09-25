// SPDX-License-Identifier: Apache-2.0
//! Opt-in local observers remain inspectable behind the ordinary owner pause.
use super::*;
use std::{sync::Mutex, time::Duration};

async fn accounting(fixture: &Fixture, task: &str) -> std::collections::BTreeMap<String, Value> {
    let output = fixture
        .run(&["inspect", task, "--view", "costs", "--limit", "128"])
        .await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    records(&output)[0]["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| {
            matches!(
                row["collection"].as_str(),
                Some("ledger" | "attempt" | "reservation")
            )
        })
        .map(|row| {
            (
                format!("{}:{}", row["collection"], row["id"]),
                row["record"].clone(),
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_observers_are_default_off_and_status_is_readable_while_paused() {
    for enabled in [false, true] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(response(0, "complete"))
                    .set_delay(Duration::from_secs(30)),
            )
            .mount(&server)
            .await;
        let fixture = Fixture::new(&server.uri(), "complete");
        if enabled {
            let mut profile: Value =
                serde_json::from_slice(&fs::read(&fixture.profile).unwrap()).unwrap();
            profile["observers"] = json!({"enabled":true,"limits":{"max_attempts":4,"steps_per_attempt":1024,"max_total_steps":4096,"deadline_ms":1000,"debounce_ms":100}});
            fs::write(&fixture.profile, serde_json::to_vec(&profile).unwrap()).unwrap();
        }
        let mut child = fixture
            .terminal("Inspect local observer status, then await owner guidance")
            .await;
        // Keep a complete bounded status page on one console line. ConPTY
        // otherwise emits cursor repositioning and repeated edge characters
        // across a wrapped JSON token, which is not a raw text substring.
        child
            .session
            .resize(codex_utils_pty::TerminalSize {
                rows: 30,
                cols: 4096,
            })
            .unwrap();
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
            tokio::time::timeout(Duration::from_secs(30), async {
                while server.received_requests().await.unwrap().is_empty() {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            })
            .await
            .unwrap();
            assert_eq!(
                server.received_requests().await.unwrap().len(),
                1,
                "opt-in local observers must not create an extra provider request"
            );
            writer.send(b"/pause\r".to_vec()).await.unwrap();
            let task = fixture.task_id();
            tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    let state = records(&fixture.run(&["tasks", "status", &task]).await);
                    if state[0]["data"]["records"][0]["state"] == "paused" {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            })
            .await
            .unwrap();
            let request_count = server.received_requests().await.unwrap().len();
            assert_eq!(request_count, 1);
            let before = tokio::time::timeout(Duration::from_secs(15), async {
                loop {
                    let snapshot = accounting(&fixture, &task).await;
                    if snapshot.iter().any(|(key, row)| {
                        key.contains("ledger")
                            && row["unresolved"].as_str().is_some_and(|v| v != "0")
                    }) {
                        break snapshot;
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            })
            .await
            .expect("paused sent request must retain its uncertain liability");
            let offset = captured.lock().unwrap().len();
            writer.send(b"/observers\r".to_vec()).await.unwrap();
            let expected = format!("\"enabled\":{enabled}");
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    if String::from_utf8_lossy(&captured.lock().unwrap()[offset..])
                        .contains(&expected)
                    {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            })
            .await
            .unwrap_or_else(|_| {
                let observed = String::from_utf8_lossy(&captured.lock().unwrap()).into_owned();
                panic!("paused observer status unavailable: {}", observed)
            });
            assert_eq!(
                server.received_requests().await.unwrap().len(),
                request_count,
                "status cannot dispatch inference or implicitly resume"
            );
            assert_eq!(accounting(&fixture, &task).await, before,
                "observer status must preserve the prior uncertain request, reservation and root ledger exactly");
            writer.send(b"/exit\r".to_vec()).await.unwrap();
            let code = (&mut child.exit_rx).await.unwrap();
            assert!(
                matches!(code, 7 | 8),
                "paused or uncertain prior request expected: {code}"
            );
        };
        if tokio::time::timeout(Duration::from_secs(70), exercise)
            .await
            .is_err()
        {
            child.session.terminate();
            panic!("observer terminal control timeout");
        }
        reader.await.unwrap();
    }
}
