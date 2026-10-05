// SPDX-License-Identifier: Apache-2.0
//! Shared retained execution mechanics. Callers own admission, cancellation,
//! deadlines and presentation; this module does not grant resume authority.
use crate::{outcome::Outcome, session::Session};
use codex_protocol::protocol::Event;
use vcp_domain::{
    task::{Task, TaskState},
    workspace::Scope,
};
use vcp_lifecycle::foundation::CanonicalHost;
use vcp_lifecycle::foundation::verification::{CompletionAttempt, CompletionRejection};
use vcp_store::contract::Collection;

#[cfg(all(test, windows, feature = "qualification"))]
mod tests;

/// A suspended elapsed deadline has no timer and no artificial far-future date.
pub(crate) async fn wait_deadline(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending::<()>().await,
    }
}

pub(crate) fn deadline_after(seconds: vcp_domain::Limit<u32>) -> Option<tokio::time::Instant> {
    seconds.finite().map(|seconds| {
        tokio::time::Instant::now() + std::time::Duration::from_secs(u64::from(*seconds))
    })
}

pub fn event_for_turn(event: &Event, turn: &str) -> bool {
    use codex_protocol::protocol::EventMsg;
    event.id == turn
        && match &event.msg {
            EventMsg::TurnComplete(end) => end.turn_id == turn,
            EventMsg::TurnAborted(end) => end.turn_id.as_deref().is_none_or(|id| id == turn),
            _ => true,
        }
}

fn current(host: &CanonicalHost, scope: &Scope) -> Result<Task, String> {
    let task: Task = host
        .current_state()?
        .record(Collection::Task, scope.task.as_str(), &scope.workspace)
        .and_then(|row| row.decode())
        .map_err(|e| e.to_string())?;
    if task.scope != *scope {
        return Err("retained execution scope mismatch".into());
    }
    Ok(task)
}

/// One event consumer for a retained session, including across Session clones.
/// Hold this owner for the entire supervisor loop, not just one next_event call.
/// Dropping it releases event consumption only; it never resumes or stops work.
pub struct RetainedExecution {
    host: CanonicalHost,
    session: Session,
    scope: Scope,
    _events: tokio::sync::OwnedMutexGuard<()>,
}

pub enum Completion {
    Deferred,
    Completed,
    Rejected(String),
    Repair,
    RequiredApproval,
}

/// A bounded lifecycle operation owns no event receiver. The caller retains its
/// single event owner and must fence admission before draining a pending job.
pub enum LifecycleResult {
    Submitted(String),
    Completed(Completion),
}

pub struct LifecycleJob(tokio::task::JoinHandle<Result<LifecycleResult, String>>);

impl LifecycleJob {
    pub async fn poll(pending: &mut Option<Self>) -> Result<LifecycleResult, String> {
        match pending.as_mut() {
            Some(job) => job.result().await,
            None => std::future::pending().await,
        }
    }

    pub async fn result(&mut self) -> Result<LifecycleResult, String> {
        (&mut self.0).await.map_err(|error| error.to_string())?
    }
}

impl RetainedExecution {
    pub fn has_output_continuation(&self) -> Result<bool, String> {
        self.host.has_output_continuation(self.session.id)
    }
    pub fn start_submission(&self, accepted: Option<vcp_domain::TurnId>) -> LifecycleJob {
        let host = self.host.clone();
        let session = self.session.clone();
        let scope = self.scope.clone();
        LifecycleJob(tokio::spawn(async move {
            let turn = match accepted {
                Some(turn) => submit_preaccepted(&host, &session, &scope, turn).await?,
                None => submit_identified(&host, &session, &scope).await?,
            };
            Ok(LifecycleResult::Submitted(turn))
        }))
    }

    pub fn start_completion(&self) -> LifecycleJob {
        let host = self.host.clone();
        let session = self.session.clone();
        let scope = self.scope.clone();
        LifecycleJob(tokio::spawn(async move {
            // Only a settled, typed output truncation may request another turn.
            // The caller reaches this job after the retained terminal event;
            // partial tool calls never acquire execution authority.
            if let Some(continuation) = host.take_output_continuation(session.id)? {
                host.begin_coding_turn(session.id, continuation.feedback.clone())?;
                return submit_text(&session, continuation.feedback)
                    .await
                    .map(LifecycleResult::Submitted);
            }
            match complete(&host, &session, &scope).await? {
                Completion::Repair => {
                    let Some(feedback) = host.completion_repair_feedback(session.id)? else {
                        return Ok(LifecycleResult::Completed(Completion::Deferred));
                    };
                    host.set_next_coding_activity(
                        session.id,
                        vcp_domain::request_allocation::Activity::Repair,
                    )?;
                    host.begin_coding_turn(session.id, feedback.clone())?;
                    submit_text(&session, feedback)
                        .await
                        .map(LifecycleResult::Submitted)
                }
                completion => Ok(LifecycleResult::Completed(completion)),
            }
        }))
    }
    pub fn claim(host: &CanonicalHost, session: &Session, scope: &Scope) -> Result<Self, String> {
        if session.scope() != scope {
            return Err("retained execution scope mismatch".into());
        }
        current(host, scope)?;
        let events = session.claim_events()?;
        Ok(Self {
            host: host.clone(),
            session: session.clone(),
            scope: scope.clone(),
            _events: events,
        })
    }

