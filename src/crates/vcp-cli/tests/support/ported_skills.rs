// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_extensions::skill_manifest::SkillDescriptor;

// SP-12 installation/context smoke. The mock proves delivery of the selected
// package bytes, not model usefulness or execution of the bundled helpers.
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
        let mut expected: Vec<(String, Vec<u8>)> = std::iter::once(&descriptor.body)
            .chain(&descriptor.resources)
            .map(|content| {
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
                std::str::from_utf8(&bytes).unwrap();
                (content.sha256.clone(), bytes)
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
        assert_eq!(tools.len(), 3, "skill must not expand tool authority");
        assert_eq!(
            tools
                .iter()
                .map(|tool| tool["name"].as_str().unwrap())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["vcp_read", "vcp_list", "vcp_search"])
        );
        let parts: Vec<Value> = request["input"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|message| message["content"].as_array().unwrap())
            .filter_map(|content| serde_json::from_str::<Value>(content["text"].as_str()?).ok())
            .filter(|part| part["kind"] == "skill")
            .collect();
        assert_eq!(
            parts.len(),
            expected.len(),
            "every installed body/resource must reach the provider for {qualified}"
        );
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
