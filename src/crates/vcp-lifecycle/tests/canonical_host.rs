// SPDX-License-Identifier: Apache-2.0
use codex_core::TurnInputRequest;
use codex_extension_api::{AllowedTools, ExtensionRegistryBuilder};
use codex_protocol::{protocol::EventMsg, user_input::UserInput};
use core_test_support::{
    responses::*,
    test_codex::{test_codex, TestCodex},
    wait_for_event,
};
use std::{sync::Arc, time::Duration};
use vcp_domain::verification::Fingerprint;
use vcp_domain::{accounting::*, artifact::*, ids::*, revision::*, task::*, workspace::*};
use vcp_lifecycle::{
    foundation::{CanonicalHost, Config, ThreadBinding},
    integration::configure_fixture_provider,
};
#[path = "support/authority_stream.rs"]
mod authority_stream;
#[cfg(windows)]
#[path = "support/backup_checkpoint.rs"]
mod backup_checkpoint;
#[path = "support/cli_control.rs"]
mod cli_control;
#[cfg(windows)]
#[path = "support/coding.rs"]
mod coding;
#[cfg(windows)]
#[path = "support/coding_verification.rs"]
mod coding_verification;
#[cfg(windows)]
#[path = "support/context_continuity.rs"]
mod context_continuity;
#[cfg(windows)]
#[path = "support/framework_verification.rs"]
mod framework_verification;
#[path = "support/history_retention.rs"]
mod history_retention;
#[cfg(windows)]
#[path = "support/host_tool_authority.rs"]
mod host_tool_authority;
#[path = "support/memory_ingestion.rs"]
mod memory_ingestion;
#[cfg(windows)]
#[path = "support/memory_publication.rs"]
mod memory_publication;
#[cfg(windows)]
#[path = "support/memory_query.rs"]
mod memory_query;
#[cfg(windows)]
#[path = "support/memory_vectors.rs"]
mod memory_vectors;
#[cfg(windows)]
#[path = "support/parent_instructions.rs"]
mod parent_instructions;
#[path = "support/portable_accounting.rs"]
mod portable_accounting;
#[path = "support/portable_operator_fixture.rs"]
mod portable_operator_fixture;
#[cfg(windows)]
#[path = "support/portable_vectors.rs"]
mod portable_vectors;
#[cfg(windows)]
#[path = "support/process_broker.rs"]
mod process_broker;
#[path = "support/provider_retries.rs"]
mod provider_retries;
#[path = "support/response_boundary.rs"]
mod response_boundary;
#[cfg(windows)]
#[path = "support/restore_search.rs"]
mod restore_search;
#[cfg(windows)]
#[path = "support/routing.rs"]
mod routing;
#[path = "support/selected_reopen.rs"]
mod selected_reopen;
#[cfg(windows)]
#[path = "support/skills.rs"]
mod skills;
#[cfg(windows)]
#[path = "support/verification.rs"]
mod verification;

