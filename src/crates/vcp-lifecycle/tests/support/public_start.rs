// SPDX-License-Identifier: Apache-2.0
use super::*;
use vcp_engine::Access;
use vcp_lifecycle::foundation::{PublicStartAdmission, PublicStartOutcome, PublicStartTicket};
use vcp_protocol::methods;

fn id(value: &str) -> methods::Id {
    value.to_owned().try_into().unwrap()
}
fn access(config: &Config) -> Access {
    Access {
        actor: config.actor.clone(),
        workspace: config.workspace.clone(),
        session: config.session.clone(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: false,
    }
}
fn request(config: &Config) -> methods::TurnStart {
    methods::TurnStart {
        scope: methods::Scope {
            workspace: id(config.workspace.as_str()),
            session: id(config.session.as_str()),
        },
        mutation: methods::Mutation {
            command_id: id("accepted-public-run"),
            expected_revision: 0.into(),
            steering_revision: 0.into(),
        },
        task: id(config.root_task.as_str()),
        turn: id("caller-selected-turn"),
        objective: "Inspect the accepted source".into(),
        constraints: vec!["Preserve files".into()],
        acceptance: vec!["Use evidence".into()],
        budget: methods::Budget {
            cap_micros: config.cap.micros.get().into(),
            currency: methods::Currency::Usd,
            max_requests: 3,
            deadline_seconds: 60,
        },
    }
}
fn accept(
    connection: &vcp_lifecycle::foundation::PublicConnection,
    request: methods::TurnStart,
    access: &Access,
) -> (vcp_protocol::command::CommandReceipt, PublicStartTicket) {
    let PublicStartAdmission::Ready(prepared) =
        connection.prepare_start_rpc(request, access).unwrap()
    else {
        panic!("fresh admission required")
    };
    let PublicStartOutcome::Accepted { receipt, ticket } = connection
        .accept_start(prepared, access, false, vec![])
        .unwrap()
    else {
        panic!("fresh acceptance required")
    };
    (receipt, ticket)
}
fn selected(host: &CanonicalHost, config: &Config) -> Task {
    host.snapshot()
        .unwrap()
        .record(
            Collection::Task,
            config.root_task.as_str(),
            &config.workspace,
        )
        .unwrap()
        .decode()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn accepted_start_constructor_activates_once_and_binds_exact_caller_turn() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        std::fs::write(workspace.join("source.txt"), "native observed input").unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let config = config(&temp.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let current = access(&config);
        let mut connection = host.public_connection(current.clone()).unwrap();
        connection.acquire(CommandId::new(), None).unwrap();
        let request = request(&config);
        let PublicStartAdmission::Ready(duplicate) = connection
            .prepare_start_rpc(request.clone(), &current)
            .unwrap()
        else {
            panic!()
        };
        let mut wrong = request.clone();
        wrong.task = id("unselected-root");
        assert!(connection.prepare_start_rpc(wrong, &current).is_err());
        let (receipt, mut ticket) = accept(&connection, request.clone(), &current);
        assert_eq!(selected(&host, &config).state, TaskState::Pending);
        assert!(
            matches!(connection.accept_start(duplicate, &current, false, vec![]).unwrap(), PublicStartOutcome::Replay(value) if value == receipt)
        );
        assert!(
            matches!(connection.prepare_start_rpc(request.clone(), &current).unwrap(), PublicStartAdmission::Replay(value) if value == receipt)
        );
        let startup = connection
            .authorize_start_startup(&mut ticket, &current)
            .unwrap();
        assert!(connection
            .authorize_start_startup(&mut ticket, &current)
            .is_err());
        assert!(host
            .lifecycle()
            .authorize_startup(&workspace, None)
            .is_err());
        let server = start_mock_server().await;
        let mut registry = ExtensionRegistryBuilder::new();
        registry.turn_start_admission(Arc::new(host.clone()));
        registry.work_admission(startup.work_admission());
        let retained = test_codex()
            .with_extensions(Arc::new(registry.build()))
            .with_auth(codex_login::CodexAuth::from_api_key(
                "synthetic-start-no-inference",
            ))
            .with_allowed_tools(AllowedTools(vec![]))
            .with_config(move |config| {
                config.cwd = workspace.try_into().unwrap();
                configure_fixture_provider(config);
            })
            .build_with_auto_env(&server)
            .await
            .unwrap();
        let thread = connection
            .attach_start_root(
                startup,
                retained.codex.clone(),
                ThreadBinding {
                    scope: ticket.scope().clone(),
                    agent: AgentId::new(),
                    role: RequestRole::Main,
                },
                &current,
            )
            .unwrap();
        assert!(host.lifecycle().inspect(thread).unwrap().local_hold);
        assert_eq!(
            connection.activate_start(ticket, &current).unwrap(),
            receipt
        );
        assert_eq!(selected(&host, &config).state, TaskState::Running);
        let (snapshot, raw) = provider_snapshot();
        host.configure_provider(snapshot, raw).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        host.configure_coding(
            thread,
            vcp_lifecycle::foundation::coding::CodingConfig {
                operating: "Do not submit in this admission test".into(),
                affected_paths: vec!["source.txt".into()],
                max_requests: 3,
                deadline: Timestamp::new(now + 30_000),
            },
        )
        .unwrap();
        let turn = TurnId::parse(request.turn.as_str()).unwrap();
        assert!(host
            .bind_preaccepted_coding_turn(thread, TurnId::new(), request.objective.clone())
            .is_err());
        assert!(host
            .bind_preaccepted_coding_turn(thread, turn.clone(), "changed payload".into())
            .is_err());
        let watermark = host.snapshot().unwrap().watermark;
        host.bind_preaccepted_coding_turn(thread, turn.clone(), request.objective.clone())
            .unwrap();
        assert_eq!(host.snapshot().unwrap().watermark, watermark);
        assert!(host
            .bind_preaccepted_coding_turn(thread, turn, request.objective.clone())
            .is_err());
        assert_eq!(
            host.snapshot()
                .unwrap()
                .records
                .values()
                .filter(|row| row.collection == Collection::Turn)
                .count(),
            1
        );
        assert!(server.received_requests().await.unwrap().is_empty());
        connection
            .disconnect()
            .unwrap()
            .wait()
            .await
            .unwrap()
            .unwrap();
        owner.close().await.unwrap();
        retained.codex.shutdown_and_wait().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn accepted_start_loss_and_reopen_preserve_receipt_but_never_reconstruct_ticket() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let config = config(&temp.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let current = access(&config);
        let mut connection = host.public_connection(current.clone()).unwrap();
        connection.acquire(CommandId::new(), None).unwrap();
        let request = request(&config);
        let (receipt, mut ticket) = accept(&connection, request.clone(), &current);
        let startup = connection
            .authorize_start_startup(&mut ticket, &current)
            .unwrap();
        let gate = startup.work_admission();
        connection.loss_signal().invalidate();
        assert!(gate.admit_startup(&workspace, None).is_err());
        assert!(connection.activate_start(ticket, &current).is_err());
        drop(startup);
        drop(gate);
        connection
            .disconnect()
            .unwrap()
            .wait()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(selected(&host, &config).state, TaskState::Paused);
        let turn: Turn = host
            .snapshot()
            .unwrap()
            .record(Collection::Turn, request.turn.as_str(), &config.workspace)
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(turn.state, TurnState::Paused);
        owner.close().await.unwrap();
        drop(host);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let mut connection = host.public_connection(current.clone()).unwrap();
        let lease_id =
            vcp_store::contract::controller_lease_id(&config.workspace, &config.session).unwrap();
        let lease: vcp_domain::controller::Lease = host
            .snapshot()
            .unwrap()
            .record(Collection::Access, &lease_id, &config.workspace)
            .unwrap()
            .decode()
            .unwrap();
        connection
            .acquire(CommandId::new(), Some(lease.revision))
            .unwrap();
        assert!(
            matches!(connection.prepare_start_rpc(request.clone(), &current).unwrap(), PublicStartAdmission::Replay(value) if value == receipt)
        );
        let mut observer = current.clone();
        observer.write = false;
        assert!(connection.prepare_start_rpc(request, &observer).is_err());
        assert_eq!(selected(&host, &config).state, TaskState::Paused);
        connection
            .disconnect()
            .unwrap()
            .wait()
            .await
            .unwrap()
            .unwrap();
        owner.close().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn expired_accepted_run_replays_but_cannot_construct_via_start_or_resume() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let config = config(&temp.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let current = access(&config);
        let mut connection = host.public_connection(current.clone()).unwrap();
        connection.acquire(CommandId::new(), None).unwrap();
        let mut request = request(&config);
        request.budget.deadline_seconds = 1;
        let (receipt, mut ticket) = accept(&connection, request.clone(), &current);
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert!(
            matches!(connection.prepare_start_rpc(request, &current).unwrap(),
            PublicStartAdmission::Replay(value) if value == receipt)
        );
        assert!(connection
            .authorize_start_startup(&mut ticket, &current)
            .is_err());
        let selected = selected(&host, &config);
        let resume = methods::SessionResume {
            scope: methods::Scope {
                workspace: id(config.workspace.as_str()),
                session: id(config.session.as_str()),
            },
            mutation: methods::Mutation {
                command_id: id("expired-explicit-resume"),
                expected_revision: selected.revision.get().into(),
                steering_revision: selected.steering.get().into(),
            },
            task: id(config.root_task.as_str()),
        };
        let vcp_lifecycle::foundation::PublicResumeAdmission::Ready(mut resume) =
            connection.prepare_resume_rpc(resume, &current).unwrap()
        else {
            panic!("no prior resume")
        };
        let watermark = host.snapshot().unwrap().watermark;
        assert!(connection
            .authorize_resume_startup(&mut resume, &current)
            .is_err());
        assert_eq!(host.snapshot().unwrap().watermark, watermark);
        assert!(host
            .lifecycle()
            .authorize_startup(&workspace, None)
            .is_err());
        connection
            .disconnect()
            .unwrap()
            .wait()
            .await
            .unwrap()
            .unwrap();
        owner.close().await.unwrap();
    }
}
