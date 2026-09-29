// SPDX-License-Identifier: Apache-2.0
// Explicit installed-promotion acceptance only. These tests never substitute
// external candidate sources, alter the repository catalog, or qualify models.
use super::*;
use std::{path::Component, sync::Mutex, time::Duration};

const SIX: [(&str, &str); 6] = [
    ("document-authoring", "1.0.5"),
    ("skill-authoring", "1.0.3"),
    ("frontend-design", "1.0.0"),
    ("mcp-development", "1.0.1"),
    ("llm-integration", "1.0.0"),
    ("webapp-testing", "1.0.1"),
];

fn checked_content(package: &std::path::Path, item: &Value) -> Vec<u8> {
    let relative = PathBuf::from(item["path"].as_str().unwrap());
    assert!(!relative.as_os_str().is_empty());
    assert!(relative
        .components()
        .all(|part| matches!(part, Component::Normal(_))));
    let bytes = fs::read(package.join(relative)).unwrap();
    assert_eq!(vcp_protocol::digest_bytes(&bytes), item["sha256"]);
    bytes
}

fn installed(fixture: &mut Fixture) -> (PathBuf, Value) {
    assert!(
        std::env::var_os("VCP_TEST_CANDIDATE_ROOT").is_none(),
        "candidate override is not installed-promotion evidence"
    );
    let package = PathBuf::from(
        std::env::var_os("VCP_TEST_SKILL_PACKAGE").expect("exact installed package required"),
    );
    let admission_file = PathBuf::from(
        std::env::var_os("VCP_TEST_PROMOTED_ADMISSION")
            .expect("authenticated promotion admission required"),
    );
    assert!(package.is_absolute() && admission_file.is_absolute());
    let bytes = fs::read(admission_file).unwrap();
    assert_eq!(
        vcp_protocol::digest_bytes(&bytes),
        std::env::var("VCP_TEST_PROMOTED_ADMISSION_SHA256").unwrap()
    );
    let admission: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(admission["schema"], "cs3-promoted-distribution-admission/1");
    assert_eq!(admission["status"], "eligible_for_package_checks");
    assert_eq!(admission["model_calls"], 0);
    assert_eq!(admission["qualification_waiver"], false);
    let skills = admission["skills"].as_array().unwrap();
    assert_eq!(skills.len(), SIX.len());
    let assets = fixture.package(true);
    assert_eq!(
        vcp_protocol::digest_bytes(&fs::read(&fixture.binary).unwrap()),
        admission["executable"]["sha256"]
    );
    let catalog_bytes = fs::read(assets.join("catalog.json")).unwrap();
    assert_eq!(
        vcp_protocol::digest_bytes(&catalog_bytes),
        admission["catalog"]["sha256"]
    );
    let catalog: Value = serde_json::from_slice(&catalog_bytes).unwrap();
    assert_eq!(catalog["skills"].as_array().unwrap().len(), 27);
    for ((id, version), expected) in SIX.iter().zip(skills) {
        let qualified = format!("vcp-builtin::{id}::{id}");
        assert_eq!(expected["id"], *id);
        assert_eq!(expected["version"], *version);
        assert_eq!(expected["qualified_id"], qualified);
        let rows: Vec<_> = catalog["skills"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["id"] == *id)
            .collect();
        assert_eq!(rows.len(), 1);
        let row = rows[0];
        assert_eq!(row["descriptor"], format!("{id}/skill.json"));
        assert_eq!(row["version"], *version);
        assert_eq!(row["descriptor_sha256"], expected["descriptor_sha256"]);
        let root = assets.join(id);
        let descriptor_bytes = fs::read(root.join("skill.json")).unwrap();
        assert_eq!(
            vcp_protocol::digest_bytes(&descriptor_bytes),
            expected["descriptor_sha256"]
        );
        let descriptor: Value = serde_json::from_slice(&descriptor_bytes).unwrap();
        assert_eq!(descriptor["id"], *id);
        assert_eq!(descriptor["version"], *version);
        for key in ["body", "resources"] {
            assert_eq!(descriptor[key], expected[key]);
            assert_eq!(row[key], expected[key]);
        }
        checked_content(&root, &expected["body"]);
        for resource in expected["resources"].as_array().unwrap() {
            checked_content(&root, resource);
        }
    }
    (assets, admission)
}

