// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_extensions::skill_manifest::SkillDescriptor;

// SP-12 installation/context smoke. The mock proves delivery of the selected
// package bytes, not model usefulness or execution of the bundled helpers.
// ADR-070/071: only context-role content reaches the provider on activation;
// file/reference bytes stay installed and verified, with metadata in a manifest.
// Integrity failure and revocation remain covered by the existing skill tests.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn installed_ported_skills_reach_the_provider_with_complete_resources_without_edits() {
    const WORKFLOW_IDS: [&str; 8] = [
        "document-authoring",
        "skill-authoring",
        "frontend-design",
        "mcp-development",
        "llm-integration",
        "pdf-workflows",
        "spreadsheet-workflows",
        "webapp-testing",
    ];
    let catalog = vcp_extensions::catalog::embedded().unwrap();
    let shipped: Vec<_> = catalog
        .skills
        .iter()
        .filter(|entry| WORKFLOW_IDS.contains(&entry.id.as_str()))
        .collect();
    assert_eq!(
        shipped.len(),
        WORKFLOW_IDS.len(),
        "the installed-port smoke must exercise all eight shipped workflows"
    );

    for entry in shipped {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/responses"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(response(2, "complete")),
            )
            .expect(1)
            .mount(&server)
            .await;
        let mut fixture = Fixture::new(&server.uri(), "complete");
        let installed = fixture.package(true);
        let descriptor_bytes = fs::read(installed.join(&entry.descriptor)).unwrap();
        assert_eq!(
            vcp_protocol::digest_bytes(&descriptor_bytes),
            entry.descriptor_sha256,
            "installed descriptor identity: {}",
            entry.id
        );
        let descriptor: SkillDescriptor = serde_json::from_slice(&descriptor_bytes).unwrap();
        assert_eq!(descriptor.id, entry.id);
        assert_eq!(descriptor.version, entry.version);
        assert_eq!(descriptor.source, entry.source);
        assert_eq!(descriptor.body, entry.body);
        assert_eq!(descriptor.resources, entry.resources);
        let package = installed.join(&entry.id);
        let mut withheld = Vec::new();
        let mut expected_manifest = Vec::new();
        let mut expected: Vec<(String, Vec<u8>)> = std::iter::once(&descriptor.body)
            .chain(&descriptor.resources)
            .filter_map(|content| {
                let bytes = fs::read(package.join(&content.path)).unwrap();
                assert_eq!(
                    vcp_protocol::digest_bytes(&bytes),
                    content.sha256,
                    "installed resource identity: {}/{}",
                    entry.id,
                    content.path
                );
                // These ports ship text instructions, code and license notices.
                // Binary assets require their own explicit conversion contract.
                let text = std::str::from_utf8(&bytes).unwrap();
                if !content.use_.is_context() {
                    withheld.push((content.sha256.clone(), text.to_owned()));
                    expected_manifest.push(json!({
                        "path": content.path,
                        "bytes": bytes.len().to_string(),
                        "use": content.use_,
                    }));
                    return None;
                }
                Some((content.sha256.clone(), bytes))
            })
            .collect();

        // Reuse the existing report-only fixture boundary: a nonempty source
        // scope, no project check scripts, and no write or execution tools.
        fs::remove_file(fixture.workspace.join("package.json")).unwrap();
        fs::remove_file(fixture.workspace.join("acceptance.cjs")).unwrap();
        let original = fs::read(fixture.workspace.join("value.txt")).unwrap();
        let mut profile: Value =
            serde_json::from_slice(&fs::read(&fixture.profile).unwrap()).unwrap();
        profile["canonical_tools"] = json!(["vcp_read", "vcp_list", "vcp_search"]);
        profile["maximum_autonomy"] = json!("plan");
        profile["automatic_effects"] = json!([]);
        profile["affected_paths"] = json!(["value.txt"]);
        profile["processes"] = json!([]);
        profile["checks"] = json!([]);
        profile["max_requests"] = json!(1);
        profile["max_transport_retries"] = json!(0);
        fs::write(&fixture.profile, serde_json::to_vec(&profile).unwrap()).unwrap();

        let qualified = format!("vcp-builtin::{0}::{0}", entry.id);
        let output = fixture
            .run(&[
                "run",
                "Report on the supplied value. No workspace edits are authorized.",
                "--autonomy",
                "plan",
                "--skill",
                &qualified,
            ])
            .await;
        let results = records(&output);
        assert!(
            output.status.success(),
            "{qualified}: {} {:?}",
            String::from_utf8_lossy(&output.stderr),
            results.last()
        );
        assert_eq!(results.last().unwrap()["conditions"]["completed"], true);
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1, "one synthetic request for {qualified}");
        let request: Value = serde_json::from_slice(&requests[0].body).unwrap();
        let tools = request["tools"].as_array().unwrap();
        // Skills never add tools; vcp_skill is implied by the owner's read
        // ceiling (ADR-071) and only reads or, with vcp_patch, copies resources.
        assert_eq!(tools.len(), 4, "skill must not expand tool authority");
        assert_eq!(
            tools
                .iter()
                .map(|tool| tool["name"].as_str().unwrap())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["vcp_read", "vcp_list", "vcp_search", "vcp_skill"])
        );
        let context_parts: Vec<Value> = request["input"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|message| message["content"].as_array().unwrap())
            .filter_map(|content| serde_json::from_str::<Value>(content["text"].as_str()?).ok())
            .collect();
        let parts: Vec<_> = context_parts
            .iter()
            .filter(|part| part["kind"] == "skill")
            .collect();
        assert_eq!(
            parts.len(),
            expected.len(),
            "every installed context body/resource must reach the provider for {qualified}"
        );
        assert!(
            !withheld.is_empty(),
            "each ported workflow withholds its license and provenance: {qualified}"
        );
        for (sha256, text) in &withheld {
            assert!(
                !request.to_string().contains(sha256.as_str())
                    && context_parts.iter().all(|part| {
                        !part["text"]
                            .as_str()
                            .is_some_and(|context| context.contains(text.as_str()))
                    }),
                "file/reference content must not reach the provider on activation: {qualified}"
            );
        }
        let manifests: Vec<Value> = context_parts
            .iter()
            .filter_map(|part| serde_json::from_str(part["text"].as_str()?).ok())
            .filter(|manifest: &Value| manifest["schema"] == "skill-resources/1")
            .collect();
        assert_eq!(manifests.len(), 1, "one resource manifest for {qualified}");
        assert_eq!(manifests[0]["skill"], qualified);
        assert_eq!(manifests[0]["resources"], json!(expected_manifest));
        for part in parts {
            assert_eq!(part["trust"], "active_skill", "{qualified}");
            let position = expected
                .iter()
                .position(|(sha256, bytes)| {
                    part["source_sha256"].as_str() == Some(sha256.as_str())
                        && part["text"]
                            .as_str()
                            .is_some_and(|text| text.as_bytes() == bytes.as_slice())
                })
                .unwrap_or_else(|| {
                    panic!(
                        "unexpected or duplicate active skill content for {qualified}: {}",
                        part["source_sha256"]
                    )
                });
            let (_, bytes) = expected.remove(position);
            assert_eq!(part["range"], json!(["0", bytes.len().to_string()]));
        }
        assert!(
            expected.is_empty(),
            "missing installed content: {qualified}"
        );
        assert_eq!(
            fs::read(fixture.workspace.join("value.txt")).unwrap(),
            original,
            "workspace changed under report-only skill activation: {qualified}"
        );
        assert_eq!(fs::read_dir(&fixture.workspace).unwrap().count(), 1);
    }
}

