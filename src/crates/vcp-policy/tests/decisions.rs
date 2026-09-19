// SPDX-License-Identifier: Apache-2.0
use std::collections::BTreeSet;
use vcp_domain::{policy::*, workspace::*, *};
use vcp_policy::*;

struct Fixture {
    workspace: Workspace,
    scope: Scope,
    actor: ActorId,
    root: RootId,
    policy: Policy,
    roots: BTreeSet<RootId>,
    isolation: BTreeSet<Isolation>,
}
impl Fixture {
    fn new() -> Self {
        let workspace = Workspace {
            id: WorkspaceId::new(),
            binding: Binding {
                host: HostId::new(),
                root: "C:/synthetic/workspace".into(),
                repository: "fixture".into(),
                worktree: "fixture".into(),
                revision: Revision::ZERO,
            },
            trust: Trust::Trusted,
            revision: Revision::ZERO,
            authority: AuthorityRevision::ZERO,
            deletion: DeletionEpoch::ZERO,
        };
        let scope = Scope {
            workspace: workspace.id.clone(),
            session: SessionId::new(),
            task: TaskId::new(),
        };
        let root = RootId::new();
        let roots = BTreeSet::from([root.clone()]);
        let policy = Policy {
            workspace: workspace.id.clone(),
            revision: PolicyRevision::ZERO,
            mode: Autonomy::Ask,
            denials: vec![],
            workspace_roots: roots.clone(),
            automatic_effects: BTreeSet::new(),
            timeout_ceiling_ms: Units::new(1000),
            output_ceiling_bytes: ByteCount::new(4096),
        };
        Self {
            workspace,
            scope,
            actor: ActorId::new(),
            root,
            policy,
            roots,
            isolation: BTreeSet::from([Isolation::PathContainment]),
        }
    }
    fn operation(&self) -> Operation {
        Operation {
            scope: self.scope.clone(),
            actor: self.actor.clone(),
            host: self.workspace.binding.host.clone(),
            binding: Revision::ZERO,
            authority: AuthorityRevision::ZERO,
            steering: SteeringRevision::ZERO,
            policy: PolicyRevision::ZERO,
            tool: "write_file".into(),
            schema: "a".repeat(64),
            arguments: "{\"path\":\"src/main.rs\",\"text\":\"new\"}".into(),
            invocation: Invocation::Local,
            resources: vec![Resource {
                root: self.root.clone(),
                path: "src/main.rs".into(),
                write: true,
                version: "b".repeat(64),
            }],
            effects: BTreeSet::from([EffectClass::Write]),
            required_isolation: self.isolation.clone(),
            timeout_ms: Units::new(100),
            output_bytes: ByteCount::new(1024),
        }
    }
    fn facts(&self) -> Facts<'_> {
        Facts {
            workspace: &self.workspace,
            scope: &self.scope,
            actor: &self.actor,
            steering: SteeringRevision::ZERO,
            policy: PolicyRevision::ZERO,
            now: Timestamp::new(100),
            owner_current: true,
            task_running: true,
            resources_current: true,
            registered_roots: &self.roots,
            isolation: &self.isolation,
            host_denials: &[],
        }
    }
    fn grant(&self, prepared: &Prepared) -> Grant {
        Grant {
            id: GrantId::new(),
            actor: self.actor.clone(),
            scope: GrantScope::Task {
                scope: self.scope.clone(),
            },
            host: self.workspace.binding.host.clone(),
            binding: Revision::ZERO,
            authority: AuthorityRevision::ZERO,
            policy: PolicyRevision::ZERO,
            expires_at: Timestamp::new(200),
            target: GrantTarget::Exact {
                digest: prepared.digest().into(),
            },
            origin: RuleOrigin::User,
            reason: "synthetic explicit approval".into(),
            revoked: false,
            revision: Revision::ZERO,
            approval: None,
        }
    }
}
fn allowed(decision: Decision) -> bool {
    matches!(decision, Decision::Allow { .. })
}

