// SPDX-License-Identifier: Apache-2.0
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_read_only_tool_ceiling_completes_report_with_host_verification() {
    let server = MockServer::start().await;
    let fixture = Fixture::new(&server.uri(), "complete");
    let mut profile: Value = serde_json::from_slice(&fs::read(&fixture.profile).unwrap()).unwrap();
    profile["canonical_tools"] = json!(["vcp_read", "vcp_list", "vcp_search"]);
    profile["checks"] = json!([]);
    profile["processes"] = json!([]);
    fs::write(&fixture.profile, serde_json::to_vec(&profile).unwrap()).unwrap();
    fs::remove_file(fixture.workspace.join("package.json")).unwrap();
    fs::remove_file(fixture.workspace.join("acceptance.cjs")).unwrap();
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                // A final report contains no model tool call, including verification.
                .set_body_string(response(2, "complete")),
        )
        .expect(1)
        .mount(&server)
        .await;
    let output = fixture
        .run(&[
            "run",
            "Analyze the supplied value without editing.",
            "--autonomy",
            "plan",
        ])
        .await;
    let values = records(&output);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{} {:?}",
        String::from_utf8_lossy(&output.stderr),
        values.last()
    );
    assert_eq!(values.last().unwrap()["conditions"]["completed"], true);
    assert!(values
        .iter()
        .any(|value| value.to_string().contains("verification_recorded")));
    assert_eq!(
        fs::read_to_string(fixture.workspace.join("value.txt")).unwrap(),
        "41\n"
    );

    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests.len(),
        1,
        "completion must require no model verification turn"
    );
    let outbound: Value = serde_json::from_slice(&requests[0].body).unwrap();
    let tools = outbound["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 3);
    assert_eq!(
        tools
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["vcp_read", "vcp_list", "vcp_search"])
    );
    let serialized = outbound.to_string();
    assert!(!serialized.contains("Run vcp_verify"));
    assert!(serialized.contains("The host performs applicable source-integrity completion checks."));
    assert!(serialized.contains(
        "Model verification is unavailable under this tool ceiling; report only observed results."
    ));
}