fn function_call(index: usize, name: &str, arguments: Value) -> String {
    let item = if name.is_empty() {
        json!({"type":"message","id":format!("final-{index}"),"role":"assistant","status":"completed","content":[{"type":"output_text","text":"Reported the helper result.","annotations":[]}]})
    } else {
        json!({"type":"function_call","id":format!("item-{index}"),"call_id":format!("call-{index}"),"name":name,"arguments":arguments.to_string(),"status":"completed"})
    };
    [json!({"type":"response.output_item.done","output_index":0,"item":item}),json!({"type":"response.completed","response":{"id":format!("response-{index}"),"status":"completed","output":[item],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}})].into_iter().map(|event|format!("data: {event}\n\n")).collect()
}

// SU-05 acceptance: an installed workflow's file-role helper is withheld from
// context, materialized byte-exactly through patch authority, and run only
// through a configured process profile. Requires VCP_TEST_PYTHON naming a
// Python with the pdf-workflows requirements; otherwise it reports not run.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn installed_pdf_helper_materializes_and_runs_through_authorized_process() {
    let Some(python) = std::env::var_os("VCP_TEST_PYTHON") else {
        eprintln!("not run: set VCP_TEST_PYTHON to a Python with pdf-workflows requirements");
        return;
    };
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let responses = calls.clone();
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(move |_: &wiremock::Request| {
            let body = match responses.fetch_add(1, Ordering::SeqCst) {
                0 => function_call(0, "vcp_skill", json!({"action":"materialize","skill":"pdf-workflows","resource":"scripts/pdf_workflows.py","destination":"pdf_workflows.py"})),
                1 => function_call(1, "vcp_exec", json!({"profile":"python","arguments":["pdf_workflows.py","--root",".","extract","--input","source.pdf","--output","extracted.json"],"directory":"","timeout_ms":60000,"output_bytes":65536,"input":null})),
                index => function_call(index, "", json!(null)),
            };
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(body)
        })
        .mount(&server)
        .await;
    let mut fixture = Fixture::new(&server.uri(), "complete");
    let installed = fixture.package(true);
    for file in ["package.json", "acceptance.cjs", "value.txt"] {
        fs::remove_file(fixture.workspace.join(file)).unwrap();
    }
    let created = std::process::Command::new(&python)
        .current_dir(&fixture.workspace)
        .args(["-c", "from reportlab.pdfgen import canvas; c = canvas.Canvas('source.pdf'); c.drawString(72, 720, 'Materialized helper extraction check'); c.save()"])
        .status()
        .unwrap();
    assert!(created.success(), "fixture PDF creation");
    let mut profile: Value = serde_json::from_slice(&fs::read(&fixture.profile).unwrap()).unwrap();
    profile["processes"] = json!([{"name":"python","executable":python.to_string_lossy(),
        "environment":{"SystemRoot":std::env::var("SystemRoot").unwrap()},
        "required_isolation":[],"reduced_isolation":true,"inputs":[]}]);
    profile["checks"] = json!([]);
    profile["affected_paths"] = json!(["pdf_workflows.py", "extracted.json"]);
    profile["max_requests"] = json!(4);
    profile["max_transport_retries"] = json!(0);
    fs::write(&fixture.profile, serde_json::to_vec(&profile).unwrap()).unwrap();
    let output = fixture
        .run(&[
            "run",
            "Extract the text of source.pdf with the active PDF skill helper.",
            "--autonomy",
            "autonomous",
            "--skill",
            "vcp-builtin::pdf-workflows::pdf-workflows",
        ])
        .await;
    let results = records(&output);
    let requests = server.received_requests().await.unwrap();
    assert!(
        requests.len() >= 3,
        "{} {:?}",
        String::from_utf8_lossy(&output.stderr),
        results.last()
    );
    let script = fs::read(installed.join("pdf-workflows/scripts/pdf_workflows.py")).unwrap();
    let first = String::from_utf8_lossy(&requests[0].body).into_owned();
    assert!(
        !first.contains("def extract("),
        "file-role helper source is not sent as context"
    );
    assert_eq!(
        fs::read(fixture.workspace.join("pdf_workflows.py")).unwrap(),
        script,
        "materialized helper is the exact installed file"
    );
    let extracted: Value =
        serde_json::from_slice(&fs::read(fixture.workspace.join("extracted.json")).unwrap())
            .unwrap();
    assert!(
        extracted
            .to_string()
            .contains("Materialized helper extraction check"),
        "{extracted}"
    );
    let materialized = String::from_utf8_lossy(&requests[1].body).into_owned();
    assert!(materialized.contains(&vcp_protocol::digest_bytes(&script)));
}
