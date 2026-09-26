// SPDX-License-Identifier: Apache-2.0
// Trusted transport/structural feasibility only. These synthetic artifacts are
// neither candidate model answers nor evidence of reader quality or authority review.
use super::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Mutex;

const REFRESH: &str = "New instruction scope selected. Review the refreshed context before issuing this operation again.";
const PACKAGE: &[u8] = b"{\"name\":\"vcp-authoring-check\",\"private\":true,\"scripts\":{\"test\":\"node --test checks/authoring.test.cjs\"}}\n";
const MARKER: &[u8] = b"// Inert VCP authoring verifier marker; never executed as JavaScript.\n";

#[derive(Clone)]
enum Action {
    Patch(String),
    Read(String),
    Descriptor,
    Verify,
}

struct Script {
    actions: VecDeque<Action>,
    pending: Option<(String, Action)>,
    hashes: BTreeMap<String, String>,
    expected: BTreeMap<String, String>,
    original_descriptor: Option<String>,
    read_evidence: BTreeMap<String, String>,
    verification_citations: Option<Vec<String>>,
    refreshes: usize,
    verified: usize,
    requests: usize,
}

fn patch(files: &[(String, Option<String>, String)]) -> String {
    let mut result = String::from("*** Begin Patch\n");
    for (name, before, after) in files {
        if let Some(before) = before {
            result.push_str(&format!("*** Update File: {name}\n@@\n"));
            for line in before.lines() {
                result.push_str(&format!("-{line}\n"));
            }
        } else {
            result.push_str(&format!("*** Add File: {name}\n"));
        }
        for line in after.lines() {
            result.push_str(&format!("+{line}\n"));
        }
    }
    result.push_str("*** End Patch");
    result
}

