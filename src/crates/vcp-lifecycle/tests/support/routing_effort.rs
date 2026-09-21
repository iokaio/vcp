// SPDX-License-Identifier: Apache-2.0
//! Synthetic endpoint effort conformance, not live model qualification.
use super::*;
use std::collections::BTreeSet;
use vcp_domain::policy::{Autonomy, EffectClass, Policy as AuthorityPolicy};
use vcp_lifecycle::foundation::{
    coding::{allowed_tools, CodingConfig},
    routing::Request,
    routing_state::{Edit, Preview},
};
use vcp_models::{
    catalog::Snapshot,
    reasoning::Effort,
    routing::{CatalogRevision, Profile},
};
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

fn clock() -> Timestamp {
    Timestamp::new(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
    )
}
fn select_effort(host: &CanonicalHost, effort: Effort) -> Preview {
    let report = host
        .routing_control(Request::Report {
            from: None,
            until: clock(),
        })
        .unwrap();
    let preview: Preview = serde_json::from_value(
        host.routing_control(Request::Preview {
            report: report["report"]["id"].as_str().unwrap().into(),
            selected: vec![Edit::ReasoningEffort(Some(effort))],
        })
        .unwrap(),
    )
    .unwrap();
    host.routing_control(Request::Apply {
        command: CommandId::new(),
        preview: Box::new(preview.clone()),
    })
    .unwrap();
    preview
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn effort_is_qualified_selected_sealed_and_bounded_without_changing_output_reservation() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for mode in ["bounded", "disabled", "unqualified"] {
            let temp = tempfile::tempdir().unwrap();
            let workspace = temp.path().join("workspace");
            std::fs::create_dir(&workspace).unwrap();
            let workspace = workspace.canonicalize().unwrap();
            std::fs::write(workspace.join("file.txt"), "Synthetic effort fixture.\n").unwrap();
            let config = config(&temp.path().join("canonical"), &workspace, backend);
            let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
            let binding = task(&host, &config, config.root_task.clone(), None);
            host.command(
                Command::SetWorkspaceTrust {
                    trust: Trust::Trusted,
                },
                None,
                Revision::ZERO,
            )
            .unwrap();
            host.command(
                Command::SetPolicy {
                    policy: AuthorityPolicy {
                        workspace: config.workspace.clone(),
                        revision: PolicyRevision::ZERO,
                        mode: Autonomy::Autonomous,
                        denials: vec![],
                        workspace_roots: BTreeSet::from([
                            RootId::parse(config.workspace.as_str()).unwrap()
                        ]),
                        automatic_effects: BTreeSet::from([
                            EffectClass::Read,
                            EffectClass::Write,
                            EffectClass::Execute,
                            EffectClass::Network,
                            EffectClass::Install,
                            EffectClass::Publish,
                            EffectClass::Opaque,
                        ]),
                        timeout_ceiling_ms: Units::new(30000),
                        output_ceiling_bytes: ByteCount::new(1024 * 1024),
                    },
                },
                None,
                Revision::ZERO,
            )
            .unwrap();
            let (snapshot, raw) = provider_snapshot();
            host.configure_provider(snapshot, raw).unwrap();
            let mut routing = super::routing::routing_configuration(Profile::Low, false);
            let candidate = &mut routing.catalog.entries[1];
            let previous = candidate.snapshot.as_ref().unwrap();
            let mut source: serde_json::Value =
                serde_json::from_str(&routing.raw_catalogs[&previous.id]).unwrap();
            source["data"]["endpoints"][0]["supported_parameters"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!("reasoning"));
            let raw = serde_json::to_vec(&source).unwrap();
            let mut compatibility = previous.compatibility.clone();
            compatibility.id = "synthetic-reasoning-medium/1".into();
            compatibility.required_parameters.insert("reasoning".into());
            compatibility.qualified_reasoning_efforts = BTreeSet::from([Effort::Medium]);
            let snapshot = Snapshot::from_endpoints(
                &raw,
                previous.observed_at,
                previous.valid_until,
                compatibility.clone(),
            )
            .unwrap();
            routing.raw_catalogs.remove(&previous.id);
            routing
                .raw_catalogs
                .insert(snapshot.id.clone(), String::from_utf8(raw).unwrap());
            candidate.snapshot = Some(snapshot);
            candidate.compatibility[0].compatibility = compatibility.id;
            routing.catalog = CatalogRevision::create(
                routing.catalog.parent,
                routing.catalog.observed_at,
                routing.catalog.effective_at,
                routing.catalog.entries,
            )
            .unwrap();
            routing.policy.reasoning_effort = (mode != "disabled").then_some(Effort::Medium);
            routing.policy = routing.policy.seal().unwrap();
            host.configure_routing(routing).unwrap();
            let preview = select_effort(
                &host,
                if mode == "unqualified" {
                    Effort::Low
                } else {
                    Effort::High
                },
            );
            assert_eq!(
                preview.effective.reasoning_effort,
                match mode {
                    "disabled" => None,
                    "unqualified" => Some(Effort::Low),
                    _ => Some(Effort::Medium),
                }
            );
            let server = start_mock_server().await;
            let observed = Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
            let captured = observed.clone();
            let changing = host.clone();
            Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |request: &wiremock::Request| {
                let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                let charge = if body["model"] == "fixture/stronger" { 0.0002 } else { 0.0001 };
                captured.lock().unwrap().push(body);
                if mode == "bounded" { select_effort(&changing, Effort::Minimal); }
                ResponseTemplate::new(200).insert_header("content-type", "text/event-stream").set_body_string(sse(vec![
                    ev_assistant_message("done", "Observed synthetic effort."),
                    serde_json::json!({"type":"response.completed","response":{"id":"effort-response","status":"completed","output":[],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":charge}}}),
                ]))
            }).mount(&server).await;
            let mut registry = ExtensionRegistryBuilder::new();
            registry.turn_start_admission(Arc::new(host.clone()));
            registry.work_admission(Arc::new(host.clone()));
            registry.tool_contributor(Arc::new(host.clone()));
            let starter = host.clone();
            let cwd = workspace.clone();
            let test = test_codex()
                .with_extensions(Arc::new(registry.build()))
                .with_auth(codex_login::CodexAuth::from_api_key("synthetic-effort-key"))
                .with_allowed_tools(allowed_tools())
                .with_config(move |c| {
                    c.cwd = cwd.try_into().unwrap();
                    configure_provider_fixture(c);
                    starter
                        .lifecycle()
                        .authorize_startup(c.cwd.as_path(), None)
                        .unwrap();
                })
                .build_with_auto_env(&server)
                .await
                .unwrap();
            let id = host.lifecycle().attach_root(test.codex.clone()).unwrap();
            host.register(id, binding).unwrap();
            host.configure_coding(
                id,
                CodingConfig {
                    operating: "Report the synthetic fixture.".into(),
                    affected_paths: vec!["file.txt".into()],
                    max_requests: 1,
                    deadline: Timestamp::new(clock().get() + 300000),
                },
            )
            .unwrap();
            host.begin_coding_turn(id, "Observe synthetic effort.".into())
                .unwrap();
            test.codex
                .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                    text: "Observe synthetic effort.".into(),
                    text_elements: vec![],
                }]))
                .await
                .unwrap();
            tokio::time::timeout(Duration::from_secs(120), async {
                loop {
                    if matches!(
                        test.codex.next_event().await.unwrap().msg,
                        EventMsg::TurnComplete(_)
                    ) {
                        break;
                    }
                }
            })
            .await
            .unwrap();
            let bodies = observed.lock().unwrap().clone();
            let state = host.snapshot().unwrap();
            let attempts: Vec<Attempt> = state
                .records
                .values()
                .filter(|r| r.collection == Collection::Attempt)
                .map(|r| r.decode().unwrap())
                .collect();
            if mode == "unqualified" {
                assert!(bodies.is_empty() && attempts.is_empty());
            } else {
                assert_eq!(bodies.len(), 1, "{backend:?}/{mode}: {attempts:?}");
                assert_eq!(attempts.len(), 1);
                assert_eq!(bodies[0]["max_output_tokens"], 1024);
                assert_eq!(attempts[0].quote.bounds.output, Units::new(1024));
                assert_eq!(attempts[0].phase, ReservationState::Settled);
                if mode == "bounded" {
                    assert_eq!(
                        bodies[0]["reasoning"],
                        serde_json::json!({"effort":"medium"})
                    );
                    assert_eq!(attempts[0].quote.price.model, "fixture/stronger");
                    assert_eq!(attempts[0].charged, Micros::new(200));
                } else {
                    assert!(bodies[0].get("reasoning").is_none());
                    assert_eq!(attempts[0].quote.price.model, "fixture/economical");
                }
            }
            owner.close().await.unwrap();
            test.codex.shutdown_and_wait().await.unwrap();
        }
    }
}
