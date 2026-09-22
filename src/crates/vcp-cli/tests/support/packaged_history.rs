// SPDX-License-Identifier: Apache-2.0
//! CR-13: exact packaged CLI, actual output tail, fresh inspectors and retention.
use super::*;

async fn data(fixture: &Fixture, arguments: &[&str]) -> Value {
    let output = fixture.run(arguments).await;
    assert!(
        output.status.success(),
        "{arguments:?}: {} {}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    records(&output)
        .into_iter()
        .find(|row| row["type"] == "result")
        .unwrap()["data"]
        .clone()
}

async fn full_artifact(fixture: &Fixture, artifact: &str) -> (Vec<u8>, Value) {
    let mut collected = Vec::new();
    let mut descriptor = Value::Null;
    let mut offset = 0;
    for _ in 0..8 {
        let page = data(
            fixture,
            &[
                "inspect",
                artifact,
                "--view",
                "tools",
                "--offset",
                &offset.to_string(),
                "--length",
                "32768",
            ],
        )
        .await;
        assert_eq!(page["items"].as_array().unwrap().len(), 1, "{page}");
        let item = &page["items"][0];
        assert_eq!(item["artifact"], artifact);
        assert_eq!(item["range"]["start"], offset);
        assert_eq!(item["visibility"], "available");
        assert_eq!(item["descriptor"]["state"], "complete", "{page}");
        // Complete observed process bytes still disclose credentials/keys
        // excluded at capture. Those annotations are not missing byte ranges.
        let exclusions = json!(["authentication_headers", "recovery_material"]);
        assert_eq!(item["descriptor"]["spec"]["omissions"], exclusions);
        let gaps = page["gaps"].as_array().unwrap();
        assert_eq!(gaps.len(), 2, "unexpected retained-byte gap: {page}");
        assert_eq!(gaps[0]["visibility"], "omitted", "{page}");
        assert_eq!(gaps[0]["capture_state"], "complete", "{page}");
        assert_eq!(gaps[1]["visibility"], "redacted", "{page}");
        assert!(gaps[1]["range"].is_null(), "{page}");
        for gap in gaps {
            assert_eq!(gap["artifact"], artifact, "{page}");
            assert_eq!(gap["omissions"], exclusions, "{page}");
            assert!(!gap["reason"].as_str().unwrap().is_empty(), "{page}");
        }
        if descriptor.is_null() {
            descriptor = item["descriptor"].clone();
        }
        assert_eq!(
            item["descriptor"], descriptor,
            "reopen cannot rewrite attribution"
        );
        let bytes: Vec<u8> = serde_json::from_value(item["bytes"].clone()).unwrap();
        collected.extend(bytes);
        let Some(next) = item["next_offset"].as_u64() else {
            return (collected, descriptor);
        };
        assert!(next > offset);
        offset = next;
    }
    panic!("bounded full-output inspection did not finish")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the exact extracted package; run explicit P8 qualification"]
async fn packaged_oversized_output_survives_compaction_and_reopen_until_explicit_purge() {
    // This row cannot silently replace the final extracted package with Cargo's
    // checkout binary. Fixture::package also checks exact compiled/package bytes.
    std::env::var_os("VCP_TEST_SKILL_PACKAGE").expect("exact extracted package required for P8-03");
    for backend in ["sqlite", "files"] {
        let server = MockServer::start().await;
        let mut fixture = Fixture::new(&server.uri(), "complete");
        fixture.package(true);
        data(&fixture, &["storage", "configure", "--backend", backend]).await;
        let marker = fixture._temp.path().join("independent-tool-executions");
        let expected = format!("retained-prefix:{}:retained-suffix", "x".repeat(80_000));
        fs::write(fixture.workspace.join("emit.cjs"), format!(
            "const fs=require('node:fs');fs.appendFileSync({},'executed\\n');process.stdout.write({});\n",
            serde_json::to_string(&marker).unwrap(), serde_json::to_string(&expected).unwrap()
        )).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |_: &wiremock::Request| {
            let index = observed.fetch_add(1, Ordering::SeqCst);
            let body = if index == 0 {
                let item = json!({"type":"function_call","id":"emit-item","call_id":"emit-call","name":"vcp_exec",
                    "arguments":json!({"profile":"node","arguments":["emit.cjs"],"directory":"","timeout_ms":10000,"output_bytes":131072,"input":null}).to_string(),"status":"completed"});
                [json!({"type":"response.output_item.done","output_index":0,"item":item}), json!({"type":"response.completed","response":{"id":"emit-response","status":"completed","output":[item],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}})]
                    .into_iter().map(|event| format!("data: {event}\n\n")).collect()
            } else { response(index - 1, "complete") };
            ResponseTemplate::new(200).insert_header("content-type", "text/event-stream").set_body_string(body)
        }).mount(&server).await;
        let output = fixture
            .run(&[
                "run",
                "Observe the declared output, change value to 42 and verify",
                "--autonomy",
                "autonomous",
            ])
            .await;
        assert!(
            output.status.success(),
            "{} {}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
        let rows = records(&output);
        let task = rows.last().unwrap()["scope"]["task"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            data(&fixture, &["tasks", "status", &task]).await["records"][0]["state"],
            "completed"
        );
        let requests = server.received_requests().await.unwrap();
        let tool_output = requests
            .iter()
            .find_map(|request| {
                let body: Value = serde_json::from_slice(&request.body).unwrap();
                body["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|item| {
                        item["type"] == "function_call_output" && item["call_id"] == "emit-call"
                    })
                    .find_map(|item| {
                        item["output"]
                            .as_str()
                            .and_then(|text| serde_json::from_str::<Value>(text).ok())
                    })
            })
            .expect("actual second request must contain the native process result");
        let stdout = &tool_output["stdout"];
        assert_eq!(stdout["truncated"], true);
        assert_eq!(stdout["bytes"], expected.len().to_string());
        let tail = stdout["tail"].as_str().unwrap();
        assert!(tail.len() < expected.len());
        assert!(!tail.contains("retained-prefix"));
        assert!(tail.ends_with("retained-suffix"));
        let artifact = stdout["artifact"].as_str().unwrap();
        let requests_before = requests.len();
        assert_eq!(fs::read(&marker).unwrap(), b"executed\n");
        let (bytes, descriptor) = full_artifact(&fixture, artifact).await;
        assert_eq!(bytes, expected.as_bytes());
        assert_eq!(
            descriptor["sha256"],
            vcp_protocol::digest_bytes(expected.as_bytes())
        );
        assert_eq!(descriptor["spec"]["scope"]["task"], task);
        let linked = data(
            &fixture,
            &["history", "list", "--artifact", artifact, "--limit", "128"],
        )
        .await;
        let linked_rows = linked["rows"].as_array().unwrap();
        assert!(
            !linked_rows.is_empty(),
            "artifact must have navigable event links"
        );
        let linked_ids: Vec<_> = linked_rows
            .iter()
            .map(|row| {
                let event = &row["event"]["event"];
                assert_eq!(event["workspace"], descriptor["spec"]["scope"]["workspace"]);
                assert_eq!(event["task"], task);
                for references in [&row["artifacts"], &event["artifacts"]] {
                    assert!(references
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|id| id == artifact));
                }
                let link = row["artifact_links"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|link| link["id"] == artifact)
                    .expect("retained artifact link");
                assert_eq!(link["availability"], "retained_with_omissions");
                assert_eq!(link["original_bytes"], descriptor["length"]);
                event["id"].as_str().unwrap().to_owned()
            })
            .collect();
        let preview = data(
            &fixture,
            &[
                "history",
                "prune",
                "--preview",
                "--task",
                &task,
                "--action",
                "compact",
            ],
        )
        .await;
        assert!(preview["selected_count"].as_u64().unwrap() > 0);
        data(
            &fixture,
            &["prune", "apply", preview["id"].as_str().unwrap()],
        )
        .await;
        let compacted = data(
            &fixture,
            &["history", "list", "--artifact", artifact, "--limit", "128"],
        )
        .await;
        assert!(compacted["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["compacted"] == true));
        for id in &linked_ids {
            assert!(
                compacted["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| row["event"]["event"]["id"] == *id),
                "compaction must preserve the original event's artifact link"
            );
        }
        let (after, attribution) = full_artifact(&fixture, artifact).await;
        assert_eq!(after, bytes);
        assert_eq!(attribution, descriptor);
        let purge = data(
            &fixture,
            &[
                "history",
                "prune",
                "--preview",
                "--task",
                &task,
                "--action",
                "purge",
            ],
        )
        .await;
        assert_eq!(purge["protected_count"], 0);
        assert!(purge["selected_count"].as_u64().unwrap() > 0);
        data(&fixture, &["prune", "apply", purge["id"].as_str().unwrap()]).await;
        let removed = data(
            &fixture,
            &[
                "inspect", artifact, "--view", "tools", "--offset", "0", "--length", "32768",
            ],
        )
        .await;
        assert!(removed["items"].as_array().unwrap().is_empty());
        assert!(removed["gaps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|gap| gap["visibility"] == "pruned"
                && gap["source"] == descriptor["spec"]["source"]));
        assert!(!removed.to_string().contains("retained-prefix"));
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            requests_before
        );
        assert_eq!(
            fs::read(&marker).unwrap(),
            b"executed\n",
            "inspection and retention cannot execute the tool again"
        );
    }
}
