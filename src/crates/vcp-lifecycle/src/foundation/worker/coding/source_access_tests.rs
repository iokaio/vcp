// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::collections::BTreeSet;
use vcp_domain::policy::*;

const TOOLS: [&str; 10] = [
    "vcp_read",
    "vcp_list",
    "vcp_search",
    "vcp_patch",
    "vcp_exec",
    "vcp_verify",
    "vcp_verify_focused",
    "vcp_mcp",
    "vcp_skill",
    "vcp_artifact_read",
];

fn fixture(temp: &tempfile::TempDir, backend: vcp_store::BackendKind) -> (Context, ThreadBinding) {
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let currency: Currency = "USD".to_owned().try_into().unwrap();
    let config = Config {
        canonical_root: temp.path().join("canonical"),
        backend,
        workspace: WorkspaceId::new(),
        session: SessionId::new(),
        binding: Binding {
            host: HostId::new(),
            root: workspace.to_string_lossy().into_owned(),
            repository: "fixture".into(),
            worktree: "main".into(),
            revision: Revision::ZERO,
        },
        actor: ActorId::new(),
        root_task: TaskId::new(),
        cap: Money {
            currency: currency.clone(),
            micros: Micros::new(1000),
        }
        .into(),
        protected: Micros::ZERO,
        price: PriceSnapshot {
            id: "a".repeat(64),
            provider: "fixture".into(),
            model: "fixture".into(),
            currency,
            capability: "b".repeat(64),
            valid_until: Timestamp::new(u64::MAX),
            rates: Default::default(),
        },
        input_ceiling: Units::new(100),
        output_ceiling: Units::new(100),
        artifact_limit: ByteCount::new(16 * 1024 * 1024),
        max_transport_retries: 0,
        host_tool_denials: vec![],
    };
    let mut context = Context::open(config).unwrap();
    let binding = ThreadBinding {
        scope: Scope {
            workspace: context.config.workspace.clone(),
            session: context.config.session.clone(),
            task: context.config.root_task.clone(),
        },
        agent: AgentId::new(),
        role: RequestRole::Main,
    };
    context
        .command(
            Command::CreateTask {
                root: binding.scope.task.clone(),
                parent: None,
                fork_origin: None,
                objective: Objective {
                    text: "validate coding source access".into(),
                    constraints: vec![],
                    acceptance: vec!["preserve every tool denial and fresh start proof".into()],
                    source: EventId::new(),
                    steering: SteeringRevision::ZERO,
                },
                fingerprint: vcp_domain::verification::Fingerprint {
                    repository: "a".repeat(64),
                    buffers: "b".repeat(64),
                    environment: "c".repeat(64),
                },
                editing: false,
                required_checks: vec![],
            },
            Some(binding.scope.task.clone()),
            Revision::ZERO,
        )
        .unwrap();
    context
        .command(
            Command::Transition {
                next: TaskState::Running,
                reason: "fixture".into(),
                verification: None,
            },
            Some(binding.scope.task.clone()),
            Revision::ZERO,
        )
        .unwrap();
    context
        .command(
            Command::SetWorkspaceTrust {
                trust: Trust::Trusted,
            },
            None,
            Revision::ZERO,
        )
        .unwrap();
    set_policy(&mut context, vec![]);
    (context, binding)
}

fn set_policy(context: &mut Context, denials: Vec<Denial>) {
    let expected = context
        .engine
        .store()
        .current()
        .records
        .get(&key(Collection::Access, context.config.workspace.as_str()))
        .map_or(Revision::ZERO, |row| row.revision);
    let revision =
        vcp_engine::policy::optional(context.engine.store().current(), &context.config.workspace)
            .unwrap()
            .map_or(PolicyRevision::ZERO, |policy| {
                PolicyRevision::new(policy.revision.get() + 1)
            });
    context
        .command(
            Command::SetPolicy {
                policy: Policy {
                    workspace: context.config.workspace.clone(),
                    revision,
                    mode: Autonomy::Autonomous,
                    denials,
                    workspace_roots: BTreeSet::from([RootId::parse(
                        context.config.workspace.as_str(),
                    )
                    .unwrap()]),
                    automatic_effects: BTreeSet::from([EffectClass::Read]),
                    timeout_ceiling_ms: Units::new(30_000),
                    output_ceiling_bytes: ByteCount::new(1024 * 1024),
                },
            },
            None,
            expected,
        )
        .unwrap();
}