impl Script {
    fn next(&mut self, request: &wiremock::Request) -> String {
        let incoming: Value = serde_json::from_slice(&request.body).unwrap();
        let mut retry = None;
        if let Some((id, action)) = self.pending.take() {
            let item = incoming["input"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["type"] == "function_call_output" && item["call_id"] == id)
                .unwrap();
            let output: Value = serde_json::from_str(item["output"].as_str().unwrap()).unwrap();
            if output == json!({"executed":false,"reason":REFRESH}) {
                self.refreshes += 1;
                assert_eq!(
                    self.refreshes, 1,
                    "Only one unexecuted scope refresh may retry"
                );
                retry = Some(action);
            } else {
                match action {
                    Action::Patch(_) => assert_eq!(output["result"]["complete"], true, "{output}"),
                    Action::Read(name) => {
                        let read = &output["result"];
                        assert_eq!(read["complete"], true, "{output}");
                        let text = read["text"].as_str().unwrap();
                        let observed = read["version"]["sha256"].as_str().unwrap();
                        assert_eq!(observed, vcp_protocol::digest_bytes(text.as_bytes()));
                        assert_eq!(read["version"]["bytes"], text.len().to_string());
                        self.hashes.insert(name.clone(), observed.to_owned());
                        let evidence = output["evidence"]
                            .as_str()
                            .expect("successful read has retained evidence");
                        assert_ne!(
                            Some(evidence),
                            output["effect"].as_str(),
                            "effect IDs are not artifact citations"
                        );
                        assert!(
                            self.read_evidence
                                .insert(name, evidence.to_owned())
                                .is_none(),
                            "each scripted file is read once"
                        );
                    }
                    Action::Verify => {
                        self.verified += 1;
                        assert_eq!(self.verified, 1, "Never retry an executed checker");
                        let citations = self
                            .verification_citations
                            .as_ref()
                            .expect("verification citations were frozen before dispatch");
                        assert!(!citations.is_empty(), "verification cites current reads");
                        assert_eq!(
                            citations.iter().collect::<BTreeSet<_>>().len(),
                            citations.len(),
                            "verification citations are distinct"
                        );
                        assert_eq!(
                            citations.iter().cloned().collect::<BTreeSet<_>>(),
                            self.read_evidence.values().cloned().collect(),
                            "verification cites only successful read evidence"
                        );
                        assert_eq!(
                            output["verification"]["checks"].as_array().unwrap().len(),
                            1
                        );
                        assert_eq!(
                            output["verification"]["checks"][0]["outcome"]["status"], "passed",
                            "{output}"
                        );
                        let diagnostics = output["diagnostics"].as_array().unwrap();
                        assert_eq!(diagnostics.len(), 1);
                        assert_eq!(diagnostics[0]["exit_code"], 0);
                        let tail = diagnostics[0]["stdout"]["tail"].as_str().unwrap();
                        assert!(tail.contains("ok 1 - authoring input preservation"));
                        assert!(tail.contains("ok 2 - authoring output structure"));
                        assert!(tail
                            .contains("# Scope: source preservation and artifact structure only"));
                        assert!(tail.contains("# Not evaluated here (not_run):"));
                    }
                    Action::Descriptor => unreachable!(),
                }
            }
        }
        let action = retry.or_else(|| self.actions.pop_front()).map(|action| {
            if !matches!(action, Action::Descriptor) { return action; }
            let descriptor = if let Some(original) = &self.original_descriptor {
                let mut value: Value = serde_json::from_str(original).unwrap();
                assert_eq!(value["body"]["sha256"], self.hashes["package/SKILL.md"]);
                assert_eq!(value["resources"][0]["sha256"], self.hashes["package/references/delivered-changes.md"]);
                value["version"] = json!("1.3.1");
                value["resources"][1]["sha256"] = json!(self.hashes["package/references/planned-changes.md"]);
                value
            } else {
                json!({"schema_version":1,"id":"orchard-note-review","version":"1.0.0",
                    "description":"Synthetic package feasibility fixture", "source":"vcp-original","license":"Apache-2.0","vcp_version":1,
                    "cues":[],"environments":[],"required_tools":["vcp_list","vcp_read"],
                    "body":{"path":"SKILL.md","sha256":self.hashes["package/SKILL.md"]},
                    "resources":[{"path":"references/review-checklist.md","sha256":self.hashes["package/references/review-checklist.md"]}]})
            };
            let bytes = serde_json::to_string_pretty(&descriptor).unwrap() + "\n";
            self.expected.insert("package/skill.json".into(), bytes.clone());
            Action::Patch(patch(&[("package/skill.json".into(), self.original_descriptor.clone(), bytes)]))
        });
        let index = self.requests;
        self.requests += 1;
        assert!(self.requests <= 16);
        let item = if let Some(action) = action {
            let (name, arguments) = match &action {
                Action::Patch(text) => ("vcp_patch", json!({"patch":text})),
                Action::Read(name) => (
                    "vcp_read",
                    json!({"path":name,"max_bytes":16384,"start_line":null,"end_line":null}),
                ),
                Action::Verify => {
                    let citations = self.read_evidence.values().cloned().collect::<Vec<_>>();
                    assert!(
                        !citations.is_empty(),
                        "verification requires current read evidence"
                    );
                    assert_eq!(
                        citations.iter().collect::<BTreeSet<_>>().len(),
                        citations.len(),
                        "read evidence citations must be distinct"
                    );
                    self.verification_citations = Some(citations.clone());
                    ("vcp_verify", json!({"citations":citations}))
                }
                Action::Descriptor => unreachable!(),
            };
            let id = format!("followup-{index}");
            self.pending = Some((id.clone(), action));
            json!({"type":"function_call","id":format!("item-{index}"),"call_id":id,"name":name,"arguments":arguments.to_string(),"status":"completed"})
        } else {
            assert_eq!(self.verified, 1);
            json!({"type":"message","id":format!("final-{index}"),"role":"assistant","status":"completed","content":[{"type":"output_text","text":"Native structural feasibility observed. Human quality and authority review remain not_run.","annotations":[]}]})
        };
        [json!({"type":"response.output_item.done","output_index":0,"item":item}),json!({"type":"response.completed","response":{"id":format!("response-{index}"),"status":"completed","output":[item],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}})].into_iter().map(|event|format!("data: {event}\n\n")).collect()
    }
}

