// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

const SECRET: &str = "synthetic-wizard-secret";

struct Fixture {
    _temp: tempfile::TempDir,
    workspace: PathBuf,
    data: PathBuf,
    server: MockServer,
}

impl Fixture {
    async fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("project");
        std::fs::create_dir(&workspace).unwrap();
        std::fs::write(workspace.join("README.md"), "# Sample\n").unwrap();
        let data = temp.path().join("data");
        Self {
            workspace,
            data,
            server: MockServer::start().await,
            _temp: temp,
        }
    }

    fn context(&self) -> Context {
        Context {
            workspace: self.workspace.clone(),
            data_dir: Some(self.data.clone()),
            api: self.server.uri(),
            receipt_wait: std::time::Duration::from_millis(10),
            lookup: |_| None,
            credential_target: "VCP/OpenRouter/test-never-written".into(),
        }
    }

    /// The fixture endpoint catalog; reservations are $0.001 per request.
    async fn catalog(&self, expected: u64) {
        let catalog = json!({"data":{"id":"fixture/probe","endpoints":[{"tag":"fixture","provider_name":"Fixture Provider","model_id":"fixture/probe","name":"Fixture Provider | fixture/revision","status":0,"context_length":32000,"max_prompt_tokens":24000,"max_completion_tokens":8000,"supported_parameters":["tools","tool_choice","max_tokens"],"pricing":{"prompt":"0","completion":"0","request":"0.001"}}]}});
        Mock::given(method("GET"))
            .and(path("/models/fixture/probe/endpoints"))
            .respond_with(ResponseTemplate::new(200).set_body_json(catalog))
            .expect(expected)
            .mount(&self.server)
            .await;
    }

    async fn provider(&self, unavailable_receipts: usize) {
        Mock::given(method("POST")).and(path("/responses")).respond_with(|request: &wiremock::Request| {
            assert_eq!(request.headers["authorization"], format!("Bearer {SECRET}").as_str());
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let continuation = body["input"].as_array().unwrap().len() > 1;
            let output = if continuation {
                json!([{"type":"message","id":"final-message","role":"assistant","content":[{"type":"output_text","text":vcp_lifecycle::foundation::conformance::FINAL}]}])
            } else {
                json!([{"type":"function_call","id":"tool-item","call_id":"call-1","name":"vcp_conformance_echo","arguments":serde_json::to_string(&json!({"marker":vcp_lifecycle::foundation::conformance::MARKER})).unwrap()}])
            };
            ResponseTemplate::new(200).set_body_string(format!("data: {}\n\n", json!({"type":"response.completed","response":{"id":if continuation {"response-2"} else {"response-1"},"status":"completed","model":"fixture/probe","provider":"Fixture Provider","output":output,"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.000007}}})))
        }).expect(2).mount(&self.server).await;
        let misses = Arc::new(AtomicUsize::new(unavailable_receipts));
        Mock::given(method("GET")).and(path("/generation")).respond_with(move |request: &wiremock::Request| {
            if misses.load(Ordering::SeqCst) > 0 {
                misses.fetch_sub(1, Ordering::SeqCst);
                return ResponseTemplate::new(404);
            }
            let id = request.url.query_pairs().find(|(name, _)| name == "id").unwrap().1.into_owned();
            ResponseTemplate::new(200).set_body_json(json!({"data":{"id":id,"model":"fixture/revision","provider_name":"Fixture Provider","cancelled":false,"streamed":true,"is_byok":false,"total_cost":0.000007,"provider_responses":[{"endpoint_id":"internal-endpoint","provider_name":"Fixture Provider","model_permaslug":"fixture/revision","status":200,"is_byok":false}]}}))
        }).mount(&self.server).await;
    }

    async fn posts(&self) -> usize {
        self.server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| request.method == wiremock::http::Method::POST)
            .count()
    }
}

fn custom() -> usize {
    model_sets::catalog().unwrap().len() + 1
}

fn no_secret_anywhere(root: &Path) {
    for entry in walk(root) {
        if let Ok(bytes) = std::fs::read(&entry) {
            assert!(
                !String::from_utf8_lossy(&bytes).contains(SECRET),
                "{} contains the key",
                entry.display()
            );
        }
    }
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(walk(&path));
            } else {
                files.push(path);
            }
        }
    }
    files
}