fn set_trust(context: &mut Context, trust: Trust) {
    let expected = context
        .engine
        .store()
        .current()
        .record(
            Collection::Workspace,
            context.config.workspace.as_str(),
            &context.config.workspace,
        )
        .unwrap()
        .revision;
    context
        .command(Command::SetWorkspaceTrust { trust }, None, expected)
        .unwrap();
}

#[test]
fn coding_access_batches_start_proof_but_rechecks_each_invocation() {
    for backend in [
        vcp_store::BackendKind::Sqlite,
        vcp_store::BackendKind::Files,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let (context, binding) = fixture(&temp, backend);
        START_WINDOW_CHECKS.with(|count| count.set(0));
        for tool in TOOLS {
            context.tool_identity(&binding, tool).unwrap();
        }
        assert_eq!(START_WINDOW_CHECKS.with(|count| count.get()), 10);
        START_WINDOW_CHECKS.with(|count| count.set(0));
        context.validate_coding_tool_access(&binding).unwrap();
        assert_eq!(START_WINDOW_CHECKS.with(|count| count.get()), 1);
        context.validate_coding_tool_access(&binding).unwrap();
        assert_eq!(
            START_WINDOW_CHECKS.with(|count| count.get()),
            2,
            "a later admission/consumption must not reuse a prior start proof"
        );
        context.close().unwrap();
    }
}

#[test]
fn coding_access_preserves_every_host_and_current_policy_denial() {
    for backend in [
        vcp_store::BackendKind::Sqlite,
        vcp_store::BackendKind::Files,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let (mut context, binding) = fixture(&temp, backend);
        for tool in TOOLS {
            for origin in [RuleOrigin::Host, RuleOrigin::User] {
                let denial = Denial {
                    id: "selected-tool-read".into(),
                    origin,
                    reason: "fixture".into(),
                    effects: BTreeSet::from([EffectClass::Read]),
                    tool: Some(tool.into()),
                    roots: BTreeSet::from([
                        RootId::parse(context.config.workspace.as_str()).unwrap()
                    ]),
                    paths: vec![],
                };
                if origin == RuleOrigin::Host {
                    context.config.host_tool_denials = vec![denial];
                } else {
                    set_policy(&mut context, vec![denial]);
                }
                if tool != "vcp_read" {
                    context.tool_identity(&binding, "vcp_read").unwrap();
                }
                assert!(
                    context
                        .validate_coding_tool_access(&binding)
                        .unwrap_err()
                        .to_string()
                        .contains("trusted read denial"),
                    "{backend:?} {origin:?} {tool}"
                );
                context.config.host_tool_denials.clear();
                set_policy(&mut context, vec![]);
                context.validate_coding_tool_access(&binding).unwrap();
            }
        }
        context.close().unwrap();
    }
}

#[test]
fn coding_access_rejects_revoked_trust_scope_and_task_after_prior_success() {
    for backend in [
        vcp_store::BackendKind::Sqlite,
        vcp_store::BackendKind::Files,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let (mut context, binding) = fixture(&temp, backend);
        context.validate_coding_tool_access(&binding).unwrap();
        let mut foreign = binding.clone();
        foreign.scope.workspace = WorkspaceId::new();
        assert!(context.validate_coding_tool_access(&foreign).is_err());
        set_trust(&mut context, Trust::Untrusted);
        assert!(context.validate_coding_tool_access(&binding).is_err());
        set_trust(&mut context, Trust::Trusted);
        context.validate_coding_tool_access(&binding).unwrap();
        context.pause_root("freshness fixture").unwrap();
        assert!(context
            .validate_coding_tool_access(&binding)
            .unwrap_err()
            .to_string()
            .contains("held"));
        context.close().unwrap();
    }
}