#[test]
fn presets_keep_planning_mutation_authority_and_configured_autonomy_distinct() {
    let mut f = Fixture::new();
    let prepared = Prepared::new(f.operation()).unwrap();
    assert!(matches!(
        evaluate(&prepared, &f.policy, &[], &f.facts()).unwrap(),
        Decision::Question { .. }
    ));
    let grant = f.grant(&prepared);
    f.policy.mode = Autonomy::Plan;
    assert!(matches!(
        evaluate(&prepared, &f.policy, &[grant], &f.facts()).unwrap(),
        Decision::Deny { .. }
    ));
    f.policy.mode = Autonomy::Workspace;
    assert!(allowed(
        evaluate(&prepared, &f.policy, &[], &f.facts()).unwrap()
    ));
    f.policy.mode = Autonomy::Autonomous;
    assert!(!allowed(
        evaluate(&prepared, &f.policy, &[], &f.facts()).unwrap()
    ));
    f.policy.automatic_effects.insert(EffectClass::Write);
    assert!(allowed(
        evaluate(&prepared, &f.policy, &[], &f.facts()).unwrap()
    ));
}
#[test]
fn explicit_denials_precede_existing_grants_and_preset_authority() {
    let mut f = Fixture::new();
    let p = Prepared::new(f.operation()).unwrap();
    let grant = f.grant(&p);
    let denial = Denial {
        id: "host:no-source-writes".into(),
        origin: RuleOrigin::Host,
        reason: "protected source".into(),
        effects: BTreeSet::from([EffectClass::Write]),
        tool: None,
        roots: f.roots.clone(),
        paths: vec!["SRC".into()],
    };
    let mut facts = f.facts();
    facts.host_denials = std::slice::from_ref(&denial);
    assert!(
        matches!(evaluate(&p, &f.policy, &[grant.clone()], &facts).unwrap(), Decision::Deny { origin, .. } if origin == denial.id)
    );
    f.policy.denials.push(Denial {
        origin: RuleOrigin::User,
        ..denial
    });
    assert!(!allowed(
        evaluate(&p, &f.policy, &[grant], &f.facts()).unwrap()
    ));
    // An opaque operation cannot establish that a scoped rule is unrelated.
    // The bounded operation above still uses its complete declared resources.
    let mut opaque = f.operation();
    opaque.effects.insert(EffectClass::Opaque);
    let opaque = Prepared::new(opaque).unwrap();
    let grant = f.grant(&opaque);
    for effect in [
        EffectClass::Write,
        EffectClass::Install,
        EffectClass::Publish,
    ] {
        let rule = &mut f.policy.denials[0];
        rule.effects = BTreeSet::from([effect]);
        rule.roots = BTreeSet::from([RootId::new()]);
        rule.paths = vec!["unrelated/declared/input".into()];
        assert!(allowed(
            evaluate(&p, &f.policy, &[f.grant(&p)], &f.facts()).unwrap()
        ));
        assert!(matches!(
            evaluate(&opaque, &f.policy, &[grant.clone()], &f.facts()).unwrap(),
            Decision::Deny { .. }
        ));
    }
    f.policy.denials[0].tool = Some("different-tool".into());
    assert!(allowed(
        evaluate(&opaque, &f.policy, &[grant], &f.facts()).unwrap()
    ));
}
#[test]
fn every_approval_bound_change_invalidates_exact_grant_without_erasing_original() {
    let f = Fixture::new();
    let op = f.operation();
    let p = Prepared::new(op.clone()).unwrap();
    let grant = f.grant(&p);
    assert!(allowed(
        evaluate(&p, &f.policy, &[grant.clone()], &f.facts()).unwrap()
    ));
    let variants: Vec<Box<dyn Fn(&mut Operation)>> = vec![
        Box::new(|o| o.arguments = "{\"path\":\"src/other.rs\"}".into()),
        Box::new(|o| o.schema = "c".repeat(64)),
        Box::new(|o| o.host = HostId::new()),
        Box::new(|o| o.binding = Revision::new(1)),
        Box::new(|o| o.authority = AuthorityRevision::new(1)),
        Box::new(|o| o.steering = SteeringRevision::new(1)),
        Box::new(|o| o.policy = PolicyRevision::new(1)),
        Box::new(|o| o.resources[0].path = "src/other.rs".into()),
        Box::new(|o| o.resources[0].version = "d".repeat(64)),
        Box::new(|o| o.timeout_ms = Units::new(101)),
        Box::new(|o| o.output_bytes = ByteCount::new(1025)),
    ];
    for change in variants {
        let mut next = op.clone();
        change(&mut next);
        let next = Prepared::new(next).unwrap();
        assert_ne!(next.digest(), p.digest());
        assert!(!allowed(
            evaluate(&next, &f.policy, &[grant.clone()], &f.facts()).unwrap()
        ));
    }
    assert!(allowed(
        evaluate(&p, &f.policy, &[grant], &f.facts()).unwrap()
    ));
}
#[test]
fn grant_reuse_scope_expiry_and_revocation_are_checked_on_each_admission() {
    let f = Fixture::new();
    let p = Prepared::new(f.operation()).unwrap();
    let mut grant = f.grant(&p);
    for _ in 0..3 {
        assert!(allowed(
            evaluate(&p, &f.policy, &[grant.clone()], &f.facts()).unwrap()
        ));
    }
    grant.scope = GrantScope::Session {
        workspace: f.workspace.id.clone(),
        session: SessionId::new(),
    };
    assert!(!allowed(
        evaluate(&p, &f.policy, &[grant.clone()], &f.facts()).unwrap()
    ));
    grant.scope = GrantScope::Workspace {
        workspace: f.workspace.id.clone(),
    };
    assert!(allowed(
        evaluate(&p, &f.policy, &[grant.clone()], &f.facts()).unwrap()
    ));
    grant.expires_at = Timestamp::new(100);
    assert!(!allowed(
        evaluate(&p, &f.policy, &[grant.clone()], &f.facts()).unwrap()
    ));
    grant.expires_at = Timestamp::new(200);
    grant.revoked = true;
    assert!(!allowed(
        evaluate(&p, &f.policy, &[grant], &f.facts()).unwrap()
    ));
}
#[test]
fn approval_does_not_enable_missing_isolation_or_resume_paused_and_stale_work() {
    let f = Fixture::new();
    let p = Prepared::new(f.operation()).unwrap();
    let grant = f.grant(&p);
    let empty = BTreeSet::new();
    for variant in 0..4 {
        let mut facts = f.facts();
        match variant {
            0 => facts.isolation = &empty,
            1 => facts.owner_current = false,
            2 => facts.task_running = false,
            _ => facts.resources_current = false,
        };
        assert!(!allowed(
            evaluate(&p, &f.policy, &[grant.clone()], &facts).unwrap()
        ));
    }
}
#[test]
fn shell_opaque_effects_and_resource_prefixes_cannot_be_rewritten_into_a_grant() {
    let f = Fixture::new();
    let mut op = f.operation();
    op.invocation = Invocation::Process {
        executable: "C:/Windows/System32/cmd.exe".into(),
        executable_identity: "c".repeat(64),
        arguments: vec!["/c".into(), "echo synthetic > marker".into()],
        directory: "C:/synthetic/workspace".into(),
        environment_digest: "d".repeat(64),
        shell: true,
    };
    assert!(Prepared::new(op.clone()).is_err());
    op.effects
        .extend([EffectClass::Execute, EffectClass::Opaque]);
    let p = Prepared::new(op.clone()).unwrap();
    let mut grant = f.grant(&p);
    grant.target = GrantTarget::Configured {
        tool: op.tool.clone(),
        schema: op.schema.clone(),
        arguments_digest: vcp_protocol::digest_bytes(op.arguments.as_bytes()),
        invocation: op.invocation.clone(),
        effects: op.effects.clone(),
        roots: f.roots.clone(),
        paths: vec!["src".into()],
        isolation: op.required_isolation.clone(),
        timeout_ms: op.timeout_ms,
        output_bytes: op.output_bytes,
    };
    assert!(allowed(
        evaluate(&p, &f.policy, &[grant.clone()], &f.facts()).unwrap()
    ));
    op.resources[0].path = "src-escape/main.rs".into();
    assert!(!allowed(
        evaluate(
            &Prepared::new(op.clone()).unwrap(),
            &f.policy,
            &[grant.clone()],
            &f.facts()
        )
        .unwrap()
    ));
    op.resources[0].path = "src/main.rs".into();
    if let Invocation::Process { arguments, .. } = &mut op.invocation {
        arguments[1] = "echo synthetic > outside".into();
    }
    assert!(!allowed(
        evaluate(&Prepared::new(op).unwrap(), &f.policy, &[grant], &f.facts()).unwrap()
    ));
    for path in [
        "../escape",
        "C:/outside",
        "src\\escape",
        "src/aux:stream",
        "src/./file",
        "src/space ",
    ] {
        assert!(!relative(path));
    }
}
