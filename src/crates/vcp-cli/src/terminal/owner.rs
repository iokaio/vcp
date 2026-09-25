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
    crate::execution::submit(host, session, scope).await
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
    if host.mcp_connections_present() {
        // The host must prove an idle owned daemon and its exact granted call.
        // Do not release a retained pause or reconcile away a live process here.
        return host
            .resume(session.id, expected, task.fingerprint)
            .map(|_| ());
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
    backup_triggers: &mut crate::backup_triggers::Triggers,
) -> Result<(), String> {
    let mut execution = crate::execution::RetainedExecution::claim(host, session, scope)?;
    let mut input = input(std::io::BufReader::new(std::io::stdin())).map_err(|e| e.to_string())?;
    let renderer = Renderer::new(std::io::stderr()).map_err(|e| e.to_string())?;
    let mut notice = String::from(
        "/pause /resume /status /observers /cost /history /groups /optimize /escalate /skills /mcp /agents [offset] /agents focus|follow|pause|cancel|resume|integrate|apply <task> /agents explore|review <scope> <USD> <seconds> <git.exe> <disposable-parent> <objective> /agents delegate <spec.json> /agents cleanup preview <task> <git.exe> [--reject-edits] | cleanup apply|reconcile <task> /agents recover <task> <git.exe> /inspect <id> /next /answer <id> allow|deny /cancel /exit; plain text steers the task",
    );
    let mut page: Option<InspectionQuery> = None;
    let mut maintenance_page: Option<vcp_lifecycle::foundation::history_retention::Request> = None;
    let mut optimization = crate::optimize::Session::default();
    let mut page_text = String::new();
    let mut pending: Option<tokio::task::JoinHandle<Result<(), String>>> = None;
    let mut mcp_pending: Option<crate::mcp::Running> = None;
    let mut shadow = crate::decision::Driver::default();
    let mut active = true;
    let mut lifecycle_pending = Some(execution.start_submission(None));
    let mut active_turn = None;
    let mut child_review_pending = false;
    let mut followed: Option<TaskId> = None;
    let mut delegation_pending: Option<
        tokio::task::JoinHandle<Result<(crate::delegation::Child, bool), String>>,
    > = None;
    let mut children: std::collections::BTreeMap<
        TaskId,
        (
            crate::delegation::Child,
            Option<tokio::task::JoinHandle<Result<(), String>>>,
        ),
    > = std::collections::BTreeMap::new();
    let (child_notices, mut child_updates) = tokio::sync::mpsc::channel::<String>(8);
    let mut integration_pending: Option<
        tokio::task::JoinHandle<
            Result<(TaskId, vcp_lifecycle::foundation::ChildIntegration), String>,
        >,
    > = None;
    let mut integration_apply: Option<
        tokio::task::JoinHandle<Result<vcp_lifecycle::foundation::ToolOutcome, String>>,
    > = None;
    let mut integration_tickets = std::collections::BTreeMap::new();
    enum CleanupOutcome {
        Preview(TaskId, vcp_lifecycle::foundation::ChildCleanupPreview),
        Receipt(serde_json::Value),
    }
    let mut cleanup_pending: Option<tokio::task::JoinHandle<Result<CleanupOutcome, String>>> = None;
    let mut cleanup_previews = std::collections::BTreeMap::new();
    let result = async {
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
                optimization.prepare_input(&line);
                let command = match parse(&line) { Ok(Some(c)) => c, Ok(None) => continue, Err(e) => {notice=e; continue;} };
                let result: Result<String,String> = async { Ok(match command {
                    Input::Pause => {stop(host,scope,TaskState::Paused)?; "Pause requested; inspect retained effects before resuming.".into()}
                    Input::Cancel => {stop(host,scope,TaskState::Cancelled)?; return Ok("Task cancelled.".into());}
                    Input::Exit => {stop(host,scope,TaskState::Paused)?; return Ok("exit".into());}
                    Input::Resume => {
                        if lifecycle_pending.is_some() || pending.is_some() || mcp_pending.is_some() || shadow.active() || active {return Err("wait for the current turn, lifecycle hooks, steering, shadow evaluation and MCP control to drain before /resume".into());}
                        if expired {return Err("execution deadline reached; reopen explicitly to renew the execution window".into());}
                        let expected=current(host,scope)?.revision;
                        prepare_resume(host,session,scope,expected)?;
                        active_turn=None; lifecycle_pending=Some(execution.start_submission(None)); active=true; child_review_pending=false; "Resume requested after revalidation; lifecycle hooks are pending.".into()
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
                    Input::Next if maintenance_page.is_some()=>{
                        let request=maintenance_page.take().ok_or("no next history page")?;
                        let result=host.history_retention(request.clone())?;
                        maintenance_page=crate::history::next_request(request,&result)?;
                        page_text=super::sanitize(&serde_json::to_string(&result).map_err(|e|e.to_string())?,1024*1024);
                        display_page(&mut page_text)
                    }
                    Input::Maintenance(words)=>{
                        let request=crate::history::terminal_request(words,&scope.workspace)?;
                        let result=host.history_retention(request.clone())?;
                        maintenance_page=crate::history::next_request(request,&result)?;page=None;
                        page_text=super::sanitize(&serde_json::to_string(&result).map_err(|e|e.to_string())?,1024*1024);
                        display_page(&mut page_text)
                    }
                    Input::Optimize(command)=>{
                        let result=optimization.execute(command,crate::settings::now(),|request|host.routing_control(request))?;
                        maintenance_page=None;page=None;
                        page_text=super::sanitize(&result,1024*1024);
                        display_page(&mut page_text)
                    }
                    Input::Escalate(command)=>{
                        let task=current(host,scope)?;
                        let result=super::escalation::execute(command,&task,|request|host.routing_control(request))?;
                        maintenance_page=None;page=None;
                        page_text=super::sanitize(&result,1024*1024);
                        display_page(&mut page_text)
                    }
                    Input::Mcp(command)=>{
                        if mcp_pending.is_some() {return Err("MCP control is pending; /pause and /cancel remain available".into());}
                        mcp_pending=Some(crate::mcp::Running::start(host.clone(),session.id,command));
                        "MCP control queued; /pause and /cancel remain available.".into()
                    }
                    Input::Skills(command)=>{
                        let result=crate::skills::execute(command,|request|host.skill_control(session.id,request))?;
                        maintenance_page=None;page=None;
                        page_text=super::sanitize(&result,1024*1024);
                        display_page(&mut page_text)
                    }
                    Input::History | Input::Cost | Input::Inspect(_) | Input::Read {..} | Input::Next => {
                        maintenance_page=None;
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
                    Input::Agent {task,action} => {
                        let state=host.snapshot()?;
                        let child=crate::agents_view::child(&state,scope,&task)?;
                        match action {
                            AgentAction::Integrate => {
                                if integration_pending.is_some() || integration_apply.is_some() {return Err("integration is already pending; pause remains available".into());}
                                let snapshotter=children.get(&task).ok_or("attach the registered child before preparing integration")?.0.snapshotter.clone();
                                let host=host.clone();let parent=session.id;
                                integration_pending=Some(tokio::spawn(async move {host.prepare_observed_child_integration(parent,task.clone(),&snapshotter).await.map(|result|(task,result))}));
                                "Child result inspection and conflict-aware integration preview requested.".into()
                            },
                            AgentAction::Apply => {
                                if integration_apply.is_some(){return Err("integration application is already pending".into());}
                                if current(host,scope)?.state!=TaskState::Running {return Err("explicitly resume the parent before applying an integration".into());}
                                let ticket=integration_tickets.remove(&task).ok_or("prepare an integration preview for this child first")?;
                                let host=host.clone();integration_apply=Some(tokio::spawn(async move{host.schedule_tool(ticket).await}));
                                format!("Agent {task}: prepared integration queued through current parent policy.")
                            },
                            AgentAction::Resume => {
                                let (child,pump)=children.get_mut(&task).ok_or("child owner is not attached; pause the root and use /agents recover first")?;
                                if pump.as_ref().is_some_and(|job|!job.is_finished()) {return Err("child observer is still draining".into());}
                                if let Some(previous)=pump.take(){let _=previous.await;}
                                crate::delegation::resume(host,child,scope)?;
                                match crate::delegation::run(host.clone(),child,scope,child_notices.clone()).await {
                                    Ok(started)=>*pump=Some(started),
                                    Err(error)=>{let mut child_scope=scope.clone();child_scope.task=task.clone();stop(host,&child_scope,TaskState::Paused)?;return Err(error);}
                                }
                                child_review_pending=true;
                                format!("Agent {task}: explicitly resumed after revalidation.")
                            },
                            AgentAction::Pause | AgentAction::Cancel => {
                                stop(host,&child.scope,if action==AgentAction::Pause {TaskState::Paused} else {TaskState::Cancelled})?;
                                format!("Agent {task}: control recorded; active effects may still be stopping or unknown.")
                            },
                            AgentAction::Focus | AgentAction::Follow => {
                                if action==AgentAction::Follow {followed=Some(task.clone());}
                                page_text=super::sanitize(&serde_json::to_string(&crate::agents_view::live_detail(host,session.id,scope,&task,crate::settings::now())?).map_err(|e|e.to_string())?,1024*1024);
                                display_page(&mut page_text)
                            }
                        }
                    },
                    Input::Cleanup {task,action} => {
                        crate::agents_view::child(&host.snapshot()?,scope,&task)?;
                        if cleanup_pending.is_some() {return Err("cleanup operation is pending; pause remains available".into());}
                        let host=host.clone();let parent=session.id;
                        cleanup_pending=Some(match action {
                            super::CleanupAction::Preview {git,reject_edits}=>tokio::spawn(async move {
                                let snapshotter=crate::delegation::native_snapshotter(git)?;
                                host.prepare_child_cleanup(parent,task.clone(),&snapshotter,reject_edits).await.map(|preview|CleanupOutcome::Preview(task,preview))
                            }),
                            super::CleanupAction::Apply=>{
                                let preview=cleanup_previews.remove(&task).ok_or("preview this child cleanup first")?;
                                tokio::task::spawn_blocking(move||host.apply_child_cleanup(preview).and_then(|r|serde_json::to_value(r).map(CleanupOutcome::Receipt).map_err(|e|e.to_string())))
                            },
                            super::CleanupAction::Reconcile=>tokio::task::spawn_blocking(move||host.reconcile_child_cleanup(parent,task).and_then(|r|serde_json::to_value(r).map(CleanupOutcome::Receipt).map_err(|e|e.to_string()))),
                        });
                        "Deliberate cleanup requested; registered identity, retained evidence and current references are checked before removal.".into()
                    },
                    Input::Helper(helper) => {
                        if delegation_pending.is_some() {return Err("child preparation is already pending; pause remains available".into());}
                        let host=host.clone();let parent=session.clone();let scope=scope.clone();
                        delegation_pending=Some(tokio::spawn(async move {crate::delegation::prepare_helper(&host,&parent,&scope,helper).await.map(|child|(child,true))}));
                        child_review_pending=true;
                        "Read-only helper requested; current model, source scope and shared budget admission are pending.".into()
                    },
                    Input::Delegate(path) => {
                        if delegation_pending.is_some() {return Err("child preparation is already pending; pause remains available".into());}
                        let host=host.clone();let parent=session.clone();let scope=scope.clone();
                        delegation_pending=Some(tokio::spawn(async move {crate::delegation::prepare(&host,&parent,&scope,&path).await.map(|child|(child,true))}));
                        child_review_pending=true;
                        "Delegation requested; snapshot, scope and shared budget admission are pending.".into()
                    },
                    Input::RecoverChild {task,git} => {
                        crate::agents_view::child(&host.snapshot()?,scope,&task)?;
                        if delegation_pending.is_some() || children.contains_key(&task) {return Err("child preparation is pending or selected owner is already attached".into());}
                        let host=host.clone();let parent=session.clone();
                        delegation_pending=Some(tokio::spawn(async move {crate::delegation::recover(&host,&parent,task,git).await.map(|child|(child,false))}));
                        child_review_pending=true;
                        "Child recovery requested; attachment remains held until explicit parent and child resume.".into()
                    },
                    Input::Agents | Input::AgentsPage(_) => {
                        followed=None;
                        let offset=match command {Input::AgentsPage(offset)=>offset,_=>0};
                        page_text=super::sanitize(&serde_json::to_string(&crate::agents_view::page(&host.snapshot()?,scope,crate::settings::now(),offset)?).map_err(|e|e.to_string())?,1024*1024);
                        display_page(&mut page_text)
                    },
                    Input::Status => serde_json::to_string(&view(&host.snapshot()?,scope,model)?).map_err(|e|e.to_string())?,
                    Input::Observers => {
                        page=None; maintenance_page=None;
                        page_text=super::observer_status_text(&host.observer_status(session.id)?)?;
                        display_page(&mut page_text)
                    },
                    Input::Help => format!("/pause /resume /status /observers /cost /history [list|search|prune --preview] /prune show|apply <preview-id> /retention show|set /groups [exact-model] [--offset <candidate-number>] /agents [offset] /agents focus|follow|pause|cancel|resume|integrate|apply <task> /agents explore|review <scope> <USD> <seconds> <git.exe> <disposable-parent> <objective> /agents delegate <spec.json> /agents cleanup preview <task> <git.exe> [--reject-edits] | cleanup apply|reconcile <task> /agents recover <task> <git.exe> /inspect <id> /read <artifact-id> <byte-offset> /next /answer <id> allow|deny /memory inspect <claim-id>|prune --preview /cancel /exit; {} ; {} ; {} ; {} ; plain text queues durable guidance",crate::optimize::HELP,super::escalation::HELP,crate::skills::HELP,crate::mcp::HELP),
                }) }.await;
                match result { Ok(message) if message=="exit"=>return Ok(()), Ok(message)=>notice=message, Err(error)=>notice=format!("Command rejected: {error}") }
            }
            event = execution.next_event(), if lifecycle_pending.is_none() && active_turn.is_some() => {
                let event=event.map_err(|e|e.to_string())?;
                if !active_turn.as_ref().is_some_and(|turn: &String|crate::execution::event_for_turn(&event,turn)) {continue;}
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
                    shadow.cancel().await;
                    active=false;
                    active_turn=None;
                    let child_work=host.snapshot()?.records.values().filter(|r|r.collection==Collection::Task && r.workspace==scope.workspace)
                        .filter_map(|r|r.decode::<Task>().ok()).any(|t|t.scope.session==scope.session && t.root==scope.task && t.scope.task!=scope.task && !t.state.terminal());
                    if matches!(event.msg,EventMsg::TurnAborted(_)) {
                        stop(host,scope,TaskState::Paused)?;
                        notice="Parent turn interrupted; inspect current state before explicit /resume.".into();
                    } else if child_work || child_review_pending || delegation_pending.is_some() {
                        child_review_pending=true;
                        notice="Parent turn ended while children remain active or paused. Inspect /agents; parent completion still requires integrated verification.".into();
                    } else {
                        lifecycle_pending=Some(execution.start_completion());
                    }
                }
            }
            update=child_updates.recv()=>{if let Some(update)=update {notice=update;}}
            outcome = crate::execution::LifecycleJob::poll(&mut lifecycle_pending) => {
                    lifecycle_pending=None;
                    match outcome {
                        Ok(crate::execution::LifecycleResult::Submitted(turn))=>{active_turn=Some(turn);},
                        Ok(crate::execution::LifecycleResult::Completed(crate::execution::Completion::Rejected(error))) | Err(error)=>{
                            active=false;
                            active_turn=None;
                            if current(host,scope)?.state==TaskState::Running {stop(host,scope,TaskState::Paused)?;}
                            notice=format!("Lifecycle operation stopped; inspect current state before explicit /resume: {error}");
                        },
                        Ok(crate::execution::LifecycleResult::Completed(_))=>{active=false;},
                    }
                }
            _ = tick.tick() => {
                if cleanup_pending.as_ref().is_some_and(|job|job.is_finished()) {
                    notice=match cleanup_pending.take().ok_or("cleanup operation missing")?.await.map_err(|e|e.to_string())? {
                        Ok(CleanupOutcome::Preview(task,preview))=>{
                            let summary=preview.summary();cleanup_previews.insert(task.clone(),preview);
                            format!("Cleanup preview: {}; use /agents cleanup apply {task} after inspecting retained-result scope.",super::sanitize(&summary.to_string(),8192))
                        },
                        Ok(CleanupOutcome::Receipt(receipt))=>format!("Cleanup receipt: {}; history and cost remain retained.",super::sanitize(&receipt.to_string(),8192)),
                        Err(error)=>format!("Cleanup blocked: {}; an existing intent must be reconciled, never replaced.",super::sanitize(&error,4096)),
                    };
                }
                if integration_pending.as_ref().is_some_and(|job|job.is_finished()) {
                    match integration_pending.take().ok_or("integration preparation missing")?.await.map_err(|e|e.to_string())? {
                        Ok((task,result))=>{
                            notice=format!("Agent {task}: packet={} plan={} rejection={:?}",result.packet,result.plan,result.rejection);
                            if let Some(ticket)=result.proposal {
                                notice.push_str(&format!("; decision={:?} question={:?}; inspect plan then /agents apply {task}",ticket.decision,ticket.question));
                                integration_tickets.insert(task,ticket);
                            }
                        },
                        Err(error)=>notice=format!("Integration preparation stopped: {error}"),
                    }
                }
                if integration_apply.as_ref().is_some_and(|job|job.is_finished()) {
                    notice=match integration_apply.take().ok_or("integration application missing")?.await.map_err(|e|e.to_string())? {
                        Ok(result)=>format!("Integration effect={} evidence={} result={}; current parent verification is still required.",result.effect,result.evidence.spec.id,super::sanitize(&result.result.to_string(),2048)),
                        Err(error)=>format!("Integration stopped: {error}; inspect receipts before preparing another attempt"),
                    };
                }
                if delegation_pending.as_ref().is_some_and(|job|job.is_finished()) {
                    let child=delegation_pending.take().ok_or("delegation preparation missing")?.await.map_err(|e|e.to_string())?;
                    match child {
                        Ok((child,start))=>{
                            let task=child.task.clone();
                            let pump=if !start {notice=format!("Agent {task}: recovered and held. Resume the parent, then /agents resume {task}.");None} else {match crate::delegation::run(host.clone(),&child,scope,child_notices.clone()).await {
                                Ok(pump)=>{notice=format!("Agent {task} started in its registered isolated workspace with a shared root allocation.");Some(pump)},
                                Err(error)=>{
                                    let mut child_scope=scope.clone();child_scope.task=task.clone();stop(host,&child_scope,TaskState::Paused)?;
                                    notice=format!("Agent {task} did not start: {error}");None
                                }
                            }};
                            children.insert(task,(child,pump));
                        },
                        Err(error)=>notice=format!("Delegation stopped: {error}"),
                    }
                }
                for (task,(_,pump)) in &mut children {
                    if pump.as_ref().is_some_and(|job|job.is_finished()) {
                        if let Err(error)=pump.take().ok_or("child observer missing")?.await.map_err(|e|e.to_string())? {notice=format!("Agent {task}: {error}");}
                    }
                }
                if !active && child_review_pending && delegation_pending.is_none() && integration_pending.is_none() && integration_apply.is_none() && current(host,scope)?.state==TaskState::Running {
                    let live_children=host.snapshot()?.records.values().filter(|r|r.collection==Collection::Task && r.workspace==scope.workspace)
                        .filter_map(|r|r.decode::<Task>().ok()).any(|t|t.scope.session==scope.session && t.root==scope.task && t.scope.task!=scope.task && !t.state.terminal());
                    if !live_children {
                        stop(host,scope,TaskState::Paused)?;
                        notice="Child turns are terminal. Review their evidence and any integration, then /resume for current parent verification and completion.".into();
                    }
                }
                if active && lifecycle_pending.is_none() && current(host,scope)?.state == TaskState::Running {
                    if let Some(message) = shadow.poll(host,session.id).await { notice=message.into(); }
                } else {
                    shadow.cancel().await;
                }
                if mcp_pending.as_ref().is_some_and(|p|p.is_finished()) {
                    let mut command=mcp_pending.take().ok_or("MCP observer missing")?;
                    notice=match command.result().await {
                        Ok(result)=>{
                            maintenance_page=None;page=None;
                            page_text=super::sanitize(&serde_json::to_string(&result).map_err(|e|e.to_string())?,1024*1024);
                            display_page(&mut page_text)
                        }
                        Err(error)=>format!("MCP control rejected: {error}"),
                    };
                }
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
        if let Some(message) = backup_triggers.observe(host, current(host, scope)?.state) {
            notice = message;
        }
        let snapshot = view(&host.snapshot()?, scope, model)?;
        // Keep control notices, questions and money separate from potentially
        // large inspection pages and untrusted commentary.
        let mut lines=vec![
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
        if let Some(task)=&followed {
            lines.push(format!("Following: {}",super::sanitize(&crate::agents_view::live_detail(host,session.id,scope,task,crate::settings::now())?.to_string(),16384)));
        }
        let display = lines.join("\n");
        if display != last {
            renderer.show_lines(&lines);
            last = display;
        }
        if current(host, scope)?.state.terminal() {
            return Ok(());
        }
    }
    }.await;
    shadow.cancel().await;
    if let Some(mut job) = lifecycle_pending {
        drop(host.hold_execution());
        if current(host, scope).is_ok_and(|task| !task.state.terminal()) {
            let _ = stop(host, scope, TaskState::Paused);
        }
        let _ = job.result().await;
    }
    // Fence descendant and integration admission before dropping observers.
    // An already dispatched blocking edit can outlive its async waiter.
    if delegation_pending.is_some()
        || cleanup_pending.is_some()
        || !children.is_empty()
        || integration_pending.is_some()
        || integration_apply.is_some()
    {
        if current(host, scope).is_ok_and(|task| !task.state.terminal()) {
            let _ = stop(host, scope, TaskState::Paused);
        }
    }
    if let Some(job) = integration_pending {
        job.abort();
        let _ = job.await;
    }
    if let Some(job) = cleanup_pending {
        job.abort();
        let _ = job.await;
    }
    if let Some(job) = integration_apply {
        job.abort();
        let _ = job.await;
    }
    if delegation_pending.is_some() || !children.is_empty() {
        if let Some(job) = delegation_pending {
            job.abort();
            let _ = job.await;
        }
        for (_, (child, pump)) in children {
            if let Some(pump) = pump {
                pump.abort();
                let _ = pump.await;
            }
            let _ = child.session.thread.shutdown_and_wait().await;
        }
    }
    result
}