    pub async fn next_event(&mut self) -> Result<Event, String> {
        self.session
            .thread
            .next_event()
            .await
            .map_err(|e| e.to_string())
    }
}

async fn submit_preaccepted(
    host: &CanonicalHost,
    session: &Session,
    scope: &Scope,
    turn: vcp_domain::TurnId,
) -> Result<String, String> {
    host.start_lifecycle_hooks(session.id).await?;
    let task = current(host, scope)?;
    let text = task
        .objectives
        .last()
        .ok_or("objective unavailable")?
        .text
        .clone();
    host.bind_preaccepted_coding_turn(session.id, turn, text.clone())?;
    submit_text(session, text).await
}

/// Attempt completion only when canonical state permits it. Deferred
/// preserves waiting/paused/budget-limited state. Callers retain their policy
/// for rejected completion evidence (interactive pause versus batch failure).
async fn complete(
    host: &CanonicalHost,
    session: &Session,
    scope: &Scope,
) -> Result<Completion, String> {
    let outcome = Outcome::read(host, scope)?;
    if !outcome.approvals.is_empty() {
        return Ok(Completion::RequiredApproval);
    }
    if outcome.task.state != TaskState::Running
        || outcome.conditions.required_input
        || outcome.conditions.budget_exhausted
    {
        return Ok(Completion::Deferred);
    }
    let hooks = match host.complete_lifecycle_hooks(session.id).await {
        Ok(hooks) => hooks,
        Err(error) => return Ok(Completion::Rejected(error)),
    };
    if !hooks.is_empty() {
        eprintln!(
            "{}",
            crate::terminal::sanitize(
                &vcp_lifecycle::foundation::hooks::adapters::presentation(&hooks).to_string(),
                8192
            )
        );
    }
    match host.try_complete_coding_turn(session.id)? {
        CompletionAttempt::Completed(_) => Ok(Completion::Completed),
        CompletionAttempt::Rejected(failure)
            if matches!(
                failure.kind,
                CompletionRejection::MissingVerification
                    | CompletionRejection::StaleVerification
                    | CompletionRejection::FailedChecks
                    | CompletionRejection::InstructionScopeRefresh
            ) =>
        {
            // Exactly one fresh verification attempt per completion request.
            // The same owner still consumes events; no model request or tool
            // reissue is needed merely to produce current acceptance evidence.
            let verification = match host.verify_for_completion(session.id).await {
                Ok(verification) => verification,
                Err(error) => {
                    return Ok(if Outcome::read(host, scope)?.approvals.is_empty() {
                        Completion::Rejected(error)
                    } else {
                        Completion::RequiredApproval
                    });
                }
            };
            Ok(
                match host.try_complete_verified(session.id, verification.id)? {
                    CompletionAttempt::Completed(_) => Completion::Completed,
                    CompletionAttempt::Rejected(failure)
                        if failure.kind == CompletionRejection::RequiredApproval =>
                    {
                        Completion::RequiredApproval
                    }
                    CompletionAttempt::Rejected(failure)
                        if failure.kind == CompletionRejection::FailedChecks =>
                    {
                        Completion::Repair
                    }
                    CompletionAttempt::Rejected(failure) => {
                        Completion::Rejected(failure.to_string())
                    }
                },
            )
        }
        CompletionAttempt::Rejected(failure)
            if failure.kind == CompletionRejection::RequiredApproval =>
        {
            Ok(Completion::RequiredApproval)
        }
        CompletionAttempt::Rejected(failure) => Ok(Completion::Rejected(failure.to_string())),
    }
}

/// Submit an already admitted task. The canonical host checks turn admission;
/// this does not resume a task, acquire a controller or retry a public command.
pub async fn submit(host: &CanonicalHost, session: &Session, scope: &Scope) -> Result<(), String> {
    submit_identified(host, session, scope).await.map(|_| ())
}

async fn submit_identified(
    host: &CanonicalHost,
    session: &Session,
    scope: &Scope,
) -> Result<String, String> {
    if session.scope() != scope {
        return Err("retained execution scope mismatch".into());
    }
    host.start_lifecycle_hooks(session.id).await?;
    let task = current(host, scope)?;
    let text = task
        .objectives
        .last()
        .ok_or("objective unavailable")?
        .text
        .clone();
    host.begin_coding_turn(session.id, text.clone())?;
    submit_text(session, text).await
}

async fn submit_text(session: &Session, text: String) -> Result<String, String> {
    let submitted = session
        .thread
        .start_or_steer_turn(codex_core::TurnInputRequest::user_input(vec![
            codex_protocol::user_input::UserInput::Text {
                text,
                text_elements: vec![],
            },
        ]))
        .await
        .map_err(|e| e.to_string())?;
    match submitted {
        codex_core::TurnInputSubmission::Started { turn_id }
        | codex_core::TurnInputSubmission::Steered { turn_id } => Ok(turn_id),
        codex_core::TurnInputSubmission::NotSubmitted { .. } => {
            Err("retained admission rejected the turn".into())
        }
    }
}
