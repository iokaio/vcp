// SPDX-License-Identifier: Apache-2.0
//! Terminal commands are host commands, never worker or provider calls.
use super::*;
use crate::{outcome::Outcome, session::Session};
use codex_protocol::protocol::EventMsg;
use std::time::Duration;
use vcp_audit::inspection::{InspectionQuery, RangeRequest, View};
use vcp_domain::{ids::*, revision::Revision, task::TaskState};
use vcp_lifecycle::foundation::CanonicalHost;
use vcp_protocol::command::{Approval, ApprovalState, Command};

fn current(host: &CanonicalHost, scope: &Scope) -> Result<Task, String> {
    host.snapshot()?
        .record(Collection::Task, scope.task.as_str(), &scope.workspace)
        .and_then(|r| r.decode())
        .map_err(|e| e.to_string())
}

/// Page the bounded query response as well as its canonical cursor so a large
/// record never skips hidden text when the user advances through evidence.
fn display_page(remaining: &mut String) -> String {
    let mut end = remaining.len().min(1200);
    while !remaining.is_char_boundary(end) {
        end -= 1;
    }
    let tail = remaining.split_off(end);
    let mut shown = std::mem::replace(remaining, tail);
    if !remaining.is_empty() {
        shown.push_str(" [continued: /next]");
    }
    shown
}

fn stop(host: &CanonicalHost, scope: &Scope, state: TaskState) -> Result<(), String> {
    let task = current(host, scope)?;
    if task.state.terminal() {
        return Ok(());
    }
    host.stop(host.control_envelope(
        CommandId::new(),
        scope.task.clone(),
        task.revision,
        Command::Transition {
            next: state,
            reason: "explicit terminal control".into(),
            verification: None,
        },
    )?)?;
    Ok(())
}

async fn submit(host: &CanonicalHost, session: &Session, scope: &Scope) -> Result<(), String> {
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
    if matches!(
        submitted,
        codex_core::TurnInputSubmission::NotSubmitted { .. }
    ) {
        return Err("retained admission rejected the turn".into());
    }
    Ok(())
}

/// Shared admission for an open terminal and an explicitly selected reopened
/// task. The caller must configure current coding/provider bindings first.
/// This never submits a turn; canonical pause fences dispatch while the retained
/// hold is released, and host resume checks current files, policy and budget.
pub fn prepare_resume(
    host: &CanonicalHost,
    session: &Session,
    scope: &Scope,
    expected: Revision,
) -> Result<(), String> {
    let task = current(host, scope)?;
    if task.scope != *scope || task.revision != expected {
        return Err("resume selection changed; inspect and select the task again".into());
    }
    if task.state.terminal() || task.state == TaskState::Running {
        return Err("resume requires a suspended task".into());
    }
    let outcome = Outcome::read(host, scope)?;
    if !outcome.approvals.is_empty() {
        return Err("answer pending questions before resume".into());
    }
    host.reconcile_effects()?;
    let retained = host
        .lifecycle()
        .inspect(session.id)
        .map_err(|e| format!("{e:?}"))?;
    if retained.local_hold {
        host.lifecycle()
            .resume(session.id, &retained.revision)
            .map_err(|e| format!("resume waits for interruption: {e:?}"))?;
    }
    host.resume(session.id, expected, task.fingerprint)?;
    Ok(())
}

pub async fn resume(host: &CanonicalHost, session: &Session, scope: &Scope) -> Result<(), String> {
    let expected = current(host, scope)?.revision;
    prepare_resume(host, session, scope, expected)?;
    if let Err(error) = submit(host, session, scope).await {
        stop(host, scope, TaskState::Paused)?;
        return Err(error);
    }
    Ok(())
}

fn answer(host: &CanonicalHost, scope: &Scope, id: String, allow: bool) -> Result<(), String> {
    let state = host.snapshot()?;
    let approval: Approval = state
        .record(Collection::Approval, &id, &scope.workspace)
        .and_then(|r| r.decode())
        .map_err(|e| e.to_string())?;
    let task: Task = state
        .record(
            Collection::Task,
            approval.scope.task.as_str(),
            &scope.workspace,
        )
        .and_then(|r| r.decode())
        .map_err(|e| e.to_string())?;
    if approval.scope.session != scope.session
        || task.root != scope.task
        || approval.state != ApprovalState::Pending
    {
        return Err("question is not pending in this task tree".into());
    }
    host.command(
        Command::Decide {
            id: approval.id,
            operation_digest: approval.operation_digest,
            effect_revision: approval.effect_revision,
            allow,
        },
        Some(approval.scope.task),
        approval.revision,
    )?;
    Ok(())
}

