// SPDX-License-Identifier: Apache-2.0
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn package_creation_requires_prepared_parents_and_completes_with_native_checker() {
    let body = "# Change notes\nSeparate completed and proposed changes; verify local source links.\nSee [checklist](references/checklist.md).\n";
    let resource = "# Checklist\nCheck completion evidence and local source links.\n";
    let descriptor = serde_json::to_string(&json!({
        "schema_version":1,"id":"change-notes","version":"1.0.0","description":"Review local change notes",
        "source":"vcp-original","license":"Apache-2.0","vcp_version":1,"cues":[],"environments":[],
        "required_tools":["vcp_list","vcp_read"],
        "body":{"path":"SKILL.md","sha256":vcp_protocol::digest_bytes(body.as_bytes())},
        "resources":[{"path":"references/checklist.md","sha256":vcp_protocol::digest_bytes(resource.as_bytes())}]
    })).unwrap() + "\n";
    let outputs = [
        ("package/SKILL.md", body),
        ("package/references/checklist.md", resource),
        ("package/skill.json", descriptor.as_str()),
    ];
    let mut patch = String::from("*** Begin Patch\n");
    for (name, content) in outputs {
        patch.push_str(&format!("*** Add File: {name}\n"));
        for line in content.lines() {
            patch.push_str(&format!("+{line}\n"));
        }
    }
    patch.push_str("*** End Patch");
    for parents_present in [false, true] {
        let server = MockServer::start().await;
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let requested_patch = patch.clone();
        Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |request: &wiremock::Request| {
            let index = observed.fetch_add(1, Ordering::SeqCst);
            let result = if index == 0 {
                let item = json!({"type":"function_call","id":"item-0","call_id":"call-0","name":"vcp_patch","arguments":json!({"patch":requested_patch}).to_string(),"status":"completed"});
                [json!({"type":"response.output_item.done","output_index":0,"item":item}),json!({"type":"response.completed","response":{"id":"response-0","status":"completed","output":[item],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}})].into_iter().map(|event|format!("data: {event}\n\n")).collect()
            } else if index == 2 && {
                let incoming: Value = serde_json::from_slice(&request.body).unwrap();
                incoming["input"].as_array().unwrap().iter().any(|item| {
                    item["type"] == "function_call_output" && item["call_id"] == "call-1"
                        && item["output"].as_str().and_then(|text| serde_json::from_str::<Value>(text).ok())
                            == Some(json!({"executed":false,"reason":"New instruction scope selected. Review the refreshed context before issuing this operation again."}))
                })
            } {
                // Only an unexecuted instruction-scope refresh permits this
                // retry. A failed or completed check cannot trigger another.
                response(1,"complete").replace("item-1","item-2").replace("call-1","call-2").replace("response-1","response-2")
            } else { response(index,"complete") };
            ResponseTemplate::new(200).insert_header("content-type","text/event-stream").set_body_string(result)
        }).mount(&server).await;
        let fixture = Fixture::new(&server.uri(), "complete");
        for name in ["value.txt", "acceptance.cjs"] {
            fs::remove_file(fixture.workspace.join(name)).unwrap();
        }
        let original = include_bytes!(
            "../../../../evals/skills/authoring/projects/SKL-normal-package-v1/contract.md"
        );
        fs::write(fixture.workspace.join("contract.md"), original).unwrap();
        fs::create_dir(fixture.workspace.join("checks")).unwrap();
        let manifest = b"{\"name\":\"vcp-authoring-check\",\"private\":true,\"scripts\":{\"test\":\"node --test checks/authoring.test.cjs\"}}\n";
        let sentinel = b"// Inert VCP authoring verifier marker; never executed as JavaScript.\n";
        fs::write(fixture.workspace.join("package.json"), manifest).unwrap();
        fs::write(
            fixture.workspace.join("checks/authoring.test.cjs"),
            sentinel,
        )
        .unwrap();
        fs::write(
            fixture.workspace.join("checks/authoring.case.json"),
            b"{\"schema_version\":1,\"case_id\":\"SKL-normal-package-v1\"}\n",
        )
        .unwrap();
        if parents_present {
            fs::create_dir_all(fixture.workspace.join("package/references")).unwrap();
        }
        let runtime = fixture._temp.path().join("checker-runtime");
        fs::create_dir(&runtime).unwrap();
        let checker = runtime.join("vcp-authoring-check.exe");
        fs::copy(env!("CARGO_BIN_EXE_vcp-authoring-check"), &checker).unwrap();
        fs::write(runtime.join("authoring-cases.json"), serde_json::to_vec(&json!({"schema_version":1,"cases":[{"workspace":fixture.workspace,"case_id":"SKL-normal-package-v1"}]})).unwrap()).unwrap();
        let mut profile: Value =
            serde_json::from_slice(&fs::read(&fixture.profile).unwrap()).unwrap();
        profile["affected_paths"] = json!(outputs.map(|(name, _)| name));
        profile["max_transport_retries"] = json!(0);
        profile["processes"] = json!([{"name":"authoring-check","executable":checker,"environment":{"SystemRoot":std::env::var("SystemRoot").unwrap()},"required_isolation":[],"reduced_isolation":true,"inputs":[]}]);
        profile["checks"] = json!([{"manifest":"package.json","runner":"node","profile":"authoring-check","timeout_ms":10000,"expected_tests":["authoring input preservation","authoring output structure"],"rationale":"Fixed read-only native authoring validation"}]);
        fs::write(&fixture.profile, serde_json::to_vec(&profile).unwrap()).unwrap();
        let output = fixture
            .run(&[
                "run",
                "Create the requested package only at the three declared output paths and verify.",
                "--autonomy",
                "autonomous",
            ])
            .await;
        let values = records(&output);
        let tool_results: Vec<Value> = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .flat_map(|request| {
                let body: Value = serde_json::from_slice(&request.body).unwrap();
                body["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|item| item["type"] == "function_call_output")
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(
            output.status.success(),
            parents_present,
            "parents={parents_present} final={:?} tools={tool_results:?} {}",
            values.last(),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            values.last().unwrap()["conditions"]["completed"],
            parents_present
        );
        let distinct: std::collections::BTreeMap<String, Value> = tool_results
            .iter()
            .map(|item| {
                (
                    item["call_id"].as_str().unwrap().to_owned(),
                    serde_json::from_str(item["output"].as_str().unwrap()).unwrap(),
                )
            })
            .collect();
        assert_eq!(
            distinct["call-1"],
            json!({"executed":false,"reason":"New instruction scope selected. Review the refreshed context before issuing this operation again."})
        );
        let executed: Vec<_> = distinct
            .values()
            .filter(|result| result.get("verification").is_some())
            .collect();
        assert_eq!(
            executed.len(),
            1,
            "Only one executed verification is permitted"
        );
        let verification = executed[0];
        assert_eq!(
            verification["diagnostics"].as_array().unwrap().len(),
            1,
            "Only one checker process is permitted"
        );
        assert_eq!(
            verification["verification"]["checks"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            verification["verification"]["checks"][0]["specification"],
            "package.json#test"
        );
        if parents_present {
            assert_eq!(
                verification["verification"]["checks"][0]["outcome"]["status"],
                "passed"
            );
            assert_eq!(verification["diagnostics"][0]["exit_code"], 0);
            let output = verification["diagnostics"][0]["stdout"]["tail"]
                .as_str()
                .unwrap();
            assert!(output.contains("ok 1 - authoring input preservation"));
            assert!(output.contains("ok 2 - authoring output structure"));
        } else {
            assert_eq!(
                verification["verification"]["checks"][0]["outcome"]["status"],
                "failed"
            );
        }
        for (name, bytes) in outputs {
            if parents_present {
                assert_eq!(
                    fs::read_to_string(fixture.workspace.join(name)).unwrap(),
                    bytes
                );
            } else {
                assert!(!fixture.workspace.join(name).exists());
            }
        }
        if !parents_present {
            assert!(server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .any(|request| String::from_utf8_lossy(&request.body).contains("os error 2")));
        }
        assert_eq!(
            fs::read(fixture.workspace.join("contract.md")).unwrap(),
            original
        );
        assert_eq!(
            fs::read(fixture.workspace.join("package.json")).unwrap(),
            manifest
        );
        assert_eq!(
            fs::read(fixture.workspace.join("checks/authoring.test.cjs")).unwrap(),
            sentinel
        );
        assert_eq!(calls.load(Ordering::SeqCst), 4);
    }
}