#[test]
fn coding_access_consumption_rechecks_revision_after_admission() {
    for backend in [
        vcp_store::BackendKind::Sqlite,
        vcp_store::BackendKind::Files,
    ] {
        for change_policy in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let (mut context, binding) = fixture(&temp, backend);
            let revisions = context.context_revisions(&binding).unwrap();
            let operating = context
                .coding_part(
                    &binding.scope,
                    Kind::Evidence,
                    ContextTrust::Observed,
                    Content::Text {
                        text: "unit admission fixture".into(),
                    },
                )
                .unwrap();
            // Seed an eligible response at the private admission boundary. The
            // existing integration fixture separately proves response capture.
            context.coding.insert(
                binding.scope.task.clone(),
                Loop {
                    turn: None,
                    config: CodingConfig {
                        canonical_tools: Default::default(),
                        operating: "fixture".into(),
                        affected_paths: vec!["file.txt".into()],
                        max_requests: 2,
                        deadline: vcp_domain::Limit::Unbounded,
                    },
                    operating,
                    history: vec![],
                    history_sources: vec![],
                    hook_parts: BTreeMap::new(),
                    hook_gate_required: false,
                    hook_gate: None,
                    hook_retry_gate: None,
                    pairs: 0,
                    revisions: Some(revisions),
                    calls: vec![Eligible {
                        attempt: AttemptId::new(),
                        call: Call {
                            id: "fixture-call".into(),
                            name: "vcp_read".into(),
                            arguments: serde_json::json!({}),
                        },
                        admitted: false,
                        sources: vec![],
                        mcp_provenance: None,
                    }],
                    probes: vec![],
                    final_response: None,
                    continuity: None,
                    instruction_parents: None,
                    allocation_history: Default::default(),
                    allocation_observations: 0,
                    allocation: None,
                    allocation_candidate: None,
                    output_continuation: None,
                    continuation_feedback: None,
                    last_continuation: None,
                    activity_override: None,
                },
            );
            START_WINDOW_CHECKS.with(|count| count.set(0));
            context
                .admit_coding_tool(&binding, "fixture-call", &ToolName::plain("vcp_read"))
                .unwrap();
            assert_eq!(
                START_WINDOW_CHECKS.with(|count| count.get()),
                2,
                "context revisions and source access each retain their own start check"
            );
            if change_policy {
                set_policy(&mut context, vec![]);
            }
            START_WINDOW_CHECKS.with(|count| count.set(0));
            let consumed = context.consume_coding_call(&binding, "fixture-call", "vcp_read", "{}");
            assert_eq!(
                START_WINDOW_CHECKS.with(|count| count.get()),
                2,
                "consumption must recheck both boundaries independently of admission"
            );
            if change_policy {
                assert!(consumed
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("coding tool context changed"));
                assert_eq!(
                    context.coding[&binding.scope.task].pairs, 1,
                    "rejected stale call retains its nonexecuted result"
                );
            } else {
                assert!(consumed.is_ok());
                // The same batched guard protects provider packet assembly.
                // A denial for the last tool must stop it before source reads.
                context.config.host_tool_denials.push(Denial {
                    id: "packet-artifact-read".into(),
                    origin: RuleOrigin::Host,
                    reason: "fixture".into(),
                    effects: BTreeSet::from([EffectClass::Read]),
                    tool: Some("vcp_artifact_read".into()),
                    roots: BTreeSet::from([
                        RootId::parse(context.config.workspace.as_str()).unwrap()
                    ]),
                    paths: vec![],
                });
                START_WINDOW_CHECKS.with(|count| count.set(0));
                assert!(context
                    .assemble_coding_context(&binding)
                    .unwrap_err()
                    .to_string()
                    .contains("trusted read denial"));
                assert_eq!(
                    START_WINDOW_CHECKS.with(|count| count.get()),
                    3,
                    "packet assembly retains entry/context checks and one common tool check"
                );
            }
            assert!(context.coding[&binding.scope.task].calls.is_empty());
            context.close().unwrap();
        }
    }
}