async fn feasible(case_id: &str) {
    let manifest: Value = serde_json::from_str(include_str!(
        "../../../../evals/skills/authoring-followup/manifest.json"
    ))
    .unwrap();
    assert_eq!(manifest["revision"], "cs-1-followup-fixtures-v3");
    let case = manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == case_id)
        .unwrap();
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../evals/skills/authoring-followup");
    let oracle_bytes =
        fs::read(source.join(case["expected"]["oracle"]["path"].as_str().unwrap())).unwrap();
    assert_eq!(
        vcp_protocol::digest_bytes(&oracle_bytes),
        case["expected"]["oracle"]["sha256"].as_str().unwrap()
    );
    let oracle: Value = serde_json::from_slice(&oracle_bytes).unwrap();
    let allowed: Vec<String> = ["allowed_outputs", "allowed_modifications"]
        .into_iter()
        .flat_map(|key| {
            oracle[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|name| name.as_str().unwrap().to_owned())
        })
        .collect();
    let server = MockServer::start().await;
    let fixture = Fixture::new(&server.uri(), "complete");
    for file in ["value.txt", "acceptance.cjs"] {
        fs::remove_file(fixture.workspace.join(file)).unwrap();
    }
    let mut originals = BTreeMap::new();
    for file in case["expected"]["source_files"].as_array().unwrap() {
        let name = file["path"].as_str().unwrap();
        let bytes = fs::read(source.join(case["project"].as_str().unwrap()).join(name)).unwrap();
        assert_eq!(
            vcp_protocol::digest_bytes(&bytes),
            file["sha256"].as_str().unwrap()
        );
        let destination = fixture.workspace.join(name);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, &bytes).unwrap();
        originals.insert(name.to_owned(), String::from_utf8(bytes).unwrap());
    }
    for file in &allowed {
        fs::create_dir_all(fixture.workspace.join(file).parent().unwrap()).unwrap();
    }
    fs::create_dir(fixture.workspace.join("checks")).unwrap();
    let marker = format!(
        "{{\"schema_version\":1,\"case_id\":{}}}\n",
        serde_json::to_string(case_id).unwrap()
    );
    let scaffold = [
        ("package.json", PACKAGE),
        ("checks/authoring.test.cjs", MARKER),
        ("checks/authoring.case.json", marker.as_bytes()),
    ];
    for (name, bytes) in scaffold {
        fs::write(fixture.workspace.join(name), bytes).unwrap();
    }
    let runtime = fixture._temp.path().join("checker-runtime");
    fs::create_dir(&runtime).unwrap();
    let checker = runtime.join("vcp-authoring-check.exe");
    fs::copy(env!("CARGO_BIN_EXE_vcp-authoring-check"), &checker).unwrap();
    fs::write(runtime.join("authoring-cases.json"), serde_json::to_vec(&json!({"schema_version":1,"cases":[{"workspace":fixture.workspace,"case_id":case_id}]})).unwrap()).unwrap();

    let mut expected: BTreeMap<String, String> = BTreeMap::new();
    let reads: Vec<&str> = match case_id {
        "DOC-followup-handoff-v3" => {
            expected.insert("handoff.md".into(), "# Structural feasibility fixture\n[Incident](incident/timeline.md), [recovery](operations/recovery.md), [shift](shift/notes.md).\nHuman handoff quality is not established by this test.\n".into());
            vec!["handoff.md"]
        }
        "DOC-followup-migration-v3" => {
            expected.insert("migration-notice.md".into(), "# Structural feasibility fixture\n[Decision](decisions/014-explicit-cache-root.md), [implementation](implementation/2.4.md), [checks](tests/checks.json).\nHuman migration guidance quality is not established by this test.\n".into());
            vec!["migration-notice.md"]
        }
        "SKL-followup-create-v3" => {
            expected.insert("package/SKILL.md".into(), "# Synthetic review fixture\nUse the [checklist](references/review-checklist.md) for local evidence. This is transport feasibility, not qualified guidance.\n".into());
            expected.insert("package/references/review-checklist.md".into(), "# Synthetic checklist\nInspect local evidence and keep unsupported claims explicit.\n".into());
            vec!["package/SKILL.md", "package/references/review-checklist.md"]
        }
        "SKL-followup-maintain-v3" => {
            expected.insert("package/references/planned-changes.md".into(), "# Planned changes\nRecord a named owner and an existing project-local issue link as checkpoint. Report unavailable issue evidence without creating issues, inventing dates or promising delivery.\n".into());
            vec![
                "package/SKILL.md",
                "package/references/delivered-changes.md",
                "package/references/planned-changes.md",
            ]
        }
        _ => unreachable!(),
    };
    let edits = expected
        .iter()
        .map(|(name, bytes)| (name.clone(), originals.get(name).cloned(), bytes.clone()))
        .collect::<Vec<_>>();
    let mut actions = VecDeque::from([Action::Patch(patch(&edits))]);
    actions.extend(reads.iter().map(|name| Action::Read((*name).into())));
    if case_id.starts_with("SKL-") {
        actions.push_back(Action::Descriptor);
        actions.push_back(Action::Read("package/skill.json".into()));
    }
    actions.push_back(Action::Verify);
    let operation_count = actions.len();
    let script = Arc::new(Mutex::new(Script {
        actions,
        pending: None,
        hashes: BTreeMap::new(),
        expected,
        original_descriptor: originals.get("package/skill.json").cloned(),
        read_evidence: BTreeMap::new(),
        verification_citations: None,
        refreshes: 0,
        verified: 0,
        requests: 0,
    }));
    let transport = script.clone();
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(move |request: &wiremock::Request| {
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(transport.lock().unwrap().next(request))
        })
        .mount(&server)
        .await;
    let mut profile: Value = serde_json::from_slice(&fs::read(&fixture.profile).unwrap()).unwrap();
    profile["affected_paths"] = json!(allowed);
    profile["max_requests"] = json!(16);
    profile["max_transport_retries"] = json!(0);
    profile["processes"] = json!([{"name":"authoring-followup-check","executable":checker,"environment":{"SystemRoot":std::env::var("SystemRoot").unwrap()},"required_isolation":[],"reduced_isolation":true,"inputs":[]}]);
    profile["checks"] = json!([{"manifest":"package.json","runner":"node","profile":"authoring-followup-check","timeout_ms":10000,"expected_tests":["authoring input preservation","authoring output structure"],"rationale":"Prospective native structural feasibility only; manual gates remain not_run"}]);
    fs::write(&fixture.profile, serde_json::to_vec(&profile).unwrap()).unwrap();
    let output = fixture
        .run(&[
            "run",
            case["prompt"].as_str().unwrap(),
            "--autonomy",
            "autonomous",
        ])
        .await;
    let records = records(&output);
    assert!(
        output.status.success(),
        "{case_id}: {:?} {}",
        records.last(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(records.last().unwrap()["conditions"]["completed"], true);
    let script = script.lock().unwrap();
    assert_eq!(script.verified, 1);
    assert_eq!(
        script.refreshes, 1,
        "the bounded scope refresh is exercised"
    );
    let citations = script
        .verification_citations
        .as_ref()
        .expect("verification dispatched with read evidence");
    assert!(!citations.is_empty());
    assert_eq!(
        citations.iter().collect::<BTreeSet<_>>().len(),
        citations.len()
    );
    assert_eq!(
        citations.iter().cloned().collect::<BTreeSet<_>>(),
        script.read_evidence.values().cloned().collect()
    );
    assert_eq!(script.requests, operation_count + script.refreshes + 1);
    assert!(script.requests <= 16);
    assert!(script.actions.is_empty() && script.pending.is_none());
    for (name, bytes) in &script.expected {
        assert_eq!(
            fs::read_to_string(fixture.workspace.join(name)).unwrap(),
            *bytes
        );
    }
    for (name, bytes) in &originals {
        if !allowed.contains(name) {
            assert_eq!(
                fs::read_to_string(fixture.workspace.join(name)).unwrap(),
                *bytes
            );
        }
    }
    for (name, bytes) in scaffold {
        assert_eq!(fs::read(fixture.workspace.join(name)).unwrap(), bytes);
    }
    println!(
        "{case_id}: {} synthetic requests, {} unexecuted refreshes, one checker; manual quality/authority gates not_run",
        script.requests, script.refreshes
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn handoff_native_feasibility() {
    feasible("DOC-followup-handoff-v3").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn migration_native_feasibility() {
    feasible("DOC-followup-migration-v3").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn create_native_feasibility_with_observed_hashes() {
    feasible("SKL-followup-create-v3").await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn maintain_native_feasibility_with_all_resource_hashes() {
    feasible("SKL-followup-maintain-v3").await;
}
