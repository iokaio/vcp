// SPDX-License-Identifier: Apache-2.0
//! Read-only attributed child progress. Rendering never starts or resumes work.
use serde_json::{json, Value};
use vcp_domain::{
    accounting::{Reservation, ReservationState},
    task::Task,
    workspace::Scope,
    Timestamp,
};
use vcp_protocol::event::EventEnvelope;
use vcp_store::{
    contract::{CanonicalStore, Collection, State},
    CurrentStateView,
};

#[cfg(windows)]
pub fn live_detail(
    host: &vcp_lifecycle::foundation::CanonicalHost,
    parent: codex_protocol::ThreadId,
    scope: &Scope,
    id: &vcp_domain::TaskId,
    now: Timestamp,
) -> Result<Value, String> {
    let reader = host.history_reader()?;
    child(reader.current(), scope, id)?;
    let mut offset = 0;
    let mut value = loop {
        let page = page_reader(&reader, scope, now, offset)?;
        if let Some(item) = page["items"]
            .as_array()
            .and_then(|items| items.iter().find(|item| item["task"] == id.as_str()))
        {
            break item.clone();
        }
        offset = page["next_offset"]
            .as_u64()
            .ok_or("selected agent disappeared")?
            .try_into()
            .map_err(|_| "agent offset overflow")?;
    };
    value["review_evidence"] = match host.child_review_findings(parent, id.clone()) {
        Ok(evidence) => evidence,
        Err(reason) => {
            json!({"status":"unavailable","reason":crate::terminal::sanitize(&reason,512),"observation":"inspect retained result references through an authorized owner"})
        }
    };
    Ok(value)
}

pub fn child<'a>(
    state: impl Into<CurrentStateView<'a>>,
    scope: &Scope,
    id: &vcp_domain::TaskId,
) -> Result<Task, String> {
    let state = state.into();
    let parent: Task = state
        .record(Collection::Task, scope.task.as_str(), &scope.workspace)
        .and_then(|r| r.decode())
        .map_err(|e| e.to_string())?;
    let child: Task = state
        .record(Collection::Task, id.as_str(), &scope.workspace)
        .and_then(|r| r.decode())
        .map_err(|e| e.to_string())?;
    if parent.scope != *scope
        || child.scope.session != scope.session
        || child.root != parent.root
        || child.scope.task == parent.root
    {
        return Err("selected agent is outside this task tree".into());
    }
    Ok(child)
}

pub fn detail(
    state: &State,
    scope: &Scope,
    id: &vcp_domain::TaskId,
    now: Timestamp,
) -> Result<Value, String> {
    child(state, scope, id)?;
    let mut offset = 0;
    loop {
        let result = page(state, scope, now, offset)?;
        if let Some(item) = result["items"]
            .as_array()
            .and_then(|items| items.iter().find(|item| item["task"] == id.as_str()))
        {
            return Ok(item.clone());
        }
        offset = result["next_offset"]
            .as_u64()
            .ok_or("selected agent disappeared")?
            .try_into()
            .map_err(|_| "agent offset overflow")?;
    }
}

pub fn page(state: &State, scope: &Scope, now: Timestamp, offset: usize) -> Result<Value, String> {
    page_with_latest(state.into(), scope, now, offset, |task| {
        state
            .events
            .iter()
            .rev()
            .find(|row| {
                row.event.workspace == scope.workspace
                    && row.event.session == scope.session
                    && (row.event.task.as_ref() == Some(&task.scope.task)
                        || row.event.id == task.cause)
            })
            .map(activity)
    })
}

fn activity(event: &EventEnvelope) -> Value {
    json!({"event":event.event.id,"watermark":event.watermark,"timestamp":event.event.timestamp,"kind":event.event.kind})
}

#[cfg(windows)]
pub fn live_page(
    host: &vcp_lifecycle::foundation::CanonicalHost,
    scope: &Scope,
    now: Timestamp,
    offset: usize,
) -> Result<Value, String> {
    let reader = host.history_reader()?;
    page_reader(&reader, scope, now, offset)
}

