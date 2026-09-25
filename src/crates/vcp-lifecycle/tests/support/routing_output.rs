// SPDX-License-Identifier: Apache-2.0
//! Controlled peer evidence, never live model qualification.
use super::*;
use std::collections::BTreeSet;
use vcp_domain::policy::{Autonomy, EffectClass, Policy as AuthorityPolicy};
use vcp_lifecycle::foundation::{
    coding::{allowed_tools, CodingConfig},
    routing::Request,
    routing_state::{Edit, Preview},
};
use vcp_models::routing::Profile;
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

fn select_output(host: &CanonicalHost, tokens: u64) {
    select_edits(host, vec![Edit::OutputTokens(Some(Units::new(tokens)))]);
}
pub(super) fn select_edits(host: &CanonicalHost, selected: Vec<Edit>) {
    let report = host
        .routing_control(Request::Report {
            from: None,
            until: clock(),
        })
        .unwrap();
    let preview = host
        .routing_control(Request::Preview {
            report: report["report"]["id"].as_str().unwrap().into(),
            selected,
        })
        .unwrap();
    let preview: Preview = serde_json::from_value(preview).unwrap();
    host.routing_control(Request::Apply {
        command: CommandId::new(),
        preview: Box::new(preview),
    })
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn selected_output_matches_body_reservation_and_survives_policy_change_after_send() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for (selected, expected, change_after_send, small_input, full_input_bound) in [
            (256, 256, true, false, false),
            (4096, 1024, false, false, false),
            (256, 256, false, true, false),
            (256, 256, false, false, true),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let workspace = temp.path().join("workspace");
            std::fs::create_dir(&workspace).unwrap();
            let workspace = workspace.canonicalize().unwrap();
            std::fs::write(
                workspace.join("file.txt"),
                "Synthetic output-limit evidence.\n",
            )
            .unwrap();
            let config = config(&temp.path().join("canonical"), &workspace, backend);
            assert_eq!(config.output_ceiling, Units::new(1024));
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
                        timeout_ceiling_ms: Units::new(30_000),
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
            if full_input_bound {
                for candidate in &mut routing.catalog.entries {
                    let prior = candidate.snapshot.as_ref().unwrap();
                    let raw = routing.raw_catalogs.remove(&prior.id).unwrap();
                    let mut compatibility = prior.compatibility.clone();
                    compatibility.byte_ceiling_qualified = false;
                    let snapshot = vcp_models::catalog::Snapshot::from_endpoints(
                        raw.as_bytes(),
                        prior.observed_at,
                        prior.valid_until,
                        compatibility,
                    )
                    .unwrap();
                    routing.raw_catalogs.insert(snapshot.id.clone(), raw);
                    candidate.snapshot = Some(snapshot);
                }
                routing.catalog.id = routing.catalog.digest().unwrap();
            }
            host.configure_routing(routing).unwrap();
            select_output(&host, selected);
            if small_input {
                select_edits(&host, vec![Edit::InputTokens(Some(Units::new(1)))]);
            }
            let server = start_mock_server().await;
            let observed = Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
            let captured = observed.clone();
            let changing = host.clone();
            Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |request:&wiremock::Request| {
                let body:serde_json::Value=serde_json::from_slice(&request.body).unwrap();
                captured.lock().unwrap().push(body);
                // The peer has received the already admitted body. A new policy
                // cannot retroactively rewrite its reservation or charge evidence.
                if change_after_send { select_output(&changing,64); }
                ResponseTemplate::new(200).insert_header("content-type","text/event-stream").set_body_string(sse(vec![
                    ev_assistant_message("done","Observed synthetic fixture."),
                    serde_json::json!({"type":"response.completed","response":{"id":"output-response","status":"completed","output":[],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}})
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
                .with_auth(codex_login::CodexAuth::from_api_key("synthetic-output-key"))
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
                    canonical_tools: Default::default(),
                    operating: "Report the synthetic fixture.".into(),
                    affected_paths: vec!["file.txt".into()],
                    max_requests: 1,
                    deadline: Timestamp::new(clock().get() + 300_000),
                },
            )
            .unwrap();
            host.begin_coding_turn(id, "Observe the synthetic fixture.".into())
                .unwrap();
            test.codex
                .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                    text: "Observe the synthetic fixture.".into(),
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
            if small_input {
                assert!(bodies.is_empty());
                assert!(attempts.is_empty(), "input cap rejects before reservation");
                owner.close().await.unwrap();
                test.codex.shutdown_and_wait().await.unwrap();
                continue;
            }
            assert_eq!(bodies.len(), 1, "{backend:?}: {attempts:?}");
            assert_eq!(bodies[0]["max_output_tokens"], expected);
            assert_eq!(attempts.len(), 1);
            assert_eq!(attempts[0].quote.bounds.output, Units::new(expected));
            if full_input_bound {
                assert_eq!(attempts[0].quote.bounds.input, Units::new(720_000));
                assert_eq!(attempts[0].quote.bounds.cache_read, Units::new(240_000));
                assert_eq!(attempts[0].quote.bounds.cache_write, Units::new(240_000));
                assert!(serde_json::to_vec(&bodies[0]).unwrap().len() < 240_000);
            }
            assert_eq!(attempts[0].phase, ReservationState::Settled);
            assert_eq!(attempts[0].charged, Micros::new(100));
            let current = host.routing_control(Request::Status).unwrap();
            let policy: vcp_lifecycle::foundation::routing_state::Published<
                vcp_models::routing::Policy,
            > = serde_json::from_value(current["policy"].clone()).unwrap();
            assert_eq!(
                policy.value.output_tokens,
                Some(Units::new(if change_after_send { 64 } else { selected }))
            );
            let decision = state
                .records
                .values()
                .find(|r| r.value["document_type"] == "vcp_routing_decision_v1")
                .unwrap();
            let decision: vcp_models::routing::RoutingDecision =
                serde_json::from_value(decision.value["decision"].clone()).unwrap();
            assert_eq!(decision.input.output_tokens, Units::new(expected));
            owner.close().await.unwrap();
            test.codex.shutdown_and_wait().await.unwrap();
        }
    }
}