fn configure_provider_fixture(config: &mut codex_core::config::Config) {
    let fixture_url = config.model_provider.base_url.clone();
    vcp_lifecycle::foundation::openrouter::configure_transport(
        config,
        &vcp_engine::capture::ProviderCredential::from_config("synthetic-openrouter-key".into()),
        "fixture/coder",
    )
    .unwrap();
    assert_eq!(config.project_doc_max_bytes, 0);
    assert!(config.model_provider.env_key.is_none());
    assert!(config.model_provider.auth.is_none());
    assert!(!format!("{:?}", config.model_provider).contains("synthetic-openrouter-key"));
    config.model_provider.base_url = fixture_url;
    config.model = Some("gpt-5.1".into());
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn native_file_broker_enforces_current_policy_approvals_and_source_versions() {
    use std::collections::BTreeSet;
    use vcp_domain::policy::{Autonomy, Policy};
    use vcp_tools::Request;
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        std::fs::write(workspace.join("file.txt"), b"before\r\n").unwrap();
        let git = std::env::var_os("VCP_TEST_GIT")
            .expect("runner supplies the explicit Git fixture dependency");
        for args in [vec!["init", "--quiet"], vec!["add", "--", "file.txt"]] {
            assert!(std::process::Command::new(&git)
                .current_dir(&workspace)
                .args(args)
                .output()
                .unwrap()
                .status
                .success());
        }
        let original_index = std::fs::read(workspace.join(".git/index")).unwrap();
        let config = config(&temp.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let binding = task(&host, &config, config.root_task.clone(), None);
        let server = start_mock_server().await;
        let mut registry = ExtensionRegistryBuilder::new();
        registry.turn_start_admission(Arc::new(host.clone()));
        registry.work_admission(Arc::new(host.clone()));
        let starter = host.clone();
        let cwd = workspace.clone();
        let test = test_codex()
            .with_extensions(Arc::new(registry.build()))
            .with_auth(codex_login::CodexAuth::from_api_key(
                "synthetic-tool-fixture",
            ))
            .with_allowed_tools(AllowedTools(vec![]))
            .with_config(move |c| {
                c.cwd = cwd.try_into().unwrap();
                configure_fixture_provider(c);
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
        host.command(
            Command::SetWorkspaceTrust {
                trust: Trust::Trusted,
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        let mut policy = Policy {
            workspace: config.workspace.clone(),
            revision: PolicyRevision::ZERO,
            mode: Autonomy::Workspace,
            denials: vec![],
            workspace_roots: BTreeSet::from([RootId::parse(config.workspace.as_str()).unwrap()]),
            automatic_effects: BTreeSet::new(),
            timeout_ceiling_ms: Units::new(30_000),
            output_ceiling_bytes: ByteCount::new(1024 * 1024),
        };
        host.command(
            Command::SetPolicy {
                policy: policy.clone(),
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        let read = host
            .prepare_tool(
                id,
                Request::Read {
                    path: "file.txt".into(),
                    max_bytes: 1024,
                },
            )
            .unwrap();
        assert!(matches!(read.decision, vcp_policy::Decision::Allow { .. }));
        let output = host.dispatch_tool(read).unwrap();
        assert_eq!(output.result["text"], "before\r\n");
        let patch="*** Begin Patch\n*** Update File: file.txt\n@@\n-before\n+after\n*** Add File: new.txt\n+created\n*** End Patch";
        let prepared = host
            .prepare_tool(
                id,
                Request::Patch {
                    patch: patch.into(),
                },
            )
            .unwrap();
        std::fs::write(workspace.join("file.txt"), b"human\r\n").unwrap();
        assert!(host.dispatch_tool(prepared).is_err());
        assert!(!workspace.join("new.txt").exists());
        assert_eq!(
            std::fs::read(workspace.join("file.txt")).unwrap(),
            b"human\r\n"
        );
        std::fs::write(workspace.join("file.txt"), b"before\r\n").unwrap();
        let prepared = host
            .prepare_tool(
                id,
                Request::Patch {
                    patch: patch.into(),
                },
            )
            .unwrap();
        policy.revision = PolicyRevision::new(1);
        policy.mode = Autonomy::Plan;
        host.command(
            Command::SetPolicy {
                policy: policy.clone(),
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        std::fs::rename(workspace.join("file.txt"), workspace.join("hidden.txt")).unwrap();
        let error = host.dispatch_tool(prepared).err().unwrap();
        assert!(
            error.contains("authority rejected before native revalidation"),
            "{error}"
        );
        assert!(!workspace.join("new.txt").exists());
        std::fs::rename(workspace.join("hidden.txt"), workspace.join("file.txt")).unwrap();
        let denied = host
            .prepare_tool(
                id,
                Request::Patch {
                    patch: patch.into(),
                },
            )
            .unwrap();
        assert!(matches!(denied.decision, vcp_policy::Decision::Deny { .. }));
        assert!(host.dispatch_tool(denied).is_err());
        policy.revision = PolicyRevision::new(2);
        policy.mode = Autonomy::Ask;
        host.command(
            Command::SetPolicy {
                policy: policy.clone(),
            },
            None,
            Revision::new(1),
        )
        .unwrap();
        let prepared = host
            .prepare_tool(
                id,
                Request::Patch {
                    patch: patch.into(),
                },
            )
            .unwrap();
        assert!(matches!(
            prepared.decision,
            vcp_policy::Decision::Question { .. }
        ));
        let approval = prepared.question.clone().unwrap();
        let current: Task = host
            .snapshot()
            .unwrap()
            .record(
                Collection::Task,
                binding.scope.task.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(current.state, TaskState::WaitingForInput);
        host.command(
            Command::Decide {
                id: approval,
                operation_digest: prepared.digest().into(),
                effect_revision: Revision::new(1),
                allow: true,
            },
            Some(binding.scope.task.clone()),
            Revision::ZERO,
        )
        .unwrap();
        assert_eq!(
            std::fs::read(workspace.join("file.txt")).unwrap(),
            b"before\r\n"
        );
        assert!(!workspace.join("new.txt").exists());
        host.resume(id, current.revision, current.fingerprint)
            .unwrap();
        let result = host.dispatch_tool(prepared).unwrap();
        assert_eq!(result.result["complete"], true, "{}", result.result);
        assert_eq!(
            std::fs::read(workspace.join("file.txt")).unwrap(),
            b"after\r\n"
        );
        assert_eq!(
            std::fs::read(workspace.join("new.txt")).ok().as_deref(),
            Some(b"created\n".as_slice()),
            "{}",
            result.result
        );
        let state = host.snapshot().unwrap();
        let effect: vcp_domain::effect::Effect = state
            .record(
                Collection::Effect,
                result.effect.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(effect.state, vcp_domain::effect::EffectState::Succeeded);
        assert_eq!(effect.observed_changes.len(), 6);
        let bytes = host.read_artifact(result.evidence.spec.id).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
            result.result
        );
        // Repeating the same explicitly approved operation after the caller
        // restores its original source needs no second question.
        std::fs::write(workspace.join("file.txt"), b"before\r\n").unwrap();
        std::fs::remove_file(workspace.join("new.txt")).unwrap();
        let reused = host
            .prepare_tool(
                id,
                Request::Patch {
                    patch: patch.into(),
                },
            )
            .unwrap();
        assert!(matches!(
            reused.decision,
            vcp_policy::Decision::Allow { grant: Some(_), .. }
        ));
        assert!(reused.question.is_none());
        let reused_result = host.dispatch_tool(reused).unwrap().result;
        assert_eq!(reused_result["complete"], true, "{reused_result}");
        assert_eq!(
            std::fs::read(workspace.join("file.txt")).unwrap(),
            b"after\r\n"
        );
        assert!(server.received_requests().await.unwrap().is_empty());
        assert_eq!(
            std::fs::read(workspace.join(".git/index")).unwrap(),
            original_index
        );
        // A later-file sharing conflict must preserve the earlier committed
        // effect and the remaining human bytes, without automatic rollback.
        policy.revision = PolicyRevision::new(3);
        policy.mode = Autonomy::Workspace;
        host.command(Command::SetPolicy { policy }, None, Revision::new(2))
            .unwrap();
        let partial=host.prepare_tool(id,Request::Patch{patch:"*** Begin Patch\n*** Update File: file.txt\n@@\n-after\n+first-applied\n*** Update File: new.txt\n@@\n-created\n+second-applied\n*** End Patch".into()}).unwrap();
        use std::os::windows::fs::OpenOptionsExt;
        let locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(workspace.join("new.txt"))
            .unwrap();
        let partial = host.dispatch_tool(partial).unwrap();
        assert_eq!(partial.result["complete"], false);
        assert_eq!(partial.result["files"].as_array().unwrap().len(), 1);
        assert_eq!(
            std::fs::read(workspace.join("file.txt")).unwrap(),
            b"first-applied\r\n"
        );
        assert_eq!(
            std::fs::read(workspace.join("new.txt")).unwrap(),
            b"created\n"
        );
        drop(locked);
        assert_eq!(
            std::fs::read(workspace.join(".git/index")).unwrap(),
            original_index
        );
        owner.close().await.unwrap();
    }
}
fn provider_snapshot() -> (vcp_models::catalog::Snapshot, Vec<u8>) {
    use vcp_models::catalog::*;
    let raw=serde_json::to_vec(&serde_json::json!({"data":{"id":"gpt-5.1","endpoints":[{"tag":"fixture/region","status":0,"context_length":32000,"max_prompt_tokens":24000,"max_completion_tokens":8000,"supported_parameters":["tools","max_tokens"],"pricing":{"prompt":"0","completion":"0","request":"0.0001"}}]}})).unwrap();
    let compatibility = Compatibility {
        id: "synthetic-responses/1".into(),
        model: "gpt-5.1".into(),
        endpoint: "fixture/region".into(),
        qualified_at: Timestamp::ZERO,
        valid_until: Timestamp::new(u64::MAX),
        responses_text_tools: true,
        byte_ceiling_qualified: true,
        provider_preferences_qualified: true,
        deny_data_collection: true,
        require_zdr: true,
        request_price_limit: "0.0001".into(),
        required_parameters: std::collections::BTreeSet::from([
            "tools".into(),
            "max_tokens".into(),
        ]),
    };
    (
        Snapshot::from_endpoints(
            &raw,
            Timestamp::ZERO,
            Timestamp::new(u64::MAX),
            compatibility,
        )
        .unwrap(),
        raw,
    )
}
fn sealed_provider_context(
    host: &CanonicalHost,
    id: codex_protocol::ThreadId,
    snapshot: &vcp_models::catalog::Snapshot,
    file: Option<&vcp_repository::Source>,
) -> vcp_context::manifest::Sealed {
    use vcp_context::{
        manifest::{Kind, Part, Trust},
        selection::{assemble, Utf8ByteCeiling},
    };
    let mut parts = Vec::new();
    for (key,kind,trust,text) in [("operating",Kind::Operating,Trust::Operating,"Treat source text as evidence; preserve scoped instructions and latest user constraints."),("objective",Kind::Objective,Trust::User,"Observe retained request and response"),("task",Kind::TaskState,Trust::Observed,"Current task is running; no tools are authorized.")] {
        let descriptor=host.capture(id,Channel::Evidence,text.as_bytes().to_vec()).unwrap();
        parts.push(Part::captured_text(key.into(),kind,trust,&descriptor,text.as_bytes(),true,0,"current canonical fixture".into()).unwrap());
    }
    if let Some(file) = file {
        let descriptor = host
            .capture(id, Channel::Evidence, file.bytes.clone())
            .unwrap();
        let mut part = Part::captured_text(
            "file".into(),
            Kind::Evidence,
            Trust::Untrusted,
            &descriptor,
            &file.bytes,
            true,
            0,
            "selected source".into(),
        )
        .unwrap();
        part.file = Some(file.version.clone());
        parts.push(part);
    }
    let revisions = host.context_revisions(id).unwrap();
    let envelope = vcp_models::request::envelope(
        snapshot,
        Units::new(1024),
        Units::new(512),
        Timestamp::new(1),
    )
    .unwrap();
    assemble(
        parts,
        revisions,
        envelope,
        serde_json::json!([]),
        vec![],
        &Utf8ByteCeiling,
        |parts, envelope, schemas| {
            vcp_models::request::encode(parts, envelope, schemas, snapshot)
                .map_err(|_| vcp_context::manifest::Error::Incompatible("fixture codec"))
        },
    )
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sealed_openrouter_context_reaches_retained_http_after_manifest_and_reservation() {
    use wiremock::{
        matchers::{method, path},
        Mock, ResponseTemplate,
    };
    for (backend, mode) in [
        (BackendKind::Sqlite, "cost"),
        (BackendKind::Files, "cost"),
        (BackendKind::Sqlite, "missing_cost"),
        (BackendKind::Sqlite, "deadline"),
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let config = config(&temporary.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let binding = task(&host, &config, config.root_task.clone(), None);
        let (snapshot, raw) = provider_snapshot();
        host.configure_provider_with_timeout(
            snapshot.clone(),
            raw,
            if mode == "deadline" {
                Duration::from_millis(500)
            } else {
                Duration::from_secs(120)
            },
        )
        .unwrap();
        let server = start_mock_server().await;
        let expected = Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
        let observed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let body = expected.clone();
        let calls = observed.clone();
        let inspect = host.clone();
        Mock::given(method("POST")).and(path("/v1/responses")).respond_with(move|request:&wiremock::Request|{
            assert_eq!(&request.body,body.lock().unwrap().as_slice());
            let state=inspect.snapshot().unwrap();
            assert!(state.records.values().filter(|r|r.collection==Collection::Artifact).map(|r|r.decode::<ArtifactDescriptor>().unwrap()).any(|d|d.spec.schema=="context-manifest/1"));
            let attempt=state.records.values().filter(|r|r.collection==Collection::Attempt).map(|r|r.decode::<Attempt>().unwrap()).find(|a|a.request_digest==vcp_protocol::digest_bytes(&request.body)).unwrap();
            assert_eq!(attempt.phase,ReservationState::Submitted);assert!(attempt.send_intent.is_some());
            assert_eq!(attempt.quote.bounds.cache_read.get(),request.body.len() as u64);
            calls.fetch_add(1,std::sync::atomic::Ordering::SeqCst);
            let cost=if mode=="missing_cost"{serde_json::Value::Null}else{serde_json::json!(0.0001)};
            let response=ResponseTemplate::new(200).insert_header("content-type","text/event-stream").set_body_string(sse(vec![ev_assistant_message("msg","Observed sealed request."),serde_json::json!({"type":"response.completed","response":{"id":"openrouter-synthetic","status":"completed","output":[],"usage":{"input_tokens":10,"output_tokens":4,"total_tokens":14,"input_tokens_details":{"cached_tokens":0},"output_tokens_details":{"reasoning_tokens":0},"cost":cost}}})]));
            if mode=="deadline"{response.set_delay(Duration::from_secs(2))}else{response}
        }).mount(&server).await;
        let mut registry = ExtensionRegistryBuilder::new();
        registry.turn_start_admission(Arc::new(host.clone()));
        registry.work_admission(Arc::new(host.clone()));
        let starter = host.clone();
        let cwd = workspace.clone();
        let test = test_codex()
            .with_extensions(Arc::new(registry.build()))
            .with_auth(codex_login::CodexAuth::from_api_key(
                "synthetic-openrouter-key",
            ))
            .with_allowed_tools(AllowedTools(vec![]))
            .with_config(move |config| {
                config.cwd = cwd.try_into().unwrap();
                config.model = Some("gpt-5.1".into());
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
        let sealed = sealed_provider_context(&host, id, &snapshot, None);
        *expected.lock().unwrap() = sealed.body().to_vec();
        host.prepare_context(id, sealed, serde_json::json!([]), vec![])
            .unwrap();
        turn(&test).await;
        assert_eq!(observed.load(std::sync::atomic::Ordering::SeqCst), 1);
        let state = host.snapshot().unwrap();
        let ledger = vcp_budget::ledger(&state, &binding.scope).unwrap();
        assert_eq!(
            ledger.settled,
            Micros::new(if mode == "cost" { 100 } else { 0 })
        );
        assert_eq!(
            ledger.unresolved,
            Micros::new(if mode == "cost" { 0 } else { 100 })
        );
        if mode == "deadline" {
            owner.close().await.unwrap();
            continue;
        }
        let normalized = state
            .records
            .values()
            .filter(|r| r.collection == Collection::Artifact)
            .map(|r| r.decode::<ArtifactDescriptor>().unwrap())
            .find(|d| d.spec.schema == "openrouter-normalized-response/1")
            .unwrap();
        let result: vcp_models::stream::ResultBody =
            serde_json::from_slice(&host.read_artifact(normalized.spec.id).unwrap()).unwrap();
        assert!(result.served_model.is_none());
        assert!(result.served_provider.is_none());
        owner.close().await.unwrap();
    }
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn provider_send_fence_rejects_changed_source_steering_and_policy_before_billable_admission()
{
    use codex_extension_api::{HostModelPurpose, HostWorkAdmission};
    let temporary = tempfile::tempdir().unwrap();
    let workspace = temporary.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    let config = config(
        &temporary.path().join("canonical"),
        &workspace,
        BackendKind::Sqlite,
    );
    let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
    let binding = task(&host, &config, config.root_task.clone(), None);
    let (snapshot, raw) = provider_snapshot();
    host.configure_provider(snapshot.clone(), raw).unwrap();
    let server = start_mock_server().await;
    let mut registry = ExtensionRegistryBuilder::new();
    registry.turn_start_admission(Arc::new(host.clone()));
    registry.work_admission(Arc::new(host.clone()));
    let starter = host.clone();
    let cwd = workspace.clone();
    let test = test_codex()
        .with_extensions(Arc::new(registry.build()))
        .with_auth(codex_login::CodexAuth::from_api_key(
            "synthetic-openrouter-key",
        ))
        .with_allowed_tools(AllowedTools(vec![]))
        .with_config(move |config| {
            config.cwd = cwd.try_into().unwrap();
            config.model = Some("gpt-5.1".into());
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
    let root = vcp_repository::Root::open(
        vcp_repository::RootIdentity {
            workspace: config.workspace.clone(),
            root: RootId::new(),
            repository: config.binding.repository.clone(),
            worktree: config.binding.worktree.clone(),
            binding: config.binding.revision,
        },
        &workspace,
    )
    .unwrap();
    std::fs::write(workspace.join("file.txt"), "first").unwrap();
    let source = root.read(std::path::Path::new("file.txt"), 1024).unwrap();
    let sealed = sealed_provider_context(&host, id, &snapshot, Some(&source));
    host.prepare_context(id, sealed, serde_json::json!([]), vec![root])
        .unwrap();
    std::fs::write(workspace.join("file.txt"), "human edit").unwrap();
    assert!(host
        .admit_model(
            id,
            &mut serde_json::json!({"model":"gpt-5.1"}),
            HostModelPurpose::Turn
        )
        .is_err());
    let sealed = sealed_provider_context(&host, id, &snapshot, None);
    host.prepare_context(id, sealed, serde_json::json!([]), vec![])
        .unwrap();
    let state = host.snapshot().unwrap();
    let task: Task = state
        .record(
            Collection::Task,
            binding.scope.task.as_str(),
            &config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap();
    host.command(
        Command::Steer {
            objective: Objective {
                text: "New constraint".into(),
                constraints: vec!["read only".into()],
                acceptance: vec!["independent transport agrees".into()],
                source: EventId::new(),
                steering: task.steering.next().unwrap(),
            },
        },
        Some(binding.scope.task.clone()),
        task.revision,
    )
    .unwrap();
    assert!(host
        .admit_model(
            id,
            &mut serde_json::json!({"model":"gpt-5.1"}),
            HostModelPurpose::Turn
        )
        .is_err());
    for revision in 0..2 {
        use vcp_domain::policy::{Autonomy, Policy};
        let before = host.context_revisions(id).unwrap();
        let sealed = sealed_provider_context(&host, id, &snapshot, None);
        host.prepare_context(id, sealed, serde_json::json!([]), vec![])
            .unwrap();
        host.command(
            Command::SetPolicy {
                policy: Policy {
                    workspace: config.workspace.clone(),
                    revision: PolicyRevision::new(revision),
                    mode: Autonomy::Ask,
                    denials: vec![],
                    workspace_roots: Default::default(),
                    automatic_effects: Default::default(),
                    timeout_ceiling_ms: Units::new(1000),
                    output_ceiling_bytes: ByteCount::new(4096),
                },
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
        let after = host.context_revisions(id).unwrap();
        assert_eq!(after.authority, before.authority.next().unwrap());
        assert_eq!(after.policy, PolicyRevision::new(revision));
        assert!(host
            .admit_model(
                id,
                &mut serde_json::json!({"model":"gpt-5.1"}),
                HostModelPurpose::Turn
            )
            .is_err());
    }
    assert!(!host
        .snapshot()
        .unwrap()
        .records
        .values()
        .any(|r| r.collection == Collection::Attempt));
    assert!(server.received_requests().await.unwrap().is_empty());
    assert_eq!(
        std::fs::read(workspace.join("file.txt")).unwrap(),
        b"human edit"
    );
    owner.close().await.unwrap();
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fresh_process_history_preserves_actual_unknown_process_paused_child_and_late_charge() {
    use vcp_domain::effect::EffectState;
    use vcp_store::contract::{CanonicalStore, Mutation, Transaction};
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let config = config(&temporary.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let binding = task(&host, &config, config.root_task.clone(), None);
        let server = start_mock_server().await;
        let observed = mount_sse_sequence(
            &server,
            vec![sse(vec![
                ev_assistant_message("history", "Retain observations."),
                ev_completed_with_tokens("late-provider-response", 7),
            ])],
        )
        .await;
        let mut registry = ExtensionRegistryBuilder::new();
        registry.turn_start_admission(Arc::new(host.clone()));
        registry.work_admission(Arc::new(host.clone()));
        let starter = host.clone();
        let cwd = workspace.clone();
        let model = config.price.model.clone();
        let test = test_codex()
            .with_extensions(Arc::new(registry.build()))
            .with_auth(codex_login::CodexAuth::from_api_key(
                "public-synthetic-p1-token",
            ))
            .with_allowed_tools(AllowedTools(vec![]))
            .with_config(move |config| {
                config.cwd = cwd.try_into().unwrap();
                config.model = Some(model.clone());
                configure_fixture_provider(config);
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
        turn(&test).await;
        let child = task(
            &host,
            &config,
            TaskId::new(),
            Some(config.root_task.clone()),
        );
        host.command(
            Command::Transition {
                next: TaskState::Paused,
                reason: "child intentionally held".into(),
                verification: None,
            },
            Some(child.scope.task.clone()),
            Revision::new(1),
        )
        .unwrap();
        let process = host
            .spawn_process(
                id,
                Revision::new(1),
                std::path::Path::new(env!("CARGO_BIN_EXE_vcp-process-fixture")),
                &["tree".into(), workspace.as_os_str().into()],
                &workspace,
                &std::collections::BTreeMap::new(),
                128,
            )
            .unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while !workspace.join("child-ready").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let marker = std::fs::read(workspace.join("child-ready")).unwrap();
        drop(process);
        host.command(
            Command::Transition {
                next: TaskState::Paused,
                reason: "retain unknown process before reconciliation".into(),
                verification: None,
            },
            Some(config.root_task.clone()),
            Revision::new(1),
        )
        .unwrap();
        let state = host.snapshot().unwrap();
        let attempt = state
            .records
            .values()
            .find(|row| row.collection == Collection::Attempt)
            .unwrap()
            .decode::<Attempt>()
            .unwrap();
        let raw = host
            .capture(
                id,
                Channel::Evidence,
                b"{\"late_actual_micros\":120}".to_vec(),
            )
            .unwrap();
        host.observe_usage(UsageObservation {
            id: ObservationId::new(),
            scope: binding.scope.clone(),
            attempt: attempt.id,
            provider_request: "late-provider-response".into(),
            mode: UsageMode::Cumulative {
                version: Units::new(2),
            },
            amount: Money {
                currency: config.cap.currency.clone(),
                micros: Micros::new(120),
            },
            final_usage: true,
            raw: raw.spec.id,
            correction: None,
        })
        .unwrap();
        let live = host.project().unwrap();
        assert_eq!(live.tasks[&child.scope.task].state, TaskState::Paused);
        assert_eq!(live.ledgers[&config.root_task].settled.get(), 120);
        assert!(live
            .effects
            .values()
            .any(|effect| effect.state == EffectState::OutcomeUnknown));
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        drop(test);
        drop(host);
        let mut store = vcp_store::Store::open(&config.canonical_root, backend, &[])
            .await
            .unwrap();
        let mutations = store
            .state()
            .records
            .values()
            .filter(|row| row.collection == Collection::Projection)
            .map(|row| Mutation::DropProjection {
                id: row.id.clone(),
                expected: row.revision,
            })
            .collect();
        let tx = Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations,
            events: vec![],
            command: None,
        };
        store.transact(tx).await.unwrap();
        drop(store);
        let output = temporary.path().join("rebuilt.json");
        let result =
            std::process::Command::new(env!("CARGO_BIN_EXE_vcp-canonical-rebuild-fixture"))
                .arg(&config.canonical_root)
                .arg(if backend == BackendKind::Sqlite {
                    "sqlite"
                } else {
                    "files"
                })
                .arg(config.workspace.as_str())
                .arg(live.watermark.get().to_string())
                .arg(&output)
                .output()
                .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let rebuilt: vcp_audit::projection::View =
            serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
        assert_eq!(rebuilt, live);
        assert_eq!(observed.requests().len(), 1);
        assert_eq!(
            std::fs::read(workspace.join("child-ready")).unwrap(),
            marker
        );
    }
}
use vcp_protocol::command::Command;
use vcp_store::{contract::Collection, BackendKind};
fn config(root: &std::path::Path, workspace: &std::path::Path, backend: BackendKind) -> Config {
    let currency: Currency = "USD".to_owned().try_into().unwrap();
    Config {
        canonical_root: root.into(),
        backend,
        workspace: WorkspaceId::parse("canonical-workspace").unwrap(),
        session: SessionId::parse("canonical-session").unwrap(),
        binding: Binding {
            host: HostId::parse("native-fixture").unwrap(),
            root: workspace.to_string_lossy().into_owned(),
            repository: "synthetic".into(),
            worktree: "main".into(),
            revision: Revision::ZERO,
        },
        actor: ActorId::parse("fixture-owner").unwrap(),
        root_task: TaskId::parse("root-task").unwrap(),
        cap: Money {
            currency: currency.clone(),
            micros: Micros::new(1000),
        },
        protected: Micros::ZERO,
        price: PriceSnapshot {
            id: "a".repeat(64),
            provider: "scripted-loopback".into(),
            model: "gpt-5.1".into(),
            currency,
            capability: "b".repeat(64),
            valid_until: Timestamp::new(u64::MAX),
            rates: [
                ChargeCategory::Input,
                ChargeCategory::Output,
                ChargeCategory::CacheRead,
                ChargeCategory::CacheWrite,
                ChargeCategory::Request,
                ChargeCategory::ProviderTool,
            ]
            .into_iter()
            .map(|kind| {
                (
                    kind,
                    Rate {
                        micros: Micros::new(if kind == ChargeCategory::Request {
                            100
                        } else {
                            0
                        }),
                        per_units: Units::new(1),
                    },
                )
            })
            .collect(),
        },
        input_ceiling: Units::new(500_000),
        output_ceiling: Units::new(1024),
        artifact_limit: ByteCount::new(vcp_store::artifact::DEFAULT_ARTIFACT_LIMIT),
        host_tool_denials: vec![],
    }
}
fn task(
    host: &CanonicalHost,
    config: &Config,
    id: TaskId,
    parent: Option<TaskId>,
) -> ThreadBinding {
    host.command(
        Command::CreateTask {
            root: config.root_task.clone(),
            parent,
            fork_origin: None,
            objective: Objective {
                text: "Observe retained request and response".into(),
                constraints: vec![],
                acceptance: vec!["independent transport agrees".into()],
                source: EventId::new(),
                steering: SteeringRevision::ZERO,
            },
            fingerprint: Fingerprint {
                repository: "a".repeat(64),
                buffers: "b".repeat(64),
                environment: "c".repeat(64),
            },
            editing: false,
            required_checks: vec![],
        },
        Some(id.clone()),
        Revision::ZERO,
    )
    .unwrap();
    host.command(
        Command::Transition {
            next: TaskState::Running,
            reason: "explicit fixture start".into(),
            verification: None,
        },
        Some(id.clone()),
        Revision::ZERO,
    )
    .unwrap();
    ThreadBinding {
        scope: Scope {
            workspace: config.workspace.clone(),
            session: config.session.clone(),
            task: id,
        },
        agent: AgentId::new(),
        role: RequestRole::Main,
    }
}
async fn turn(test: &TestCodex) {
    test.codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: "Run the synthetic request.".into(),
            text_elements: vec![],
        }]))
        .await
        .unwrap();
    tokio::time::timeout(
        Duration::from_secs(30),
        wait_for_event(&test.codex, |event| {
            matches!(event, EventMsg::TurnComplete(_))
        }),
    )
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn root_helper_compaction_and_child_transport_require_one_shared_reservation() {
    use codex_core::{StartThreadOptions, TurnStartOptions};
    use codex_protocol::protocol::{Op, SessionSource, SubAgentSource};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    };
    use wiremock::{
        matchers::{method, path},
        Mock, ResponseTemplate,
    };
    let temporary = tempfile::tempdir().unwrap();
    let workspace = temporary.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let workspace = workspace.canonicalize().unwrap();
    let mut config = config(
        &temporary.path().join("canonical"),
        &workspace,
        BackendKind::Sqlite,
    );
    config.cap.micros = Micros::new(500);
    let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
    let root_binding = task(&host, &config, config.root_task.clone(), None);
    let server = start_mock_server().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::new(Mutex::new(Vec::<AttemptId>::new()));
    let inspect = host.clone();
    let count = calls.clone();
    let ids = observed.clone();
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(move |request: &wiremock::Request| {
            // Independent HTTP arrival observer: admission and send intent must
            // already be visible before the scripted provider emits any response.
            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            let digest = vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(&body).unwrap());
            let state = inspect.snapshot().unwrap();
            let attempt = state
                .records
                .values()
                .filter(|record| record.collection == Collection::Attempt)
                .map(|record| record.decode::<Attempt>().unwrap())
                .find(|attempt| {
                    attempt.request_digest == digest && attempt.phase == ReservationState::Submitted
                })
                .expect("HTTP request lacks durable reservation/send intent");
            assert!(attempt.send_intent.is_some());
            ids.lock().unwrap().push(attempt.id);
            let index = count.fetch_add(1, Ordering::SeqCst);
            assert!(index < 5, "unaffordable HTTP request escaped host");
            let response_id = format!("observed-{index}");
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse(vec![
                    ev_response_created(&response_id),
                    ev_assistant_message("summary", "Synthetic retained summary."),
                    ev_completed_with_tokens(&response_id, 7),
                ]))
        })
        .mount(&server)
        .await;
    let mut registry = ExtensionRegistryBuilder::new();
    registry.turn_start_admission(Arc::new(host.clone()));
    registry.work_admission(Arc::new(host.clone()));
    let starter = host.clone();
    let cwd = workspace.clone();
    let model = config.price.model.clone();
    let test = test_codex()
        .with_extensions(Arc::new(registry.build()))
        .with_auth(codex_login::CodexAuth::from_api_key(
            "public-synthetic-p1-token",
        ))
        .with_allowed_tools(AllowedTools(vec![]))
        .with_config(move |config| {
            config.cwd = cwd.try_into().unwrap();
            config.model = Some(model.clone());
            configure_fixture_provider(config);
            config.model_provider.experimental_bearer_token =
                Some("public-synthetic-p1-secret-header".into());
            starter
                .lifecycle()
                .authorize_startup(config.cwd.as_path(), None)
                .unwrap();
        })
        .build_with_auto_env(&server)
        .await
        .unwrap();
    let root = host.lifecycle().attach_root(test.codex.clone()).unwrap();
    host.register(root, root_binding.clone()).unwrap();
    turn(&test).await;
    test.codex.submit(Op::Compact).await.unwrap();
    tokio::time::timeout(
        Duration::from_secs(30),
        wait_for_event(&test.codex, |event| {
            matches!(event, EventMsg::TurnComplete(_))
        }),
    )
    .await
    .unwrap();
    let mut children = Vec::new();
    for role in [RequestRole::Helper, RequestRole::Child] {
        let mut binding = task(
            &host,
            &config,
            TaskId::new(),
            Some(config.root_task.clone()),
        );
        binding.role = role;
        host.lifecycle()
            .authorize_startup(&workspace, None)
            .unwrap();
        let mut extension_init = codex_extension_api::ExtensionDataInit::default();
        extension_init.insert(AllowedTools(vec![]));
        let child = test
            .thread_manager
            .start_thread(StartThreadOptions {
                thread_extension_init: extension_init,
                session_source: Some(SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                    parent_thread_id: root,
                    depth: 1,
                    agent_path: None,
                    agent_nickname: None,
                    agent_role: None,
                })),
                environments: Some(test.codex.environment_selections().await),
                ..StartThreadOptions::new(test.config.clone())
            })
            .await
            .unwrap()
            .thread;
        let id = host
            .lifecycle()
            .attach_child(
                root,
                &host.lifecycle().inspect(root).unwrap().revision,
                child.clone(),
            )
            .unwrap();
        host.register(id, binding).unwrap();
        let input = TurnInputRequest::user_input(vec![UserInput::Text {
            text: "Synthetic child request.".into(),
            text_elements: vec![],
        }])
        .on_start(TurnStartOptions {
            parent_turn_id: Some("owner-fixture".into()),
            ..Default::default()
        });
        child.start_or_steer_turn(input).await.unwrap();
        tokio::time::timeout(
            Duration::from_secs(30),
            wait_for_event(&child, |event| {
                if let EventMsg::Error(error) = event {
                    panic!("child error: {error:?}");
                }
                matches!(event, EventMsg::TurnComplete(_))
            }),
        )
        .await
        .unwrap();
        children.push(child);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    let state = host.snapshot().unwrap();
    let roles: Vec<_> = state
        .records
        .values()
        .filter(|record| record.collection == Collection::Attempt)
        .map(|record| record.decode::<Attempt>().unwrap().role)
        .collect();
    for role in [
        RequestRole::Main,
        RequestRole::Compaction,
        RequestRole::Helper,
        RequestRole::Child,
    ] {
        assert!(roles.contains(&role), "missing {role:?}");
    }
    // One request remains affordable. Actual retained entry points race; the
    // provider observer fails immediately if an unreserved request arrives.
    let barrier = Arc::new(tokio::sync::Barrier::new(3));
    let mut jobs = Vec::new();
    for thread in [test.codex.clone(), children[0].clone(), children[1].clone()] {
        let barrier = barrier.clone();
        jobs.push(tokio::spawn(async move {
            barrier.wait().await;
            thread
                .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                    text: "Compete for last request.".into(),
                    text_elements: vec![],
                }]))
                .await
                .unwrap();
            tokio::time::timeout(
                Duration::from_secs(30),
                wait_for_event(&thread, |event| matches!(event, EventMsg::TurnComplete(_))),
            )
            .await
            .unwrap();
        }));
    }
    for job in jobs {
        job.await.unwrap();
    }
    assert_eq!(calls.load(Ordering::SeqCst), 5);
    let state = host.snapshot().unwrap();
    assert_eq!(
        state
            .records
            .values()
            .filter(|record| record.collection == Collection::Attempt)
            .count(),
        5
    );
    assert_eq!(
        vcp_budget::ledger(&state, &root_binding.scope)
            .unwrap()
            .settled
            .get(),
        500
    );
    let ids = observed.lock().unwrap();
    let unique: std::collections::BTreeSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), 5);
    drop(ids);
    owner.close().await.unwrap();
    for child in children {
        child.shutdown_and_wait().await.unwrap();
    }
    test.codex.shutdown_and_wait().await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn actual_retained_http_body_and_full_response_match_canonical_admission() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let config = config(&temporary.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let binding = task(&host, &config, config.root_task.clone(), None);
        let server = start_mock_server().await;
        let full = "observed-model-output-".repeat(10_000);
        let response = sse(vec![
            ev_response_created("canonical-response"),
            ev_assistant_message("full-output", &full),
            ev_completed_with_tokens("canonical-response", 7),
        ]);
        let observed = mount_sse_sequence(
            &server,
            vec![
                response.clone(),
                sse(vec![
                    ev_assistant_message("resumed", "Explicit new work after reopen."),
                    ev_completed_with_tokens("resumed", 7),
                ]),
            ],
        )
        .await;
        let mut registry = ExtensionRegistryBuilder::new();
        registry.turn_start_admission(Arc::new(host.clone()));
        registry.work_admission(Arc::new(host.clone()));
        let starter = host.clone();
        let cwd = workspace.clone();
        let model = config.price.model.clone();
        let test = test_codex()
            .with_extensions(Arc::new(registry.build()))
            .with_auth(codex_login::CodexAuth::from_api_key(
                "public-synthetic-p1-secret-header",
            ))
            .with_allowed_tools(AllowedTools(vec![]))
            .with_config(move |config| {
                config.cwd = cwd.try_into().unwrap();
                config.model = Some(model.clone());
                configure_fixture_provider(config);
                config.model_provider.experimental_bearer_token =
                    Some("public-synthetic-p1-secret-header".into());
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
        turn(&test).await;
        let requests = observed.requests();
        assert_eq!(requests.len(), 1);
        let body = requests[0].body_json();
        assert_eq!(body["max_output_tokens"], 1024);
        assert_eq!(
            requests[0].header("authorization").as_deref(),
            Some("Bearer public-synthetic-p1-secret-header")
        );
        let state = host.snapshot().unwrap();
        let attempts: Vec<Attempt> = state
            .records
            .values()
            .filter(|record| record.collection == Collection::Attempt)
            .map(|record| record.decode().unwrap())
            .collect();
        assert_eq!(attempts.len(), requests.len());
        assert_eq!(attempts[0].phase, ReservationState::Settled);
        assert_eq!(attempts[0].charged.get(), 100);
        assert!(attempts[0].send_intent.is_some());
        let captured = host.read_artifact(attempts[0].request.clone()).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&captured).unwrap(),
            body
        );
        assert!(!String::from_utf8_lossy(&captured).contains("public-synthetic-p1-secret-header"));
        let responses: Vec<ArtifactDescriptor> = state
            .records
            .values()
            .filter(|record| record.collection == Collection::Artifact)
            .map(|record| record.decode::<ArtifactDescriptor>().unwrap())
            .filter(|artifact| artifact.spec.channel == Channel::Response)
            .collect();
        assert_eq!(responses.len(), 1);
        assert_eq!(
            host.read_artifact(responses[0].spec.id.clone()).unwrap(),
            response.as_bytes()
        );
        assert!(responses[0].length.get() > 100_000);
        #[cfg(windows)]
        {
            let process = host
                .spawn_process(
                    id,
                    Revision::new(1),
                    std::path::Path::new(env!("CARGO_BIN_EXE_vcp-process-fixture")),
                    &["flood".into(), workspace.as_os_str().into()],
                    &workspace,
                    &std::collections::BTreeMap::new(),
                    1024,
                )
                .unwrap();
            let output = tokio::time::timeout(Duration::from_secs(30), process.wait())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(output.exit_code, Some(0));
            assert_eq!(output.stdout_tail, vec![b'x'; 1024]);
            assert_eq!(output.stderr_tail, vec![b'x'; 1024]);
            for artifact in [output.stdout, output.stderr] {
                assert_eq!(artifact.length.get(), 2 * 1024 * 1024);
                assert_eq!(
                    host.read_artifact(artifact.spec.id).unwrap(),
                    vec![b'x'; 2 * 1024 * 1024]
                );
            }
        }
        let view = host.project().unwrap();
        assert_eq!(view.ledgers[&config.root_task].settled.get(), 100);
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        drop(test);
        drop(host);
        let (restored, restored_owner) = CanonicalHost::open(config.clone()).unwrap();
        let state = restored.snapshot().unwrap();
        let root: Task = state
            .record(
                Collection::Task,
                config.root_task.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(root.state, TaskState::Paused);
        assert_eq!(
            vcp_budget::ledger(&state, &binding.scope)
                .unwrap()
                .settled
                .get(),
            100
        );
        assert_eq!(observed.requests().len(), 1);
        let mut registry = ExtensionRegistryBuilder::new();
        registry.turn_start_admission(Arc::new(restored.clone()));
        registry.work_admission(Arc::new(restored.clone()));
        let starter = restored.clone();
        let cwd = workspace.clone();
        let model = config.price.model.clone();
        let reopened = test_codex()
            .with_extensions(Arc::new(registry.build()))
            .with_auth(codex_login::CodexAuth::from_api_key(
                "public-synthetic-p1-token",
            ))
            .with_allowed_tools(AllowedTools(vec![]))
            .with_config(move |config| {
                config.cwd = cwd.try_into().unwrap();
                config.model = Some(model.clone());
                configure_fixture_provider(config);
                starter
                    .lifecycle()
                    .authorize_startup(config.cwd.as_path(), None)
                    .unwrap();
            })
            .build_with_auto_env(&server)
            .await
            .unwrap();
        let id = restored
            .lifecycle()
            .attach_root(reopened.codex.clone())
            .unwrap();
        restored.register(id, binding.clone()).unwrap();
        assert!(matches!(
            reopened
                .codex
                .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                    text: "not yet authorized to resume".into(),
                    text_elements: vec![]
                }]))
                .await
                .unwrap(),
            codex_core::TurnInputSubmission::NotSubmitted { .. }
        ));
        let mut stale = root.fingerprint.clone();
        stale.repository = "d".repeat(64);
        assert!(restored.resume(id, root.revision, stale).is_err());
        restored
            .resume(id, root.revision, root.fingerprint)
            .unwrap();
        turn(&reopened).await;
        assert_eq!(observed.requests().len(), 2);
        assert_eq!(
            vcp_budget::ledger(&restored.snapshot().unwrap(), &binding.scope)
                .unwrap()
                .settled
                .get(),
            200
        );
        restored_owner.close().await.unwrap();
        reopened.codex.shutdown_and_wait().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn real_capture_capacity_failure_fences_transport_and_preserves_exact_prefix() {
    for request_failure in [true, false] {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let mut config = config(
            &temporary.path().join("canonical"),
            &workspace,
            BackendKind::Files,
        );
        config.artifact_limit = ByteCount::new(if request_failure { 1 } else { 65_536 });
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let binding = task(&host, &config, config.root_task.clone(), None);
        let server = start_mock_server().await;
        let full = "capacity-failure-output-".repeat(20_000);
        let response = sse(vec![
            ev_response_created("interrupted"),
            ev_assistant_message("large", &full),
            ev_completed_with_tokens("interrupted", 7),
        ]);
        let observed = if request_failure {
            mount_sse_once(&server, response.clone()).await
        } else {
            mount_sse_sequence(&server, vec![response.clone()]).await
        };
        let mut registry = ExtensionRegistryBuilder::new();
        registry.turn_start_admission(Arc::new(host.clone()));
        registry.work_admission(Arc::new(host.clone()));
        let starter = host.clone();
        let cwd = workspace.clone();
        let model = config.price.model.clone();
        let test = test_codex()
            .with_extensions(Arc::new(registry.build()))
            .with_auth(codex_login::CodexAuth::from_api_key(
                "public-synthetic-capacity-token",
            ))
            .with_allowed_tools(AllowedTools(vec![]))
            .with_config(move |config| {
                config.cwd = cwd.try_into().unwrap();
                config.model = Some(model.clone());
                configure_fixture_provider(config);
                config.model_provider.experimental_bearer_token =
                    Some("public-synthetic-p1-secret-header".into());
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
        turn(&test).await;
        let state = host.snapshot().unwrap();
        let root: Task = state
            .record(
                Collection::Task,
                config.root_task.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(root.state, TaskState::Paused);
        assert_eq!(observed.requests().len(), usize::from(!request_failure));
        assert!(matches!(
            test.codex
                .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
                    text: "must remain fenced".into(),
                    text_elements: vec![]
                }]))
                .await
                .unwrap(),
            codex_core::TurnInputSubmission::NotSubmitted { .. }
        ));
        if request_failure {
            assert_eq!(
                state
                    .records
                    .values()
                    .filter(|record| record.collection == Collection::Attempt)
                    .count(),
                0
            );
            let pending = host.unfinished_captures().unwrap();
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].length.get(), 0);
        } else {
            let attempt = state
                .records
                .values()
                .find(|record| record.collection == Collection::Attempt)
                .unwrap()
                .decode::<Attempt>()
                .unwrap();
            assert_eq!(attempt.phase, ReservationState::ReconciliationPending);
            assert_eq!(
                vcp_budget::ledger(&state, &binding.scope)
                    .unwrap()
                    .unresolved
                    .get(),
                100
            );
            let response_artifact = state
                .records
                .values()
                .filter(|record| record.collection == Collection::Artifact)
                .map(|record| record.decode::<ArtifactDescriptor>().unwrap())
                .find(|artifact| artifact.spec.channel == Channel::Response)
                .unwrap();
            assert_eq!(response_artifact.state, CaptureState::Aborted);
            assert!(response_artifact
                .spec
                .omissions
                .contains(&Omission::CaptureFailure));
            let retained = host.read_artifact(response_artifact.spec.id).unwrap();
            assert!(retained.len() <= 65_536);
            assert!(response.as_bytes().starts_with(&retained));
        }
        owner.close().await.unwrap();
        test.codex.shutdown_and_wait().await.unwrap();
        drop(test);
        drop(host);
        let (restored, restored_owner) = CanonicalHost::open(config.clone()).unwrap();
        let root: Task = restored
            .snapshot()
            .unwrap()
            .record(
                Collection::Task,
                config.root_task.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(root.state, TaskState::Paused);
        assert_eq!(observed.requests().len(), usize::from(!request_failure));
        restored_owner.close().await.unwrap();
    }
}