#[cfg(windows)]
pub(crate) fn page_reader(
    reader: &vcp_lifecycle::foundation::history_reader::HistoryReader,
    scope: &Scope,
    now: Timestamp,
    offset: usize,
) -> Result<Value, String> {
    let state = reader.current();
    let mut selected = Vec::new();
    let mut value = page_with_latest(state, scope, now, offset, |task| {
        selected.push((task.scope.task.clone(), task.cause.clone()));
        None
    })?;
    let mut latest = vec![Value::Null; selected.len()];
    let mut at = 0u64;
    loop {
        let page = reader.page(at.checked_sub(1), 256)?;
        if page.events.is_empty() && at < page.count {
            return Err("agent history ended before its source cut".into());
        }
        for event in page.events {
            at += 1;
            if event.event.workspace != scope.workspace || event.event.session != scope.session {
                continue;
            }
            for (index, (task, cause)) in selected.iter().enumerate() {
                if event.event.task.as_ref() == Some(task) || &event.event.id == cause {
                    latest[index] = activity(&event);
                }
            }
        }
        if at == page.count {
            break;
        }
        if at > page.count {
            return Err("agent history exceeded its source cut".into());
        }
    }
    for (item, activity) in value["items"]
        .as_array_mut()
        .ok_or("agent view items unavailable")?
        .iter_mut()
        .zip(latest)
    {
        item["last_activity"] = activity;
    }
    Ok(value)
}

pub async fn page_store(
    store: &impl CanonicalStore,
    scope: &Scope,
    now: Timestamp,
    offset: usize,
) -> Result<Value, String> {
    let watermark = store.current().watermark;
    let mut selected = Vec::new();
    let mut value = page_with_latest(store.current(), scope, now, offset, |task| {
        selected.push((task.scope.task.clone(), task.cause.clone()));
        None
    })?;
    let mut latest = vec![Value::Null; selected.len()];
    let count = store
        .history_event_count()
        .await
        .map_err(|e| e.to_string())?;
    let mut at = 0u64;
    while at < count {
        let limit = (count - at).min(256) as usize;
        let events = store
            .history_events(at.checked_sub(1), limit)
            .await
            .map_err(|e| e.to_string())?;
        if events.is_empty() || events.len() > limit || store.current().watermark != watermark {
            return Err("agent history cut changed or incomplete".into());
        }
        for event in events {
            if event.watermark > watermark {
                return Err("agent event exceeds source cut".into());
            }
            at += 1;
            if event.event.workspace != scope.workspace || event.event.session != scope.session {
                continue;
            }
            for (index, (task, cause)) in selected.iter().enumerate() {
                if event.event.task.as_ref() == Some(task) || &event.event.id == cause {
                    latest[index] = activity(&event);
                }
            }
        }
    }
    if store.current().watermark != watermark {
        return Err("agent source changed".into());
    }
    let items = value["items"]
        .as_array_mut()
        .ok_or("agent view items unavailable")?;
    for (item, activity) in items.iter_mut().zip(latest) {
        item["last_activity"] = activity;
    }
    Ok(value)
}