async fn offline_list(fixture: &Fixture) -> Output {
    let mut command = fixture.command(&["skills", "list"]);
    command.env_remove("OPENROUTER_API_KEY");
    tokio::task::spawn_blocking(move || command.output())
        .await
        .unwrap()
        .unwrap()
}

async fn persisted_state(fixture: &Fixture) -> Value {
    let directory = fs::read_dir(fixture.data.join("workspaces"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.join("workspace.json").is_file())
        .unwrap();
    let entry: vcp_cli::settings::WorkspaceEntry =
        serde_json::from_slice(&fs::read(directory.join("workspace.json")).unwrap()).unwrap();
    let store = vcp_store::Store::open(
        &entry.config.canonical_root,
        entry.config.backend,
        &[fixture.workspace.canonicalize().unwrap()],
    )
    .await
    .unwrap();
    let state = store
        .state()
        .records
        .values()
        .find(|record| {
            record.collection == vcp_store::contract::Collection::Projection
                && record.value["document_type"] == "vcp_task_skills_v1"
                && record.value["scope"]["task"] == entry.config.root_task.as_str()
        })
        .unwrap()
        .value
        .clone();
    store.close().await.unwrap();
    state
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires authenticated all-six promotion and exact installed distribution"]
async fn executable_promoted_six_builtin_offline_discovery_and_integrity() {
    let server = MockServer::start().await;
    let mut fixture = Fixture::new(&server.uri(), "budget");
    let (assets, admission) = installed(&mut fixture);
    let registration = fixture
        .run(&[
            "run",
            "Register installed skills",
            "--autonomy",
            "autonomous",
            "--budget-usd",
            "0.000001",
        ])
        .await;
    assert_eq!(registration.status.code(), Some(5));
    fs::remove_file(fixture.data.join("catalog.json")).unwrap();
    // Discovery must not depend on body/resource readability or provider setup.
    let mut removed = Vec::new();
    for skill in admission["skills"].as_array().unwrap() {
        let root = assets.join(skill["id"].as_str().unwrap());
        for item in std::iter::once(&skill["body"]).chain(skill["resources"].as_array().unwrap()) {
            let file = root.join(item["path"].as_str().unwrap());
            removed.push((file.clone(), fs::read(&file).unwrap()));
            fs::remove_file(file).unwrap();
        }
    }
    let output = offline_list(&fixture).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let values = records(&output);
    let data = &values.last().unwrap()["data"];
    assert_eq!(data["total_skills"], 27);
    assert_eq!(data["reads"]["bodies"], 0);
    assert_eq!(data["reads"]["resources"], 0);
    assert!(!data["integrity"].is_null());
    assert!(data["skills"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["source"] == "vcp-builtin"));
    for (id, version) in SIX {
        let matches: Vec<_> = data["skills"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["qualified_id"] == format!("vcp-builtin::{id}::{id}"))
            .collect();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["version"], version);
    }
    for (file, bytes) in removed {
        fs::write(file, bytes).unwrap();
    }
    let paths = SIX
        .iter()
        .map(|(id, _)| format!("{id}/skill.json"))
        .chain(["catalog.json".into(), "coverage.json".into()]);
    for relative in paths {
        let file = assets.join(&relative);
        let bytes = fs::read(&file).unwrap();
        fs::write(&file, b"{}").unwrap();
        assert!(
            !offline_list(&fixture).await.status.success(),
            "tampered {relative} accepted"
        );
        fs::write(file, bytes).unwrap();
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires authenticated all-six promotion and exact installed distribution"]
async fn executable_promoted_six_builtin_report_only_profiles_complete_without_workspace_edits() {
    for (id, _) in SIX {
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
        let (assets, admission) = installed(&mut fixture);
        fs::remove_file(fixture.workspace.join("package.json")).unwrap();
        fs::remove_file(fixture.workspace.join("acceptance.cjs")).unwrap();
        let source = fs::read(fixture.workspace.join("value.txt")).unwrap();
        let mut profile: Value =
            serde_json::from_slice(&fs::read(&fixture.profile).unwrap()).unwrap();
        profile["skills"] = json!({"version":1,"revision":"0","sources":[]});
        profile["canonical_tools"] = json!(["vcp_read", "vcp_list", "vcp_search"]);
        profile["maximum_autonomy"] = json!("plan");
        profile["automatic_effects"] = json!([]);
        profile["affected_paths"] = json!(["value.txt"]);
        profile["processes"] = json!([]);
        profile["checks"] = json!([]);
        profile["max_requests"] = json!(1);
        profile["max_transport_retries"] = json!(0);
        fs::write(&fixture.profile, serde_json::to_vec(&profile).unwrap()).unwrap();
        let qualified = format!("vcp-builtin::{id}::{id}");
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
        let values = records(&output);
        assert!(
            output.status.success(),
            "{id}: {} {:?}",
            String::from_utf8_lossy(&output.stderr),
            values.last()
        );
        assert_eq!(values.last().unwrap()["conditions"]["completed"], true);
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        let request: Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(
            request["tools"]
                .as_array()
                .unwrap()
                .iter()
                .map(|tool| tool["name"].as_str().unwrap())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["vcp_read", "vcp_list", "vcp_search"])
        );
        assert_eq!(request["tools"].as_array().unwrap().len(), 3);
        let parts: Vec<Value> = request["input"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|message| message["content"].as_array().unwrap())
            .filter_map(|content| serde_json::from_str::<Value>(content["text"].as_str()?).ok())
            .filter(|part| part["kind"] == "skill")
            .collect();
        let skill = admission["skills"]
            .as_array()
            .unwrap()
            .iter()
            .find(|skill| skill["id"] == id)
            .unwrap();
        let content: Vec<_> = std::iter::once(&skill["body"])
            .chain(skill["resources"].as_array().unwrap())
            .collect();
        assert_eq!(parts.len(), content.len());
        for item in content {
            let text = String::from_utf8(checked_content(&assets.join(id), item)).unwrap();
            assert_eq!(
                parts
                    .iter()
                    .filter(|part| part["trust"] == "active_skill" && part["text"] == text)
                    .count(),
                1
            );
        }
        let state = persisted_state(&fixture).await;
        let active = state["active"].as_object().unwrap();
        assert_eq!(active.len(), 1);
        let selected = active.get(&qualified).unwrap();
        assert_eq!(selected["qualified_id"], qualified);
        assert_eq!(selected["source_id"], "vcp-builtin");
        assert_eq!(selected["version"], skill["version"]);
        assert_eq!(
            fs::read(fixture.workspace.join("value.txt")).unwrap(),
            source
        );
        assert_eq!(fs::read_dir(&fixture.workspace).unwrap().count(), 1);
    }
}

async fn wait_for(captured: &Arc<Mutex<Vec<u8>>>, needle: &str) {
    wait_for_after(captured, 0, needle).await;
}

async fn wait_for_after(captured: &Arc<Mutex<Vec<u8>>>, offset: usize, needle: &str) {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if String::from_utf8_lossy(&captured.lock().unwrap()[offset..]).contains(needle) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("terminal never showed {needle}"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires authenticated all-six promotion and exact installed distribution"]
async fn executable_promoted_six_builtin_terminal_controls_preserve_precedence_and_revocation() {
    for (id, version) in SIX {
        let server = MockServer::start().await;
        let mut fixture = Fixture::new(&server.uri(), "budget");
        let (assets, admission) = installed(&mut fixture);
        // A deliberate workspace collision proves unqualified precedence without
        // replacing the installed builtin selected by its fully qualified ID.
        let shadow = fixture.skills();
        let descriptor_file = shadow.join("skill.json");
        let mut descriptor: Value =
            serde_json::from_slice(&fs::read(&descriptor_file).unwrap()).unwrap();
        descriptor["id"] = json!(id);
        fs::write(descriptor_file, serde_json::to_vec(&descriptor).unwrap()).unwrap();
        let qualified = format!("vcp-builtin::{id}::{id}");
        let mut child = fixture
            .terminal_args(&[
                "run",
                "Inspect the supplied project",
                "--autonomy",
                "autonomous",
                "--budget-usd",
                "0.000001",
            ])
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
            wait_for(&captured, "/skills").await;
            writer.send(b"/pause\r".to_vec()).await.unwrap();
            let skill = admission["skills"]
                .as_array()
                .unwrap()
                .iter()
                .find(|skill| skill["id"] == id)
                .unwrap();
            for item in
                std::iter::once(&skill["body"]).chain(skill["resources"].as_array().unwrap())
            {
                let file = assets.join(id).join(item["path"].as_str().unwrap());
                let bytes = fs::read(&file).unwrap();
                fs::write(&file, b"changed installed content").unwrap();
                // Require a new response for each changed content item; retain
                // all earlier captured terminal bytes rather than reusing them.
                let offset = captured.lock().unwrap().len();
                writer
                    .send(format!("/skills activate {qualified}\r").into_bytes())
                    .await
                    .unwrap();
                wait_for_after(&captured, offset, "skill source or dependency changed").await;
                fs::write(file, bytes).unwrap();
            }
            writer
                .send(
                    format!("/skills activate {qualified} promoted-builtin-evidence\r")
                        .into_bytes(),
                )
                .await
                .unwrap();
            wait_for(&captured, "promoted-builtin-evidence").await;
            wait_for(&captured, version).await;
            writer
                .send(format!("/skills disable {qualified}\r").into_bytes())
                .await
                .unwrap();
            wait_for(&captured, "disabled").await;
            writer
                .send(format!("/skills activate {id} workspace-precedence-evidence\r").into_bytes())
                .await
                .unwrap();
            wait_for(&captured, "workspace-precedence-evidence").await;
            wait_for(&captured, "1.2.0").await;
            let inspected = fixture.run(&["skills", "list"]).await;
            assert!(inspected.status.success());
            let rows = records(&inspected);
            let data = &rows.last().unwrap()["data"];
            assert_eq!(data["total_skills"], 28);
            assert_eq!(data["reads"]["bodies"], 0);
            assert!(data["configuration"]
                .as_str()
                .unwrap()
                .contains("current canonical owner"));
            writer
                .send(format!("/skills disable project::review::{id}\r/exit\r").into_bytes())
                .await
                .unwrap();
            let code = (&mut child.exit_rx).await.unwrap();
            assert!(matches!(code, 5 | 8), "{id}: exit {code}");
        };
        if tokio::time::timeout(Duration::from_secs(90), exercise)
            .await
            .is_err()
        {
            child.session.terminate();
            panic!("{id}: promoted terminal controls timed out");
        }
        reader.await.unwrap();
        let state = persisted_state(&fixture).await;
        assert!(state["active"].as_object().unwrap().is_empty());
        let disabled = state["disabled"].as_array().unwrap();
        assert!(disabled.iter().any(|item| item == &qualified));
        assert!(disabled
            .iter()
            .any(|item| item == &format!("project::review::{id}")));
        assert_eq!(
            state["revision"], "4",
            "only successful builtin/workspace activation and revocation persist"
        );
        assert!(server.received_requests().await.unwrap().is_empty());
    }
}
