// SPDX-License-Identifier: Apache-2.0
//! Pure admission projections; actual dispatch and workspace effects stay in the host.
use crate::{Error, Result};
use std::collections::{BTreeMap, BTreeSet};
use vcp_domain::{accounting::Ledger, agents::*, policy::*, task::*, workspace::*, *};
use vcp_store::contract::*;

pub fn graph(state: &State, scope: &Scope, root: &TaskId) -> Result<Option<TaskGraph>> {
    state
        .records
        .get(&key(Collection::Projection, &graph_id(root)))
        .map(|row| {
            if row.workspace != scope.workspace {
                return Err(Error::Access);
            }
            let graph: TaskGraph = row.decode()?;
            graph.validate()?;
            if graph.scope.session != scope.session || &graph.scope.task != root {
                return Err(Error::Access);
            }
            Ok(graph)
        })
        .transpose()
}
fn task(state: &State, scope: &Scope, id: &TaskId) -> Result<Task> {
    let task: Task = state
        .record(Collection::Task, id.as_str(), &scope.workspace)?
        .decode()?;
    if task.scope.session != scope.session {
        return Err(Error::Access);
    }
    Ok(task)
}
/// Validate only a declared ceiling. A grant for one exact operation cannot
/// authorize arbitrary operations by a child; the broker still checks every use.
pub fn inherited_grant(
    state: &State,
    parent: &Task,
    grant: &Grant,
    now: Timestamp,
) -> Result<bool> {
    if !matches!(grant.target, GrantTarget::Configured { .. })
        || grant.revoked
        || grant.expires_at <= now
    {
        return Ok(false);
    }
    let graph = graph(state, &parent.scope, &parent.root)?;
    let mut cursor = parent.clone();
    loop {
        if grant.scope.contains(&cursor.scope) {
            return Ok(true);
        }
        let Some(assignment) = graph
            .as_ref()
            .and_then(|g| g.children.get(&cursor.scope.task))
        else {
            return Ok(false);
        };
        if assignment.grants.get(&grant.id) != Some(&grant.revision)
            || assignment.actor != grant.actor
            || assignment.authority != grant.authority
            || assignment.binding != grant.binding
            || assignment.policy != grant.policy
            || assignment.deadline <= now
        {
            return Ok(false);
        }
        let owner = task(state, &parent.scope, &assignment.parent)?;
        if assignment.parent_steering != owner.steering {
            return Ok(false);
        }
        cursor = owner;
    }
}
pub fn current_scope(
    state: &State,
    parent: &Task,
    child: &ChildSpec,
    now: Timestamp,
) -> Result<bool> {
    let workspace: Workspace = state
        .record(
            Collection::Workspace,
            parent.scope.workspace.as_str(),
            &parent.scope.workspace,
        )?
        .decode()?;
    let policy = crate::policy::current(state, &parent.scope.workspace)?;
    if child.parent_steering != parent.steering
        || child.authority != workspace.authority
        || child.binding != workspace.binding.revision
        || child.policy != policy.revision
        || workspace.trust != Trust::Trusted
        || child.deadline <= now
        || child
            .paths
            .iter()
            .any(|p| !policy.workspace_roots.contains(&p.root))
    {
        return Ok(false);
    }
    let mut allowed = BTreeSet::from([EffectClass::Read]);
    if policy.mode == Autonomy::Workspace {
        allowed.insert(EffectClass::Write);
    }
    if policy.mode == Autonomy::Autonomous {
        allowed.extend(&policy.automatic_effects);
    }
    let grants = crate::policy::grants(state, &parent.scope.workspace)?;
    for (id, revision) in &child.grants {
        let Some(grant) = grants.iter().find(|g| &g.id == id) else {
            return Ok(false);
        };
        if grant.revision != *revision
            || grant.revoked
            || grant.actor != child.actor
            || !inherited_grant(state, parent, grant, now)?
            || grant.host != workspace.binding.host
            || grant.binding != child.binding
            || grant.authority != child.authority
            || grant.policy != child.policy
            || grant.expires_at <= now
        {
            return Ok(false);
        }
        if let GrantTarget::Configured {
            effects,
            roots,
            paths,
            ..
        } = &grant.target
        {
            if child.paths.iter().all(|p| {
                roots.contains(&p.root)
                    && paths
                        .iter()
                        .any(|prefix| vcp_domain::agents::prefix(prefix, &p.path))
            }) {
                allowed.extend(effects);
            }
        }
    }
    if policy.mode == Autonomy::Plan {
        allowed = BTreeSet::from([EffectClass::Read]);
    }
    Ok(child.effects.is_subset(&allowed))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Blocker {
    Owner,
    State,
    Ancestor,
    Dependency,
    Scope,
    Deadline,
    Workspace,
    Concurrency,
    ResourceConflict,
    Budget,
}
pub fn eligibility(
    state: &State,
    child_task: &Task,
    now: Timestamp,
    owner_current: bool,
) -> Result<Vec<Blocker>> {
    eligibility_for_state(state, child_task, now, owner_current, false)
}
/// Explicit resume admission only. A paused child remains blocked in ordinary
/// scheduling/UI projections; domain transition still requires resume evidence.
pub fn eligibility_for_resume(
    state: &State,
    child_task: &Task,
    now: Timestamp,
    owner_current: bool,
) -> Result<Vec<Blocker>> {
    eligibility_for_state(state, child_task, now, owner_current, true)
}
fn eligibility_for_state(
    state: &State,
    child_task: &Task,
    now: Timestamp,
    owner_current: bool,
    resume: bool,
) -> Result<Vec<Blocker>> {
    let mut blockers = Vec::new();
    if !owner_current {
        blockers.push(Blocker::Owner);
    }
    let Some(graph) = graph(state, &child_task.scope, &child_task.root)? else {
        return Ok(vec![Blocker::Scope]);
    };
    let Some(child) = graph.children.get(&child_task.scope.task) else {
        return Ok(vec![Blocker::Scope]);
    };
    let admissible = if resume {
        matches!(
            child_task.state,
            TaskState::Paused | TaskState::WaitingForInput | TaskState::Blocked
        )
    } else {
        matches!(child_task.state, TaskState::Pending | TaskState::Running)
    };
    if !admissible {
        blockers.push(Blocker::State);
    }
    let parent = task(state, &child_task.scope, &child.parent)?;
    let mut cursor = Some(parent.clone());
    while let Some(ancestor) = cursor {
        if ancestor.state != TaskState::Running {
            blockers.push(Blocker::Ancestor);
            break;
        }
        if let Some(spec) = graph.children.get(&ancestor.scope.task) {
            let owner = task(state, &ancestor.scope, &spec.parent)?;
            if !current_scope(state, &owner, spec, now)? {
                blockers.push(Blocker::Scope);
                break;
            }
        }
        cursor = ancestor
            .parent
            .as_ref()
            .map(|id| task(state, &child_task.scope, id))
            .transpose()?;
    }
    if !current_scope(state, &parent, child, now)? {
        blockers.push(Blocker::Scope);
    }
    if child.deadline <= now {
        blockers.push(Blocker::Deadline);
    }
    if !graph.ready.contains_key(&child_task.scope.task) {
        blockers.push(Blocker::Workspace);
    }
    for dependency in &child.dependencies {
        if task(state, &child_task.scope, dependency)?.state != TaskState::Completed {
            blockers.push(Blocker::Dependency);
            break;
        }
    }
    let mut active = BTreeSet::new();
    for row in state
        .records
        .values()
        .filter(|r| r.workspace == child_task.scope.workspace)
    {
        if row.collection == Collection::Effect {
            let effect: vcp_domain::effect::Effect = row.decode()?;
            if matches!(
                effect.state,
                vcp_domain::effect::EffectState::DispatchRecorded
                    | vcp_domain::effect::EffectState::Running
                    | vcp_domain::effect::EffectState::OutcomeUnknown
            ) {
                active.insert(effect.scope.task);
            }
        }
        if row.collection == Collection::Reservation {
            let reservation: vcp_domain::accounting::Reservation = row.decode()?;
            if reservation.liability != Micros::ZERO {
                active.insert(reservation.scope.task);
            }
        }
    }
    let mut running = 0;
    for (id, other) in &graph.children {
        if id == &child_task.scope.task {
            continue;
        }
        if task(state, &child_task.scope, id)?.state != TaskState::Running && !active.contains(id) {
            continue;
        }
        running += 1;
        // Distinct disposable roots isolate overlapping logical write sets.
        let isolated = child.mode == ChildMode::IsolatedWrite
            && other.mode == ChildMode::IsolatedWrite
            && child.isolated_root.is_some()
            && child.isolated_root != other.isolated_root;
        if !isolated
            && child
                .paths
                .iter()
                .any(|a| other.paths.iter().any(|b| a.overlaps(b)))
        {
            blockers.push(Blocker::ResourceConflict);
        }
    }
    if running >= graph.limits.concurrency {
        blockers.push(Blocker::Concurrency);
    }
    let ledger: Ledger = state
        .record(
            Collection::Ledger,
            child_task.root.as_str(),
            &child_task.scope.workspace,
        )?
        .decode()?;
    if ledger.overrun
        || ledger.allocations.get(&child_task.scope.task) != Some(&child.allocation)
        || exhausted_capacity(state, child_task, &ledger)?
    {
        blockers.push(Blocker::Budget);
    }
    Ok(blockers)
}

/// Match reservation admission's root and allocated-ancestor exposure, without
/// reserving a future quote or counting unused sibling allocations as charges.
fn exhausted_capacity(state: &State, child: &Task, ledger: &Ledger) -> Result<bool> {
    let root_exposure = u128::from(ledger.settled.get())
        + u128::from(ledger.active.get())
        + u128::from(ledger.unresolved.get())
        + u128::from(ledger.protected.get());
    if root_exposure >= u128::from(ledger.cap.get()) {
        return Ok(true);
    }
    let mut remaining = BTreeMap::new();
    let mut cursor = Some(child.clone());
    while let Some(current) = cursor {
        if let Some(cap) = ledger.allocations.get(&current.scope.task) {
            remaining.insert(current.scope.task.clone(), (u128::from(cap.get()), 0u128));
        }
        cursor = current
            .parent
            .as_ref()
            .map(|id| task(state, &child.scope, id))
            .transpose()?;
    }
    for row in state.records.values().filter(|row| {
        row.collection == Collection::Reservation && row.workspace == child.scope.workspace
    }) {
        let reservation: vcp_domain::accounting::Reservation = row.decode()?;
        if reservation.root != child.root {
            continue;
        }
        let exposure =
            u128::from(reservation.charged.get()) + u128::from(reservation.liability.get());
        let mut cursor = Some(task(state, &child.scope, &reservation.scope.task)?);
        while let Some(current) = cursor {
            if let Some((_, used)) = remaining.get_mut(&current.scope.task) {
                *used += exposure;
            }
            cursor = current
                .parent
                .as_ref()
                .map(|id| task(state, &child.scope, id))
                .transpose()?;
        }
    }
    Ok(remaining.values().any(|(cap, used)| used >= cap))
}

pub(crate) fn create(
    state: &State,
    parent: &Task,
    child_id: &TaskId,
    spec: &ChildSpec,
    limits: &GraphLimits,
    expected_graph: Option<Revision>,
    expected_ledger: Revision,
    now: Timestamp,
) -> Result<(TaskGraph, Ledger)> {
    spec.validate()?;
    let mut ancestor = parent.parent.clone();
    while let Some(id) = ancestor {
        let current = task(state, &parent.scope, &id)?;
        if current.state != TaskState::Running {
            return Err(Error::Access);
        }
        ancestor = current.parent;
    }
    if parent.state != TaskState::Running
        || spec.parent != parent.scope.task
        || (!parent.editing && spec.mode == ChildMode::IsolatedWrite)
        || !current_scope(state, parent, spec, now)?
    {
        return Err(Error::Access);
    }
    let existing = graph(state, &parent.scope, &parent.root)?;
    if existing.as_ref().map(|g| g.revision) != expected_graph {
        return Err(vcp_domain::Error::Stale.into());
    }
    let mut graph = if let Some(mut graph) = existing {
        if graph.limits != *limits {
            return Err(Error::Target);
        }
        graph.revision = graph.revision.next()?;
        graph
    } else {
        let root = task(state, &parent.scope, &parent.root)?;
        TaskGraph {
            document_type: GRAPH.into(),
            schema_version: 1,
            scope: root.scope,
            revision: Revision::ZERO,
            limits: limits.clone(),
            children: BTreeMap::new(),
            ready: BTreeMap::new(),
            results: BTreeMap::new(),
        }
    };
    if graph
        .children
        .insert(child_id.clone(), spec.clone())
        .is_some()
    {
        return Err(Error::Target);
    }
    graph.validate()?;
    let mut ledger: Ledger = state
        .record(
            Collection::Ledger,
            parent.root.as_str(),
            &parent.scope.workspace,
        )?
        .decode()?;
    if ledger.revision != expected_ledger || ledger.overrun {
        return Err(vcp_domain::Error::Stale.into());
    }
    if ledger
        .allocations
        .insert(child_id.clone(), spec.allocation)
        .is_some()
    {
        return Err(Error::Target);
    }
    ledger.revision = ledger.revision.next()?;
    ledger.policy = ledger.policy.next()?;
    ledger.validate()?;
    Ok((graph, ledger))
}

/// Trusted native observation supplied by the host after snapshot materialization.
/// This type is deliberately not deserializable as a protocol command.
pub struct NativeWorkspaceEvidence {
    pub child: TaskId,
    pub expected_graph: Revision,
    pub ready: WorkspaceReady,
}
impl<S: CanonicalStore> crate::Engine<S> {
    pub async fn record_child_workspace(
        &mut self,
        scope: &Scope,
        evidence: NativeWorkspaceEvidence,
        access: &crate::Access,
        host: &crate::HostFacts,
    ) -> Result<Receipt> {
        self.authorize(access)?;
        if !access.write
            || access.workspace != scope.workspace
            || access.session != scope.session
            || !host.may_execute
        {
            return Err(Error::Access);
        }
        let parent = task(self.store().state(), scope, &scope.task)?;
        let mut graph = graph(self.store().state(), scope, &parent.root)?.ok_or(Error::Target)?;
        if graph.revision != evidence.expected_graph {
            return Err(vcp_domain::Error::Stale.into());
        }
        let spec = graph.children.get(&evidence.child).ok_or(Error::Target)?;
        let child = task(self.store().state(), scope, &evidence.child)?;
        if spec.parent != scope.task
            || spec.actor != access.actor
            || parent.state != TaskState::Running
            || child.state != TaskState::Pending
            || !current_scope(self.store().state(), &parent, spec, host.now)?
        {
            return Err(Error::Access);
        }
        if let Some(ready) = graph.ready.get(&evidence.child) {
            if ready != &evidence.ready {
                return Err(Error::Target);
            }
        }
        graph.ready.insert(evidence.child.clone(), evidence.ready);
        graph.revision = graph.revision.next()?;
        graph.validate()?;
        let event = vcp_protocol::event::EventInput {
            id: EventId::new(),
            workspace: scope.workspace.clone(),
            session: scope.session.clone(),
            task: Some(evidence.child),
            actor: access.actor.clone(),
            correlation: CommandId::new(),
            causation: None,
            timestamp: host.now,
            kind: vcp_protocol::event::EventKind::ChildGraphChanged,
            artifacts: vec![],
            data: serde_json::json!({"schema_version":1,"workspace_ready":true}),
            metadata: None,
        };
        let transaction = Transaction {
            id: TransactionId::new(),
            expected_watermark: self.store().state().watermark,
            mutations: vec![Mutation::Put {
                expected: Some(evidence.expected_graph),
                record: Record::typed(
                    Collection::Projection,
                    graph_id(&graph.scope.task),
                    scope.workspace.clone(),
                    graph.revision,
                    &graph,
                )?,
            }],
            events: vec![event],
            command: None,
        };
        Ok(self.store_mut().transact(transaction).await?)
    }
}