fn page_with_latest(
    state: CurrentStateView<'_>,
    scope: &Scope,
    now: Timestamp,
    offset: usize,
    mut latest: impl FnMut(&Task) -> Option<Value>,
) -> Result<Value, String> {
    let selected: Task = state
        .record(Collection::Task, scope.task.as_str(), &scope.workspace)
        .and_then(|r| r.decode())
        .map_err(|e| e.to_string())?;
    if selected.scope != *scope {
        return Err("agent view scope mismatch".into());
    }
    let graph =
        vcp_engine::agents::graph(state, scope, &selected.root).map_err(|e| e.to_string())?;
    let mut children = Vec::new();
    for record in state
        .records
        .values()
        .filter(|r| r.workspace == scope.workspace && r.collection == Collection::Task)
    {
        let task: Task = record.decode().map_err(|e| e.to_string())?;
        if task.scope.session == scope.session
            && task.root == selected.root
            && task.scope.task != selected.root
        {
            children.push(task);
        }
    }
    children.sort_by(|a, b| a.scope.task.cmp(&b.scope.task));
    if offset > children.len() {
        return Err("agent page offset exceeds current tree".into());
    }
    let mut items = Vec::new();
    for task in children.iter().skip(offset).take(8) {
        let spec = graph
            .as_ref()
            .and_then(|g| g.children.get(&task.scope.task));
        let mut known = 0u64;
        let mut reserved = vcp_domain::accounting::EstimatedMicros::ZERO;
        let mut uncertain = vcp_domain::accounting::EstimatedMicros::ZERO;
        for record in state
            .records
            .values()
            .filter(|r| r.workspace == scope.workspace && r.collection == Collection::Reservation)
        {
            let reservation: Reservation = record.decode().map_err(|e| e.to_string())?;
            if reservation.scope != task.scope {
                continue;
            }
            known = known
                .checked_add(reservation.charged.get())
                .ok_or("agent cost overflow")?;
            let target = match reservation.phase {
                ReservationState::ReconciliationPending => &mut uncertain,
                ReservationState::Created | ReservationState::Submitted => &mut reserved,
                _ => continue,
            };
            *target = target
                .checked_add(reservation.liability)
                .map_err(|_| "agent liability overflow")?;
        }
        let latest = latest(task);
        let constraints = if spec.is_some() {
            vcp_engine::agents::eligibility(state, task, now, true)
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|b| format!("{b:?}"))
                .collect::<Vec<_>>()
        } else {
            vec!["No registered assignment".into()]
        };
        let active_effects: Vec<_> = state
            .records
            .values()
            .filter(|r| {
                r.workspace == scope.workspace
                    && r.collection == Collection::Effect
                    && r.value["scope"]["session"] == scope.session.as_str()
                    && r.value["scope"]["task"] == task.scope.task.as_str()
                    && !matches!(
                        r.value["state"].as_str(),
                        Some("succeeded" | "failed" | "cancelled")
                    )
            })
            .take(8)
            .map(|r| json!({"id":r.id,"state":r.value["state"]}))
            .collect();
        items.push(json!({"task":task.scope.task,"root":task.root,"parent":task.parent,
            "objective":task.objectives.last().map(|o|crate::terminal::sanitize(&o.text,512)),
            "state":task.state,"reason":crate::terminal::sanitize(&task.reason,256),
            "role":spec.map(|s|crate::terminal::sanitize(&s.role,128)),
            "model_policy":spec.map(|s|crate::terminal::sanitize(&s.model_policy,256)),
            "workspace":spec.and_then(|s|s.isolated_root.as_ref()),
            "registration":spec.and_then(|s|s.registration.as_ref()),
            "cleanup":graph.as_ref().and_then(|g|g.cleanup.get(&task.scope.task)).map(|c|json!({"intent":c.intent,"retained_result":c.retained_result,"rejected_edits":c.rejected_edits,"receipt":c.receipt,"status":if c.receipt.is_some(){"removed; history retained"}else{"pending or failed; only deliberate reconciliation may remove remaining entries"},"last_diagnostic":c.diagnostics.last().map(|d|json!({"artifact":d.artifact,"reason":crate::terminal::sanitize(&d.reason,2048)})),"diagnostic_count":c.diagnostics.len()})),
            "read_scope":spec.map(|s|s.paths.iter().filter(|p| !p.write).collect::<Vec<_>>()),
            "helper_template":task.objectives.first().map(|o|o.constraints.iter().filter(|c|c.starts_with("helper-template:")).map(|c|crate::terminal::sanitize(c,2048)).collect::<Vec<_>>()),
            "readiness":{
                "materialization":if graph.as_ref().is_some_and(|g|g.ready.contains_key(&task.scope.task)) {"recorded_ready; current native identity is rechecked before dispatch"} else {"not_ready; inspect setup reason and required inputs before explicit recovery"},
                "process_checks":"unavailable: child process filesystem isolation is not qualified; applicable checks must run on the integrated parent",
                "checks_not_run":task.required_checks,
                "setup_reason":crate::terminal::sanitize(&task.reason,256)
            },
            "allocation":spec.map(|s|s.allocation),
            "cost":{"known":known,"reserved":reserved,"uncertain":uncertain,"units":"micros","scope":"this node only"},
            "canonical_constraints":constraints,"active_effects":active_effects,
            "last_activity":latest,
            "latest_result":graph.as_ref().and_then(|g|g.results.get(&task.scope.task)).and_then(|r|r.last()),
            "result_evidence_status":"historical untrusted evidence at its examined revision; not current parent acceptance",
            "result_count":graph.as_ref().and_then(|g|g.results.get(&task.scope.task)).map_or(0,Vec::len)}));
    }
    Ok(
        json!({"root":selected.root,"items":items,"total":children.len(),
        "next_offset":(offset+items.len()<children.len()).then_some(offset+items.len()),
        "observation":"canonical snapshot; dispatch also requires current native state and a live owner"}),
    )
}
