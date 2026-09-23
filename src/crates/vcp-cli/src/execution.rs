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
use vcp_store::contract::Collection;

fn current(host: &CanonicalHost, scope: &Scope) -> Result<Task, String> {
    let task: Task = host
        .snapshot()?
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
}

impl RetainedExecution {
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

    pub async fn submit(&self) -> Result<(), String> {
        self.submit_identified().await.map(|_| ())
    }

    /// Retained submission identity binds an event owner across explicit resumes;
    /// queued events from an older interrupted turn cannot complete new work.
    pub async fn submit_identified(&self) -> Result<String, String> {
        submit_identified(&self.host, &self.session, &self.scope).await
    }

    /// Attempt completion only when canonical state permits it. Deferred
    /// preserves waiting/paused/budget-limited state. Callers retain their policy
    /// for rejected completion evidence (interactive pause versus batch failure).
    pub fn complete(&self) -> Result<Completion, String> {
        let outcome = Outcome::read(&self.host, &self.scope)?;
        if outcome.task.state != TaskState::Running
            || outcome.conditions.required_input
            || outcome.conditions.budget_exhausted
        {
            return Ok(Completion::Deferred);
        }
        Ok(match self.host.complete_coding_turn(self.session.id) {
            Ok(_) => Completion::Completed,
            Err(error) => Completion::Rejected(error),
        })
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
    let task = current(host, scope)?;
    let text = task
        .objectives
        .last()
        .ok_or("objective unavailable")?
        .text
        .clone();
    host.begin_coding_turn(session.id, text.clone())?;
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
