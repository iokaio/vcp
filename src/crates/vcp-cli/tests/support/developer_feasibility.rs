// SPDX-License-Identifier: Apache-2.0
// CS-2 campaign feasibility with a synthetic provider: frozen v5 cases run through
// the real CLI, the canonical tool ceiling, explicit skill arms and the pinned
// data-only developer checker. No model quality or paid behavior is claimed.
use super::*;

const MARKER: &str = "// Inert VCP developer verifier marker; never executed as JavaScript.\n";
const REFRESH: &str = "New instruction scope selected. Review the refreshed context before issuing this operation again.";

fn developer_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../evals/skills/developer")
}
/// The frozen task and its oracle from the v5 manifest.
fn frozen(case: &str) -> (Value, Value) {
    let manifest: Value =
        serde_json::from_slice(&fs::read(developer_root().join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["revision"], "cs-2-developer-fixtures-v5");
    let task = manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["id"] == case)
        .unwrap()
        .clone();
    let oracle = serde_json::from_slice(
        &fs::read(developer_root().join(task["expected"]["oracle"]["path"].as_str().unwrap()))
            .unwrap(),
    )
    .unwrap();
    (task, oracle)
}
fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap().to_owned())
        .collect()
}
/// Replace the default fixture workspace with the frozen project, plus the checker
/// scaffold for write cases, and bind the profile to the case's tool ceiling.
fn stage(fixture: &Fixture, case: &str) -> (Value, Vec<String>) {
    let (task, oracle) = frozen(case);
    for name in ["value.txt", "package.json", "acceptance.cjs"] {
        fs::remove_file(fixture.workspace.join(name)).unwrap();
    }
    let project = developer_root().join(task["project"].as_str().unwrap());
    let mut sources = Vec::new();
    for source in task["expected"]["source_files"].as_array().unwrap() {
        let name = source["path"].as_str().unwrap();
        fs::copy(project.join(name), fixture.workspace.join(name)).unwrap();
        sources.push(name.to_owned());
    }
    let editable = strings(&oracle["allowed_modifications"]);
    let mut profile: Value = serde_json::from_slice(&fs::read(&fixture.profile).unwrap()).unwrap();
    profile["canonical_tools"] = task["context"]["tools"].clone();
    profile["max_transport_retries"] = json!(0);
    if editable.is_empty() {
        profile["maximum_autonomy"] = json!("plan");
        profile["automatic_effects"] = json!([]);
        profile["processes"] = json!([]);
        profile["checks"] = json!([]);
        // Report-only cases still need a nonempty acceptance scope; it grants no edits.
        profile["affected_paths"] = json!(sources);
    } else {
        fs::create_dir(fixture.workspace.join("checks")).unwrap();
        fs::write(fixture.workspace.join("checks/developer.test.cjs"), MARKER).unwrap();
        fs::write(
            fixture.workspace.join("checks/developer.case.json"),
            format!(
                "{{\"schema_version\":1,\"case_id\":{}}}\n",
                serde_json::to_string(case).unwrap()
            ),
        )
        .unwrap();
        let runtime = fixture._temp.path().join("checker-runtime");
        fs::create_dir(&runtime).unwrap();
        let checker = runtime.join("vcp-developer-check.exe");
        fs::copy(env!("CARGO_BIN_EXE_vcp-developer-check"), &checker).unwrap();
        fs::write(runtime.join("developer-cases.json"), serde_json::to_vec(&json!({"schema_version":1,"cases":[{"workspace":fixture.workspace,"case_id":case}]})).unwrap()).unwrap();
        profile["affected_paths"] = json!(editable);
        profile["processes"] = json!([{"name":"developer-check","executable":checker,"environment":{"SystemRoot":std::env::var("SystemRoot").unwrap()},"required_isolation":[],"reduced_isolation":true,"inputs":[]}]);
        profile["checks"] = json!([{"manifest":"package.json","runner":"node","profile":"developer-check","timeout_ms":10000,"expected_tests":["developer input preservation","developer output structure"],"rationale":"Frozen CS-2 native read-only developer validation"}]);
    }
    fs::write(&fixture.profile, serde_json::to_vec(&profile).unwrap()).unwrap();
    (task, editable)
}
fn event(index: usize, item: Value) -> String {
    [json!({"type":"response.output_item.done","output_index":0,"item":item}),json!({"type":"response.completed","response":{"id":format!("response-{index}"),"status":"completed","output":[item],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}})].into_iter().map(|event| format!("data: {event}\n\n")).collect()
}
fn call(index: usize, name: &str, arguments: Value) -> String {
    event(
        index,
        json!({"type":"function_call","id":format!("item-{index}"),"call_id":format!("call-{index}"),"name":name,"arguments":arguments.to_string(),"status":"completed"}),
    )
}
fn finish(index: usize) -> String {
    event(
        index,
        json!({"type":"message","id":format!("final-{index}"),"role":"assistant","status":"completed","content":[{"type":"output_text","text":"Observed completion.","annotations":[]}]}),
    )
}
fn outputs(request: &wiremock::Request) -> Vec<Value> {
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    body["input"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["type"] == "function_call_output")
        .map(|item| serde_json::from_str(item["output"].as_str().unwrap()).unwrap_or(Value::Null))
        .collect()
}
/// Serve a mock that reacts to the latest tool result: first `opening`, then a
/// verification (retried once after an instruction-scope refresh), then a final answer.
async fn scripted(server: &MockServer, opening: (&'static str, Value), cite_read: bool) {
    let counter = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(move |request: &wiremock::Request| {
            let index = counter.fetch_add(1, Ordering::SeqCst);
            let seen = outputs(request);
            let body = match seen.last() {
                None => call(index, opening.0, opening.1.clone()),
                Some(last) if last.get("verification").is_some() => finish(index),
                Some(last) if last["reason"] == REFRESH || seen.len() == 1 => {
                    let citations = if cite_read {
                        vec![seen[0]["evidence"].clone()]
                    } else {
                        Vec::new()
                    };
                    call(index, "vcp_verify", json!({"citations": citations}))
                }
                Some(_) => finish(index),
            };
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(body)
        })
        .mount(server)
        .await;
}
fn tool_names(request: &wiremock::Request) -> BTreeSet<String> {
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    body["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect()
}
fn active_skills(request: &wiremock::Request) -> Vec<String> {
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    body["input"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|message| message["content"].as_array().into_iter().flatten())
        .filter_map(|content| serde_json::from_str::<Value>(content["text"].as_str()?).ok())
        .filter(|part| part["kind"] == "skill" && part["trust"] == "active_skill")
        .map(|part| part["text"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_developer_write_cases_complete_through_ceiling_arms_and_pinned_checker() {
    let fixed = [
        ("UI-near-miss-parser-v2", "parse-count.js", "exports.parseCount = value => parseInt(value, 10);",
         "exports.parseCount = value => { if (typeof value !== 'string' || !/^[0-9]+$/.test(value)) throw TypeError('count'); const number = Number(value); if (!Number.isSafeInteger(number)) throw TypeError('count'); return number; };"),
        ("MCP-near-miss-rest-v3", "validate.js", "exports.validate = body => !!body.name;",
         "exports.validate = body => { if (!body || typeof body !== 'object' || Array.isArray(body) || Object.keys(body).length !== 1 || typeof body.name !== 'string') return false; const name = body.name.trim(); return name.length >= 1 && name.length <= 40; };"),
        ("LLM-near-miss-parser-v2", "label.js", "exports.normalizeLabel = value => value.toLowerCase();",
         "exports.normalizeLabel = value => { if (typeof value !== 'string') throw TypeError('label'); return value.trim().replace(/[A-Z]/g, c => c.toLowerCase()); };"),
    ];
    for (arm, (case, file, before, after)) in
        ["none", "nearest", "candidate"].into_iter().zip(fixed)
    {
        let server = MockServer::start().await;
        let patch = format!("*** Begin Patch\n*** Update File: {file}\n@@\n 'use strict';\n-{before}\n+{after}\n*** End Patch");
        scripted(&server, ("vcp_patch", json!({"patch": patch})), false).await;
        let mut fixture = Fixture::new(&server.uri(), "complete");
        let builtin = fixture.package(true);
        let (task, editable) = stage(&fixture, case);
        assert_eq!(editable, vec![file.to_owned()]);
        let mut expected_skills = Vec::new();
        let collection = tempfile::tempdir().unwrap();
        let mut args = vec![
            "run".to_owned(),
            task["prompt"].as_str().unwrap().to_owned(),
            "--autonomy".into(),
            "autonomous".into(),
        ];
        match arm {
            // MCP's nearest arm activates two built-in skills together.
            "nearest" => {
                for skill in ["architecture", "javascript-typescript"] {
                    args.extend(["--skill".into(), format!("vcp-builtin::{skill}::{skill}")]);
                    expected_skills
                        .push(fs::read_to_string(builtin.join(skill).join("SKILL.md")).unwrap());
                }
            }
            // Candidates are selected from an explicit user source, never the catalog.
            "candidate" => {
                let package = collection.path().join("developer-feasibility-probe");
                fs::create_dir_all(&package).unwrap();
                let body =
                    "Original feasibility probe guidance. It grants no tools or authority.\n";
                fs::write(package.join("SKILL.md"), body).unwrap();
                fs::write(package.join("skill.json"), serde_json::to_vec(&json!({
                    "schema_version":1,"id":"developer-feasibility-probe","version":"1.0.0","description":"Feasibility probe for explicit candidate selection.",
                    "source":"vcp-original","license":"Apache-2.0","vcp_version":1,"cues":["explicit:developer-feasibility-probe"],"environments":[],
                    "required_tools":["vcp_list","vcp_read"],"body":{"path":"SKILL.md","sha256":vcp_protocol::digest_bytes(body.as_bytes())},"resources":[]
                })).unwrap()).unwrap();
                let mut profile: Value =
                    serde_json::from_slice(&fs::read(&fixture.profile).unwrap()).unwrap();
                profile["skills"] = json!({"version":1,"revision":"0","sources":[{"id":"vcp-developer-candidates","root_id":vcp_domain::RootId::new(),"kind":"user","enabled":true,"path":package}]});
                fs::write(&fixture.profile, serde_json::to_vec(&profile).unwrap()).unwrap();
                args.extend([
                    "--skill".into(),
                    "vcp-developer-candidates::.::developer-feasibility-probe".into(),
                ]);
                expected_skills.push(body.to_owned());
            }
            _ => {}
        }
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = fixture.run(&args).await;
        let values = records(&output);
        let requests = server.received_requests().await.unwrap();
        assert!(
            output.status.success(),
            "{case}: {} {:?}",
            String::from_utf8_lossy(&output.stderr),
            values.last()
        );
        assert_eq!(
            values.last().unwrap()["conditions"]["completed"],
            true,
            "{case}"
        );
        let ceiling: BTreeSet<String> = strings(&task["context"]["tools"]).into_iter().collect();
        for request in &requests {
            assert_eq!(
                tool_names(request),
                ceiling,
                "{case}: advertised tools must equal the frozen allowlist"
            );
        }
        assert_eq!(
            active_skills(&requests[0]),
            expected_skills,
            "{case}: {arm} arm activation"
        );
        let verifications: Vec<Value> = requests
            .iter()
            .flat_map(outputs)
            .filter(|result| result.get("verification").is_some())
            .collect();
        let verification = verifications
            .last()
            .unwrap_or_else(|| panic!("{case}: no executed verification"));
        assert_eq!(
            verification["verification"]["checks"][0]["specification"],
            "package.json#test"
        );
        assert_eq!(
            verification["verification"]["checks"][0]["outcome"]["status"], "passed",
            "{case}: {verification}"
        );
        let tail = verification["diagnostics"][0]["stdout"]["tail"]
            .as_str()
            .unwrap();
        let lines: Vec<&str> = tail.lines().map(str::trim_end).collect();
        assert!(
            lines.contains(&"ok 1 - developer input preservation")
                && lines.contains(&"ok 2 - developer output structure"),
            "{tail}"
        );
        let (_, oracle) = frozen(case);
        let project = developer_root().join(task["project"].as_str().unwrap());
        for name in strings(&oracle["preserve_files"]) {
            assert_eq!(
                fs::read(fixture.workspace.join(&name)).unwrap(),
                fs::read(project.join(&name)).unwrap(),
                "{case}: {name}"
            );
        }
        assert!(fs::read_to_string(fixture.workspace.join(file))
            .unwrap()
            .contains(after));
        assert_eq!(
            fs::read_to_string(fixture.workspace.join("checks/developer.test.cjs")).unwrap(),
            MARKER
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn executable_developer_report_only_case_completes_by_citing_read_evidence() {
    let server = MockServer::start().await;
    scripted(
        &server,
        (
            "vcp_read",
            json!({"path":"provider.json","max_bytes":65536,"start_line":null,"end_line":null}),
        ),
        true,
    )
    .await;
    let fixture = Fixture::new(&server.uri(), "complete");
    let (task, editable) = stage(&fixture, "LLM-hostile-diagnostics-v2");
    assert!(editable.is_empty());
    let before: Vec<(PathBuf, Vec<u8>)> = fs::read_dir(&fixture.workspace)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let bytes = fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect();
    let output = fixture
        .run(&[
            "run",
            task["prompt"].as_str().unwrap(),
            "--autonomy",
            "plan",
        ])
        .await;
    let values = records(&output);
    let requests = server.received_requests().await.unwrap();
    assert!(
        output.status.success(),
        "{} {:?} {:?}",
        String::from_utf8_lossy(&output.stderr),
        values.last(),
        requests.last().map(outputs)
    );
    assert_eq!(values.last().unwrap()["conditions"]["completed"], true);
    let ceiling: BTreeSet<String> = strings(&task["context"]["tools"]).into_iter().collect();
    assert_eq!(
        ceiling,
        BTreeSet::from(["vcp_list", "vcp_read", "vcp_search", "vcp_verify"].map(String::from))
    );
    for request in &requests {
        assert_eq!(tool_names(request), ceiling);
    }
    let verification = requests
        .iter()
        .flat_map(outputs)
        .rfind(|result| result.get("verification").is_some())
        .expect("cited verification");
    assert!(
        verification["diagnostics"]
            .as_array()
            .is_none_or(Vec::is_empty),
        "report-only verification runs no process"
    );
    for (path, bytes) in before {
        assert_eq!(
            fs::read(&path).unwrap(),
            bytes,
            "report-only workspace changed: {}",
            path.display()
        );
    }
    assert_eq!(
        fs::read_dir(&fixture.workspace).unwrap().count(),
        task["expected"]["source_files"].as_array().unwrap().len()
    );
}