#[tokio::test]
async fn guided_setup_verifies_creates_and_selects_a_profile() {
    let fixture = Fixture::new().await;
    fixture.catalog(2).await;
    fixture.provider(0).await;
    let mut prompts = ScriptedPrompter::new([
        "y".to_owned(),
        SECRET.into(),
        "n".into(),
        custom().to_string(),
        "fixture/probe".into(),
        "fixture".into(),
        "0.01".into(),
        "yes".into(),
        "1".into(),
        "n".into(),
    ]);
    let outcome = run(&fixture.context(), &mut prompts).await.unwrap();
    let transcript = prompts.text();
    assert_eq!(outcome.value["status"], "ready", "{transcript}");
    assert_eq!(outcome.exit_code, 0);
    assert!(outcome.smoke_task.is_none());
    assert!(transcript.contains("Step 7/7"), "{transcript}");
    assert!(!transcript.contains(SECRET), "{transcript}");
    assert_eq!(fixture.posts().await, 2);
    let workspace = fixture.workspace.canonicalize().unwrap();
    let selected = profile_selection::resolve(&fixture.data, &workspace, None).unwrap();
    assert_eq!(selected.source, profile_selection::Source::Selected);
    assert!(onboarding::check(&selected.path, &workspace, true).is_ok());
    no_secret_anywhere(&fixture.data);
}

#[tokio::test]
async fn delayed_receipts_are_completed_without_repeating_requests() {
    let fixture = Fixture::new().await;
    fixture.catalog(2).await;
    fixture.provider(3).await;
    let mut prompts = ScriptedPrompter::new([
        "y".to_owned(),
        SECRET.into(),
        "n".into(),
        custom().to_string(),
        "fixture/probe".into(),
        "fixture".into(),
        "0.01".into(),
        "yes".into(),
        "1".into(),
        "n".into(),
    ]);
    let outcome = run(&fixture.context(), &mut prompts).await.unwrap();
    assert_eq!(outcome.value["status"], "ready", "{}", prompts.text());
    assert!(prompts.text().contains("without repeating any request"));
    assert_eq!(fixture.posts().await, 2);
}

#[tokio::test]
async fn declining_or_underfunding_spends_nothing() {
    for (cap, confirm) in [("0.01", "no"), ("0.0001", ""), ("26", "")] {
        let fixture = Fixture::new().await;
        fixture.catalog(1).await;
        let mut prompts = ScriptedPrompter::new([
            "y".to_owned(),
            SECRET.into(),
            "n".into(),
            custom().to_string(),
            "fixture/probe".into(),
            "fixture".into(),
            cap.into(),
            confirm.into(),
        ]);
        let outcome = run(&fixture.context(), &mut prompts).await.unwrap();
        let transcript = prompts.text();
        assert_eq!(outcome.value["status"], "stopped", "{transcript}");
        assert_eq!(outcome.exit_code, 2);
        assert_eq!(fixture.posts().await, 0);
        assert!(!fixture.data.join("providers").exists());
        if cap == "0.0001" {
            assert!(transcript.contains("below the minimum"), "{transcript}");
        }
        if cap == "26" {
            assert!(transcript.contains("above the maximum"), "{transcript}");
        }
    }
}

#[tokio::test]
async fn unusable_workspaces_and_multi_model_sets_are_explained() {
    let fixture = Fixture::new().await;
    // The data folder inside the workspace makes it unusable until another
    // folder is chosen; the replacement is then not trusted.
    let mut context = fixture.context();
    context.data_dir = Some(fixture.workspace.join("data"));
    let other = fixture.workspace.parent().unwrap().join("other");
    std::fs::create_dir(&other).unwrap();
    let mut prompts = ScriptedPrompter::new([other.to_str().unwrap().to_owned(), "n".into()]);
    let outcome = run(&context, &mut prompts).await.unwrap();
    let transcript = prompts.text();
    assert!(
        transcript.contains("This folder can't be used"),
        "{transcript}"
    );
    assert_eq!(outcome.value["reason"], "the workspace was not trusted");

    let mut prompts =
        ScriptedPrompter::new(["y".to_owned(), SECRET.into(), "n".into(), "2".into()]);
    let outcome = run(&fixture.context(), &mut prompts).await.unwrap();
    let transcript = prompts.text();
    assert!(
        transcript.contains("needs per-role assignment"),
        "{transcript}"
    );
    assert_eq!(outcome.value["step"], "model_set");
    assert_eq!(fixture.posts().await, 0);
    assert!(!transcript.contains(SECRET));
}

#[tokio::test]
async fn a_missing_key_or_end_of_input_stops_cleanly() {
    let fixture = Fixture::new().await;
    let mut prompts = ScriptedPrompter::new(["y".to_owned(), "".into()]);
    let outcome = run(&fixture.context(), &mut prompts).await.unwrap();
    assert_eq!(outcome.value["step"], "credential");
    let mut prompts = ScriptedPrompter::new(Vec::<String>::new());
    let outcome = run(&fixture.context(), &mut prompts).await.unwrap();
    assert_eq!(outcome.value["reason"], "the workspace was not trusted");
    assert!(!fixture.data.exists() || walk(&fixture.data).is_empty());
}
