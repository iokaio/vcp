// SPDX-License-Identifier: Apache-2.0
//! The durable resume primitive commits Running without submitting retained work.
use super::*;
use codex_extension_api::TurnStartAdmission;
use vcp_engine::{rpc::RpcHost, Access};
use vcp_lifecycle::foundation::{CanonicalOwner, PublicConnection};
use vcp_protocol::methods::{self, Call};

fn id(value: &str) -> methods::Id {
    value.to_owned().try_into().unwrap()
}
struct Fixture {
    _temporary: tempfile::TempDir,
    config: Config,
    host: CanonicalHost,
    owner: CanonicalOwner,
    controller: PublicConnection,
    current: Access,
    thread: codex_protocol::ThreadId,
    retained: TestCodex,
    server: wiremock::MockServer,
}
impl Fixture {
    async fn new(backend: BackendKind) -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let workspace = temporary.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let workspace = workspace.canonicalize().unwrap();
        let config = config(&temporary.path().join("canonical"), &workspace, backend);
        let (host, owner) = CanonicalHost::open(config.clone()).unwrap();
        let current = Access {
            actor: config.actor.clone(),
            workspace: config.workspace.clone(),
            session: config.session.clone(),
            authority: AuthorityRevision::ZERO,
            read: true,
            write: true,
            bootstrap: false,
        };
        let mut controller = host.public_connection(current.clone()).unwrap();
        controller.acquire(CommandId::new(), None).unwrap();
        let binding = task(&host, &config, config.root_task.clone(), None);
        let server = start_mock_server().await;
        let mut registry = ExtensionRegistryBuilder::new();
        registry.turn_start_admission(Arc::new(host.clone()));
        registry.work_admission(Arc::new(host.clone()));
        let startup = host.clone();
        let retained = test_codex()
            .with_extensions(Arc::new(registry.build()))
            .with_auth(codex_login::CodexAuth::from_api_key(
                "synthetic-resume-no-inference",
            ))
            .with_allowed_tools(AllowedTools(vec![]))
            .with_config(move |config| {
                config.cwd = workspace.try_into().unwrap();
                configure_fixture_provider(config);
                startup
                    .lifecycle()
                    .authorize_startup(config.cwd.as_path(), None)
                    .unwrap();
            })
            .build_with_auto_env(&server)
            .await
            .unwrap();
        let thread = host
            .lifecycle()
            .attach_root(retained.codex.clone())
            .unwrap();
        host.register(thread, binding).unwrap();
        Self {
            _temporary: temporary,
            config,
            host,
            owner,
            controller,
            current,
            thread,
            retained,
            server,
        }
    }
    fn selected(&self) -> Task {
        self.host
            .snapshot()
            .unwrap()
            .record(
                Collection::Task,
                self.config.root_task.as_str(),
                &self.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap()
    }
    fn request(&self, command: &str) -> methods::SessionResume {
        let task = self.selected();
        methods::SessionResume {
            scope: methods::Scope {
                workspace: id(self.config.workspace.as_str()),
                session: id(self.config.session.as_str()),
            },
            mutation: methods::Mutation {
                command_id: id(command),
                expected_revision: task.revision.get().into(),
                steering_revision: task.steering.get().into(),
            },
            task: id(self.config.root_task.as_str()),
        }
    }
    fn pause_canonical(&self) {
        let task = self.selected();
        self.host
            .command(
                Command::Transition {
                    next: TaskState::Paused,
                    reason: "explicit resume fixture pause".into(),
                    verification: None,
                },
                Some(task.scope.task),
                task.revision,
            )
            .unwrap();
    }
    async fn hold(&self) {
        let revision = self.host.lifecycle().inspect(self.thread).unwrap().revision;
        tokio::time::timeout(
            Duration::from_secs(5),
            self.host
                .lifecycle()
                .hold(self.thread, &revision)
                .unwrap()
                .wait(),
        )
        .await
        .unwrap()
        .unwrap();
    }
    async fn close(self) {
        self.controller.disconnect().unwrap().wait().await.unwrap();
        self.owner.close().await.unwrap();
        self.retained.codex.shutdown_and_wait().await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn public_resume_is_durable_replays_without_releasing_new_hold_and_never_submits_work() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let mut fixture = Fixture::new(backend).await;
        fixture.hold().await;
        fixture.pause_canonical();
        let request = fixture.request("resume-once");
        assert!(!fixture
            .controller
            .supported_methods()
            .contains(&"session/resume"));
        let before = fixture.host.snapshot().unwrap();
        let accepted = fixture
            .controller
            .resume_canonical(request.clone(), &fixture.current)
            .unwrap();
        assert_eq!(accepted.command.as_str(), "resume-once");
        assert_eq!(fixture.selected().state, TaskState::Running);
        assert!(
            !fixture
                .host
                .lifecycle()
                .inspect(fixture.thread)
                .unwrap()
                .local_hold
        );
        let running = fixture.host.snapshot().unwrap();
        assert_eq!(
            fixture
                .controller
                .resume_canonical(request.clone(), &fixture.current)
                .unwrap(),
            accepted
        );
        assert_eq!(fixture.host.snapshot().unwrap(), running);
        // Replaying an old durable identity must not release a later, deliberate hold.
        fixture.hold().await;
        fixture.pause_canonical();
        let paused = fixture.host.snapshot().unwrap();
        assert_eq!(
            fixture
                .controller
                .resume_canonical(request.clone(), &fixture.current)
                .unwrap(),
            accepted
        );
        assert_eq!(fixture.host.snapshot().unwrap(), paused);
        let held = fixture.host.lifecycle().inspect(fixture.thread).unwrap();
        assert!(held.local_hold);
        for collection in [
            Collection::Turn,
            Collection::Attempt,
            Collection::Effect,
            Collection::Reservation,
        ] {
            assert_eq!(
                before
                    .records
                    .values()
                    .filter(|row| row.collection == collection)
                    .count(),
                paused
                    .records
                    .values()
                    .filter(|row| row.collection == collection)
                    .count()
            );
        }
        assert!(fixture
            .server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|request| !request.url.path().ends_with("/responses")));
        // Same durable command with different semantics is not a new resume grant.
        let mut changed = request.clone();
        changed.mutation.expected_revision = fixture.selected().revision.get().into();
        assert!(fixture
            .controller
            .resume_canonical(changed, &fixture.current)
            .is_err());
        assert_eq!(fixture.host.snapshot().unwrap(), paused);
        let mut observer_access = fixture.current.clone();
        observer_access.write = false;
        let observer = fixture
            .host
            .public_connection(observer_access.clone())
            .unwrap();
        assert!(observer
            .resume_canonical(request.clone(), &observer_access)
            .is_err());
        observer.disconnect().unwrap().wait().await.unwrap();
        let token = fixture.controller.controller_token().unwrap();
        fixture
            .controller
            .call(
                Call::ControllerRelease(methods::ControllerRelease {
                    scope: request.scope.clone(),
                    command_id: id("release-after-resume"),
                    expected_revision: token.revision().get().into(),
                    generation: token.generation().get().into(),
                }),
                &fixture.current,
            )
            .await
            .unwrap();
        assert!(fixture
            .controller
            .resume_canonical(request, &fixture.current)
            .is_err());
        assert!(
            fixture
                .host
                .lifecycle()
                .inspect(fixture.thread)
                .unwrap()
                .local_hold
        );
        fixture.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn public_resume_denies_stale_access_revisions_and_unfinished_interrupt_before_commit() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let fixture = Fixture::new(backend).await;
        let permit = fixture
            .host
            .admit_turn_start_for_thread(fixture.thread)
            .unwrap();
        let revision = fixture
            .host
            .lifecycle()
            .inspect(fixture.thread)
            .unwrap()
            .revision;
        let waiter = fixture
            .host
            .lifecycle()
            .hold(fixture.thread, &revision)
            .unwrap();
        fixture.pause_canonical();
        let request = fixture.request("resume-after-drain");
        let before = fixture.host.snapshot().unwrap();
        assert!(fixture
            .controller
            .resume_canonical(request.clone(), &fixture.current)
            .is_err());
        assert_eq!(fixture.host.snapshot().unwrap(), before);
        assert!(
            fixture
                .host
                .lifecycle()
                .inspect(fixture.thread)
                .unwrap()
                .local_hold
        );
        drop(permit);
        tokio::time::timeout(Duration::from_secs(5), waiter.wait())
            .await
            .unwrap()
            .unwrap();
        for case in [
            "read",
            "write",
            "authority",
            "actor",
            "session",
            "task-revision",
            "steering",
        ] {
            let mut access = fixture.current.clone();
            let mut invalid = request.clone();
            match case {
                "read" => access.read = false,
                "write" => access.write = false,
                "authority" => access.authority = access.authority.next().unwrap(),
                "actor" => access.actor = ActorId::new(),
                "session" => access.session = SessionId::new(),
                "task-revision" => invalid.mutation.expected_revision = 0.into(),
                "steering" => invalid.mutation.steering_revision = 1.into(),
                _ => unreachable!(),
            }
            assert!(
                fixture
                    .controller
                    .resume_canonical(invalid, &access)
                    .is_err(),
                "{case}"
            );
            assert_eq!(fixture.host.snapshot().unwrap(), before, "{case}");
            assert!(
                fixture
                    .host
                    .lifecycle()
                    .inspect(fixture.thread)
                    .unwrap()
                    .local_hold,
                "{case}"
            );
        }
        fixture.controller.loss_signal().invalidate();
        assert!(fixture
            .controller
            .resume_canonical(request, &fixture.current)
            .is_err());
        assert_eq!(fixture.selected().state, TaskState::Paused);
        assert!(!fixture
            .host
            .snapshot()
            .unwrap()
            .commands
            .values()
            .any(|receipt| receipt.command.as_str() == "resume-after-drain"));
        assert!(fixture
            .server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|request| !request.url.path().ends_with("/responses")));
        fixture.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn public_resume_retains_uncertain_effect_liability_and_paused_admission() {
    use vcp_domain::effect::EffectState;
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let fixture = Fixture::new(backend).await;
        let effect = ToolRunId::parse("unreconciled-resume-effect").unwrap();
        fixture
            .host
            .command(
                Command::ProposeEffect {
                    id: effect.clone(),
                    operation_digest: "a".repeat(64),
                },
                Some(fixture.config.root_task.clone()),
                fixture.selected().revision,
            )
            .unwrap();
        let execution = ExecutionId::new();
        for (revision, next) in [
            EffectState::Validated,
            EffectState::Authorized,
            EffectState::DispatchRecorded,
            EffectState::OutcomeUnknown,
        ]
        .into_iter()
        .enumerate()
        {
            fixture
                .host
                .command(
                    Command::AdvanceEffect {
                        id: effect.clone(),
                        next,
                        reason: "synthetic uncertain dispatch receipt; no process launched".into(),
                        execution: if revision >= 2 {
                            Some(execution.clone())
                        } else {
                            None
                        },
                        exit_code: None,
                        observed_changes: vec![],
                    },
                    Some(fixture.config.root_task.clone()),
                    Revision::new(revision as u64),
                )
                .unwrap();
        }
        fixture.hold().await;
        fixture.pause_canonical();
        let request = fixture.request("resume-unreconciled");
        let before = fixture.host.snapshot().unwrap();
        assert!(fixture
            .controller
            .resume_canonical(request, &fixture.current)
            .is_err());
        let after = fixture.host.snapshot().unwrap();
        for collection in [
            Collection::Task,
            Collection::Effect,
            Collection::Ledger,
            Collection::Reservation,
            Collection::Attempt,
        ] {
            assert_eq!(
                before
                    .records
                    .values()
                    .filter(|row| row.collection == collection)
                    .collect::<Vec<_>>(),
                after
                    .records
                    .values()
                    .filter(|row| row.collection == collection)
                    .collect::<Vec<_>>(),
                "resume must preserve {collection:?} while effect outcome is unknown",
            );
        }
        let retained: vcp_domain::effect::Effect = after
            .record(
                Collection::Effect,
                effect.as_str(),
                &fixture.config.workspace,
            )
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!(retained.state, EffectState::OutcomeUnknown);
        assert_eq!(retained.execution, Some(execution));
        assert!(!after
            .commands
            .values()
            .any(|receipt| receipt.command.as_str() == "resume-unreconciled"));
        let reports: Vec<ArtifactDescriptor> = after
            .records
            .values()
            .filter(|row| {
                row.collection == Collection::Artifact && !before.records.contains_key(&row.key())
            })
            .map(|row| row.decode::<ArtifactDescriptor>().unwrap())
            .filter(|artifact| artifact.spec.schema == "vcp-effect-reconciliation-v1")
            .collect();
        assert_eq!(reports.len(), 1);
        let report: serde_json::Value = serde_json::from_slice(
            &fixture
                .host
                .read_artifact(reports[0].spec.id.clone())
                .unwrap(),
        )
        .unwrap();
        assert_eq!(report["effect"], effect.as_str());
        assert_eq!(report["outcome"], "outcome_unknown");
        assert_eq!(report["replayed"], false);
        assert_eq!(fixture.selected().state, TaskState::Paused);
        assert!(
            fixture
                .host
                .lifecycle()
                .inspect(fixture.thread)
                .unwrap()
                .local_hold
        );
        assert!(fixture
            .server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|request| !request.url.path().ends_with("/responses")));
        fixture.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn public_resume_concurrent_duplicate_has_one_commit_and_one_retained_release() {
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let fixture = Fixture::new(backend).await;
        fixture.hold().await;
        fixture.pause_canonical();
        let request = fixture.request("concurrent-resume");
        let before = fixture.host.snapshot().unwrap();
        let barrier = std::sync::Barrier::new(3);
        let results = std::thread::scope(|scope| {
            let invoke = || {
                barrier.wait();
                let receipt = fixture
                    .controller
                    .resume_canonical(request.clone(), &fixture.current)
                    .unwrap();
                let revision = fixture
                    .host
                    .lifecycle()
                    .inspect(fixture.thread)
                    .unwrap()
                    .revision;
                (receipt, revision)
            };
            let first = scope.spawn(invoke);
            let second = scope.spawn(invoke);
            barrier.wait();
            (first.join().unwrap(), second.join().unwrap())
        });
        assert_eq!(results.0 .0, results.1 .0);
        let after = fixture.host.snapshot().unwrap();
        assert_eq!(after.commands.len(), before.commands.len() + 1);
        assert_eq!(
            after
                .commands
                .values()
                .filter(|receipt| receipt.command.as_str() == "concurrent-resume")
                .count(),
            1
        );
        assert_eq!(fixture.selected().state, TaskState::Running);
        assert_eq!(
            fixture.selected().revision.get(),
            request
                .mutation
                .expected_revision
                .as_str()
                .parse::<u64>()
                .unwrap()
                + 1
        );
        // Both observed revisions still name the same released lifecycle state;
        // a duplicate release/checkpoint would invalidate the earlier revision.
        for revision in [&results.0 .1, &results.1 .1] {
            assert_eq!(
                fixture.host.lifecycle().resume(fixture.thread, revision),
                Err(vcp_lifecycle::Error::NotHeld)
            );
        }
        assert_eq!(
            before
                .records
                .values()
                .filter(|row| row.collection != Collection::Task)
                .collect::<Vec<_>>(),
            after
                .records
                .values()
                .filter(|row| row.collection != Collection::Task)
                .collect::<Vec<_>>(),
            "duplicate resume must not capture evidence, create turns, or allocate work",
        );
        assert!(fixture
            .server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|request| !request.url.path().ends_with("/responses")));
        fixture.close().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn public_resume_without_local_hold_still_requires_retained_work_receipt() {
    use codex_extension_api::{HostWorkAdmission, HostWorkKind};
    for backend in [BackendKind::Sqlite, BackendKind::Files] {
        let fixture = Fixture::new(backend).await;
        // Admit actual retained work without launching a tool or provider. Its
        // durable lifecycle receipt is independently required by final admission.
        let mut work = HostWorkAdmission::admit(
            fixture.host.lifecycle(),
            fixture.thread,
            HostWorkKind::Tool,
            "unresolved retained resume work",
        )
        .unwrap();
        fixture.pause_canonical();
        let request = fixture.request("resume-retained-work");
        let before = fixture.host.snapshot().unwrap();
        let view = fixture.host.lifecycle().inspect(fixture.thread).unwrap();
        assert!(!view.local_hold);
        assert_eq!(view.unresolved_work, 1);
        assert!(fixture
            .controller
            .resume_canonical(request.clone(), &fixture.current)
            .is_err());
        assert_eq!(fixture.host.snapshot().unwrap(), before);
        assert_eq!(fixture.selected().state, TaskState::Paused);
        assert_eq!(
            fixture
                .host
                .lifecycle()
                .inspect(fixture.thread)
                .unwrap()
                .unresolved_work,
            1
        );
        work.complete().unwrap();
        drop(work);
        assert_eq!(
            fixture
                .host
                .lifecycle()
                .inspect(fixture.thread)
                .unwrap()
                .unresolved_work,
            0
        );
        let receipt = fixture
            .controller
            .resume_canonical(request, &fixture.current)
            .unwrap();
        assert_eq!(receipt.command.as_str(), "resume-retained-work");
        assert_eq!(fixture.selected().state, TaskState::Running);
        assert!(fixture
            .server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|request| !request.url.path().ends_with("/responses")));
        fixture.close().await;
    }
}
