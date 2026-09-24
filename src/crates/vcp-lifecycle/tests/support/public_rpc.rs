// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_engine::{
    query::{Query, QueryResult},
    rpc::RpcHost,
    Access,
};
use vcp_protocol::methods::{self, Call, ResultValue};

fn access(config: &Config, write: bool) -> Access {
    Access {
        actor: config.actor.clone(),
        workspace: config.workspace.clone(),
        session: config.session.clone(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write,
        bootstrap: false,
    }
}
fn id(value: &str) -> methods::Id {
    value.to_owned().try_into().unwrap()
}
fn scope(config: &Config) -> methods::Scope {
    methods::Scope {
        workspace: id(config.workspace.as_str()),
        session: id(config.session.as_str()),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn public_workspace_trust_requires_controller_and_current_binding_and_replays_once() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let config = config(&temp.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let current = access(&config, true);
        let mut controller = host.public_connection(current.clone()).unwrap();
        let mut observer = host.public_connection(access(&config, false)).unwrap();
        let call = Call::WorkspaceSetTrust(methods::WorkspaceSetTrust {
            scope: scope(&config),
            mutation: methods::Mutation {
                command_id: id("trust-once"),
                expected_revision: 0.into(),
                steering_revision: 0.into(),
            },
            expected_binding_revision: 0.into(),
            trusted: true,
        });
        assert!(controller.call(call.clone(), &current).await.is_err());
        controller.acquire(CommandId::new(), None).unwrap();
        assert!(observer
            .call(call.clone(), &access(&config, false))
            .await
            .is_err());
        for mismatch in 0..3 {
            let mut stale = call.clone();
            if let Call::WorkspaceSetTrust(p) = &mut stale {
                match mismatch {
                    0 => p.expected_binding_revision = 1.into(),
                    1 => p.mutation.expected_revision = 1.into(),
                    _ => p.mutation.steering_revision = 1.into(),
                }
            }
            assert!(controller.call(stale, &current).await.is_err());
        }
        let result = controller.call(call.clone(), &current).await.unwrap();
        let state: vcp_domain::workspace::Workspace = host
            .snapshot()
            .unwrap()
            .record(
                Collection::Workspace,
                config.workspace.as_str(),
                &config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(state.trust, Trust::Trusted);
        assert_eq!(state.revision, Revision::new(1));
        assert_eq!(state.binding.revision, Revision::ZERO);
        let current = Access {
            authority: state.authority,
            ..current
        };
        assert!(controller.call(call.clone(), &current).await.is_err());
        controller.disconnect().unwrap().wait().await.unwrap();
        let mut controller = host.public_connection(current.clone()).unwrap();
        let lease = host
            .snapshot()
            .unwrap()
            .records
            .values()
            .filter(|row| row.collection == Collection::Access)
            .find_map(|row| row.decode::<vcp_domain::controller::Lease>().ok())
            .unwrap();
        controller
            .acquire(CommandId::new(), Some(lease.revision))
            .unwrap();
        assert_eq!(
            controller.call(call.clone(), &current).await.unwrap(),
            result
        );
        let mut conflict = call;
        if let Call::WorkspaceSetTrust(p) = &mut conflict {
            p.trusted = false;
        }
        assert!(controller.call(conflict, &current).await.is_err());
        observer.disconnect().unwrap().wait().await.unwrap();
        controller.disconnect().unwrap().wait().await.unwrap();
        owner.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn public_rpc_reads_remain_scoped_and_mutations_require_live_controller() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let config = config(&temp.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let mut controller = host.public_connection(access(&config, true)).unwrap();
        let mut observer = host.public_connection(access(&config, false)).unwrap();
        let create = Call::SessionCreate(methods::SessionCreate {
            scope: scope(&config),
            mutation: methods::Mutation {
                command_id: id("public-create"),
                expected_revision: 0.into(),
                steering_revision: 0.into(),
            },
            new_session: id("created-session"),
            configuration_revision: 0.into(),
        });
        assert!(controller
            .call(create.clone(), &access(&config, true))
            .await
            .is_err());
        controller
            .acquire(CommandId::parse("acquire").unwrap(), None)
            .unwrap();
        assert!(observer
            .call(create.clone(), &access(&config, false))
            .await
            .is_err());
        let receipt = controller
            .call(create.clone(), &access(&config, true))
            .await
            .unwrap();
        assert_eq!(
            controller
                .call(create, &access(&config, true))
                .await
                .unwrap(),
            receipt
        );
        let ResultValue::Acceptance(accepted) = receipt else {
            panic!("acceptance required")
        };
        assert_eq!(accepted.command_id.as_str(), "public-create");
        let observed = observer
            .call(
                Call::CommandRead(methods::CommandRead {
                    scope: scope(&config),
                    command_id: id("public-create"),
                }),
                &access(&config, false),
            )
            .await
            .unwrap();
        assert_eq!(observed, ResultValue::Acceptance(accepted));
        let mut other = access(&config, false);
        other.session = SessionId::parse("created-session").unwrap();
        assert!(observer
            .call(
                Call::SessionRead(methods::SessionRead {
                    scope: scope(&config)
                }),
                &other
            )
            .await
            .is_err());
        controller.disconnect().unwrap().wait().await.unwrap();
        assert!(matches!(
            observer
                .call(
                    Call::SessionRead(methods::SessionRead {
                        scope: scope(&config)
                    }),
                    &access(&config, false)
                )
                .await
                .unwrap(),
            ResultValue::Session(_)
        ));
        observer.disconnect().unwrap().wait().await.unwrap();
        owner.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn public_rpc_steering_holds_retained_owner_and_replays_without_another_hold() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let config = config(&temp.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let current = access(&config, true);
        let mut controller = host.public_connection(current.clone()).unwrap();
        controller
            .acquire(CommandId::parse("acquire").unwrap(), None)
            .unwrap();
        let binding = task(&host, &config, config.root_task.clone(), None);
        let server = start_mock_server().await;
        let mut registry = ExtensionRegistryBuilder::new();
        registry.turn_start_admission(Arc::new(host.clone()));
        registry.work_admission(Arc::new(host.clone()));
        let starter = host.clone();
        let cwd = workspace.clone();
        let test = test_codex()
            .with_extensions(Arc::new(registry.build()))
            .with_auth(codex_login::CodexAuth::from_api_key("synthetic-public-rpc"))
            .with_allowed_tools(AllowedTools(vec![]))
            .with_config(move |config| {
                config.cwd = cwd.try_into().unwrap();
                configure_fixture_provider(config);
                starter
                    .lifecycle()
                    .authorize_startup(config.cwd.as_path(), None)
                    .unwrap();
            })
            .build_with_auto_env(&server)
            .await
            .unwrap();
        let thread = host.lifecycle().attach_root(test.codex.clone()).unwrap();
        host.register(thread, binding.clone()).unwrap();
        let trigger = host
            .capture(
                thread,
                Channel::Evidence,
                b"public steering fixture".to_vec(),
            )
            .unwrap();
        let turn = TurnId::parse("public-turn").unwrap();
        host.command(
            Command::StartTurn {
                id: turn.clone(),
                trigger: trigger.spec.id,
            },
            Some(config.root_task.clone()),
            Revision::new(1),
        )
        .unwrap();
        let mut call = Call::TurnSteer(methods::TurnSteer {
            scope: scope(&config),
            mutation: methods::Mutation {
                command_id: id("public-steer"),
                expected_revision: 0.into(),
                steering_revision: 0.into(),
            },
            task: id(config.root_task.as_str()),
            turn: id(turn.as_str()),
            objective: "Review the controlled public change".into(),
            constraints: vec![],
            acceptance: vec!["one canonical steering change".into()],
        });
        let before = host.lifecycle().inspect(thread).unwrap().revision;
        assert!(controller.call(call.clone(), &current).await.is_err());
        assert_eq!(
            host.lifecycle().resume(thread, &before),
            Err(vcp_lifecycle::Error::NotHeld)
        );
        if let Call::TurnSteer(params) = &mut call {
            params.mutation.expected_revision = 1.into();
        }
        let receipt = controller.call(call.clone(), &current).await.unwrap();
        let held = host.lifecycle().inspect(thread).unwrap();
        assert!(held.local_hold);
        let QueryResult::Task { task: selected, .. } = controller
            .query(Query::Task {
                task: config.root_task.clone(),
            })
            .unwrap()
        else {
            panic!("task required")
        };
        assert_eq!(selected.state, TaskState::Paused);
        assert_eq!(selected.steering, SteeringRevision::new(1));
        assert_eq!(controller.call(call, &current).await.unwrap(), receipt);
        // The original held revision still authorizes this explicit retained
        // resume only if retry did not issue another hold. Canonical task state
        // remains paused, so this probe cannot restart execution.
        host.lifecycle().resume(thread, &held.revision).unwrap();
        let QueryResult::Task { task: after, .. } = controller
            .query(Query::Task {
                task: config.root_task.clone(),
            })
            .unwrap()
        else {
            panic!("task required")
        };
        assert_eq!(after, selected);
        controller.disconnect().unwrap().wait().await.unwrap();
        owner.close().await.unwrap();
    }
}
