// SPDX-License-Identifier: Apache-2.0
//! Scripted transport evidence only. Synthetic `Live` labels exercise admission;
//! no fixture in this module qualifies a real endpoint or public default.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::policy::{Autonomy, EffectClass, Policy as AuthorityPolicy};
use vcp_lifecycle::foundation::{
    coding::{allowed_tools, CodingConfig},
    routing::Configuration,
};
use vcp_models::{catalog::Snapshot, routing::*};
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

pub(super) fn routing_configuration(profile: Profile, forbidden: bool) -> Configuration {
    let observed = clock();
    let valid_until = Timestamp::new(observed.get() + 600_000);
    let provenance = vec![Provenance {
        source: "fixture://retained-routing-scripted".into(), sha256: "a".repeat(64),
        observed_at: observed, effective_at: None,
        limitations: vec!["Synthetic Live labels exercise admission only; scripted evidence does not qualify a real model".into()],
    }];
    let mut raw_catalogs = BTreeMap::new();
    let mut entries = Vec::new();
    for (name, group, main_quality, helper_quality, price) in [
        ("economical", Group::Low, 8500, 9700, "0.0001"),
        ("stronger", Group::High, 9700, 9000, "0.0002"),
    ] {
        let model = format!("fixture/{name}");
        let endpoint = format!("fixture/{name}-region");
        let (base, _) = provider_snapshot();
        let mut compatibility = base.compatibility;
        compatibility.id = format!("synthetic-{name}/1");
        compatibility.model = model.clone();
        compatibility.endpoint = endpoint.clone();
        compatibility.qualified_at = observed;
        compatibility.valid_until = valid_until;
        compatibility.request_price_limit = price.into();
        let raw = serde_json::to_vec(&serde_json::json!({"data":{"id":model,"endpoints":[{"tag":endpoint,"status":0,"context_length":256000,"max_prompt_tokens":240000,"max_completion_tokens":8000,"supported_parameters":["tools","max_tokens"],"pricing":{"prompt":"0","completion":"0","request":price}}]}})).unwrap();
        let snapshot =
            Snapshot::from_endpoints(&raw, observed, valid_until, compatibility).unwrap();
        raw_catalogs.insert(snapshot.id.clone(), String::from_utf8(raw).unwrap());
        entries.push(Candidate {
            identity: ModelEndpoint { model, endpoint },
            availability: State::Supported,
            reasons: vec![],
            provenance: provenance.clone(),
            capabilities: BTreeMap::from([("responses_text_tools".into(), State::Supported)]),
            compatibility: vec![CompatibilityObservation {
                id: format!("synthetic-{name}-probe"),
                compatibility: snapshot.compatibility.id.clone(),
                kind: EvidenceKind::Live,
                state: State::Supported,
                observed_at: observed,
                valid_until,
                provenance: provenance.clone(),
            }],
            snapshot: Some(snapshot),
            memberships: vec![GroupMembership {
                version: format!("synthetic-{name}-membership/1"),
                group,
                roles: [
                    (RequestRole::Main, main_quality),
                    (RequestRole::Helper, helper_quality),
                ]
                .into_iter()
                .map(|(role, quality_bps)| RoleEvidence {
                    id: format!("synthetic-{name}-{role:?}"),
                    role,
                    task_class: "coding".into(),
                    kind: EvidenceKind::Live,
                    observed_at: observed,
                    valid_until,
                    samples: 100,
                    quality_bps,
                    latency_p50_ms: 5,
                    latency_p95_ms: 10,
                    usage_p50: None,
                    usage_p95: None,
                    provenance: provenance.clone(),
                })
                .collect(),
            }],
        });
    }
    let policy = vcp_models::routing::Policy {
        schema_version: 1,
        id: String::new(),
        parent: None,
        profile,
        allowed_models: entries.iter().map(|e| e.identity.model.clone()).collect(),
        allowed_endpoints: if forbidden {
            BTreeSet::from(["fixture/forbidden".into()])
        } else {
            entries
                .iter()
                .map(|e| e.identity.endpoint.clone())
                .collect()
        },
        allowed_groups: BTreeSet::from([Group::Low, Group::High]),
        quality_floor_bps: 8000,
        minimum_samples: 20,
        maximum_evidence_age_ms: 600_000,
        deny_data_collection: true,
        require_zdr: true,
        ordering: if profile == Profile::High {
            vec![
                Preference::Quality,
                Preference::TotalCost,
                Preference::Latency,
                Preference::Capability,
            ]
        } else {
            vec![
                Preference::TotalCost,
                Preference::Quality,
                Preference::Latency,
                Preference::Capability,
            ]
        },
        pin: None,
        broader_task_class: None,
        output_tokens: None,
        input_tokens: None,
        escalation_limits: None,
        reasoning_effort: None,
        retrieval_limits: None,
    }
    .seal()
    .unwrap();
    let zero = Money {
        currency: "USD".to_string().try_into().unwrap(),
        micros: Micros::ZERO,
    };
    let estimates = entries.iter().map(|e| CostEstimate {
        candidate: e.identity.clone(), first_attempt: Usage {input: Units::new(200_000), output: Units::new(1024), requests: Units::new(1), ..Usage::default()}, retries: Usage::default(), handoff: Usage::default(),
        support: Some(zero.clone()), children: Some(zero.clone()), verification: Some(zero.clone()),
        assumptions: vec!["Single scripted answer, no tools, retries, children or additional verification calls".into()], evidence_refs: vec!["fixture://retained-routing-scripted".into()],
    }).collect();
    Configuration {
        escalation: None,
        catalog: CatalogRevision::create(None, observed, None, entries).unwrap(),
        policy,
        task_class: "coding".into(),
        estimates,
        raw_catalogs,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn retained_routing_selects_admits_and_sends_the_same_model_and_price() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for (mode, profile, role, expected, price) in [
            (
                "low-main",
                Profile::Low,
                RequestRole::Main,
                "fixture/economical",
                100,
            ),
            (
                "high-main",
                Profile::High,
                RequestRole::Main,
                "fixture/stronger",
                200,
            ),
            (
                "high-helper",
                Profile::High,
                RequestRole::Helper,
                "fixture/economical",
                100,
            ),
            ("forbidden", Profile::Low, RequestRole::Main, "", 0),
            ("fixed", Profile::Low, RequestRole::Main, "gpt-5.1", 100),
            (
                "retry-zero",
                Profile::Low,
                RequestRole::Main,
                "fixture/economical",
                100,
            ),
            (
                "escalate",
                Profile::Low,
                RequestRole::Main,
                "fixture/stronger",
                200,
            ),
            (
                "escalation-cap",
                Profile::Low,
                RequestRole::Main,
                "fixture/economical",
                100,
            ),
            (
                "escalation-selected-cap",
                Profile::Low,
                RequestRole::Main,
                "fixture/economical",
                100,
            ),
            (
                "escalation-pin",
                Profile::Low,
                RequestRole::Main,
                "fixture/economical",
                100,
            ),
            (
                "escalate-owner-complexity",
                Profile::Low,
                RequestRole::Main,
                "fixture/stronger",
                200,
            ),
            (
                "escalate-owner-capability",
                Profile::Low,
                RequestRole::Main,
                "fixture/stronger",
                200,
            ),
            (
                "escalation-owner-pin",
                Profile::Low,
                RequestRole::Main,
                "fixture/economical",
                100,
            ),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let workspace = temp.path().join("workspace");
            std::fs::create_dir(&workspace).unwrap();
            let workspace = workspace.canonicalize().unwrap();
            std::fs::write(workspace.join("file.txt"), "Synthetic routing evidence.\n").unwrap();
            let config = config(&temp.path().join("canonical"), &workspace, backend);
            let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
            let mut binding = task(&host, &config, config.root_task.clone(), None);
            binding.role = role;
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
            if mode != "fixed" {
                let mut routing = routing_configuration(profile, mode == "forbidden");
                if mode.starts_with("escalat") || mode == "retry-zero" {
                    routing.escalation = Some(vcp_models::escalation::Policy {
                        max_transport_retries: if mode == "retry-zero" { 0 } else { 2 },
                        max_quality_switches: if mode == "escalation-cap" { 0 } else { 1 },
                        max_decompositions: 0,
                        max_total_attempts: 3,
                        minimum_repeated_failures: 1,
                        deadline: Timestamp::new(clock().get() + 300_000),
                    });
                    if mode == "escalation-pin" || mode == "escalation-owner-pin" {
                        routing.policy.pin = Some(Pin {
                            candidate: routing.catalog.entries[0].identity.clone(),
                            fallback_candidates: BTreeSet::new(),
                        });
                        routing.policy = routing.policy.seal().unwrap();
                    }
                    if mode == "escalate-owner-capability" {
                        routing.catalog.entries[1]
                            .capabilities
                            .insert("owner_required".into(), State::Supported);
                        // A cheaper fallback without the owner's capability
                        // must remain excluded on the following request too.
                        let mut alternate = routing.catalog.entries[0].clone();
                        let prior = alternate.snapshot.as_ref().unwrap();
                        let raw = routing.raw_catalogs[&prior.id]
                            .replace("fixture/economical", "fixture/alternate")
                            .replace("0.0001", "0.00015");
                        let mut compatibility = prior.compatibility.clone();
                        compatibility.id = "synthetic-alternate/1".into();
                        compatibility.model = "fixture/alternate".into();
                        compatibility.endpoint = "fixture/alternate-region".into();
                        compatibility.request_price_limit = "0.00015".into();
                        let snapshot = Snapshot::from_endpoints(
                            raw.as_bytes(),
                            prior.observed_at,
                            prior.valid_until,
                            compatibility.clone(),
                        )
                        .unwrap();
                        alternate.identity = ModelEndpoint {
                            model: compatibility.model,
                            endpoint: compatibility.endpoint,
                        };
                        alternate.compatibility[0].compatibility = compatibility.id;
                        routing.raw_catalogs.insert(snapshot.id.clone(), raw);
                        alternate.snapshot = Some(snapshot);
                        routing
                            .policy
                            .allowed_models
                            .insert(alternate.identity.model.clone());
                        routing
                            .policy
                            .allowed_endpoints
                            .insert(alternate.identity.endpoint.clone());
                        routing.policy = routing.policy.seal().unwrap();
                        let mut estimate = routing.estimates[0].clone();
                        estimate.candidate = alternate.identity.clone();
                        routing.estimates.push(estimate);
                        routing.catalog.entries.push(alternate);
                        routing.catalog = CatalogRevision::create(
                            routing.catalog.parent,
                            routing.catalog.observed_at,
                            routing.catalog.effective_at,
                            routing.catalog.entries,
                        )
                        .unwrap();
                    }
                }
                host.configure_routing(routing).unwrap();
                if mode == "escalation-selected-cap" {
                    super::routing_output::select_edits(&host, vec![vcp_lifecycle::foundation::routing_state::Edit::EscalationMaxQualitySwitches(Some(0))]);
                }
            }
            let server = start_mock_server().await;
            let observed = Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
            let captured = observed.clone();
            let declaring_host = mode.contains("owner").then(|| host.clone());
            let declaration_scope = binding.scope.clone();
            Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |request: &wiremock::Request| {
                let body:serde_json::Value=serde_json::from_slice(&request.body).unwrap();
                let mut observations=captured.lock().unwrap();
                let index=observations.len();
                let charged=if body["model"]=="fixture/stronger" {200}else{100};
                observations.push(body);
                if mode=="retry-zero" {return ResponseTemplate::new(503).insert_header("retry-after","0").set_body_string("scripted transient failure");}
                let mut events=Vec::new();let mut output=Vec::new();
                if (mode.starts_with("escalat") && index==0) || (mode == "escalate-owner-capability" && index == 1) {
                    if mode.contains("owner") && index == 0 {
                        let declaring_host = declaring_host.as_ref().unwrap();
                        use vcp_lifecycle::foundation::{routing::Request, routing_state::declarations::{Input, Kind}};
                        let state = declaring_host.snapshot().unwrap();
                        let task: Task = state.record(Collection::Task, declaration_scope.task.as_str(), &declaration_scope.workspace).unwrap().decode().unwrap();
                        let attempt: Attempt = state.records.values().find(|r| r.collection == Collection::Attempt).unwrap().decode().unwrap();
                        let declaration = Input { command: CommandId::new(), task: task.scope.task.clone(), expected_revision: task.revision,
                            steering: task.steering, declaration: if mode == "escalate-owner-capability" { Kind::UnsupportedCapability { capability: "owner_required".into() } } else { Kind::DeclaredComplexity }, evidence: vec![attempt.request] };
                        declaring_host.routing_control(Request::DeclareEscalation { declaration }).unwrap();
                    }
                    let arguments = serde_json::json!({"path": if mode.contains("owner") { "file.txt" } else { "missing-file.txt" }, "max_bytes":1024,"start_line":null,"end_line":null}).to_string();
                    let call_id = format!("routing-read-{index}");
                    let call=serde_json::json!({"type":"function_call","id":call_id,"call_id":call_id,"name":"vcp_read","arguments":arguments,"status":"completed"});
                    events.push(serde_json::json!({"type":"response.output_item.done","output_index":0,"item":call}));output.push(call);
                } else {events.push(ev_assistant_message("done", "Observed synthetic routing evidence."));}
                events.push(serde_json::json!({"type":"response.completed","response":{"id":format!("routing-response-{index}"),"status":"completed","output":output,"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":charged as f64 / 1_000_000.0}}}));
                ResponseTemplate::new(200).insert_header("content-type", "text/event-stream").set_body_string(sse(events))
            }).mount(&server).await;
            let mut registry = ExtensionRegistryBuilder::new();
            registry.turn_start_admission(Arc::new(host.clone()));
            registry.work_admission(Arc::new(host.clone()));
            registry.tool_contributor(Arc::new(host.clone()));
            let starter = host.clone();
            let cwd = workspace.clone();
            let test = test_codex()
                .with_extensions(Arc::new(registry.build()))
                .with_auth(codex_login::CodexAuth::from_api_key(
                    "synthetic-routing-key",
                ))
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
            host.register(id, binding.clone()).unwrap();
            host.configure_coding(
                id,
                CodingConfig {
                    canonical_tools: Default::default(),
                    operating: "Report the observed fixture only.".into(),
                    affected_paths: vec!["file.txt".into()],
                    max_requests: if mode.starts_with("escalat") || mode == "retry-zero" {
                        3
                    } else {
                        1
                    },
                    deadline: Timestamp::new(clock().get() + 300_000),
                },
            )
            .unwrap();
            let turn = host
                .begin_coding_turn(id, "Observe the synthetic fixture.".into())
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
            let state = host.snapshot().unwrap();
            let attempts: Vec<Attempt> = state
                .records
                .values()
                .filter(|r| r.collection == Collection::Attempt)
                .map(|r| r.decode().unwrap())
                .collect();
            let decisions: Vec<_> = state
                .records
                .values()
                .filter(|r| r.value["document_type"] == "vcp_routing_decision_v1")
                .collect();
            let bodies = observed.lock().unwrap().clone();
            if mode == "forbidden" {
                assert!(bodies.is_empty());
                assert!(attempts.is_empty());
                assert!(decisions.is_empty());
                let turn: Turn = state
                    .record(Collection::Turn, turn.as_str(), &config.workspace)
                    .unwrap()
                    .decode()
                    .unwrap();
                assert_eq!(turn.state, TurnState::Paused);
            } else if mode.starts_with("escalat") {
                let switched = mode == "escalate" || mode.starts_with("escalate-owner");
                assert_eq!(
                    bodies.len(),
                    if mode == "escalate-owner-capability" {
                        3
                    } else if switched {
                        2
                    } else {
                        1
                    },
                    "{backend:?}/{mode}: {attempts:?}"
                );
                assert_eq!(attempts.len(), bodies.len());
                assert_eq!(bodies[0]["model"], "fixture/economical");
                let first = attempts
                    .iter()
                    .find(|a| a.quote.price.model == "fixture/economical")
                    .unwrap();
                assert_eq!(
                    first.phase,
                    ReservationState::Settled,
                    "the valid response must settle before its failed read can trigger escalation"
                );
                assert_eq!(first.charged, Micros::new(100));
                assert_eq!(bodies.last().unwrap()["model"], expected);
                let escalations: Vec<_> = state
                    .records
                    .values()
                    .filter(|r| r.value["document_type"] == "vcp_escalation_admission_v1")
                    .collect();
                assert_eq!(escalations.len(), usize::from(switched));
                if switched {
                    let record: &serde_json::Value = &escalations[0].value;
                    let plan: vcp_models::escalation::Plan =
                        serde_json::from_value(record["plan"].clone()).unwrap();
                    if mode.contains("owner") {
                        assert_eq!(
                            plan.trigger.kind,
                            if mode == "escalate-owner-capability" {
                                vcp_models::escalation::TriggerKind::UnsupportedCapability
                            } else {
                                vcp_models::escalation::TriggerKind::DeclaredComplexity
                            }
                        );
                        if mode == "escalate-owner-capability" {
                            assert_eq!(bodies[2]["model"], "fixture/stronger");
                            assert_eq!(
                                decisions
                                    .iter()
                                    .filter(|row| row.value["decision"]["input"]
                                        ["required_capabilities"]
                                        .as_array()
                                        .unwrap()
                                        .iter()
                                        .any(|capability| capability == "owner_required"))
                                    .count(),
                                2
                            );
                            let selected: RoutingDecision = decisions
                                .iter()
                                .map(|row| {
                                    serde_json::from_value::<RoutingDecision>(
                                        row.value["decision"].clone(),
                                    )
                                    .unwrap()
                                })
                                .find(|decision| {
                                    decision.selected.as_ref().is_some_and(|candidate| {
                                        candidate.model == "fixture/stronger"
                                    })
                                })
                                .unwrap();
                            assert!(selected
                                .input
                                .required_capabilities
                                .contains("owner_required"));
                            assert!(selected
                                .candidates
                                .iter()
                                .find(|candidate| candidate.identity.model == "fixture/economical")
                                .unwrap()
                                .exclusions
                                .contains(&Exclusion::UnknownCapability));
                        }
                    }
                    assert_eq!(plan.before.total_attempts, 1);
                    assert_eq!(plan.after.total_attempts, 2);
                    assert_eq!(plan.after.quality_switches, 1);
                    assert_eq!(plan.selected.model, "fixture/stronger");
                    let prior = attempts
                        .iter()
                        .find(|a| a.id == plan.previous_attempt)
                        .unwrap();
                    assert_eq!(prior.quote.price.model, "fixture/economical");
                    let next = attempts
                        .iter()
                        .find(|a| record["attempt"] == a.id.as_str())
                        .unwrap();
                    assert_eq!(next.quote.amount.micros.get(), 200);
                    assert_eq!(
                        record["handoff"]["destination_request_sha256"],
                        next.request_digest
                    );
                } else {
                    let stopped: Turn = state
                        .record(Collection::Turn, turn.as_str(), &config.workspace)
                        .unwrap()
                        .decode()
                        .unwrap();
                    assert_eq!(stopped.state, TurnState::Paused);
                }
            } else {
                assert_eq!(bodies.len(), 1, "{backend:?}/{mode}: {attempts:?}");
                assert_eq!(attempts.len(), 1);
                assert_eq!(bodies[0]["model"], expected);
                assert_eq!(attempts[0].quote.price.model, expected);
                assert_eq!(attempts[0].quote.amount.micros.get(), price);
                assert_eq!(attempts[0].role, role);
                if mode == "fixed" {
                    assert!(decisions.is_empty());
                } else {
                    assert_eq!(decisions.len(), 1);
                    let decision: RoutingDecision =
                        serde_json::from_value(decisions[0].value["decision"].clone()).unwrap();
                    decision.validate().unwrap();
                    let selected = decision.selected.as_ref().unwrap();
                    assert_eq!(selected.model, expected);
                    assert_eq!(attempts[0].quote.price.provider, selected.endpoint);
                    assert_eq!(
                        bodies[0]["provider"]["only"],
                        serde_json::json!([selected.endpoint])
                    );
                    assert_eq!(decision.profile, profile);
                    assert_eq!(decision.input.role, role);
                    let winner = decision
                        .candidates
                        .iter()
                        .find(|candidate| candidate.identity == *selected)
                        .unwrap();
                    assert!(winner.exclusions.is_empty());
                    assert_eq!(
                        winner.group,
                        Some(if expected == "fixture/stronger" {
                            Group::High
                        } else {
                            Group::Low
                        })
                    );
                    assert_eq!(
                        decisions[0].value["request_digest"],
                        attempts[0].request_digest
                    );
                    assert_eq!(decisions[0].value["attempt"], attempts[0].id.as_str());
                }
            }
            owner.close().await.unwrap();
            test.codex.shutdown_and_wait().await.unwrap();
            drop(test);
            drop(host);
            if mode == "escalate" {
                let (reopened, owner) = CanonicalHost::open(config.clone()).unwrap();
                let restored = reopened.snapshot().unwrap();
                let records: Vec<_> = restored
                    .records
                    .values()
                    .filter(|r| r.value["document_type"] == "vcp_escalation_admission_v1")
                    .collect();
                assert_eq!(records.len(), 1);
                assert_eq!(records[0].value["plan"]["after"]["quality_switches"], 1);
                let (snapshot, raw) = provider_snapshot();
                reopened.configure_provider(snapshot, raw).unwrap();
                let mut registry = ExtensionRegistryBuilder::new();
                registry.turn_start_admission(Arc::new(reopened.clone()));
                registry.work_admission(Arc::new(reopened.clone()));
                registry.tool_contributor(Arc::new(reopened.clone()));
                let starter = reopened.clone();
                let cwd = workspace.clone();
                let resumed = test_codex()
                    .with_extensions(Arc::new(registry.build()))
                    .with_auth(codex_login::CodexAuth::from_api_key(
                        "synthetic-routing-reopen-key",
                    ))
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
                let id = reopened
                    .lifecycle()
                    .attach_root(resumed.codex.clone())
                    .unwrap();
                reopened.register(id, binding.clone()).unwrap();
                let task = reopened.project().unwrap().tasks[&config.root_task].clone();
                reopened
                    .resume(id, task.revision, task.fingerprint)
                    .unwrap();
                reopened
                    .configure_coding(
                        id,
                        CodingConfig {
                            canonical_tools: Default::default(),
                            operating: "Preserve retained routing policy.".into(),
                            affected_paths: vec!["file.txt".into()],
                            max_requests: 4,
                            deadline: Timestamp::new(clock().get() + 300_000),
                        },
                    )
                    .unwrap();
                let before = observed.lock().unwrap().len();
                reopened
                    .begin_coding_turn(id, "Resume without routing startup configuration.".into())
                    .unwrap();
                resumed
                    .codex
                    .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                        text: "Resume without routing startup configuration.".into(),
                        text_elements: vec![],
                    }]))
                    .await
                    .unwrap();
                tokio::time::timeout(Duration::from_secs(120), async {
                    loop {
                        if matches!(
                            resumed.codex.next_event().await.unwrap().msg,
                            EventMsg::TurnComplete(_)
                        ) {
                            break;
                        }
                    }
                })
                .await
                .unwrap();
                assert_eq!(
                    observed.lock().unwrap().len(),
                    before,
                    "omitted routing configuration cannot bypass persisted policy"
                );
                assert_eq!(
                    reopened
                        .snapshot()
                        .unwrap()
                        .records
                        .values()
                        .filter(|r| r.collection == Collection::Attempt)
                        .count(),
                    attempts.len()
                );
                assert_eq!(
                    reopened.project().unwrap().tasks[&config.root_task].state,
                    TaskState::Paused
                );
                owner.close().await.unwrap();
                resumed.codex.shutdown_and_wait().await.unwrap();
            }
        }
    }
}
