// SPDX-License-Identifier: Apache-2.0
use super::{inspect, HostPolicy};
use vcp_domain::{
    ids::*,
    revision::*,
    task::Objective,
    verification::Fingerprint,
    workspace::{Binding, Scope},
};
use vcp_engine::{Engine, HostFacts};

use vcp_protocol::{
    command::{Command, CommandEnvelope},
    methods, policy_inspection as wire,
};
use vcp_store::{
    contract::{CanonicalStore, Collection, Mutation, Record, Transaction},
    BackendKind, Store,
};
fn fingerprint() -> Fingerprint {
    Fingerprint {
        repository: "a".repeat(64),
        buffers: "b".repeat(64),
        environment: "c".repeat(64),
    }
}
fn engine_access(workspace: &str) -> vcp_engine::Access {
    vcp_engine::Access {
        actor: ActorId::parse("owner").unwrap(),
        workspace: WorkspaceId::parse(workspace).unwrap(),
        session: SessionId::parse(format!("session-{workspace}")).unwrap(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: true,
    }
}
fn command(
    engine: &Engine<Store>,
    access: &vcp_engine::Access,
    task: Option<TaskId>,
    expected: Revision,
    payload: Command,
) -> CommandEnvelope {
    CommandEnvelope {
        version: 1,
        id: CommandId::new(),
        workspace: access.workspace.clone(),
        session: access.session.clone(),
        task,
        caller: access.actor.clone(),
        controller: engine.controller().clone(),
        owner_epoch: engine.owner_epoch(),
        expected,
        steering: SteeringRevision::ZERO,
        payload,
    }
}
fn binding() -> Binding {
    Binding {
        host: HostId::new(),
        root: "C:/synthetic-memory-fixture".into(),
        repository: "repository".into(),
        worktree: "main".into(),
        revision: Revision::ZERO,
    }
}
async fn seed(engine: &mut Engine<Store>, workspace: &str) -> Scope {
    let access = engine_access(workspace);
    let facts = HostFacts::inspect(Timestamp::new(100));
    let initialize = command(
        engine,
        &access,
        None,
        Revision::ZERO,
        Command::Initialize { binding: binding() },
    );
    engine.handle(initialize, &access, &facts).await.unwrap();
    let scope = Scope {
        workspace: access.workspace.clone(),
        session: access.session.clone(),
        task: TaskId::new(),
    };
    let create = command(
        engine,
        &access,
        Some(scope.task.clone()),
        Revision::ZERO,
        Command::CreateTask {
            root: scope.task.clone(),
            parent: None,
            fork_origin: None,
            objective: Objective {
                text: r#"{"memory_preference":{"key":"test-output","value":"retain evidence"}}"#
                    .into(),
                constraints: vec![],
                acceptance: vec!["scope preserved".into()],
                source: EventId::new(),
                steering: SteeringRevision::ZERO,
            },
            fingerprint: fingerprint(),
            editing: false,
            required_checks: vec!["cargo-test".into()],
        },
    );
    engine.handle(create, &access, &facts).await.unwrap();
    scope
}

use std::collections::BTreeSet;
use vcp_domain::policy::*;
fn policy(workspace: WorkspaceId) -> Policy {
    Policy {
        workspace,
        revision: PolicyRevision::ZERO,
        mode: Autonomy::Ask,
        denials: vec![],
        workspace_roots: BTreeSet::from([RootId::parse("root").unwrap()]),
        automatic_effects: BTreeSet::new(),
        timeout_ceiling_ms: Units::new(1000),
        output_ceiling_bytes: ByteCount::new(65536),
    }
}
async fn put(store: &mut Store, document: AuthorityDocument, expected: Option<Revision>) {
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations: vec![Mutation::Put {
                record: Record::typed(
                    Collection::Access,
                    document.id.as_str(),
                    WorkspaceId::parse("workspace").unwrap(),
                    document.revision,
                    &document,
                )
                .unwrap(),
                expected,
            }],
            events: vec![],
            command: None,
        })
        .await
        .unwrap();
}
fn grant(scope: &Scope, host: &HostId, index: u32) -> Grant {
    Grant {
        id: GrantId::parse(format!("grant-{index:04}")).unwrap(),
        actor: ActorId::parse("owner").unwrap(),
        scope: GrantScope::Task {
            scope: scope.clone(),
        },
        host: host.clone(),
        binding: Revision::ZERO,
        authority: AuthorityRevision::ZERO,
        policy: PolicyRevision::ZERO,
        expires_at: Timestamp::new(1000),
        target: GrantTarget::Configured {
            tool: "vcp_read".into(),
            schema: "a".repeat(64),
            arguments_digest: "b".repeat(64),
            invocation: Invocation::Process {
                executable: "C:/secret-config/bin.exe".into(),
                executable_identity: "c".repeat(64),
                arguments: vec!["credential-SENTINEL".into()],
                directory: "C:/private-config".into(),
                environment_digest: "d".repeat(64),
                shell: false,
            },
            effects: BTreeSet::from([EffectClass::Read]),
            roots: BTreeSet::from([RootId::parse("root").unwrap()]),
            paths: vec!["src".into()],
            isolation: BTreeSet::new(),
            timeout_ms: Units::new(1000),
            output_bytes: ByteCount::new(65536),
        },
        origin: RuleOrigin::User,
        reason: "explicit user grant".into(),
        revoked: false,
        revision: Revision::ZERO,
        approval: None,
    }
}
async fn save_grant(store: &mut Store, g: Grant, expected: Option<Revision>) {
    put(
        store,
        AuthorityDocument {
            document_type: AuthorityFormat::VcpAuthorityV1,
            schema_version: 1,
            workspace: WorkspaceId::parse("workspace").unwrap(),
            id: AuthorityId::parse(g.id.as_str()).unwrap(),
            revision: g.revision,
            data: AuthorityData::Grant { grant: g },
        },
        expected,
    )
    .await;
}