pub async fn run(
    host: &CanonicalHost,
    session: &Session,
    scope: &Scope,
    model: &str,
    seconds: u32,
) -> Result<(), String> {
    let mut input = input(std::io::BufReader::new(std::io::stdin())).map_err(|e| e.to_string())?;
    let renderer = Renderer::new(std::io::stderr()).map_err(|e| e.to_string())?;
    let mut notice = String::from("/pause /resume /status /cost /history /agents /inspect <id> /next /answer <id> allow|deny /cancel /exit; plain text steers the task");
    let mut page: Option<InspectionQuery> = None;
    let mut page_text = String::new();
    let mut pending: Option<tokio::task::JoinHandle<Result<(), String>>> = None;
    let mut active = true;
    submit(host, session, scope).await?;
    let mut tick = tokio::time::interval(Duration::from_millis(200));
    let deadline = tokio::time::sleep(Duration::from_secs(u64::from(seconds)));
    tokio::pin!(deadline);
    let mut expired = false;
    let mut last = String::new();
    let mut commentary = String::new();
    let mut commentary_item = String::new();
    loop {
        tokio::select! {
            line = input.recv() => {
                let line = match line {
                    None => { stop(host,scope,TaskState::Paused)?; return Ok(()); }
                    Some(Err(e)) => { stop(host,scope,TaskState::Paused)?; return Err(e.to_string()); }
                    Some(Ok(line)) => line,
                };
                let command = match parse(&line) { Ok(Some(c)) => c, Ok(None) => continue, Err(e) => {notice=e; continue;} };
                let result: Result<String,String> = async { Ok(match command {
                    Input::Pause => {stop(host,scope,TaskState::Paused)?; "Pause requested; inspect retained effects before resuming.".into()}
                    Input::Cancel => {stop(host,scope,TaskState::Cancelled)?; return Ok("Task cancelled.".into());}
                    Input::Exit => {stop(host,scope,TaskState::Paused)?; return Ok("exit".into());}
                    Input::Resume => {
                        if pending.is_some() || active {return Err("wait for the current turn/steering to drain before /resume".into());}
                        if expired {return Err("execution deadline reached; reopen explicitly to renew the execution window".into());}
                        resume(host,session,scope).await?; active=true; "Resumed after revalidation.".into()
                    }
                    Input::Answer{id,allow} => {answer(host,scope,id,allow)?; "Answer recorded. Use /resume deliberately; no work was dispatched by the answer.".into()}
                    Input::Steer(text) => {
                        if pending.is_some() {return Err("guidance already queued; wait for its durable result".into());}
                        let task=current(host,scope)?;
                        let mut objective=task.objectives.last().ok_or("objective unavailable")?.clone();
                        objective.text=format!("{}\n\nUser guidance:\n{text}",objective.text);
                        if objective.text.len()>INPUT_LIMIT {return Err("combined objective exceeds 64 KiB".into());}
                        objective.source=EventId::new();
                        let change=host.change_authority(Command::Steer{objective},Some(scope.task.clone()),task.revision)?;
                        pending=Some(tokio::spawn(async move{change.wait().await.map(|_|())}));
                        "Guidance queued; admission is fenced while existing effects stop. Task stays paused for /resume.".into()
                    }
                    Input::Next if !page_text.is_empty() => display_page(&mut page_text),
                    Input::History | Input::Cost | Input::Inspect(_) | Input::Read {..} | Input::Next => {
                        let query=match command {
                            Input::History=>InspectionQuery{id:scope.task.to_string(),view:View::Chain,limit:8,cursor:None,range:None},
                            Input::Cost=>InspectionQuery{id:scope.task.to_string(),view:View::Costs,limit:8,cursor:None,range:None},
                            Input::Inspect(id)=>InspectionQuery{id,view:View::Chain,limit:8,cursor:None,range:None},
                            Input::Read{id,offset}=>InspectionQuery{id,view:View::Outputs,limit:1,cursor:None,range:Some(RangeRequest{offset,length:256})},
                            _=>page.clone().ok_or("no next inspection page")?,
                        };
                        let result=host.inspect(query.clone())?;
                        page=if query.range.is_some() {
                            result.items.first().and_then(|item|item["next_offset"].as_u64()).map(|offset|InspectionQuery{range:Some(RangeRequest{offset,length:256}),..query})
                        } else {result.next_cursor.clone().map(|cursor|InspectionQuery{cursor:Some(cursor),..query})};
                        page_text=super::sanitize(&serde_json::to_string(&result).map_err(|e|e.to_string())?,1024*1024);
                        display_page(&mut page_text)
                    }
                    Input::Unavailable(service)=>format!("{service}: service not ready in this stage; no work scheduled"),
                    Input::Status | Input::Agents => serde_json::to_string(&view(&host.snapshot()?,scope,model)?).map_err(|e|e.to_string())?,
                    Input::Help => "/pause /resume /status /cost /history /agents /inspect <id> /read <artifact-id> <byte-offset> /next /answer <id> allow|deny /memory /optimize /cancel /exit; plain text queues durable guidance".into(),
                }) }.await;
                match result { Ok(message) if message=="exit"=>return Ok(()), Ok(message)=>notice=message, Err(error)=>notice=format!("Command rejected: {error}") }
            }
            event = session.thread.next_event() => {
                let event=event.map_err(|e|e.to_string())?;
                if let EventMsg::AgentMessageContentDelta(message)=&event.msg {
                    if commentary_item!=message.item_id {
                        commentary_item=message.item_id.clone();
                        commentary=format!("Agent task={} turn={}: ",scope.task,message.turn_id);
                    }
                    if commentary.len()<2048 {
                        commentary.push_str(&super::sanitize(&message.delta,2048-commentary.len()));
                    }
                }
                if let EventMsg::AgentMessage(message)=&event.msg {
                    commentary=format!("Agent task={} turn={}: {}",scope.task,event.id,super::sanitize(&message.message,2048));
                }
                if matches!(event.msg,EventMsg::TurnComplete(_) | EventMsg::TurnAborted(_)) {
                    active=false;
                    let outcome=Outcome::read(host,scope)?;
                    if outcome.task.state==TaskState::Running && !outcome.conditions.required_input && !outcome.conditions.budget_exhausted {
                        if let Err(error)=host.complete_coding_turn(session.id) {
                            stop(host,scope,TaskState::Paused)?;
                            notice=format!("Completion evidence unavailable; task paused: {error}");
                        }
                    }
                }
            }
            _ = tick.tick() => {
                if pending.as_ref().is_some_and(|p|p.is_finished()) {
                    let result=pending.take().ok_or("steering waiter missing")?.await.map_err(|e|e.to_string())?;
                    notice=match result {Ok(())=>"Guidance applied to a new durable revision; /resume when ready.".into(),Err(e)=>format!("Guidance not applied: {e}")};
                }
                if renderer.failed() {stop(host,scope,TaskState::Paused)?;return Err("terminal output unavailable".into());}
            }
            _ = &mut deadline, if !expired => {
                expired=true; stop(host,scope,TaskState::Paused)?;
                notice="Execution deadline reached; paused for inspection. /exit preserves the task.".into();
            }
        }
        let snapshot = view(&host.snapshot()?, scope, model)?;
        // Keep control notices, questions and money separate from potentially
        // large inspection pages and untrusted commentary.
        let lines=vec![
            format!("Task {} | state={} | revision={} | model={}",scope.task,snapshot["state"],snapshot["revision"],super::sanitize(model,256)),
            format!("Objective: {}",snapshot["objective"]),
            format!("Step: {} | {}",snapshot["current_step"],snapshot["group"]),
            format!("Cost: {}",snapshot["cost"]),
            format!("Questions ({}): {} | /answer <id> allow|deny; no default",snapshot["question_count"],snapshot["pending_questions"]),
            format!("Changes ({}): {}",snapshot["change_count"],snapshot["changes"]),
            format!("Current checks: {}",snapshot["current_checks"]),
            format!("Children: {}",snapshot["children"]),
            notice.clone(),commentary.clone(),
            "Pause fences admission; effects can still be stopping or unknown. /history /next /inspect for full evidence.".into(),
        ];
        let display = lines.join("\n");
        if display != last {
            renderer.show_lines(&lines);
            last = display;
        }
        if current(host, scope)?.state.terminal() {
            return Ok(());
        }
    }
}
