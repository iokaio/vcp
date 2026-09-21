// SPDX-License-Identifier: Apache-2.0
//! Read-only attributed child progress. Rendering never starts or resumes work.
use serde_json::{json, Value};
use vcp_domain::{
    accounting::{Reservation, ReservationState},
    task::Task,
    workspace::Scope,
    Timestamp,
};
use vcp_store::contract::{Collection, State};

pub fn child(state: &State, scope: &Scope, id: &vcp_domain::TaskId) -> Result<Task, String> {
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
        let mut reserved = 0u64;
        let mut uncertain = 0u64;
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
                .checked_add(reservation.liability.get())
                .ok_or("agent liability overflow")?;
        }
        let latest = state.events.iter().rev().find(|row| {
            row.event.workspace == scope.workspace
                && row.event.session == scope.session
                && (row.event.task.as_ref() == Some(&task.scope.task) || row.event.id == task.cause)
        });
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
            "allocation":spec.map(|s|s.allocation),
            "cost":{"known":known,"reserved":reserved,"uncertain":uncertain,"units":"micros","scope":"this node only"},
            "canonical_constraints":constraints,"active_effects":active_effects,
            "last_activity":latest.map(|e|json!({"event":e.event.id,"watermark":e.watermark,"timestamp":e.event.timestamp,"kind":e.event.kind})),
            "result_count":graph.as_ref().and_then(|g|g.results.get(&task.scope.task)).map_or(0,Vec::len)}));
    }
    Ok(
        json!({"root":selected.root,"items":items,"total":children.len(),
        "next_offset":(offset+items.len()<children.len()).then_some(offset+items.len()),
        "observation":"canonical snapshot; dispatch also requires current native state and a live owner"}),
    )
}
