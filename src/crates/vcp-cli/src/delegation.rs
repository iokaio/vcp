// SPDX-License-Identifier: Apache-2.0
//! Explicit owner delegation; JSON selects work, never provider credentials or grants.
use serde::Deserialize;
use std::{
    collections::BTreeSet,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use vcp_domain::{
    agents::ChildMode,
    workspace::{Scope, Workspace},
    CommandId, RootId, TaskId, Timestamp,
};
use vcp_lifecycle::foundation::{CanonicalHost, DelegationRequest};
use vcp_repository::{worktree::Snapshotter, Root, RootIdentity};
use vcp_store::contract::Collection;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Specification {
    pub version: u32,
    pub git: PathBuf,
    pub disposable_parent: PathBuf,
    pub objective: String,
    pub acceptance: Vec<String>,
    pub mode: ChildMode,
    pub write_paths: BTreeSet<String>,
    pub untracked_inputs: BTreeSet<String>,
    pub allocation_usd: String,
    pub seconds: u32,
    pub required_checks: Vec<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub read_paths: Option<BTreeSet<String>>,
    #[serde(default)]
    pub helper: Option<vcp_lifecycle::foundation::HelperTemplate>,
}

pub struct Child {
    pub task: TaskId,
    pub session: crate::session::Session,
    pub snapshotter: Arc<Snapshotter>,
}
fn event_for_turn(event: &codex_protocol::protocol::Event, turn: &str) -> bool {
    use codex_protocol::protocol::EventMsg;
    // Error handling fences immediately, so a prior turn's terminal event can
    // remain queued. Submission identity, never queue position, owns the pump.
    event.id == turn
        && match &event.msg {
            EventMsg::TurnComplete(end) => end.turn_id == turn,
            EventMsg::TurnAborted(end) => end.turn_id.as_deref().is_none_or(|id| id == turn),
            _ => true,
        }
}
pub(crate) fn native_snapshotter(git: PathBuf) -> Result<Arc<Snapshotter>, String> {
    Ok(Arc::new(
        Snapshotter::new(
            git,
            ["PATH", "SystemRoot", "WINDIR", "TEMP", "TMP"]
                .into_iter()
                .filter_map(|name| {
                    std::env::var_os(name).map(|value| (std::ffi::OsString::from(name), value))
                })
                .collect(),
            Duration::from_secs(30),
            8 * 1024 * 1024,
        )
        .map_err(|e| e.to_string())?,
    ))
}
pub async fn recover(
    host: &CanonicalHost,
    parent: &crate::session::Session,
    task: TaskId,
    git: PathBuf,
) -> Result<Child, String> {
    let snapshotter = native_snapshotter(git)?;
    let (session, _evidence) = parent
        .recover_child(host, task.clone(), &snapshotter)
        .await?;
    if let Err(error) = host.configure_child_from_parent(parent.id, session.id) {
        let cleanup = session.thread.shutdown_and_wait().await;
        return Err(format!(
            "child {task} recovery setup failed: {error}; retained cleanup: {cleanup:?}"
        ));
    }
    Ok(Child {
        task,
        session,
        snapshotter,
    })
}
pub fn resume(host: &CanonicalHost, child: &Child, scope: &Scope) -> Result<(), String> {
    let task = crate::agents_view::child(&host.snapshot()?, scope, &child.task)?;
    if task.state.terminal() || task.state == vcp_domain::task::TaskState::Running {
        return Err("child resume requires a suspended task".into());
    }
    let runtime = host.lifecycle();
    let view = runtime
        .inspect(child.session.id)
        .map_err(|e| format!("child owner: {e:?}"))?;
    if view.inherited_hold {
        return Err("resume the parent before this child".into());
    }
    if view.local_hold {
        runtime
            .resume(child.session.id, &view.revision)
            .map_err(|e| format!("child interruption has not drained: {e:?}"))?;
    }
    host.resume(child.session.id, task.revision, task.fingerprint)?;
    Ok(())
}

pub async fn run(
    host: CanonicalHost,
    child: &Child,
    scope: &Scope,
    notices: tokio::sync::mpsc::Sender<String>,
) -> Result<tokio::task::JoinHandle<Result<(), String>>, String> {
    use codex_protocol::{protocol::EventMsg, user_input::UserInput};
    // Cooperative ownership shared with terminal/batch supervisors. Session's
    // retained thread remains accessible to trusted callers, so every event
    // consumer must claim this guard before submitting or observing work.
    let events = child.session.claim_events()?;
    let mut scope = scope.clone();
    scope.task = child.task.clone();
    let task: vcp_domain::task::Task = host
        .snapshot()?
        .record(Collection::Task, scope.task.as_str(), &scope.workspace)
        .and_then(|r| r.decode())
        .map_err(|e| e.to_string())?;
    let text = task
        .objectives
        .last()
        .ok_or("child objective missing")?
        .text
        .clone();
    host.begin_coding_turn(child.session.id, text.clone())?;
    let submitted = child
        .session
        .thread
        .start_or_steer_turn(codex_core::TurnInputRequest::user_input(vec![
            UserInput::Text {
                text,
                text_elements: vec![],
            },
        ]))
        .await
        .map_err(|e| e.to_string())?;
    let turn = match submitted {
        codex_core::TurnInputSubmission::Started { turn_id }
        | codex_core::TurnInputSubmission::Steered { turn_id } => turn_id,
        codex_core::TurnInputSubmission::NotSubmitted { .. } => {
            return Err("child turn admission rejected".into())
        }
    };
    let session = child.session.clone();
    Ok(tokio::spawn(async move {
        let _events = events;
        let outcome: Result<(), String> = async {
            loop {
                let event = session
                    .thread
                    .next_event()
                    .await
                    .map_err(|e| e.to_string())?;
                if !event_for_turn(&event, &turn) {
                    continue;
                }
                match event.msg {
                    EventMsg::AgentMessage(message) => {
                        let capture = host.open_output(
                            session.id,
                            vcp_domain::artifact::Channel::ChildTranscript,
                        )?;
                        capture.write(message.message.as_bytes())?;
                        capture.finish()?;
                        let _ = notices.try_send(format!(
                            "Agent {}: {}",
                            scope.task,
                            crate::terminal::sanitize(&message.message, 1024)
                        ));
                    }
                    EventMsg::Error(error) => {
                        return Err(format!("child turn failed: {}", error.message));
                    }
                    EventMsg::TurnAborted(_) => {
                        return Err("child turn was interrupted; explicit resume required".into());
                    }
                    EventMsg::TurnComplete(complete) => {
                        if let Some(error) = complete.error {
                            return Err(format!("child turn failed: {}", error.message));
                        }
                        let outcome = crate::outcome::Outcome::read(&host, &scope)?;
                        if outcome.task.state == vcp_domain::task::TaskState::Running {
                            if outcome.conditions.required_input || outcome.conditions.budget_exhausted {
                                return Err("child requires input or budget reconciliation before explicit resume".into());
                            }
                            host.complete_coding_turn(session.id)?;
                        }
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }
        .await;
        if outcome.is_err() {
            let task: vcp_domain::task::Task = host
                .snapshot()?
                .record(Collection::Task, scope.task.as_str(), &scope.workspace)
                .and_then(|r| r.decode())
                .map_err(|e| e.to_string())?;
            if !task.state.terminal() {
                host.stop(host.control_envelope(
                    CommandId::new(),
                    task.scope.task,
                    task.revision,
                    vcp_protocol::command::Command::Transition {
                        next: vcp_domain::task::TaskState::Paused,
                        reason: "child turn stopped without current completion evidence".into(),
                        verification: None,
                    },
                )?)?;
            }
        }
        let message = match &outcome {
            Ok(()) => format!(
                "Agent {}: turn ended; canonical status and costs retained.",
                scope.task
            ),
            Err(error) => format!("Agent {}: stopped: {error}", scope.task),
        };
        let _ = notices.send(message).await;
        outcome
    }))
}

#[cfg(test)]
mod tests {
    use super::event_for_turn;
    use codex_protocol::protocol::{Event, EventMsg, TurnAbortReason, TurnAbortedEvent};

    fn aborted(submission: &str, turn: Option<&str>) -> Event {
        Event {
            id: submission.into(),
            msg: EventMsg::TurnAborted(TurnAbortedEvent {
                turn_id: turn.map(str::to_owned),
                reason: TurnAbortReason::Interrupted,
                started_at: None,
                completed_at: None,
                duration_ms: None,
            }),
        }
    }

    #[test]
    fn resumed_child_ignores_terminal_events_queued_by_failed_previous_turn() {
        let queued = [
            aborted("failed", Some("failed")),
            aborted("failed", None),
            aborted("resumed", Some("resumed")),
        ];
        let owned: Vec<_> = queued
            .iter()
            .filter(|event| event_for_turn(event, "resumed"))
            .collect();
        assert_eq!(owned.len(), 1);
        assert_eq!(owned[0].id, "resumed");
        assert!(!event_for_turn(
            &aborted("resumed", Some("failed")),
            "resumed"
        ));
        assert!(event_for_turn(&aborted("resumed", None), "resumed"));
    }
}

pub async fn prepare(
    host: &CanonicalHost,
    parent: &crate::session::Session,
    scope: &Scope,
    path: &Path,
) -> Result<Child, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 65536 {
        return Err("delegation specification exceeds 64 KiB".into());
    }
    let spec: Specification =
        serde_json::from_slice(&bytes).map_err(|e| format!("delegation specification: {e}"))?;
    prepare_specification(host, parent, scope, spec).await
}

pub async fn prepare_helper(
    host: &CanonicalHost,
    parent: &crate::session::Session,
    scope: &Scope,
    helper: crate::terminal::Helper,
) -> Result<Child, String> {
    prepare_specification(host, parent, scope, Specification {
        version: 1,
        git: helper.git,
        disposable_parent: helper.disposable_parent,
        objective: helper.objective,
        acceptance: vec!["Return useful evidence and source references within the assigned scope; state limitations.".into()],
        mode: ChildMode::ReadOnly,
        write_paths: BTreeSet::new(),
        untracked_inputs: BTreeSet::new(),
        allocation_usd: helper.allocation_usd,
        seconds: helper.seconds,
        required_checks: vec![],
        role: Some(helper.name.clone()),
        read_paths: Some(BTreeSet::from([if helper.scope == "." { String::new() } else { helper.scope }])),
        helper: Some(vcp_lifecycle::foundation::HelperTemplate { name: helper.name, revision: vcp_lifecycle::foundation::HelperTemplate::REVISION }),
    }).await
}

async fn prepare_specification(
    host: &CanonicalHost,
    parent: &crate::session::Session,
    scope: &Scope,
    spec: Specification,
) -> Result<Child, String> {
    if spec.version != 1
        || !(1..=3600).contains(&spec.seconds)
        || !spec.git.is_absolute()
        || !spec.disposable_parent.is_absolute()
    {
        return Err(
            "delegation requires version 1, absolute native paths and 1–3600 seconds".into(),
        );
    }
    let snapshotter = native_snapshotter(spec.git)?;
    let state = host.snapshot()?;
    let workspace: Workspace = state
        .record(
            Collection::Workspace,
            scope.workspace.as_str(),
            &scope.workspace,
        )
        .and_then(|r| r.decode())
        .map_err(|e| e.to_string())?;
    let disposable = Root::open(
        RootIdentity {
            workspace: workspace.id,
            root: RootId::new(),
            repository: workspace.binding.repository,
            worktree: "registered-child-directory".into(),
            binding: workspace.binding.revision,
        },
        &spec.disposable_parent,
    )
    .map_err(|e| e.to_string())?;
    let request = DelegationRequest {
        role: spec
            .role
            .unwrap_or_else(|| "bounded development child".into()),
        read_paths: spec
            .read_paths
            .unwrap_or_else(|| BTreeSet::from([String::new()])),
        helper: spec.helper,
        objective: spec.objective,
        acceptance: spec.acceptance,
        mode: spec.mode,
        write_paths: spec.write_paths,
        untracked_inputs: spec.untracked_inputs,
        allocation: crate::args::parse_usd(&spec.allocation_usd)?,
        deadline: Timestamp::new(
            crate::settings::now()
                .get()
                .checked_add(u64::from(spec.seconds) * 1000)
                .ok_or("child deadline overflow")?,
        ),
        required_checks: spec.required_checks,
    };
    let task = host
        .delegate_child(parent.id, request, &snapshotter, &disposable)
        .await?;
    let config = parent.thread.config().await.as_ref().clone();
    let session = parent
        .start_child(host, config, task.clone(), &snapshotter)
        .await?;
    if let Err(error) = host.configure_child_from_parent(parent.id, session.id) {
        let pause = (|| -> Result<(), String> {
            let state = host.snapshot()?;
            let child: vcp_domain::task::Task = state
                .record(Collection::Task, task.as_str(), &scope.workspace)
                .and_then(|r| r.decode())
                .map_err(|e| e.to_string())?;
            host.control_envelope(
                CommandId::new(),
                task.clone(),
                child.revision,
                vcp_protocol::command::Command::Transition {
                    next: vcp_domain::task::TaskState::Paused,
                    reason: "child setup failed; explicit recovery required".into(),
                    verification: None,
                },
            )
            .and_then(|command| host.stop(command).map(|_| ()))
        })();
        let cleanup = session.thread.shutdown_and_wait().await;
        return Err(format!(
            "child {task} setup failed: {error}; pause={pause:?}; retained cleanup: {cleanup:?}"
        ));
    }
    Ok(Child {
        task,
        session,
        snapshotter,
    })
}