#[tokio::test]
async fn scoped_policy_pages_preserve_provenance_and_exclude_private_grants_and_invocations() {
    for backend in [BackendKind::Files, BackendKind::Sqlite] {
        let temp = tempfile::tempdir().unwrap();
        let mut engine =
            Engine::new(Store::open(temp.path(), backend, &[]).await.unwrap()).unwrap();
        let scope = seed(&mut engine, "workspace").await;
        let access = engine_access("workspace");
        let other_task = TaskId::new();
        let create = command(
            &engine,
            &access,
            Some(other_task.clone()),
            Revision::ZERO,
            Command::CreateTask {
                root: other_task.clone(),
                parent: None,
                fork_origin: None,
                objective: Objective {
                    text: "other task".into(),
                    constraints: vec![],
                    acceptance: vec![],
                    source: EventId::new(),
                    steering: SteeringRevision::ZERO,
                },
                fingerprint: fingerprint(),
                editing: false,
                required_checks: vec![],
            },
        );
        engine
            .handle(create, &access, &HostFacts::inspect(Timestamp::new(100)))
            .await
            .unwrap();
        let mut store = engine.into_store();
        let workspace: vcp_domain::workspace::Workspace = store
            .state()
            .record(Collection::Workspace, "workspace", &scope.workspace)
            .unwrap()
            .decode()
            .unwrap();
        let mut configured = policy(scope.workspace.clone());
        configured.denials.push(Denial {
            id: "project-denial".into(),
            origin: RuleOrigin::User,
            reason: "bounded denial".into(),
            effects: BTreeSet::from([EffectClass::Network]),
            tool: None,
            roots: BTreeSet::new(),
            paths: (0..16)
                .map(|i| format!("{i}{}", "x".repeat(1000)))
                .collect(),
        });
        put(
            &mut store,
            AuthorityDocument {
                document_type: AuthorityFormat::VcpAuthorityV1,
                schema_version: 1,
                workspace: WorkspaceId::parse("workspace").unwrap(),
                id: AuthorityId::parse("workspace").unwrap(),
                revision: Revision::ZERO,
                data: AuthorityData::Policy {
                    policy: configured.clone(),
                },
            },
            None,
        )
        .await;
        for index in 0..35 {
            let mut g = grant(&scope, &workspace.binding.host, index);
            if index == 0 {
                g.revoked = true;
            }
            if index == 1 {
                g.expires_at = Timestamp::new(1);
            }
            save_grant(&mut store, g, None).await;
        }
        for index in 100..104 {
            let mut g = grant(&scope, &workspace.binding.host, index);
            match index {
                100 => {
                    g.scope = GrantScope::Workspace {
                        workspace: scope.workspace.clone(),
                    }
                }
                101 => {
                    g.scope = GrantScope::Session {
                        workspace: scope.workspace.clone(),
                        session: scope.session.clone(),
                    }
                }
                102 => g.actor = ActorId::parse("other-actor").unwrap(),
                _ => {
                    if let GrantScope::Task { scope } = &mut g.scope {
                        scope.task = other_task.clone();
                    }
                }
            }
            save_grant(&mut store, g, None).await;
        }
        let facts = HostPolicy {
            host_denials: vec![Denial {
                id: "host-denial".into(),
                origin: RuleOrigin::Host,
                reason: "host boundary".into(),
                effects: BTreeSet::from([EffectClass::Execute]),
                tool: None,
                roots: BTreeSet::new(),
                paths: vec![],
            }],
            effective: None,
            inherited: vec![],
            parent: None,
            unavailable: wire::Unavailable::BindingUnavailable,
        };
        let mut p = wire::Request {
            scope: methods::Scope {
                workspace: "workspace".to_owned().try_into().unwrap(),
                session: scope.session.to_string().try_into().unwrap(),
            },
            task: scope.task.to_string().try_into().unwrap(),
            section: wire::Section::Grants,
            limit: 32,
            cursor: None,
        };
        let before = store.state().watermark;
        let first = inspect(&store, &access, &p, &facts, Timestamp::new(200), &|| Ok(())).unwrap();
        assert_eq!(first.rows.len(), 32);
        assert!(!first.complete);
        assert!(matches!(
            first.effective,
            wire::Effective::Unavailable {
                reason: wire::Unavailable::BindingUnavailable
            }
        ));
        assert_eq!(first.persisted.as_ref().unwrap().mode, wire::Mode::Ask);
        let serialized = serde_json::to_string(&first).unwrap();
        for forbidden in [
            "credential-SENTINEL",
            "private-config",
            "secret-config",
            "grant-0100",
            "grant-0101",
            "grant-0102",
            "grant-0103",
        ] {
            assert!(!serialized.contains(forbidden));
        }
        let wire::Row::Grant { value: revoked } = &first.rows[0] else {
            panic!()
        };
        assert!(revoked.revoked);
        assert!(!revoked.current_matches.not_revoked);
        let wire::Row::Grant { value: expired } = &first.rows[1] else {
            panic!()
        };
        assert!(!expired.current_matches.unexpired);
        p.cursor = first.next_cursor.clone();
        let second = inspect(&store, &access, &p, &facts, Timestamp::new(200), &|| Ok(())).unwrap();
        assert_eq!(second.rows.len(), 3);
        assert!(second.complete);
        assert_eq!(before, store.state().watermark);
        let mut denied = access.clone();
        denied.read = false;
        assert!(inspect(&store, &denied, &p, &facts, Timestamp::new(200), &|| Ok(())).is_err());
        denied = access.clone();
        denied.actor = ActorId::parse("other-actor").unwrap();
        assert!(inspect(&store, &denied, &p, &facts, Timestamp::new(200), &|| Ok(())).is_err());
        denied = access.clone();
        denied.authority = AuthorityRevision::new(1);
        assert!(inspect(&store, &denied, &p, &facts, Timestamp::new(200), &|| Ok(())).is_err());
        let mut foreign = p.clone();
        foreign.scope.session = "foreign-session".to_owned().try_into().unwrap();
        assert!(inspect(
            &store,
            &access,
            &foreign,
            &facts,
            Timestamp::new(200),
            &|| Ok(())
        )
        .is_err());
        foreign = p.clone();
        foreign.section = wire::Section::Denials;
        assert!(inspect(
            &store,
            &access,
            &foreign,
            &facts,
            Timestamp::new(200),
            &|| Ok(())
        )
        .is_err());
        let mut changed = grant(&scope, &workspace.binding.host, 34);
        changed.revision = Revision::new(1);
        changed.revoked = true;
        save_grant(&mut store, changed, Some(Revision::ZERO)).await;
        assert!(inspect(&store, &access, &p, &facts, Timestamp::new(200), &|| Ok(())).is_err());
        p.cursor = None;
        // HostPolicy is trusted output of child_policy. Only exact canonical
        // pinned revisions may cross the ordinary direct-task visibility fence.
        let canonical_grant = |name: &str| {
            let document: AuthorityDocument = store
                .state()
                .record(Collection::Access, name, &scope.workspace)
                .unwrap()
                .decode()
                .unwrap();
            let AuthorityData::Grant { grant } = document.data else {
                panic!()
            };
            grant
        };
        let inherited = canonical_grant("grant-0103");
        let other_actor = canonical_grant("grant-0102");
        let mut stale_pin = canonical_grant("grant-0100");
        stale_pin.revision = stale_pin.revision.next().unwrap();
        let inherited_facts = HostPolicy {
            host_denials: facts.host_denials.clone(),
            effective: Some(configured.clone()),
            inherited: vec![inherited.clone(), other_actor, stale_pin],
            parent: Some(other_task.clone()),
            unavailable: wire::Unavailable::ChildScopeUnavailable,
        };
        let first = inspect(
            &store,
            &access,
            &p,
            &inherited_facts,
            Timestamp::new(200),
            &|| Ok(()),
        )
        .unwrap();
        assert!(matches!(
            first.effective,
            wire::Effective::Observed {
                source: wire::Source::ChildInherited,
                ..
            }
        ));
        let mut inherited_query = p.clone();
        inherited_query.cursor = first.next_cursor;
        let last = inspect(
            &store,
            &access,
            &inherited_query,
            &inherited_facts,
            Timestamp::new(200),
            &|| Ok(()),
        )
        .unwrap();
        assert!(last.complete);
        let values = first
            .rows
            .iter()
            .chain(&last.rows)
            .filter_map(|row| match row {
                wire::Row::Grant { value } => Some(value),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(values.len(), 36);
        for hidden in ["grant-0100", "grant-0101", "grant-0102"] {
            assert!(!values.iter().any(|value| value.id.as_str() == hidden));
        }
        let projected = values
            .iter()
            .find(|value| value.id.as_str() == inherited.id.as_str())
            .unwrap();
        assert_eq!(
            projected.revision.as_str(),
            inherited.revision.get().to_string()
        );
        assert_eq!(
            projected.inherited_from.as_ref().unwrap().as_str(),
            other_task.as_str()
        );
        assert!(
            matches!(&projected.scope, wire::GrantScope::Task { scope: original, task: original_task } if original.workspace.as_str() == scope.workspace.as_str() && original.session.as_str() == scope.session.as_str() && original_task.as_str() == other_task.as_str())
        );
        p.section = wire::Section::Denials;
        let denials =
            inspect(&store, &access, &p, &facts, Timestamp::new(200), &|| Ok(())).unwrap();
        assert_eq!(denials.rows.len(), 2);
        assert!(serde_json::to_vec(&denials).unwrap().len() < 65536);
        let wire::Row::Denial { value: projected } = &denials.rows[0] else {
            panic!()
        };
        assert_eq!(projected.path_count.as_str(), "16");
        p.limit = 1;
        let single = inspect(&store, &access, &p, &facts, Timestamp::new(200), &|| Ok(())).unwrap();
        assert_eq!(single.rows.len(), 1);
        assert!(single.next_cursor.is_some());
        assert!(serde_json::to_vec(&single).unwrap().len() < 65536);
        p.limit = 32;
        assert!(projected.paths.len() < 16);
        assert_eq!(denials.assessment, wire::Assessment::OperationNotEvaluated);
        assert!(
            inspect(&store, &access, &p, &facts, Timestamp::new(200), &|| Err(
                super::unavailable()
            ))
            .is_err()
        );
        let mut observed = facts;
        observed.effective = Some(configured);
        let current = inspect(&store, &access, &p, &observed, Timestamp::new(200), &|| {
            Ok(())
        })
        .unwrap();
        assert!(matches!(
            current.effective,
            wire::Effective::Observed {
                source: wire::Source::Workspace,
                ..
            }
        ));
        store.close().await.unwrap();
        let reopened = Store::open(temp.path(), backend, &[]).await.unwrap();
        assert_eq!(
            inspect(
                &reopened,
                &access,
                &p,
                &observed,
                Timestamp::new(200),
                &|| Ok(())
            )
            .unwrap(),
            current
        );
        reopened.close().await.unwrap();
    }
}

#[test]
fn escaped_path_summaries_are_bounded_and_preserve_omission() {
    let paths = vec!["\u{0001}".repeat(1000); 16];
    let projected = super::bounded_paths(&paths).unwrap();
    assert!(!projected.is_empty() && projected.len() < 16);
    assert!(projected.iter().all(|p| p.truncated));
    assert!(serde_json::to_vec(&projected).unwrap().len() < 4200);
}
