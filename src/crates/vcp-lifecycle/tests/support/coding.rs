// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use vcp_domain::policy::{Autonomy, EffectClass, Policy};
use vcp_lifecycle::foundation::coding::{allowed_tools, CodingConfig};
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn coding_context_lists_only_public_process_invocation_metadata() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for configured in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let workspace = temp.path().join("workspace");
            std::fs::create_dir(&workspace).unwrap();
            let workspace = workspace.canonicalize().unwrap();
            std::fs::write(workspace.join("file.txt"), "observed source").unwrap();
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
                    policy: Policy {
                        workspace: config.workspace.clone(),
                        revision: PolicyRevision::ZERO,
                        mode: Autonomy::Plan,
                        denials: vec![],
                        workspace_roots: std::collections::BTreeSet::from([RootId::parse(
                            config.workspace.as_str(),
                        )
                        .unwrap()]),
                        automatic_effects: std::collections::BTreeSet::from([EffectClass::Read]),
                        timeout_ceiling_ms: Units::new(30_000),
                        output_ceiling_bytes: ByteCount::new(1024 * 1024),
                    },
                },
                None,
                Revision::ZERO,
            )
            .unwrap();
            if configured {
                // Reverse insertion order exercises deterministic discovery. These
                // unavailable executable/input paths must never reach the model.
                for (name, mode, terminal, maximum) in [
                    (
                        "z-terminal",
                        vcp_tools::process::Mode::PowerShell,
                        true,
                        90_000,
                    ),
                    ("a-direct", vcp_tools::process::Mode::Direct, false, 120_000),
                ] {
                    let mut profile = vcp_tools::process::Profile::new(
                        name.into(),
                        temp.path().join("private-profile-executable.exe"),
                        mode,
                        std::collections::BTreeMap::from([(
                            "LANG".into(),
                            "private-profile-environment".into(),
                        )]),
                        Default::default(),
                        true,
                    )
                    .unwrap()
                    .with_inputs(vec!["private-profile-input.cfg".into()])
                    .unwrap()
                    .with_max_timeout_ms(maximum)
                    .unwrap();
                    if terminal {
                        profile = profile.with_terminal(24, 80).unwrap();
                    }
                    host.configure_process_profile(profile).unwrap();
                }
            }
            let (snapshot, raw) = provider_snapshot();
            host.configure_provider(snapshot, raw).unwrap();
            let observed = Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
            let requests = observed.clone();
            let server = start_mock_server().await;
            Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |request: &wiremock::Request| {
                requests.lock().unwrap().push(serde_json::from_slice(&request.body).unwrap());
                ResponseTemplate::new(200).insert_header("content-type", "text/event-stream").set_body_string(sse(vec![
                    ev_assistant_message("observed", "Profile metadata observed; no process was executed."),
                    serde_json::json!({"type":"response.completed","response":{"id":"profile-metadata","status":"completed","output":[],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":0.0001}}}),
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
                .with_auth(codex_login::CodexAuth::from_api_key(
                    "synthetic-profile-metadata",
                ))
                .with_allowed_tools(allowed_tools())
                .with_config(move |config| {
                    config.cwd = cwd.try_into().unwrap();
                    configure_provider_fixture(config);
                    starter
                        .lifecycle()
                        .authorize_startup(config.cwd.as_path(), None)
                        .unwrap();
                })
                .build_with_auto_env(&server)
                .await
                .unwrap();
            let thread = host.lifecycle().attach_root(test.codex.clone()).unwrap();
            host.register(thread, binding).unwrap();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            host.configure_coding(
                thread,
                CodingConfig {
                    canonical_tools: Default::default(),
                    operating: "Observe configured tools; do not execute processes.".into(),
                    affected_paths: vec!["file.txt".into()],
                    max_requests: 2,
                    deadline: Timestamp::new(now + 300_000).into(),
                },
            )
            .unwrap();
            // Direct-host legacy setup seals the default before coding becomes active.
            // Child registration can obtain that same ceiling without fresh setup.
            let child = TaskId::new();
            task(
                &host,
                &config,
                child.clone(),
                Some(config.root_task.clone()),
            );
            assert!(host.startup_canonical_tools(child).unwrap().is_all());
            let narrow = serde_json::from_value(serde_json::json!(["vcp_read"])).unwrap();
            assert!(host.configure_canonical_tools(narrow).is_err());
            coding_turn(&test, backend, "public-process-metadata", None).await;
            let requests = observed.lock().unwrap().clone();
            assert_allowance(&host, &requests[0], &config.root_task, 2, 0);
            assert_eq!(requests.len(), 1);
            let operating = requests[0]["input"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|item| item["type"] == "message")
                .filter_map(|item| {
                    serde_json::from_str::<serde_json::Value>(item["content"][0]["text"].as_str()?)
                        .ok()
                })
                .find(|part| part["kind"] == "operating")
                .unwrap();
            let text = operating["text"].as_str().unwrap();
            let public = text
                .lines()
                .find_map(|line| line.strip_prefix("Configured process profiles: "));
            if configured {
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(public.unwrap()).unwrap(),
                    serde_json::json!([
                        {"name":"a-direct","mode":"direct","terminal":false,"max_timeout_ms":120000},
                        {"name":"z-terminal","mode":"power_shell","terminal":true,"max_timeout_ms":90000},
                    ])
                );
            } else {
                assert!(
                    public.is_none(),
                    "empty profiles do not expand unrelated context"
                );
            }
            let body = requests[0].to_string();
            for private in [
                "private-profile-executable",
                "private-profile-environment",
                "private-profile-input",
            ] {
                assert!(!body.contains(private), "private profile metadata leaked");
            }
            let snapshot = host.snapshot().unwrap();
            assert!(!snapshot
                .records
                .values()
                .any(|record| record.collection == Collection::Effect));
            let policy = vcp_engine::policy::current(&snapshot, &config.workspace).unwrap();
            assert_eq!(policy.mode, Autonomy::Plan);
            assert_eq!(
                policy.automatic_effects,
                std::collections::BTreeSet::from([EffectClass::Read])
            );
            owner.close().await.unwrap();
            test.codex.shutdown_and_wait().await.unwrap();
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canonical_coding_loop_assembles_current_sources_and_dispatches_prepared_files() {
    run_coding_modes(&[
        "complete",
        "nested",
        "process_fail",
        "limit",
        "missing_cost",
        "incomplete_usage",
        "invalid_call_usage",
        "stale_instructions",
        "deadline",
        "empty",
    ])
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn registered_directory_tools_reject_quoted_root_then_accept_empty_root() {
    run_coding_modes(&["quoted_root"]).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn instruction_refresh_retains_typed_unexecuted_call_before_reissue() {
    run_coding_modes(&["nested"]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn coding_source_access_rejects_stale_instructions_before_dispatch() {
    run_coding_modes(&["stale_instructions"]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn output_limit_continuation_preserves_partial_evidence_and_stops_without_progress() {
    run_coding_modes(&["incomplete_usage"]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pending_output_continuation_reopen_retains_evidence_without_dispatch() {
    run_coding_modes(&["incomplete_pending_reopen"]).await;
}

#[cfg(feature = "qualification")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn completed_tool_pairs_preserve_adaptive_usage_across_owner_reopen() {
    run_coding_modes(&["allocation_reopen"]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn routed_candidate_usage_does_not_shrink_a_smaller_fresh_candidate() {
    run_coding_modes(&["routed_allocation", "routed_allocation_fallback"]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn encoding_diagnostics_attribute_actual_routed_trials_and_sealed_requests() {
    run_coding_modes(&["encoding_routed"]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn output_limit_recovery_applies_one_complete_large_patch_with_exact_admission() {
    run_coding_modes(&["large_patch"]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn completed_responses_with_unknown_cost_preserve_liability_without_duplicate_effects() {
    run_coding_modes(&["missing_cost", "missing_cost_unbounded"]).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unpriced_routed_requests_keep_unknown_estimates_and_exact_admission_on_both_stores() {
    run_coding_modes(&["unpriced_routed"]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unbounded_execution_does_not_turn_provider_timeout_into_a_total_response_deadline() {
    run_coding_modes(&["unbounded_provider_time"]).await;
}

async fn run_coding_modes(modes: &[&'static str]) {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        for &requested_mode in modes {
            let unbounded_time = requested_mode == "unbounded_provider_time";
            let unpriced = requested_mode == "unpriced_routed";
            if unpriced {
                eprintln!("EE01 unpriced routed {backend:?}: setup begins");
            }
            let unknown_cost = requested_mode == "missing_cost_unbounded" || unpriced;
            let pending_reopen = requested_mode == "incomplete_pending_reopen";
            let mode = if unknown_cost || unbounded_time {
                "complete"
            } else if pending_reopen {
                "incomplete_usage"
            } else {
                requested_mode
            };
            let temp = tempfile::tempdir().unwrap();
            let workspace = temp.path().join("workspace");
            std::fs::create_dir(&workspace).unwrap();
            let workspace = workspace.canonicalize().unwrap();
            std::fs::write(workspace.join("file.txt"), "before\n").unwrap();
            std::fs::write(workspace.join("AGENTS.md"), "instruction version one").unwrap();
            std::fs::create_dir(workspace.join("nested")).unwrap();
            std::fs::write(workspace.join("nested/AGENTS.md"), "nested scoped guidance").unwrap();
            std::fs::write(workspace.join("nested/file.txt"), "nested evidence").unwrap();
            std::fs::write(
                workspace.join("fixture.txt"),
                if mode == "process_fail" {
                    "answer = 41\n"
                } else {
                    "answer = 42\n"
                },
            )
            .unwrap();
            let executable = temp.path().join("fixture.exe");
            std::fs::copy(env!("CARGO_BIN_EXE_vcp-process-fixture"), &executable).unwrap();
            let mut config = config(&temp.path().join("canonical"), &workspace, backend);
            if unknown_cost || unbounded_time {
                config.cap.micros = vcp_domain::Limit::Unbounded;
            }
            if matches!(mode, "incomplete_usage" | "large_patch") {
                // The host ceiling may exceed this fixed provider's 8000-token
                // capacity; every actual request/reservation must still fit.
                config.output_ceiling = Units::new(16_000);
            }
            if mode.starts_with("routed_allocation") || mode == "encoding_routed" {
                config.output_ceiling = Units::new(4096);
            }
            let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
            let binding = task(&host, &config, config.root_task.clone(), None);
            if unbounded_time {
                host.configure_execution_constraints(vcp_domain::Limit::Unbounded)
                    .unwrap();
            }
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
                    policy: Policy {
                        workspace: config.workspace.clone(),
                        revision: PolicyRevision::ZERO,
                        mode: Autonomy::Autonomous,
                        denials: vec![],
                        workspace_roots: std::collections::BTreeSet::from([
                            RootId::parse(config.workspace.as_str()).unwrap(),
                            RootId::parse("exec-fixture").unwrap(),
                        ]),
                        automatic_effects: std::collections::BTreeSet::from([
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
            host.configure_process_profile(
                vcp_tools::process::Profile::new(
                    "fixture".into(),
                    executable,
                    vcp_tools::process::Mode::Direct,
                    std::collections::BTreeMap::from([(
                        "SystemRoot".into(),
                        std::env::var("SystemRoot").unwrap(),
                    )]),
                    Default::default(),
                    true,
                )
                .unwrap()
                .with_inputs(vec!["fixture.txt".into()])
                .unwrap(),
            )
            .unwrap();
            // The large-edit scenario declares its larger input capacity before
            // any admission. Output remains the same qualified 8000-token bound.
            let (snapshot, raw) = if mode == "large_patch" {
                provider_snapshot_capacity(128_000, 120_000)
            } else {
                provider_snapshot()
            };
            // This fixture exercises two correlated parallel calls. Optional
            // request parameters must be qualified by both endpoint metadata
            // and the compatibility record before the encoder enables them.
            let mut endpoint: serde_json::Value = serde_json::from_slice(&raw).unwrap();
            endpoint["data"]["endpoints"][0]["supported_parameters"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!("parallel_tool_calls"));
            let raw = serde_json::to_vec(&endpoint).unwrap();
            let mut compatibility = snapshot.compatibility;
            compatibility
                .required_parameters
                .insert("parallel_tool_calls".into());
            let snapshot = vcp_models::catalog::Snapshot::from_endpoints(
                &raw,
                snapshot.observed_at,
                snapshot.valid_until,
                compatibility,
            )
            .unwrap();
            if unbounded_time {
                host.configure_provider_with_timeout(snapshot, raw, Duration::from_millis(30))
                    .unwrap();
            } else {
                host.configure_provider(snapshot, raw).unwrap();
            }
            let mut encoding_catalogs = std::collections::BTreeSet::new();
            if mode == "encoding_routed" {
                let mut routing =
                    super::routing::routing_configuration(vcp_models::routing::Profile::Low, false);
                for estimate in &mut routing.estimates {
                    estimate.first_attempt.output = Units::new(4096);
                }
                encoding_catalogs.extend(routing.catalog.entries.iter().map(|candidate| candidate.snapshot.as_ref().unwrap().id.clone()));
                host.configure_routing(routing).unwrap();
            }
            if unpriced {
                let mut routing = super::routing::routing_configuration(vcp_models::routing::Profile::Low, false);
                let mut entry = routing.catalog.entries.remove(0);
                let previous = entry.snapshot.as_ref().unwrap();
                let mut raw: serde_json::Value = serde_json::from_str(&routing.raw_catalogs[&previous.id]).unwrap();
                raw["data"]["endpoints"][0]["pricing"] = serde_json::Value::Null;
                raw["data"]["endpoints"][0]["supported_parameters"].as_array_mut().unwrap()
                    .push(serde_json::json!("parallel_tool_calls"));
                let mut compatibility = previous.compatibility.clone();
                compatibility.required_parameters.insert("parallel_tool_calls".into());
                let raw = serde_json::to_string(&raw).unwrap();
                let snapshot = vcp_models::catalog::Snapshot::from_endpoints_unbounded(raw.as_bytes(),
                    previous.observed_at, previous.valid_until, compatibility).unwrap();
                routing.raw_catalogs = std::collections::BTreeMap::from([(snapshot.id.clone(), raw)]);
                entry.snapshot = Some(snapshot);
                routing.estimates.retain(|estimate| estimate.candidate == entry.identity);
                routing.catalog = vcp_models::routing::CatalogRevision::create(None,
                    routing.catalog.observed_at, None, vec![entry]).unwrap();
                host.configure_routing(routing).unwrap();
                eprintln!(
                    "EE01 unpriced routed {backend:?}: configured {:?}",
                    host.store_diagnostics()
                );
            }
            if mode.starts_with("routed_allocation") {
                let mut routing =
                    super::routing::routing_configuration(vcp_models::routing::Profile::Low, false);
                let smaller = routing
                    .catalog
                    .entries
                    .iter_mut()
                    .find(|entry| entry.identity.model == "fixture/stronger")
                    .unwrap();
                let prior = smaller.snapshot.as_ref().unwrap();
                let mut raw: serde_json::Value =
                    serde_json::from_str(&routing.raw_catalogs.remove(&prior.id).unwrap()).unwrap();
                raw["data"]["endpoints"][0]["max_completion_tokens"] = serde_json::json!(1500);
                let raw = serde_json::to_string(&raw).unwrap();
                let snapshot = vcp_models::catalog::Snapshot::from_endpoints(
                    raw.as_bytes(),
                    prior.observed_at,
                    prior.valid_until,
                    prior.compatibility.clone(),
                )
                .unwrap();
                routing.raw_catalogs.insert(snapshot.id.clone(), raw);
                smaller.snapshot = Some(snapshot);
                routing.catalog.id = routing.catalog.digest().unwrap();
                for estimate in &mut routing.estimates {
                    estimate.first_attempt.output =
                        Units::new(if estimate.candidate.model == "fixture/stronger" {
                            1500
                        } else {
                            4096
                        });
                }
                if mode == "routed_allocation_fallback" {
                    routing.owner_assignments =
                        vec![vcp_lifecycle::foundation::routing::OwnerAssignment {
                            role: vcp_domain::accounting::RequestRole::Main,
                            candidates: ["fixture/economical", "fixture/stronger"]
                                .into_iter()
                                .map(|model| {
                                    routing
                                        .catalog
                                        .entries
                                        .iter()
                                        .find(|entry| entry.identity.model == model)
                                        .unwrap()
                                        .identity
                                        .clone()
                                })
                                .collect(),
                        }];
                }
                host.configure_routing(routing).unwrap();
            }
            let count = Arc::new(AtomicUsize::new(0));
            let observed = Arc::new(std::sync::Mutex::new(Vec::new()));
            let requests = observed.clone();
            let wire_bytes = Arc::new(std::sync::Mutex::new(Vec::new()));
            let captured_bytes = wire_bytes.clone();
            let calls = count.clone();
            let directory = workspace.clone();
            let routing_owner = mode.starts_with("routed_allocation").then(|| host.clone());
            let large_contents = (0..512)
                .map(|line| {
                    format!("line {line:04}: complete patch preserves every distinct row\n")
                })
                .collect::<String>();
            let complete_patch = format!(
                "*** Begin Patch\n*** Update File: file.txt\n@@\n-before\n{}*** End Patch",
                large_contents
                    .lines()
                    .map(|line| format!("+{line}\n"))
                    .collect::<String>()
            );
            let expected_large_contents = large_contents;
            let server = start_mock_server().await;
            Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move |request: &wiremock::Request| {
                let index = calls.fetch_add(1, Ordering::SeqCst);
                let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                requests.lock().unwrap().push(body.clone());
                captured_bytes.lock().unwrap().push(request.body.clone());
                if mode == "routed_allocation_fallback" && index == 5 {
                    return ResponseTemplate::new(429).insert_header("retry-after", "0").set_body_string("local candidate unavailable");
                }
                if mode == "routed_allocation" && index == 4 {
                    super::routing_output::select_edits(routing_owner.as_ref().unwrap(), vec![vcp_lifecycle::foundation::routing_state::Edit::Pin(Some(vcp_models::routing::Pin {
                        candidate: vcp_models::routing::ModelEndpoint { model:"fixture/stronger".into(), endpoint:"fixture/stronger-region".into() }, fallback_candidates: Default::default(),
                    }))]);
                }
                let mut events = vec![];
                let mut output = vec![];
                if mode == "encoding_routed" {
                    events.push(ev_assistant_message("encoding-done", "The exact encoded request was observed."));
                } else if mode == "invalid_call_usage" {
                    let arguments = serde_json::json!({"path":"file.txt","max_bytes":678}).to_string();
                    let mut item = serde_json::json!({"type":"function_call","id":"invalid-item","call_id":"invalid-call","name":"vcp_read","arguments":"","status":"in_progress"});
                    events.push(serde_json::json!({"type":"response.output_item.added","output_index":0,"item":item}));
                    events.push(serde_json::json!({"type":"response.function_call_arguments.delta","item_id":"invalid-item","delta":arguments}));
                    events.push(serde_json::json!({"type":"response.function_call_arguments.done","item_id":"invalid-item","arguments":arguments}));
                    item["arguments"] = serde_json::json!(arguments);
                    item["status"] = serde_json::json!("completed");
                    events.push(serde_json::json!({"type":"response.output_item.done","output_index":0,"item":item}));
                    output.push(item);
                    let sibling = serde_json::json!({"type":"function_call","id":"valid-patch","call_id":"valid-patch-call","name":"vcp_patch","arguments":serde_json::json!({"patch":"*** Begin Patch\n*** Update File: file.txt\n@@\n-before\n+after\n*** End Patch"}).to_string(),"status":"completed"});
                    events.push(serde_json::json!({"type":"response.output_item.done","output_index":1,"item":sibling}));
                    output.push(sibling);
                } else if mode == "incomplete_usage" || (mode == "large_patch" && index == 0) {
                    let mut item = serde_json::json!({"type":"function_call","id":"truncated-item","call_id":"truncated-call","name":"vcp_patch","arguments":"","status":"in_progress"});
                    events.push(serde_json::json!({"type":"response.output_item.added","output_index":0,"item":item}));
                    let partial_arguments = if mode == "large_patch" {
                        let arguments = serde_json::json!({"patch":complete_patch}).to_string();
                        arguments[..4096].to_owned()
                    } else { String::new() };
                    if !partial_arguments.is_empty() { events.push(serde_json::json!({"type":"response.function_call_arguments.delta","item_id":"truncated-item","delta":partial_arguments})); }
                    events.push(serde_json::json!({"type":"response.function_call_arguments.done","item_id":"truncated-item","arguments":partial_arguments}));
                    item["status"] = serde_json::json!("incomplete");
                    if mode == "large_patch" { item["arguments"] = serde_json::json!(partial_arguments); }
                    events.push(serde_json::json!({"type":"response.output_item.done","output_index":0,"item":item}));
                    if mode != "large_patch" { item["arguments"] = serde_json::json!("{}"); }
                    output.push(item);
                } else if mode == "large_patch" && index == 1 {
                    let item = serde_json::json!({"type":"function_call","id":"large-patch-item","call_id":"large-patch-call","name":"vcp_patch","arguments":serde_json::json!({"patch":complete_patch}).to_string(),"status":"completed"});
                    events.push(serde_json::json!({"type":"response.output_item.done","output_index":0,"item":item}));
                    output.push(item);
                } else if mode == "large_patch" {
                    events.push(ev_assistant_message("large-patch-done", "The complete large patch was applied."));
                } else if mode.starts_with("routed_allocation") && index < if mode == "routed_allocation_fallback" {7} else {6} {
                    let item = serde_json::json!({"type":"function_call","id":format!("read-item-{index}"),"call_id":format!("read-call-{index}"),"name":"vcp_read","arguments":serde_json::json!({"path":"file.txt","max_bytes":1024,"start_line":null,"end_line":null}).to_string(),"status":"completed"});
                    events.push(serde_json::json!({"type":"response.output_item.done","output_index":0,"item":item}));
                    output.push(item);
                } else if mode.starts_with("routed_allocation") {
                    events.push(ev_assistant_message("routed-done", "The candidate-specific trace is complete."));
                } else if mode == "empty" {
                    events.push(ev_assistant_message("empty-answer", " \n\t "));
                } else if index < 4 {
                    let (name, arguments) = if mode == "allocation_reopen" {
                        ("vcp_read", serde_json::json!({"path":"file.txt","max_bytes":1024,"start_line":null,"end_line":null}))
                    } else if mode == "quoted_root" {
                        let root = if index % 2 == 0 { r#"\"\""# } else { "" };
                        if index < 2 {
                            ("vcp_exec", serde_json::json!({"profile":"fixture","arguments":["verify",directory.to_str().unwrap()],"directory":root,"timeout_ms":10_000,"output_bytes":1_048_576,"input":null}))
                        } else {
                            ("vcp_list", serde_json::json!({"path":root,"max_entries":100}))
                        }
                    } else if index == 1 {
                        ("vcp_patch", serde_json::json!({"patch":"*** Begin Patch\n*** Update File: file.txt\n@@\n-before\n+after\n*** Update File: AGENTS.md\n@@\n-instruction version one\n+instruction version two\n*** End Patch"}))
                    } else if index==3 { ("vcp_exec", serde_json::json!({"profile":"fixture","arguments":["verify",directory.to_str().unwrap()],"directory":"","timeout_ms":10_000,"output_bytes":1_048_576,"input":null})) }
                    else { ("vcp_read", serde_json::json!({"path":if mode=="nested" && index==0 {"nested/file.txt"}else{"file.txt"},"max_bytes":1024,"start_line":null,"end_line":null})) };
                    let item = serde_json::json!({"type":"function_call","id":format!("item-{index}"),"call_id":format!("call-{index}"),"name":name,"arguments":arguments.to_string(),"status":"completed"});
                    events.push(serde_json::json!({"type":"response.output_item.done","output_index":0,"item":item}));
                    output.push(item);
                    if mode == "complete" && index == 0 {
                        let sibling = serde_json::json!({"type":"function_call","id":"sibling-item","call_id":"sibling-read","name":"vcp_read","arguments":serde_json::json!({"path":"fixture.txt","max_bytes":1024,"start_line":null,"end_line":null}).to_string(),"status":"completed"});
                        events.push(serde_json::json!({"type":"response.output_item.done","output_index":1,"item":sibling}));
                        output.push(sibling);
                    }
                } else { events.push(ev_assistant_message("done", "Observed the file change.")); }
                let cost = if unknown_cost || mode=="missing_cost" {serde_json::Value::Null}else if body["model"] == "fixture/stronger" {serde_json::json!(0.0002)}else{serde_json::json!(0.0001)};
                if mode=="stale_instructions" { std::fs::write(directory.join("AGENTS.md"), "concurrent human guidance").unwrap(); }
                if mode == "large_patch" && index == 0 {
                    events.push(serde_json::json!({"type":"response.incomplete","response":{"id":format!("coding-{index}"),"status":"incomplete","incomplete_details":{"reason":"max_output_tokens"},"output":output,"usage":{"input_tokens":1000,"output_tokens":4096,"total_tokens":5096,"cost":cost}}}));
                } else if mode == "large_patch" && index == 1 {
                    events.push(serde_json::json!({"type":"response.completed","response":{"id":format!("coding-{index}"),"status":"completed","output":output,"usage":{"input_tokens":1000,"output_tokens":6000,"total_tokens":7000,"cost":cost}}}));
                } else if mode == "incomplete_usage" {
                    events.push(serde_json::json!({"type":"response.incomplete","response":{"id":format!("coding-{index}"),"status":"incomplete","incomplete_details":{"reason":"max_output_tokens"},"output":output,"usage":{"input_tokens":5950,"output_tokens":512,"total_tokens":6462,"cost":0.0021275}}}));
                } else if mode == "allocation_reopen" {
                    // Missing subcounts leave accounting usage unknown, but
                    // the explicit output count is valid allocation evidence.
                    events.push(serde_json::json!({"type":"response.completed","response":{"id":format!("coding-{index}"),"status":"completed","output":output,"usage":{"input_tokens":4354,"output_tokens":97,"total_tokens":4451,"cost":0.00120975}}}));
                } else if mode == "invalid_call_usage" {
                    events.push(serde_json::json!({"type":"response.completed","response":{"id":format!("coding-{index}"),"status":"completed","output":output,"usage":{"input_tokens":4354,"output_tokens":97,"total_tokens":4451,"cost":0.00120975}}}));
                } else {
                events.push(serde_json::json!({"type":"response.completed","response":{"id":format!("coding-{index}"),"status":"completed","output":output,"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"cost":cost}}}));
                }
                let response = ResponseTemplate::new(200).insert_header("content-type", "text/event-stream").set_body_string(sse(events));
                if mode=="deadline" {response.set_delay(Duration::from_secs(20))}else if unbounded_time { response.set_delay(Duration::from_millis(150)) } else{response}
            }).mount(&server).await;
            let mut registry = ExtensionRegistryBuilder::new();
            registry.turn_start_admission(Arc::new(host.clone()));
            registry.work_admission(Arc::new(host.clone()));
            registry.tool_contributor(Arc::new(host.clone()));
            let starter = host.clone();
            let cwd = workspace.clone();
            let test = test_codex()
                .with_extensions(Arc::new(registry.build()))
                .with_auth(codex_login::CodexAuth::from_api_key("synthetic-coding-key"))
                .with_allowed_tools(allowed_tools())
                .with_config(move |config| {
                    config.cwd = cwd.try_into().unwrap();
                    configure_provider_fixture(config);
                    starter
                        .lifecycle()
                        .authorize_startup(config.cwd.as_path(), None)
                        .unwrap();
                })
                .build_with_auto_env(&server)
                .await
                .unwrap();
            let id = host.lifecycle().attach_root(test.codex.clone()).unwrap();
            host.register(id, binding.clone()).unwrap();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            host.configure_coding(
                id,
                CodingConfig {
                    canonical_tools: Default::default(),
                    operating: "Use prepared tools and report observed evidence only.".into(),
                    affected_paths: vec!["file.txt".into()],
                    max_requests: match mode {
                        "limit" => 2,
                        "allocation_reopen" => 4,
                        _ => 8,
                    },
                    // Leave native capture/startup headroom before submission;
                    // the server then withholds its response beyond this bound.
                    deadline: if unbounded_time {
                        vcp_domain::Limit::Unbounded
                    } else {
                        Timestamp::new(now + if mode == "deadline" { 10_000 } else { 300_000 })
                            .into()
                    },
                },
            )
            .unwrap();
            assert!(codex_extension_api::HostWorkAdmission::admit_tool(
                &host,
                id,
                "call-0",
                &codex_extension_api::ToolName::plain("vcp_read")
            )
            .is_err());
            assert_eq!(
                host.project().unwrap().tasks[&config.root_task].state,
                TaskState::Running,
                "invented pre-response calls cannot pause the root"
            );
            let selected_turn = vcp_domain::TurnId::parse("explicit-canonical-turn").unwrap();
            let turn = host
                .begin_coding_turn_identified(
                    id,
                    "Run the synthetic request.".into(),
                    selected_turn.clone(),
                )
                .unwrap();
            assert_eq!(turn, selected_turn);
            let before_duplicate = host.snapshot().unwrap();
            assert!(host
                .begin_coding_turn_identified(
                    id,
                    "Different input cannot reuse this turn identity.".into(),
                    selected_turn,
                )
                .is_err());
            assert_eq!(host.snapshot().unwrap(), before_duplicate);
            if unpriced {
                eprintln!(
                    "EE01 unpriced routed {backend:?}: before turn {:?}",
                    host.store_diagnostics()
                );
            }
            coding_turn(&test, backend, mode, unpriced.then_some(&host)).await;
            if mode == "large_patch" {
                assert_eq!(count.load(Ordering::SeqCst), 1);
                assert!(host.project().unwrap().effects.is_empty());
                assert_eq!(
                    std::fs::read_to_string(workspace.join("file.txt")).unwrap(),
                    "before\n"
                );
                let continuation = host.take_output_continuation(id).unwrap().unwrap();
                assert!(!continuation.evidence.is_empty());
                assert!(
                    continuation
                        .evidence
                        .iter()
                        .any(|artifact| String::from_utf8_lossy(
                            &host.read_artifact(artifact.clone()).unwrap()
                        )
                        .contains("line 0000")),
                    "retain the actual incomplete patch prefix"
                );
                host.begin_coding_turn(id, continuation.feedback).unwrap();
                coding_turn(&test, backend, mode, None).await;
                assert_eq!(count.load(Ordering::SeqCst), 3);
                assert_eq!(
                    std::fs::read_to_string(workspace.join("file.txt")).unwrap(),
                    expected_large_contents
                );
                assert_eq!(
                    host.project().unwrap().effects.len(),
                    1,
                    "the incomplete patch must not execute or duplicate the complete patch"
                );
                assert!(!host.has_output_continuation(id).unwrap());
                let bodies = observed.lock().unwrap().clone();
                assert_eq!(bodies[0]["max_output_tokens"], 4096);
                assert_eq!(bodies[1]["max_output_tokens"], 8000);
                assert_eq!(
                    bodies[2]["max_output_tokens"], 4096,
                    "editing selects its own activity baseline"
                );
                let allocations = assert_adaptive_admission_trace(
                    &host,
                    &bodies,
                    &wire_bytes.lock().unwrap(),
                    None,
                );
                assert_eq!(
                    allocations[1].reason,
                    vcp_domain::request_allocation::Reason::ProviderCapacity
                );
                assert_eq!(allocations[1].previous_output_limit, Some(Units::new(4096)));
                assert_eq!(allocations[1].previous_output, Some(Units::new(4096)));
                owner.close().await.unwrap();
                test.codex.shutdown_and_wait().await.unwrap();
                continue;
            }
            if mode == "encoding_routed" {
                let scope = host.project().unwrap().tasks[&config.root_task]
                    .scope
                    .clone();
                let diagnostics = host.execution_diagnostics(scope.clone()).unwrap();
                use vcp_lifecycle::foundation::execution_diagnostics::EncodingPurpose;
                let candidates: std::collections::BTreeSet<_> = diagnostics
                    .encodings
                    .iter()
                    .filter(|row| row.purpose == EncodingPurpose::CandidateFit)
                    .map(|row| row.catalog.clone()).collect();
                assert_eq!(candidates, encoding_catalogs, "candidate sizing must retain exact catalog attribution");
                assert!(diagnostics.encodings.iter().all(|row| row.scope == scope && row.attempt.is_none()));
                for purpose in [EncodingPurpose::CandidateFit, EncodingPurpose::FinalAssembly, EncodingPurpose::SealedValidation] {
                    assert!(diagnostics.encodings.iter().any(|row| row.purpose == purpose && row.work.encode_calls > 0), "missing {purpose:?}; sent={} task={:?} encodings={:?}", count.load(Ordering::SeqCst), host.project().unwrap().tasks[&config.root_task], diagnostics.encodings);
                }
                assert!(diagnostics
                    .encodings
                    .iter()
                    .any(|row| row.purpose == EncodingPurpose::CandidateFit
                        && row.work.encode_calls >= 2));
                // The independent ring may omit early stages in a long turn;
                // require the latest actual requests, not unlimited retention.
                for bytes in wire_bytes.lock().unwrap().iter().rev().take(2) {
                    let digest = vcp_protocol::digest_bytes(bytes);
                    assert!(diagnostics.encodings.iter().any(|row| row.purpose
                        == EncodingPurpose::SealedValidation
                        && row.request_sha256.as_deref() == Some(digest.as_str())
                        && row.work.validation_calls == 1
                        && row.work.validation_failures == 0
                        && row.work.encode_calls == 1
                        && row.work.encoded_bytes == bytes.len() as u64));
                }
                assert_eq!(count.load(Ordering::SeqCst), 1);
                assert!(host.project().unwrap().effects.is_empty());
                if let Some(directory) = std::env::var_os("VCP_TEST_ENCODING_EVIDENCE") {
                    let directory = std::path::PathBuf::from(directory);
                    std::fs::create_dir_all(&directory).unwrap();
                    let path =
                        directory.join(format!("routed-{backend:?}-{}.json", EventId::new()));
                    let file = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(path)
                        .unwrap();
                    serde_json::to_writer(file, &serde_json::json!({"kind":"actual_routed_codec_diagnostics", "backend":format!("{backend:?}"), "scope":scope,"snapshot":diagnostics})).unwrap();
                }
                owner.close().await.unwrap();
                test.codex.shutdown_and_wait().await.unwrap();
                continue;
            }
            if mode.starts_with("routed_allocation") {
                let fallback = mode == "routed_allocation_fallback";
                let bodies = observed.lock().unwrap().clone();
                assert_eq!(
                    bodies.len(),
                    if fallback { 8 } else { 7 },
                    "routing stopped: {:?}",
                    host.project().unwrap().tasks[&config.root_task]
                );
                let outputs = if fallback {
                    vec![4096, 2048, 2048, 2048, 1024, 1024, 1500, 2048]
                } else {
                    vec![4096, 2048, 2048, 2048, 1024, 1500, 1500]
                };
                let fresh_index = if fallback { 6 } else { 5 };
                for (index, output) in outputs.into_iter().enumerate() {
                    assert_eq!(
                        bodies[index]["max_output_tokens"], output,
                        "request {index} in {mode}"
                    );
                    assert_eq!(
                        bodies[index]["model"],
                        if index < fresh_index || (fallback && index == 7) {
                            "fixture/economical"
                        } else {
                            "fixture/stronger"
                        }
                    );
                }
                assert_eq!(host.project().unwrap().effects.len(), 6);
                let allocations = assert_adaptive_admission_trace(
                    &host,
                    &bodies,
                    &wire_bytes.lock().unwrap(),
                    fallback.then_some(5),
                );
                assert_eq!(
                    allocations[4].reason,
                    vcp_domain::request_allocation::Reason::ShrinkAfterRepeatedUnderuse
                );
                assert_eq!(
                    allocations[fresh_index].reason,
                    vcp_domain::request_allocation::Reason::ActivityDefault
                );
                assert_eq!(allocations[fresh_index].previous_output, None);
                let state = host.snapshot().unwrap();
                if fallback {
                    let attempts: Vec<Attempt> = state
                        .records
                        .values()
                        .filter(|row| row.collection == Collection::Attempt)
                        .map(|row| row.decode().unwrap())
                        .collect();
                    let wire = wire_bytes.lock().unwrap();
                    let primary = attempts
                        .iter()
                        .find(|attempt| {
                            attempt.request_digest == vcp_protocol::digest_bytes(&wire[5])
                        })
                        .unwrap();
                    let alternative = attempts
                        .iter()
                        .find(|attempt| {
                            attempt.request_digest == vcp_protocol::digest_bytes(&wire[6])
                        })
                        .unwrap();
                    assert_eq!(alternative.previous.as_ref(), Some(&primary.id));
                    let ledger: vcp_domain::accounting::Ledger = state
                        .record(
                            Collection::Ledger,
                            config.root_task.as_str(),
                            &config.workspace,
                        )
                        .unwrap()
                        .decode()
                        .unwrap();
                    assert_eq!(ledger.unresolved, Micros::new(100));
                    assert_eq!(ledger.settled, Micros::new(800));
                    assert_eq!(ledger.active, Micros::ZERO);
                    assert!(!ledger.overrun);
                    assert_eq!(
                        allocations[7].previous_output, None,
                        "a new primary request cannot inherit the fallback's calibration"
                    );
                }
                let mut observations: Vec<serde_json::Value> = state
                    .records
                    .values()
                    .filter(|row| row.collection == Collection::Artifact)
                    .map(|row| row.decode::<ArtifactDescriptor>().unwrap())
                    .filter(|artifact| artifact.spec.schema == "coding-allocation-observation/2")
                    .map(|artifact| {
                        serde_json::from_slice(&host.read_artifact(artifact.spec.id).unwrap())
                            .unwrap()
                    })
                    .collect();
                observations.sort_by_key(|value| value["sequence"].as_u64().unwrap());
                assert_eq!(observations.len(), 7);
                assert_eq!(observations[4]["history"]["underuse_streak"], 4);
                assert_eq!(
                    observations[5]["history"]["underuse_streak"], 1,
                    "new candidate starts fresh calibration"
                );
                assert_eq!(
                    observations[5]["history"]["candidate"]["endpoint"],
                    "fixture/stronger-region"
                );
                owner.close().await.unwrap();
                test.codex.shutdown_and_wait().await.unwrap();
                continue;
            }
            #[cfg(feature = "qualification")]
            if mode == "allocation_reopen" {
                assert_eq!(count.load(Ordering::SeqCst), 4);
                let before = host.project().unwrap();
                assert_eq!(before.effects.len(), 4);
                assert_eq!(before.tasks[&config.root_task].state, TaskState::Paused);
                let prior = host.snapshot().unwrap();
                let normalized: Vec<_> = prior
                    .records
                    .values()
                    .filter(|row| row.collection == Collection::Artifact)
                    .map(|row| row.decode::<ArtifactDescriptor>().unwrap())
                    .filter(|artifact| artifact.spec.schema == "openrouter-normalized-response/1")
                    .map(|artifact| {
                        serde_json::from_slice::<vcp_models::stream::ResultBody>(
                            &host.read_artifact(artifact.spec.id).unwrap(),
                        )
                        .unwrap()
                    })
                    .collect();
                assert_eq!(normalized.len(), 4);
                for response in normalized {
                    let usage = response.usage.unwrap();
                    assert_eq!(usage.output_tokens, Some(Units::new(97)));
                    assert!(
                        usage.tokens.is_none(),
                        "missing subcounts remain unknown accounting usage"
                    );
                    assert_eq!(usage.cost.unwrap().micros.get(), 1210);
                    assert!(usage.raw.get("input_tokens_details").is_none());
                    assert!(usage.raw.get("output_tokens_details").is_none());
                }
                let observations: Vec<_> = prior
                    .records
                    .values()
                    .filter(|row| row.collection == Collection::Artifact)
                    .map(|row| row.decode::<ArtifactDescriptor>().unwrap())
                    .filter(|artifact| artifact.spec.schema == "coding-allocation-observation/2")
                    .map(|artifact| {
                        serde_json::from_slice::<serde_json::Value>(
                            &host.read_artifact(artifact.spec.id).unwrap(),
                        )
                        .unwrap()
                    })
                    .collect();
                assert_eq!(observations.len(), 4);
                let last = observations
                    .iter()
                    .find(|value| value["sequence"] == 3)
                    .unwrap();
                assert_eq!(
                    last["completed_pairs"], 3,
                    "observation precedes fourth result publication"
                );
                assert_eq!(last["history"]["underuse_streak"], 3, "{last}");
                assert_eq!(last["version"], 2);
                assert_eq!(
                    last["history"]["candidate"]["model"],
                    provider_snapshot().0.compatibility.model
                );
                owner.close().await.unwrap();
                test.codex.shutdown_and_wait().await.unwrap();
                drop(test);
                drop(host);
                let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
                assert_eq!(host.project().unwrap().effects, before.effects);
                assert_eq!(
                    count.load(Ordering::SeqCst),
                    4,
                    "owner reopen does not dispatch"
                );
                assert!(host.has_output_continuation(id).is_err());
                let (snapshot, raw) = provider_snapshot();
                host.configure_provider(snapshot, raw).unwrap();
                let mut registry = ExtensionRegistryBuilder::new();
                registry.turn_start_admission(Arc::new(host.clone()));
                registry.work_admission(Arc::new(host.clone()));
                registry.tool_contributor(Arc::new(host.clone()));
                let starter = host.clone();
                let cwd = workspace.clone();
                let test = test_codex()
                    .with_extensions(Arc::new(registry.build()))
                    .with_auth(codex_login::CodexAuth::from_api_key(
                        "synthetic-allocation-reopen",
                    ))
                    .with_allowed_tools(allowed_tools())
                    .with_config(move |config| {
                        config.cwd = cwd.try_into().unwrap();
                        configure_provider_fixture(config);
                        starter
                            .lifecycle()
                            .authorize_startup(config.cwd.as_path(), None)
                            .unwrap();
                    })
                    .build_with_auto_env(&server)
                    .await
                    .unwrap();
                let id = host.lifecycle().attach_root(test.codex.clone()).unwrap();
                host.register(id, binding.clone()).unwrap();
                let task = host.project().unwrap().tasks[&config.root_task].clone();
                host.resume(id, task.revision, task.fingerprint).unwrap();
                host.configure_coding(
                    id,
                    CodingConfig {
                        canonical_tools: Default::default(),
                        operating: "Use prepared tools and report observed evidence only.".into(),
                        affected_paths: vec!["file.txt".into()],
                        max_requests: 4,
                        deadline: Timestamp::new(now + 300_000).into(),
                    },
                )
                .unwrap();
                assert_eq!(count.load(Ordering::SeqCst), 4);
                assert!(!host.has_output_continuation(id).unwrap());
                let allocation = host
                    .qualification_coding_allocation(id, provider_snapshot().0)
                    .unwrap();
                assert_eq!(
                    allocation.activity,
                    vcp_domain::request_allocation::Activity::Discovery
                );
                assert_eq!(allocation.previous_output, Some(Units::new(97)));
                assert_eq!(allocation.output_limit, Units::new(512));
                assert_eq!(
                    allocation.reason,
                    vcp_domain::request_allocation::Reason::ShrinkAfterRepeatedUnderuse
                );
                let (base, raw) = provider_snapshot();
                let mut catalog: serde_json::Value = serde_json::from_slice(&raw).unwrap();
                catalog["data"]["id"] = serde_json::json!("fixture/other-model");
                let mut compatibility = base.compatibility;
                compatibility.model = "fixture/other-model".into();
                let other = vcp_models::catalog::Snapshot::from_endpoints(
                    &serde_json::to_vec(&catalog).unwrap(),
                    base.observed_at,
                    base.valid_until,
                    compatibility,
                )
                .unwrap();
                let fresh = host.qualification_coding_allocation(id, other).unwrap();
                assert_eq!(
                    fresh.previous_output, None,
                    "another model cannot inherit calibrated usage"
                );
                assert_eq!(
                    fresh.reason,
                    vcp_domain::request_allocation::Reason::ActivityDefault
                );
                assert_eq!(fresh.output_limit, Units::new(1024));
                coding_turn(&test, backend, "allocation-reopen", None).await;
                assert_eq!(
                    count.load(Ordering::SeqCst),
                    4,
                    "reopen retains the finite request ceiling"
                );
                let requests = observed.lock().unwrap();
                assert_eq!(requests[3]["max_output_tokens"], 1024);
                assert_eq!(host.project().unwrap().effects, before.effects);
                assert_eq!(
                    host.project().unwrap().ledgers[&config.root_task]
                        .unresolved
                        .known().unwrap().get(),
                    0
                );
                drop(requests);
                owner.close().await.unwrap();
                test.codex.shutdown_and_wait().await.unwrap();
                continue;
            }
            if mode == "incomplete_usage" {
                assert_eq!(
                    host.project().unwrap().tasks[&config.root_task].state,
                    TaskState::Running
                );
                assert!(host.project().unwrap().effects.is_empty());
                assert!(host.has_output_continuation(id).unwrap());
                if pending_reopen {
                    let prior = host.snapshot().unwrap();
                    let artifacts: Vec<_> = prior
                        .records
                        .values()
                        .filter(|row| row.collection == Collection::Artifact)
                        .map(|row| row.decode::<ArtifactDescriptor>().unwrap())
                        .filter(|artifact| {
                            matches!(
                                artifact.spec.schema.as_str(),
                                "coding-output-continuation/1" | "coding-allocation-observation/2"
                            )
                        })
                        .collect();
                    assert_eq!(artifacts.len(), 2);
                    owner.close().await.unwrap();
                    test.codex.shutdown_and_wait().await.unwrap();
                    drop(test);
                    drop(host);
                    // Opening canonical history alone has no retained owner,
                    // no continuation capability and no source of dispatch.
                    let store = vcp_store::Store::open(&config.canonical_root, backend, &[])
                        .await
                        .unwrap();
                    assert_eq!(
                        store
                            .current()
                            .records
                            .values()
                            .filter(|row| row.collection == Collection::Attempt)
                            .count(),
                        1
                    );
                    assert_eq!(
                        store
                            .current()
                            .records
                            .values()
                            .filter(|row| row.collection == Collection::Effect)
                            .count(),
                        0
                    );
                    for artifact in &artifacts {
                        assert_eq!(
                            store
                                .current()
                                .record(
                                    Collection::Artifact,
                                    artifact.spec.id.as_str(),
                                    &config.workspace
                                )
                                .unwrap()
                                .decode::<ArtifactDescriptor>()
                                .unwrap(),
                            *artifact
                        );
                    }
                    assert_eq!(count.load(Ordering::SeqCst), 1);
                    drop(store);
                    let (reopened, owner) = CanonicalHost::open(config.clone()).unwrap();
                    assert_eq!(
                        reopened.project().unwrap().tasks[&config.root_task].state,
                        TaskState::Paused
                    );
                    assert!(
                        reopened.has_output_continuation(id).is_err(),
                        "old retained binding cannot be recreated from diagnostic artifacts"
                    );
                    assert_eq!(count.load(Ordering::SeqCst), 1);
                    assert_eq!(
                        reopened.project().unwrap().ledgers[&config.root_task]
                            .settled
                            .get(),
                        2128
                    );
                    assert!(reopened.project().unwrap().effects.is_empty());
                    for artifact in artifacts {
                        assert!(!reopened.read_artifact(artifact.spec.id).unwrap().is_empty());
                    }
                    owner.close().await.unwrap();
                    continue;
                }
                let continuation = host.take_output_continuation(id).unwrap().unwrap();
                assert_eq!(continuation.evidence.len(), 2);
                for evidence in &continuation.evidence {
                    assert!(!host.read_artifact(evidence.clone()).unwrap().is_empty());
                }
                assert!(host.take_output_continuation(id).unwrap().is_none());
                host.begin_coding_turn(id, continuation.feedback).unwrap();
                coding_turn(&test, backend, mode, None).await;
                assert!(host.has_output_continuation(id).unwrap());
                let continuation = host.take_output_continuation(id).unwrap().unwrap();
                host.begin_coding_turn(id, continuation.feedback).unwrap();
                coding_turn(&test, backend, mode, None).await;
                assert!(!host.has_output_continuation(id).unwrap());
                let requests = observed.lock().unwrap();
                assert_eq!(requests[0]["max_output_tokens"], 4096);
                assert_eq!(requests[1]["max_output_tokens"], 8000);
                assert_eq!(requests[2]["max_output_tokens"], 8000);
                assert!(requests[1]
                    .to_string()
                    .contains("No tool call from that response was executed"));
            }
            let canonical_turn: vcp_domain::task::Turn = host
                .snapshot()
                .unwrap()
                .record(Collection::Turn, turn.as_str(), &config.workspace)
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(
                canonical_turn.state,
                if matches!(mode, "complete" | "nested" | "process_fail" | "quoted_root") {
                    vcp_domain::task::TurnState::Verifying
                } else if mode == "incomplete_usage" {
                    vcp_domain::task::TurnState::Failed
                } else {
                    vcp_domain::task::TurnState::Paused
                },
                "{backend:?} {mode}"
            );
            let expected = if matches!(mode, "complete" | "nested" | "process_fail" | "quoted_root")
            {
                5
            } else if mode == "incomplete_usage" {
                3
            } else if mode == "limit" {
                2
            } else {
                1
            };
            assert_eq!(count.load(Ordering::SeqCst), expected, "{backend:?} {mode}");
            let requests = observed.lock().unwrap().clone();
            for (used, request) in requests.iter().enumerate() {
                assert_allowance(
                    &host,
                    request,
                    &config.root_task,
                    if mode == "limit" { 2 } else { 8 },
                    used,
                );
            }
            assert_eq!(requests[0]["parallel_tool_calls"], true);
            if mode == "quoted_root" {
                let result = |index: usize| -> serde_json::Value {
                    let item = requests[index + 1]["input"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|item| {
                            item["type"] == "function_call_output"
                                && item["call_id"] == format!("call-{index}")
                        })
                        .unwrap();
                    serde_json::from_str(item["output"].as_str().unwrap()).unwrap()
                };
                for index in [0, 2] {
                    let rejected = result(index);
                    assert!(
                        rejected.to_string().contains("zero characters"),
                        "{rejected}"
                    );
                    assert!(!rejected.to_string().contains("alternate stream"));
                }
                assert_eq!(result(1)["exit_code"], 0);
                assert!(result(3).to_string().contains("fixture.txt"));
            }
            if mode == "complete" {
                let input = requests[1]["input"].as_array().unwrap();
                for (call, expected) in [("call-0", "before"), ("sibling-read", "answer = 42")] {
                    let output = input
                        .iter()
                        .find(|item| {
                            item["type"] == "function_call_output" && item["call_id"] == call
                        })
                        .unwrap();
                    assert!(
                        output["output"].as_str().unwrap().contains(expected),
                        "result must retain original call correlation: {output}"
                    );
                }
            }
            let first = requests[0].to_string();
            assert!(first.contains("Observe retained request and response"));
            assert!(!first.contains("Run the synthetic request."));
            assert!(first.contains("instruction version one"));
            assert!(first.contains("not_ready"));
            assert!(first.contains("fixed_qualified_model"));
            assert!(!first.contains("nested scoped guidance"));
            if mode == "nested" {
                assert!(requests[1].to_string().contains("nested scoped guidance"));
                let item = requests[1]["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|item| item["type"] == "function_call_output")
                    .unwrap();
                let refreshed: serde_json::Value =
                    serde_json::from_str(item["output"].as_str().unwrap()).unwrap();
                assert_eq!(refreshed["executed"], false);
                assert_eq!(refreshed["code"], "instruction_scope_refresh");
                let scope = host.project().unwrap().tasks[&config.root_task]
                    .scope
                    .clone();
                let diagnostics = host.execution_diagnostics(scope).unwrap();
                use vcp_lifecycle::foundation::execution_diagnostics::{Phase, Status};
                for status in [Status::Skipped, Status::Succeeded] {
                    assert!(
                        diagnostics
                            .observations
                            .iter()
                            .any(|span| span.phase == Phase::ToolDispatch && span.status == status),
                        "scope refresh must be skipped before a later independently admitted dispatch"
                    );
                }
                assert!(item["output"]
                    .as_str()
                    .unwrap()
                    .contains("New instruction scope selected"));
            }
            if !matches!(
                mode,
                "missing_cost"
                    | "incomplete_usage"
                    | "invalid_call_usage"
                    | "stale_instructions"
                    | "deadline"
                    | "empty"
                    | "quoted_root"
            ) {
                if matches!(mode, "complete" | "process_fail") {
                    assert!(requests[2].to_string().contains("instruction version two"));
                    let item = requests[4]["input"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|item| {
                            item["type"] == "function_call_output" && item["call_id"] == "call-3"
                        })
                        .unwrap();
                    let output: serde_json::Value =
                        serde_json::from_str(item["output"].as_str().unwrap()).unwrap();
                    assert_eq!(
                        output["exit_code"],
                        if mode == "process_fail" { 1 } else { 0 }
                    );
                    let stdout =
                        ArtifactId::parse(output["stdout"]["artifact"].as_str().unwrap()).unwrap();
                    if mode == "complete" {
                        assert!(String::from_utf8(host.read_artifact(stdout).unwrap())
                            .unwrap()
                            .contains("fixture assertion passed"));
                    } else {
                        assert!(output["stderr"]["tail"]
                            .as_str()
                            .unwrap()
                            .contains("fixture assertion failed"));
                    }
                }
                assert!(requests[1]["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|item| item["type"] == "function_call_output"
                        && item["call_id"] == "call-0"));
                assert_eq!(
                    std::fs::read_to_string(workspace.join("file.txt")).unwrap(),
                    "after\n"
                );
            } else {
                assert_eq!(
                    std::fs::read_to_string(workspace.join("file.txt")).unwrap(),
                    "before\n"
                );
            }
            let view = if mode == "invalid_call_usage" {
                // The rejected response still fences this owner. Accounting
                // cleanup must succeed without reopening ordinary admission.
                assert!(host.project().unwrap_err().contains("fenced"));
                let state = host.snapshot().unwrap();
                vcp_audit::projection::rebuild(&state, &config.workspace, 2, state.watermark)
                    .unwrap()
            } else {
                host.project()
                    .unwrap_or_else(|error| panic!("{backend:?} {mode}: {error}"))
            };
            if matches!(mode, "incomplete_usage" | "invalid_call_usage") {
                assert_eq!(view.ledgers[&config.root_task].unresolved.known().unwrap().get(), 0);
                assert_eq!(view.ledgers[&config.root_task].active.known().unwrap().get(), 0);
                assert!(host.complete_coding_turn(id).is_err());
                assert!(view.effects.is_empty(), "partial patch never dispatched");
            }
            if mode == "incomplete_usage" {
                let state = host.snapshot().unwrap();
                let mut outputs: Vec<_> = state
                    .records
                    .values()
                    .filter(|record| record.collection == Collection::Attempt)
                    .map(|record| {
                        record
                            .decode::<vcp_domain::accounting::Attempt>()
                            .unwrap()
                            .quote
                            .bounds
                            .output
                            .get()
                    })
                    .collect();
                outputs.sort_unstable();
                assert_eq!(outputs, vec![4096, 8000, 8000]);
            }
            if mode == "empty" {
                assert!(
                    host.complete_coding_turn(id).is_err(),
                    "empty response must not establish an accounted final answer"
                );
            }
            assert_eq!(
                view.effects.len(),
                if mode == "complete" {
                    5
                } else if mode == "process_fail" {
                    4
                } else if mode == "nested" {
                    3
                } else if mode == "limit" {
                    2
                } else if mode == "quoted_root" {
                    2
                } else {
                    0
                }
            );
            assert_eq!(
                view.effects
                    .values()
                    .filter(|effect| effect.state == vcp_domain::effect::EffectState::Failed)
                    .count(),
                usize::from(mode == "process_fail")
            );
            assert!(view.effects.values().all(|effect| matches!(
                effect.state,
                vcp_domain::effect::EffectState::Succeeded
                    | vcp_domain::effect::EffectState::Failed
            )));
            if matches!(
                mode,
                "limit"
                    | "missing_cost"
                    | "incomplete_usage"
                    | "invalid_call_usage"
                    | "stale_instructions"
                    | "deadline"
                    | "empty"
            ) {
                assert_eq!(view.tasks[&config.root_task].state, TaskState::Paused);
            }
            assert_eq!(
                view.ledgers[&config.root_task].settled.get(),
                if unknown_cost || matches!(mode, "missing_cost" | "deadline") {
                    0
                } else {
                    if mode == "incomplete_usage" {
                        6384
                    } else if mode == "invalid_call_usage" {
                        1210
                    } else {
                        expected as u64 * 100
                    }
                }
            );
            if unknown_cost {
                assert!(!view.ledgers[&config.root_task].unresolved.is_zero());
                if unpriced {
                    assert!(view.ledgers[&config.root_task].unresolved.known().is_none());
                    assert_eq!(view.ledgers[&config.root_task].unresolved.known_component(), Micros::ZERO);
                } else {
                    assert!(view.ledgers[&config.root_task].unresolved.known().unwrap() > Micros::ZERO);
                }
                assert_eq!(view.ledgers[&config.root_task].active, Micros::ZERO);
                let attempts: Vec<vcp_domain::accounting::Attempt> = host
                    .snapshot()
                    .unwrap()
                    .records
                    .values()
                    .filter(|row| row.collection == Collection::Attempt)
                    .map(|row| row.decode().unwrap())
                    .collect();
                assert_eq!(attempts.len(), expected);
                if unpriced {
                    let wire = wire_bytes.lock().unwrap();
                    for (body, bytes) in requests.iter().zip(wire.iter()) {
                        assert_eq!(body["model"], "fixture/economical");
                        assert!(body["provider"].get("max_price").is_none());
                        assert_eq!(body["provider"]["only"], serde_json::json!(["fixture/economical-region"]));
                        assert_eq!(body["provider"]["allow_fallbacks"], false);
                        assert_eq!(body["provider"]["data_collection"], "deny");
                        let digest = vcp_protocol::digest_bytes(bytes);
                        let matches: Vec<_> = attempts.iter().filter(|attempt| attempt.request_digest == digest).collect();
                        assert_eq!(matches.len(), 1);
                        let attempt = matches[0];
                        assert_eq!(host.read_artifact(attempt.request.clone()).unwrap(), *bytes);
                        assert_eq!(attempt.quote.normalization_version, 2);
                        assert!(attempt.quote.amount.micros.known().is_none());
                        assert_eq!(attempt.quote.bounds.output.get(), body["max_output_tokens"].as_u64().unwrap());
                        let reservation: vcp_domain::accounting::Reservation = host.snapshot().unwrap()
                            .record(Collection::Reservation, attempt.reservation.as_str(), &config.workspace)
                            .unwrap().decode().unwrap();
                        assert_eq!(reservation.amount, attempt.quote.amount);
                        assert_eq!(reservation.liability, attempt.quote.amount.micros);
                    }
                }
                assert!(attempts.iter().all(|attempt| attempt.phase
                    == vcp_domain::accounting::ReservationState::ReconciliationPending
                    && attempt.send_intent.is_some()));
                let proofs: Vec<ArtifactDescriptor> = host.snapshot().unwrap().records.values()
                    .filter(|row| row.collection == Collection::Artifact)
                    .map(|row| row.decode::<ArtifactDescriptor>().unwrap())
                    .filter(|artifact| artifact.spec.schema == "provider-completed-execution/1").collect();
                assert_eq!(proofs.len(), expected, "one proof per complete missing-cost response");
                for proof in proofs {
                    let value: serde_json::Value = serde_json::from_slice(&host.read_artifact(proof.spec.id).unwrap()).unwrap();
                    let raw = ArtifactId::parse(value["raw"]["spec"]["id"].as_str().unwrap()).unwrap();
                    let captured = host.read_artifact(raw).unwrap();
                    let identity = vcp_models::stream::retained_completed_terminal(&captured).unwrap();
                    assert_eq!(identity.map(|identity| identity.request_id), value["response_id"].as_str().map(str::to_owned));
                }
                for attempt in attempts {
                    assert!(host.completed_financial_uncertainty(attempt).unwrap());
                }
            }
            if mode == "missing_cost" || mode == "deadline" {
                for row in host.snapshot().unwrap().records.values().filter(|row| row.collection == Collection::Attempt) {
                    assert!(!host.completed_financial_uncertainty(row.decode().unwrap()).unwrap());
                }
            }
            assert!(codex_extension_api::HostWorkAdmission::admit_tool(
                &host,
                id,
                "call-0",
                &codex_extension_api::ToolName::plain("vcp_read")
            )
            .is_err());
            assert!(codex_extension_api::HostWorkAdmission::admit_tool(
                &host,
                id,
                "call-0",
                &codex_extension_api::ToolName::plain("exec_command")
            )
            .is_err());
            owner.close().await.unwrap();
            test.codex.shutdown_and_wait().await.unwrap();
            drop(test);
            drop(host);
            if matches!(mode, "incomplete_usage" | "invalid_call_usage") {
                let (reopened, owner) = CanonicalHost::open(config.clone()).unwrap();
                let restored = reopened.project().unwrap();
                assert_eq!(
                    restored.ledgers[&config.root_task].settled.get(),
                    if mode == "incomplete_usage" {
                        6384
                    } else {
                        1210
                    }
                );
                assert_eq!(restored.ledgers[&config.root_task].unresolved.known().unwrap().get(), 0);
                assert_eq!(restored.tasks[&config.root_task].state, TaskState::Paused);
                assert!(restored.effects.is_empty());
                owner.close().await.unwrap();
            }
            if unknown_cost {
                let (reopened, owner) = CanonicalHost::open(config.clone()).unwrap();
                let restored = reopened.project().unwrap();
                assert_eq!(
                    restored.ledgers[&config.root_task].unresolved,
                    view.ledgers[&config.root_task].unresolved
                );
                assert_eq!(restored.ledgers[&config.root_task].settled, Micros::ZERO);
                assert_eq!(restored.effects.len(), view.effects.len());
                for row in reopened.snapshot().unwrap().records.values().filter(|row| row.collection == Collection::Attempt) {
                    assert!(reopened.completed_financial_uncertainty(row.decode().unwrap()).unwrap());
                }
                assert_eq!(
                    count.load(Ordering::SeqCst),
                    expected,
                    "reopen sends no provider request"
                );
                assert_eq!(
                    std::fs::read_to_string(workspace.join("file.txt")).unwrap(),
                    "after\n"
                );
                // Even identical duplicate proof records are ambiguous. Retain
                // all bytes, but fail closed when interpreting the report.
                use vcp_store::artifact::ArtifactWriter;
                let proof = reopened.snapshot().unwrap().records.values()
                    .filter(|row| row.collection == Collection::Artifact)
                    .map(|row| row.decode::<ArtifactDescriptor>().unwrap())
                    .find(|artifact| artifact.spec.schema == "provider-completed-execution/1").unwrap();
                let bytes = reopened.read_artifact(proof.spec.id.clone()).unwrap();
                let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                let id = AttemptId::parse(value["attempt"].as_str().unwrap()).unwrap();
                let attempt = reopened.snapshot().unwrap().record(Collection::Attempt, id.as_str(), &config.workspace).unwrap().decode().unwrap();
                assert!(reopened.completed_financial_uncertainty(attempt).unwrap());
                let spool = vcp_store::artifact::Spool::open(&config.canonical_root.join("spool"), &[workspace.clone()], config.artifact_limit.get()).unwrap();
                let mut spec = proof.spec;
                spec.id = ArtifactId::new();
                let mut writer = spool.create(spec).unwrap();
                writer.write_chunk(&bytes).unwrap();
                let duplicate = writer.finalize().unwrap();
                drop(writer);
                reopened.command(Command::AttachArtifact { descriptor: duplicate }, Some(config.root_task.clone()), Revision::ZERO).unwrap();
                let attempt = reopened.snapshot().unwrap().record(Collection::Attempt, id.as_str(), &config.workspace).unwrap().decode().unwrap();
                assert!(!reopened.completed_financial_uncertainty(attempt).unwrap());
                owner.close().await.unwrap();
            }
            if mode == "complete" && !unknown_cost && !unbounded_time {
                std::fs::write(
                    workspace.join("AGENTS.md"),
                    "instruction version three after reopen",
                )
                .unwrap();
                let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
                let (snapshot, raw) = provider_snapshot();
                host.configure_provider(snapshot, raw).unwrap();
                let mut registry = ExtensionRegistryBuilder::new();
                registry.turn_start_admission(Arc::new(host.clone()));
                registry.work_admission(Arc::new(host.clone()));
                registry.tool_contributor(Arc::new(host.clone()));
                let starter = host.clone();
                let cwd = workspace.clone();
                let test = test_codex()
                    .with_extensions(Arc::new(registry.build()))
                    .with_auth(codex_login::CodexAuth::from_api_key(
                        "synthetic-coding-reopen",
                    ))
                    .with_allowed_tools(allowed_tools())
                    .with_config(move |config| {
                        config.cwd = cwd.try_into().unwrap();
                        configure_provider_fixture(config);
                        starter
                            .lifecycle()
                            .authorize_startup(config.cwd.as_path(), None)
                            .unwrap();
                    })
                    .build_with_auto_env(&server)
                    .await
                    .unwrap();
                let id = host.lifecycle().attach_root(test.codex.clone()).unwrap();
                host.register(id, binding.clone()).unwrap();
                let task = host.project().unwrap().tasks[&config.root_task].clone();
                host.resume(id, task.revision, task.fingerprint).unwrap();
                host.configure_coding(
                    id,
                    CodingConfig {
                        canonical_tools: Default::default(),
                        operating: "Use prepared tools and report observed evidence only.".into(),
                        affected_paths: vec!["file.txt".into()],
                        max_requests: 6,
                        deadline: Timestamp::new(now + 300_000).into(),
                    },
                )
                .unwrap();
                assert!(codex_extension_api::HostWorkAdmission::admit_tool(
                    &host,
                    id,
                    "call-3",
                    &codex_extension_api::ToolName::plain("vcp_exec")
                )
                .is_err());
                coding_turn(&test, backend, "reopen", None).await;
                assert_eq!(count.load(Ordering::SeqCst), 6);
                let request = observed.lock().unwrap()[5].clone();
                let (allowance, _) = request_allowance(&request);
                assert_eq!(allowance["max_requests"], 6);
                assert_eq!(allowance["requests_used"], 5);
                assert_eq!(allowance["requests_remaining_including_this_request"], 1);
                assert!(request
                    .to_string()
                    .contains("instruction version three after reopen"));
                let old = requests[4]["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|item| item["type"] == "function_call_output")
                    .collect::<Vec<_>>();
                let restored = request["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|item| item["type"] == "function_call_output")
                    .collect::<Vec<_>>();
                assert_eq!(restored, old);
                let mut helper_binding = super::task(
                    &host,
                    &config,
                    TaskId::new(),
                    Some(config.root_task.clone()),
                );
                helper_binding.role = RequestRole::Helper;
                host.lifecycle()
                    .authorize_startup(&workspace, None)
                    .unwrap();
                let mut extension_init = codex_extension_api::ExtensionDataInit::default();
                extension_init.insert(AllowedTools(vec![]));
                let helper = test
                    .thread_manager
                    .start_thread(codex_core::StartThreadOptions {
                        thread_extension_init: extension_init,
                        environments: Some(test.codex.environment_selections().await),
                        ..codex_core::StartThreadOptions::new(test.config.clone())
                    })
                    .await
                    .unwrap()
                    .thread;
                let helper_id = host
                    .lifecycle()
                    .attach_child(
                        id,
                        &host.lifecycle().inspect(id).unwrap().revision,
                        helper.clone(),
                    )
                    .unwrap();
                host.register(helper_id, helper_binding).unwrap();
                host.configure_coding(
                    helper_id,
                    CodingConfig {
                        canonical_tools: Default::default(),
                        operating: "Bounded helper fixture".into(),
                        affected_paths: vec!["file.txt".into()],
                        max_requests: 128,
                        deadline: Timestamp::new(now + 300_000).into(),
                    },
                )
                .unwrap();
                helper
                    .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                        text: "A helper must not widen the root request limit".into(),
                        text_elements: vec![],
                    }]))
                    .await
                    .unwrap();
                coding_turn_complete(&helper, backend, "helper-limit", None).await;
                assert_eq!(
                    count.load(Ordering::SeqCst),
                    6,
                    "root request limit survives reopen and a larger helper allowance"
                );
                assert_eq!(
                    host.project().unwrap().tasks[&config.root_task].state,
                    TaskState::Paused
                );
                owner.close().await.unwrap();
                helper.shutdown_and_wait().await.unwrap();
                test.codex.shutdown_and_wait().await.unwrap();
            }
        }
    }
}

pub(super) fn request_allowance(request: &serde_json::Value) -> (serde_json::Value, String) {
    let found: Vec<_> = request["input"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["type"] == "message")
        .flat_map(|item| item["content"].as_array().unwrap())
        .filter_map(|content| {
            let text = content["text"].as_str()?;
            let part: serde_json::Value = serde_json::from_str(text).ok()?;
            let text = part["text"].as_str()?;
            let value: serde_json::Value = serde_json::from_str(text).ok()?;
            (value["kind"] == "canonical_root_request_allowance").then(|| (value, text.to_owned()))
        })
        .collect();
    assert_eq!(
        found.len(),
        1,
        "one fresh allowance observation per request"
    );
    found.into_iter().next().unwrap()
}

fn assert_allowance(
    host: &CanonicalHost,
    request: &serde_json::Value,
    root: &TaskId,
    limit: usize,
    used: usize,
) {
    let (allowance, text) = request_allowance(request);
    assert_eq!(allowance["root"], root.as_str());
    assert_eq!(allowance["max_requests"], limit);
    assert_eq!(allowance["requests_used"], used);
    assert_eq!(
        allowance["requests_remaining_including_this_request"],
        limit - used
    );
    let digest = vcp_protocol::digest_bytes(text.as_bytes());
    assert!(
        host.snapshot()
            .unwrap()
            .records
            .values()
            .filter(|record| record.collection == Collection::Artifact)
            .map(|record| record.decode::<ArtifactDescriptor>().unwrap())
            .any(|descriptor| descriptor.sha256 == digest
                && descriptor.spec.schema == "canonical-coding-content/1"
                && descriptor.state == CaptureState::Complete),
        "model-visible allowance must have canonical captured provenance"
    );
}

fn assert_adaptive_admission_trace(
    host: &CanonicalHost,
    bodies: &[serde_json::Value],
    wire: &[Vec<u8>],
    unresolved_index: Option<usize>,
) -> Vec<vcp_domain::request_allocation::Allocation> {
    let state = host.snapshot().unwrap();
    let attempts: Vec<Attempt> = state
        .records
        .values()
        .filter(|row| row.collection == Collection::Attempt)
        .map(|row| row.decode().unwrap())
        .collect();
    let manifests: Vec<vcp_context::manifest::Manifest> = state
        .records
        .values()
        .filter(|row| row.collection == Collection::Artifact)
        .map(|row| row.decode::<ArtifactDescriptor>().unwrap())
        .filter(|artifact| artifact.spec.schema == "context-manifest/1")
        .map(|artifact| {
            serde_json::from_slice(&host.read_artifact(artifact.spec.id).unwrap()).unwrap()
        })
        .collect();
    assert_eq!(attempts.len(), bodies.len());
    assert_eq!(wire.len(), bodies.len());
    bodies
        .iter()
        .zip(wire)
        .enumerate()
        .map(|(index, (body, bytes))| {
            let digest = vcp_protocol::digest_bytes(bytes);
            let matching: Vec<_> = attempts
                .iter()
                .filter(|attempt| attempt.request_digest == digest)
                .collect();
            assert_eq!(
                matching.len(),
                1,
                "each exact transmitted request has one admission"
            );
            let attempt = matching[0];
            assert_eq!(host.read_artifact(attempt.request.clone()).unwrap(), *bytes);
            assert_eq!(
                attempt.phase,
                if unresolved_index == Some(index) {
                    ReservationState::ReconciliationPending
                } else {
                    ReservationState::Settled
                }
            );
            let output = Units::new(body["max_output_tokens"].as_u64().unwrap());
            assert_eq!(attempt.quote.bounds.output, output);
            assert_eq!(attempt.quote.price.model, body["model"].as_str().unwrap());
            let reservation: vcp_domain::accounting::Reservation = state
                .record(
                    Collection::Reservation,
                    attempt.reservation.as_str(),
                    &attempt.scope.workspace,
                )
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(reservation.attempt, attempt.id);
            assert_eq!(reservation.amount, attempt.quote.amount);
            let manifest = manifests
                .iter()
                .find(|manifest| manifest.request_sha256 == digest)
                .unwrap();
            assert_eq!(manifest.envelope.output, output);
            let allocation = manifest.allocation.clone().unwrap();
            assert_eq!(allocation.output_limit, output);
            assert!(output <= allocation.host_output_ceiling);
            if let Some(record) = state.records.values().find(|row| {
                row.value["document_type"] == "vcp_routing_decision_v1"
                    && row.value["attempt"] == serde_json::json!(attempt.id)
            }) {
                assert_eq!(record.value["request_digest"], digest);
                let decision: vcp_models::routing::RoutingDecision =
                    serde_json::from_value(record.value["decision"].clone()).unwrap();
                decision.validate().unwrap();
                let selected = decision.selected.as_ref().unwrap();
                assert_eq!(selected.model, attempt.quote.price.model);
                assert_eq!(selected.endpoint, attempt.quote.price.provider);
                assert_eq!(
                    body["provider"]["only"],
                    serde_json::json!([selected.endpoint])
                );
                assert_eq!(decision.input.request_for(selected).unwrap().1, output);
            } else {
                assert!(
                    body["model"] != "fixture/economical" && body["model"] != "fixture/stronger",
                    "routed request requires decision evidence"
                );
            }
            allocation
        })
        .collect()
}

async fn coding_turn(test: &TestCodex, backend: BackendKind, mode: &str, diagnostics: Option<&CanonicalHost>) {
    test.codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: "Run the synthetic request.".into(),
            text_elements: vec![],
        }]))
        .await
        .unwrap();
    coding_turn_complete(&test.codex, backend, mode, diagnostics).await;
}

async fn coding_turn_complete(thread: &codex_core::CodexThread, backend: BackendKind, mode: &str, diagnostics: Option<&CanonicalHost>) {
    let mut last = None;
    // Native durable capture and process setup can contend with other local
    // qualification. Product request/deadline limits remain independently set.
    let result = tokio::time::timeout(Duration::from_secs(120), async {
        loop {
            let event = thread.next_event().await.unwrap();
            if mode == "routed_allocation" {
                if let EventMsg::Error(error) = &event.msg {
                    panic!("routed allocation request rejected: {error:?}");
                }
            }
            if matches!(event.msg, EventMsg::TurnComplete(_)) {
                break;
            }
            last = Some(event.msg);
        }
    })
    .await;
    if let Some(host) = diagnostics {
        eprintln!(
            "EE01 observed coding turn {backend:?}/{mode}: finished={} diagnostics={:?}",
            result.is_ok(),
            host.store_diagnostics()
        );
    }
    assert!(
        result.is_ok(),
        "{backend:?}/{mode}: turn did not finish; last event: {last:?}"
    );
}
