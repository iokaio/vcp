// SPDX-License-Identifier: Apache-2.0
use serde_json::json;
use vcp_domain::{workspace::Scope, ArtifactId, SessionId, TaskId, WorkspaceId};
use vcp_extensions::hooks::{
    input::{HookInput, HookLimits},
    planner::plan,
    registry::{FailurePolicy, HookCommand, HookDefinition, HookEffectScope, HookEvent},
    result::{failure_disposition, validate_output, FailureDisposition},
};

fn input() -> HookInput {
    HookInput {
        schema_version: 1,
        event: HookEvent::BeforeToolAuthorization,
        event_id: "event-1".into(),
        scope: Scope {
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
            task: TaskId::new(),
        },
        root_task: TaskId::new(),
        steering_revision: 1,
        causation_id: "cause-1".into(),
        depth: 0,
        artifact_refs: vec![ArtifactId::parse("artifact-1").unwrap()],
        payload: json!({"operation_digest": "a".repeat(64)}),
    }
}
fn hook(id: &str, priority: i32) -> HookDefinition {
    HookDefinition {
        id: id.into(),
        version: 1,
        source_hash: "b".repeat(64),
        event: HookEvent::BeforeToolAuthorization,
        priority,
        before: vec![],
        after: vec![],
        command: HookCommand {
            profile: "hook-profile".into(),
            arguments: vec![],
            working_directory: "".into(),
        },
        effect_scope: HookEffectScope::BrokerProfile,
        timeout_ms: 1000,
        max_output_bytes: 4096,
        failure_policy: FailurePolicy::Block,
    }
}
#[test]
fn stable_order_and_explicit_edges_override_priority() {
    let input = input();
    let a = hook("a", 0);
    let b = hook("b", 0);
    let mut c = hook("c", 9);
    c.before.push("a".into());
    let first = plan(
        &[a.clone(), b.clone(), c.clone()],
        &input,
        HookLimits::default(),
    )
    .unwrap();
    let second = plan(&[c, b, a], &input, HookLimits::default()).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        first
            .iter()
            .map(|p| p.definition.id.as_str())
            .collect::<Vec<_>>(),
        vec!["b", "c", "a"]
    );
    first[0].validate(HookLimits::default()).unwrap();
}
#[test]
fn rejects_duplicate_unknown_ambiguous_and_cyclic_ordering() {
    let input = input();
    let mut a = hook("a", 0);
    let mut b = hook("b", 0);
    assert!(plan(&[a.clone(), a.clone()], &input, HookLimits::default()).is_err());
    a.before.push("missing".into());
    assert!(plan(&[a.clone()], &input, HookLimits::default()).is_err());
    a.before = vec!["b".into()];
    b.before = vec!["a".into()];
    assert!(plan(&[a.clone(), b.clone()], &input, HookLimits::default()).is_err());
    a.after.push("b".into());
    b.before.clear();
    assert!(plan(&[a, b], &input, HookLimits::default()).is_err());
}
#[test]
fn depth_fanout_and_input_bounds_apply_before_dispatch() {
    let mut input = input();
    let limits = HookLimits {
        max_depth: 1,
        max_fanout: 1,
        max_input_bytes: 4096,
    };
    assert!(plan(&[hook("a", 0), hook("b", 0)], &input, limits).is_err());
    input.depth = 2;
    assert!(plan(&[hook("a", 0)], &input, limits).is_err());
    input.depth = 0;
    input.payload = json!({"large": "x".repeat(4096)});
    assert!(plan(&[], &input, limits).is_err());
    assert!(plan(
        &[],
        &input,
        HookLimits {
            max_depth: 33,
            ..limits
        }
    )
    .is_err());
}
#[test]
fn identities_bind_source_arguments_scope_and_steering() {
    let input = input();
    let definition = hook("a", 0);
    let identity = |d: HookDefinition, i: &HookInput| {
        plan(&[d], i, HookLimits::default()).unwrap()[0]
            .identity
            .clone()
    };
    let original = identity(definition.clone(), &input);
    let mut changed = definition.clone();
    changed.source_hash = "c".repeat(64);
    assert_ne!(original, identity(changed, &input));
    let mut changed = definition.clone();
    changed.command.arguments.push("different".into());
    assert_ne!(original, identity(changed, &input));
    let mut changed = input.clone();
    changed.steering_revision += 1;
    assert_ne!(original, identity(definition.clone(), &changed));
    changed = input.clone();
    changed.scope.workspace = WorkspaceId::new();
    assert_ne!(original, identity(definition.clone(), &changed));
    let mut planned = plan(&[definition], &input, HookLimits::default())
        .unwrap()
        .remove(0);
    planned.input.depth += 1;
    assert!(planned.validate(HookLimits::default()).is_err());
}
#[test]
fn validates_rewrite_binding_artifact_scope_and_strict_schema() {
    let planned = plan(&[hook("a", 0)], &input(), HookLimits::default())
        .unwrap()
        .remove(0);
    let mut output = json!({"schema_version":1,"findings":[],"context":null,"rewrite":{"original_digest":"a".repeat(64),"tool":"process","arguments":{"path":"changed"}},"block":false});
    assert!(validate_output(&serde_json::to_vec(&output).unwrap(), &planned).is_ok());
    output["block"] = json!(true);
    assert!(validate_output(&serde_json::to_vec(&output).unwrap(), &planned).is_err());
    output["block"] = json!(false);
    output["rewrite"]["original_digest"] = json!("b".repeat(64));
    assert!(validate_output(&serde_json::to_vec(&output).unwrap(), &planned).is_err());
    output["rewrite"] = serde_json::Value::Null;
    output["context"] = json!({"text":"proposal","artifact_refs":["ungranted"]});
    assert!(validate_output(&serde_json::to_vec(&output).unwrap(), &planned).is_err());
    output["context"] = serde_json::Value::Null;
    output["budget_balance"] = json!(100);
    assert!(validate_output(&serde_json::to_vec(&output).unwrap(), &planned).is_err());
    assert!(validate_output(b"not JSON", &planned).is_err());
    assert!(validate_output(&vec![b' '; 4097], &planned).is_err());
    assert_eq!(
        failure_disposition(FailurePolicy::Warn, true),
        FailureDisposition::Block
    );
    assert_eq!(
        failure_disposition(FailurePolicy::Warn, false),
        FailureDisposition::Warn
    );
}
#[test]
fn gate_failures_block_and_wire_request_binds_full_plan() {
    let mut definition = hook("a", 0);
    definition.failure_policy = FailurePolicy::Warn;
    assert!(plan(&[definition.clone()], &input(), HookLimits::default()).is_err());
    definition.failure_policy = FailurePolicy::Block;
    let planned = plan(&[definition], &input(), HookLimits::default())
        .unwrap()
        .remove(0);
    let request = vcp_extensions::hooks::runner::request(&planned, HookLimits::default()).unwrap();
    assert_eq!(request.digest, vcp_protocol::digest_bytes(&request.stdin));
    assert_eq!(
        request.arguments,
        vec!["--vcp-hook-input-sha256", &request.digest]
    );
    assert_eq!(
        serde_json::from_slice::<vcp_extensions::hooks::planner::PlannedHook>(&request.stdin)
            .unwrap(),
        planned
    );
}
#[test]
fn every_reserved_event_roundtrips_and_version_is_explicit() {
    for event in [
        HookEvent::SessionStart,
        HookEvent::TaskStart,
        HookEvent::BeforeContextAssembly,
        HookEvent::BeforeToolAuthorization,
        HookEvent::AfterToolCompletion,
        HookEvent::BeforeCompaction,
        HookEvent::AfterVerification,
        HookEvent::TaskCompletion,
    ] {
        let mut input = input();
        input.event = event;
        let mut definition = hook("a", 0);
        definition.event = event;
        assert_eq!(
            serde_json::from_slice::<HookInput>(&serde_json::to_vec(&input).unwrap()).unwrap(),
            input
        );
        assert_eq!(
            plan(&[definition], &input, HookLimits::default())
                .unwrap()
                .len(),
            1
        );
        input.schema_version = 2;
        assert!(plan(&[], &input, HookLimits::default()).is_err());
    }
}
