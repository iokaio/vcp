// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::output::OwnedJsonl;
use std::{collections::BTreeSet, io::IsTerminal, time::Duration};
use vcp_domain::{
    accounting::*,
    policy::{Autonomy as PolicyAutonomy, *},
    revision::*,
    task::{Objective, TaskState, Turn, TurnState},
    verification::Fingerprint,
    workspace::*,
};
use vcp_lifecycle::foundation::{CanonicalHost, Config, ThreadBinding};
use vcp_protocol::command::Command;
use vcp_store::BackendKind;

pub(super) struct Locations<'a> {
    pub data: &'a Path,
    pub directory: &'a Path,
    pub entry_path: &'a Path,
    pub pipe: &'a str,
    pub key: &'a str,
}

pub(super) async fn execute(
    cli: ValidatedCli,
    profile: settings::Profile,
    cap: Option<Micros>,
    entry: Option<WorkspaceEntry>,
    locations: Locations<'_>,
) -> Result<u8, String> {
    let descriptor_version = entry.as_ref().map_or(1, |entry| entry.version);
    let expected_identity = entry.as_ref().and_then(|entry| entry.identity.clone());
    let interactive = cli.interactive_terminal(
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
        std::io::stderr().is_terminal(),
    );
    let Locations {
        data,
        directory,
        entry_path,
        pipe,
        key,
    } = locations;
    let requested = match &cli.command {
        ValidatedCommand::Run(run) => settings::autonomy(run.autonomy),
        _ => {
            let entry = entry
                .as_ref()
                .ok_or("workspace has no session to resume or fork")?;
            let store = Store::open(
                &entry.config.canonical_root,
                entry.config.backend,
                std::slice::from_ref(&cli.workspace),
            )
            .await
            .map_err(|e| e.to_string())?;
            let policy = vcp_engine::policy::optional(store.state(), &entry.config.workspace)
                .map_err(|e| e.to_string())?;
            let mode = policy.map_or(PolicyAutonomy::Ask, |p| p.mode);
            store.close().await.map_err(|e| e.to_string())?;
            mode
        }
    };
    let prepared = profile.prepare(requested)?;
    let credential = vcp_engine::capture::ProviderCredential::from_config(
        std::env::var("OPENROUTER_API_KEY").map_err(|_| "OPENROUTER_API_KEY is required")?,
    );
    #[allow(unused_mut)]
    let mut retained = crate::session::configuration(
        &data.join("retained"),
        &cli.workspace,
        &credential,
        &prepared.profile.provider.compatibility.model,
    )
    .await?;
    #[cfg(feature = "qualification")]
    if let Some(endpoint) = &prepared.profile.qualification_endpoint {
        let url = endpoint
            .strip_prefix("http://127.0.0.1:")
            .ok_or("qualification transport must be loopback")?;
        if !url
            .strip_suffix("/v1")
            .is_some_and(|port| port.parse::<u16>().is_ok())
            || credential.header_for_transport() != "synthetic-cli-qualification"
        {
            return Err("qualification transport requires synthetic credentials".into());
        }
        retained.model_provider.base_url = Some(endpoint.clone());
    }
    let mut objective = match &cli.command {
        ValidatedCommand::Run(run) => Some(run.objective.clone()),
        _ => None,
    };
    let mut fork_origin = None;
    let mut fork_boundary = None;
    let mut selected_revision = None;
    if entry.is_none() && objective.is_none() {
        return Err("workspace has no session to resume or fork".into());
    }
    let mut config = if let Some(entry) = entry {
        entry.config
    } else {
        Config {
            canonical_root: directory.join("canonical"),
            backend: crate::storage::preference(directory)?.unwrap_or(BackendKind::Sqlite),
            workspace: WorkspaceId::new(),
            session: SessionId::new(),
            binding: Binding {
                host: HostId::new(),
                root: cli.workspace.to_string_lossy().into_owned(),
                repository: key.into(),
                worktree: key.into(),
                revision: Revision::ZERO,
            },
            actor: ActorId::new(),
            root_task: TaskId::new(),
            cap: Money {
                currency: prepared.profile.provider.price.currency.clone(),
                micros: cap.unwrap_or(Micros::ZERO),
            },
            protected: Micros::ZERO,
            price: prepared.profile.provider.price.clone(),
            input_ceiling: prepared.profile.provider.max_input,
            output_ceiling: Units::new(prepared.profile.provider.max_output.get().min(4096)),
            artifact_limit: ByteCount::new(vcp_store::artifact::DEFAULT_ARTIFACT_LIMIT),
            host_tool_denials: vec![],
        }
    };
    if objective.is_none() {
        let store = Store::open(
            &config.canonical_root,
            config.backend,
            std::slice::from_ref(&cli.workspace),
        )
        .await
        .map_err(|e| e.to_string())?;
        let selected = match &cli.command {
            ValidatedCommand::Resume(resume) => match &resume.task {
                Some(id) => task_from(store.state(), &config.workspace, id),
                None => latest(store.state(), &config.workspace, None),
            },
            ValidatedCommand::Sessions(Sessions::Resume { session }) => {
                latest(store.state(), &config.workspace, Some(session))
            }
            ValidatedCommand::Sessions(Sessions::Fork {
                session,
                through_turn,
            }) => {
                let turn: Turn = store
                    .state()
                    .record(Collection::Turn, through_turn.as_str(), &config.workspace)
                    .and_then(|r| r.decode())
                    .map_err(|e| e.to_string())?;
                if turn.scope.session != *session || turn.state != TurnState::Completed {
                    return Err("fork requires a completed turn in the selected session".into());
                }
                let source = task_from(store.state(), &config.workspace, &turn.scope.task)?;
                objective = Some(
                    source
                        .objectives
                        .iter()
                        .rev()
                        .find(|o| o.steering <= turn.steering)
                        .ok_or("fork objective unavailable")?
                        .text
                        .clone(),
                );
                fork_origin = Some(source.scope.task.clone());
                fork_boundary = Some(through_turn.clone());
                Ok(source)
            }
            _ => Err("unsupported execution command".into()),
        };
        let selected = selected?;
        if objective.is_none() {
            selected_revision = Some(selected.revision);
            let summary = crate::continuation::candidates(store.state(), &config.workspace)?
                .into_iter()
                .find(|row| row.task == selected.scope.task);
            if let Some(summary) = summary {
                let summary = crate::continuation::summarize(summary)?;
                eprintln!(
                    "Continuation review: {}",
                    serde_json::to_string(&summary).map_err(|e| e.to_string())?
                );
            }
            if let ValidatedCommand::Resume(resume) = &cli.command {
                if resume
                    .expected_revision
                    .is_some_and(|r| r != selected.revision.get())
                {
                    return Err("task changed since selection; refresh workspace discovery".into());
                }
            }
            if selected.parent.is_some() {
                return Err(
                    "resume selects a root task; child control belongs to its owner".into(),
                );
            }
            let ledger: Ledger = store
                .state()
                .record(
                    Collection::Ledger,
                    selected.scope.task.as_str(),
                    &config.workspace,
                )
                .and_then(|r| r.decode())
                .map_err(|_| "task has no durable budget admission; start a new run")?;
            config.cap.micros = ledger.cap;
        }
        store.close().await.map_err(|e| e.to_string())?;
        config.session = selected.scope.session;
        config.root_task = selected.scope.task;
    }
    if objective.is_some() {
        config.root_task = TaskId::new();
        config.cap.micros = match &cli.command {
            ValidatedCommand::Run(run) => run.budget,
            _ => cap.ok_or("fork requires persisted budget cap")?,
        };
    }
    config.price = prepared.profile.provider.price.clone();
    config.input_ceiling = prepared.profile.provider.max_input;
    config.output_ceiling = Units::new(prepared.profile.provider.max_output.get().min(4096));
    let root = vcp_repository::Root::open(
        vcp_repository::RootIdentity {
            workspace: config.workspace.clone(),
            root: RootId::parse(config.workspace.as_str()).map_err(|e| e.to_string())?,
            repository: config.binding.repository.clone(),
            worktree: config.binding.worktree.clone(),
            binding: config.binding.revision,
        },
        &cli.workspace,
    )
    .map_err(|e| e.to_string())?;
    let _root_pin = root.hold(None, true).map_err(|e| e.to_string())?;
    let _git_pin = if expected_identity
        .as_ref()
        .is_some_and(|identity| identity.git_directory_identity.is_some())
    {
        Some(
            root.hold(Some(Path::new(".git")), true)
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };
    if let Some(identity) = &expected_identity {
        crate::binding::verify(&root, identity)?;
    }
    let registered_identity = match expected_identity {
        Some(identity) => identity,
        None => crate::binding::capture(&root)?,
    };
    let observation = root
        .observe(None, &Default::default())
        .await
        .map_err(|e| e.to_string())?;
    vcp_tools::verification::discover(&observation, &prepared.profile.checks)
        .map_err(|e| e.to_string())?;
    let fingerprint = Fingerprint {
        repository: observation.digest,
        buffers: digest_bytes(b""),
        environment: digest_bytes(b"vcp-cli-explicit-user-profile/1"),
    };
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let (mut host, mut owner) = CanonicalHost::open_selected(config.clone(), selected_revision)?;
    if let Some(boundary) = fork_boundary {
        let session = SessionId::new();
        host.command(
            Command::CreateSession {
                id: session.clone(),
                fork_through: Some(boundary),
            },
            None,
            Revision::ZERO,
        )?;
        owner.close().await?;
        drop(host);
        config.session = session;
        (host, owner) = CanonicalHost::open(config.clone())?;
    }
    let _service = AbortOnDrop(control::serve(
        pipe,
        host.clone(),
        config.workspace.clone(),
    )?);
    let _console = host.install_console_close_handler()?;
    setup_policy(&host, &config, &prepared, requested)?;
    let resuming = objective.is_none();
    let accepted = if let Some(objective) = objective {
        host.command(
            Command::CreateTask {
                root: config.root_task.clone(),
                parent: None,
                fork_origin,
                objective: Objective {
                    text: objective,
                    constraints: vec![],
                    acceptance: prepared
                        .profile
                        .checks
                        .iter()
                        .map(|c| c.rationale.clone())
                        .collect(),
                    source: EventId::new(),
                    steering: SteeringRevision::ZERO,
                },
                fingerprint,
                editing: !prepared.profile.checks.is_empty(),
                required_checks: prepared
                    .profile
                    .checks
                    .iter()
                    .map(|check| format!("{}#test", check.manifest))
                    .collect(),
            },
            Some(config.root_task.clone()),
            Revision::ZERO,
        )?
    } else {
        let task = task_from(&host.snapshot()?, &config.workspace, &config.root_task)?;
        crate::outcome::Outcome::read(&host, &task.scope)?.receipt
    };
    settings::save(
        entry_path,
        &WorkspaceEntry {
            rebind_pending: false,
            version: descriptor_version,
            config: config.clone(),
            identity: Some(registered_identity),
        },
    )?;
    host.initialize_root_budget()?;
    let scope = Scope {
        workspace: config.workspace.clone(),
        session: config.session.clone(),
        task: config.root_task.clone(),
    };
    let correlation = accepted.command.clone();
    let mut output = OwnedJsonl::new(
        DisplayOutput {
            jsonl: cli.format == Format::Jsonl,
            frame: Vec::new(),
        },
        owner,
    )
    .map_err(|e| e.to_string())?;
    let mut after = SessionSeq::ZERO;
    let mut active_session = None;
    let automatic_backup = crate::backup::configuration_status(
        data,
        &cli.workspace,
        &WorkspaceEntry {
            rebind_pending: false,
            version: descriptor_version,
            config: config.clone(),
            identity: None,
        },
    )
    .ok()
    .is_some_and(|status| status["configuration"]["automatic"] == true);
    let mut backup_triggers = crate::backup_triggers::Triggers::new(automatic_backup);
    let signal_host = host.clone();
    let signal_config = config.clone();
    let _signal = AbortOnDrop(tokio::spawn(async move {
        while tokio::signal::ctrl_c().await.is_ok() {
            let next = if interactive {
                TaskState::Paused
            } else {
                TaskState::Cancelled
            };
            let _ = stop(&signal_host, &signal_config, next);
            if !interactive {
                break;
            }
        }
    }));
    // From here onward, errors must finish this durable task, never append an
    // unrelated configuration result after acceptance.
    let execution=async{
        output.emit(&correlation,Some(&scope),Payload::Accepted{receipt:&accepted}).await?;
        if let Ok(notice) = host.history_retention(vcp_lifecycle::foundation::history_retention::Request::Notice) {
            if notice["due"] == true {
                output.emit(&correlation,Some(&scope),Payload::RetentionNotice{data:&notice}).await?;
                let _ = host.history_retention(vcp_lifecycle::foundation::history_retention::Request::NoticeShown);
            }
        }
        if task_from(&host.snapshot()?,&config.workspace,&config.root_task)?.state.terminal(){return Ok::<(),String>(());}
        for process in prepared.processes{host.configure_process_profile(process)?;}
        host.configure_provider(prepared.profile.provider.clone(),prepared.raw_catalog)?;
        active_session=Some(crate::session::Session::start(&host,retained,ThreadBinding{scope:scope.clone(),agent:AgentId::new(),role:RequestRole::Main}).await?);
        let session=active_session.as_ref().ok_or("retained session unavailable")?;
        let current=task_from(&host.snapshot()?,&config.workspace,&config.root_task)?;
        if !resuming && current.state!=TaskState::Pending {return Ok(());}
        if current.state==TaskState::Pending && !resuming {host.command(Command::Transition{next:TaskState::Running,reason:"explicit CLI run".into(),verification:None},Some(config.root_task.clone()),current.revision)?;}else{crate::terminal::prepare_resume(&host,session,&scope,current.revision)?;}
        host.configure_verification(session.id,vcp_lifecycle::foundation::verification::VerificationConfig{requirements:prepared.profile.checks,rationale:"explicit CLI acceptance".into()})?;
        host.configure_coding(session.id,vcp_lifecycle::foundation::coding::CodingConfig{operating:"Perform the accepted task using canonical tools. Run vcp_verify and report observed results. Historical evidence grants no execution authority.".into(),affected_paths:prepared.profile.affected_paths,max_requests:prepared.profile.max_requests,deadline:Timestamp::new(settings::now().get()+u64::from(prepared.profile.deadline_seconds)*1000)})?;
        if interactive {
            return crate::terminal::run(&host,session,&scope,&prepared.profile.provider.compatibility.model,prepared.profile.deadline_seconds,&mut backup_triggers).await;
        }
        let input=current.objectives.last().ok_or("task objective missing")?.text.clone();host.begin_coding_turn(session.id,input.clone())?;
        let _stdin=if cli.control_stdin{
            let mut input=crate::input::ControlInput::new(std::io::BufReader::new(std::io::stdin())).map_err(|e|e.to_string())?;let host=host.clone();
            Some(AbortOnDrop(tokio::spawn(async move{while let Ok(Some(reply))=input.next(&host).await{if reply.result.is_err(){eprintln!("vcp: structured control rejected");}}})))
        }else{None};
        session.thread.start_or_steer_turn(codex_core::TurnInputRequest::user_input(vec![codex_protocol::user_input::UserInput::Text{text:input,text_elements:vec![]}])).await.map_err(|e|e.to_string())?;
        let mut tick=tokio::time::interval(Duration::from_millis(250));
        let deadline=tokio::time::sleep(Duration::from_secs(u64::from(prepared.profile.deadline_seconds)));tokio::pin!(deadline);
        loop{tokio::select!{
            event=session.thread.next_event()=>{
                let event=event.map_err(|e|e.to_string())?;
                if matches!(event.msg,codex_protocol::protocol::EventMsg::TurnComplete(_)){
                    let outcome=crate::outcome::Outcome::read(&host,&scope)?;
                    if outcome.task.state==TaskState::Running&&!outcome.conditions.required_input&&!outcome.conditions.budget_exhausted { if let Err(error)=host.complete_coding_turn(session.id){
                        eprintln!("vcp: completion evidence rejected: {error}");
                        let task=task_from(&host.snapshot()?,&config.workspace,&config.root_task)?;
                        if task.state==TaskState::Running{host.command(Command::Transition{next:TaskState::Failed,reason:"retained turn ended without current completion evidence".into(),verification:None},Some(config.root_task.clone()),task.revision)?;}
                    }}break;
                }
            }
            _=tick.tick()=>{let outcome=crate::outcome::Outcome::read(&host,&scope)?;if outcome.task.state!=TaskState::Running||outcome.conditions.required_input{break;}after=output.drain_events(&host,&correlation,after).await?;}
            _=&mut deadline=>{stop(&host,&config,TaskState::Paused)?;break;}
        }}Ok(())
    }.await;
    if execution.is_err() {
        eprintln!("vcp: execution stopped; inspect the durable task for recovery");
    }
    if let Ok(state) = host.snapshot() {
        if let Ok(task) = task_from(&state, &config.workspace, &config.root_task) {
            if let Some(message) = backup_triggers.observe(&host, task.state) {
                eprintln!("vcp: {message}");
            }
        }
    }
    // The output owner's finish closes canonical authority. Finish only bounded
    // configured local maintenance before that boundary, without resuming work.
    backup_triggers.shutdown(&host).await;
    let result = output
        .finish_with_error(&host, &correlation, &scope, after, execution.is_err())
        .await;
    if let Some(session) = active_session {
        let _ = session.thread.shutdown_and_wait().await;
    }
    Ok(result.unwrap_or(1))
}
struct AbortOnDrop(tokio::task::JoinHandle<()>);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}
fn stop(host: &CanonicalHost, config: &Config, next: TaskState) -> Result<(), String> {
    let task = task_from(&host.snapshot()?, &config.workspace, &config.root_task)?;
    host.stop(host.control_envelope(
        CommandId::new(),
        config.root_task.clone(),
        task.revision,
        Command::Transition {
            next,
            reason: "explicit CLI interruption".into(),
            verification: None,
        },
    )?)?;
    Ok(())
}
fn setup_policy(
    host: &CanonicalHost,
    config: &Config,
    prepared: &settings::PreparedProfile,
    mode: PolicyAutonomy,
) -> Result<(), String> {
    let workspace: Workspace = host
        .snapshot()?
        .record(
            Collection::Workspace,
            config.workspace.as_str(),
            &config.workspace,
        )
        .and_then(|r| r.decode())
        .map_err(|e| e.to_string())?;
    if workspace.trust != Trust::Trusted {
        host.command(
            Command::SetWorkspaceTrust {
                trust: Trust::Trusted,
            },
            None,
            workspace.revision,
        )?;
    }
    let mut roots =
        BTreeSet::from([RootId::parse(config.workspace.as_str()).map_err(|e| e.to_string())?]);
    for profile in &prepared.profile.processes {
        roots.insert(RootId::parse(format!("exec-{}", profile.name)).map_err(|e| e.to_string())?);
    }
    let state = host.snapshot()?;
    let policy =
        vcp_engine::policy::optional(&state, &config.workspace).map_err(|e| e.to_string())?;
    let previous = policy.map(|p| p.revision);
    let revision = previous
        .map_or(Ok(PolicyRevision::ZERO), PolicyRevision::next)
        .map_err(|e| e.to_string())?;
    host.command(
        Command::SetPolicy {
            policy: Policy {
                workspace: config.workspace.clone(),
                revision,
                mode,
                denials: vec![],
                workspace_roots: roots,
                automatic_effects: prepared.profile.automatic_effects.clone(),
                timeout_ceiling_ms: Units::new(120000),
                output_ceiling_bytes: ByteCount::new(1024 * 1024),
            },
        },
        None,
        Revision::new(previous.unwrap_or(PolicyRevision::ZERO).get()),
    )?;
    Ok(())
}
