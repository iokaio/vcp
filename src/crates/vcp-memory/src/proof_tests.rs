// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde_json::json;
use vcp_domain::{memory::*, *};

fn proposal() -> Proposal {
    Proposal {
        id: ProposalId::new(),
        command: CommandId::new(),
        claim: ClaimId::new(),
        scope: vcp_domain::workspace::Scope {
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
            task: TaskId::new(),
        },
        actor: ActorId::new(),
        epochs: Epochs {
            authority: AuthorityRevision::ZERO,
            deletion: DeletionEpoch::ZERO,
            policy: PolicyRevision::ZERO,
        },
        registry_version: REGISTRY_VERSION,
        extractor: "test".into(),
        output_key: "command".into(),
        origins: vec![EventId::new()],
        subject: "test command".into(),
        predicate: "passed".into(),
        statement: "cargo test passed".into(),
        value: ClaimValue::Command {
            purpose: CommandPurpose::Test,
            argv: vec!["cargo".into(), "test".into()],
            cwd: ".".into(),
            configuration: ArtifactId::new(),
            outcome: Some(CheckOutcome::Passed),
            verification: Some(VerificationId::new()),
        },
        applicability: Applicability {
            repository: "r".into(),
            worktree: "w".into(),
            roots: vec![],
            paths: vec![],
            symbols: vec![],
            branch: None,
            fingerprint: None,
            conditions: Default::default(),
            valid_from: None,
            valid_until: None,
        },
        evidence: vec![],
        predecessor: None,
        correction_reason: None,
        retention: "history".into(),
    }
}

fn receipt() -> serde_json::Value {
    json!({"plan":{"runner":"cargo","origin":{"sha256":"a".repeat(64)},"directory":"","request":{"arguments":["test"],"directory":""},"not_run":null},"applicability":"current","outcome":{"status":"passed"},"exit_code":0})
}

#[test]
fn native_check_requires_exact_command_configuration_directory_and_current_result() {
    let proposal = proposal();
    let raw = receipt();
    assert!(matches(
        &serde_json::from_value(raw.clone()).unwrap(),
        &proposal,
        &"a".repeat(64)
    ));
    for (pointer, replacement) in [
        ("/plan/runner", json!("node")),
        ("/plan/origin/sha256", json!("b".repeat(64))),
        ("/plan/directory", json!("another")),
        ("/plan/request/directory", json!("another")),
        ("/plan/request/arguments", json!(["build"])),
        ("/plan/not_run", json!("not executed")),
        ("/applicability", json!("stale")),
        ("/outcome", json!({"status":"failed","reason":"failure"})),
        ("/exit_code", json!(1)),
    ] {
        let mut changed = raw.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            !matches(
                &serde_json::from_value(changed).unwrap(),
                &proposal,
                &"a".repeat(64)
            ),
            "{pointer}"
        );
    }
}

#[test]
fn test_receipt_does_not_prove_build_and_node_arguments_keep_their_identity() {
    let mut proposal = proposal();
    if let ClaimValue::Command { purpose, .. } = &mut proposal.value {
        *purpose = CommandPurpose::Build;
    }
    assert!(!matches(
        &serde_json::from_value(receipt()).unwrap(),
        &proposal,
        &"a".repeat(64)
    ));
    if let ClaimValue::Command {
        purpose, argv, cwd, ..
    } = &mut proposal.value
    {
        *purpose = CommandPurpose::Test;
        *argv = vec!["node".into(), "--test".into(), "checks.cjs".into()];
        *cwd = "nested".into();
    }
    let mut raw = receipt();
    raw["plan"]["runner"] = json!("node");
    raw["plan"]["directory"] = json!("nested");
    raw["plan"]["request"]["directory"] = json!("nested");
    raw["plan"]["request"]["arguments"] = json!(["--test", "checks.cjs"]);
    assert!(matches(
        &serde_json::from_value(raw).unwrap(),
        &proposal,
        &"a".repeat(64)
    ));
}
