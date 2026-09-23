// SPDX-License-Identifier: Apache-2.0
use crate::{Error, Result};
use vcp_domain::policy::*;
use vcp_domain::{
    artifact::*, effect::*, ids::*, revision::*, task::*, verification::*, workspace::*,
};
use vcp_protocol::{command::*, event::*};
use vcp_store::contract::*;

/// Supplied by the authenticated local host, never accepted from JSON commands.
/// Revocation changes the workspace authority revision, invalidating old grants.
#[derive(Clone)]
pub struct Access {
    pub actor: ActorId,
    pub workspace: WorkspaceId,
    pub session: SessionId,
    pub authority: AuthorityRevision,
    pub read: bool,
    pub write: bool,
    pub bootstrap: bool,
}
/// Observations collected by the host immediately before command admission.
/// A user-supplied boolean inside an envelope cannot become this capability.
pub struct HostFacts {
    pub now: Timestamp,
    pub policy: PolicyRevision,
    pub resume: Option<ResumeEvidence>,
    pub may_execute: bool,
}
impl HostFacts {
    pub fn inspect(now: Timestamp) -> Self {
        Self {
            now,
            policy: PolicyRevision::ZERO,
            resume: None,
            may_execute: false,
        }
    }
}

pub struct Engine<S: CanonicalStore> {
    store: S,
    controller: ControllerId,
    owner: OwnerEpoch,
    pub(crate) subscriptions:
        std::collections::BTreeMap<SnapshotId, vcp_protocol::subscription::Cursor>,
}
impl<S: CanonicalStore> Engine<S> {
    pub fn new(store: S) -> Result<Self> {
        let owner = OwnerEpoch::new(store.state().watermark.get()).next()?;
        Ok(Self {
            store,
            controller: ControllerId::new(),
            owner,
            subscriptions: Default::default(),
        })
    }
    pub fn controller(&self) -> &ControllerId {
        &self.controller
    }
    pub fn owner_epoch(&self) -> OwnerEpoch {
        self.owner
    }
    pub fn store(&self) -> &S {
        &self.store
    }
    pub fn store_mut(&mut self) -> &mut S {
        &mut self.store
    }
    pub fn into_store(self) -> S {
        self.store
    }
    pub fn authorize(&self, access: &Access) -> Result<()> {
        if !access.read {
            return Err(Error::Access);
        }
        let workspace = self.store.state().record(
            Collection::Workspace,
            access.workspace.as_str(),
            &access.workspace,
        );
        match workspace {
            Ok(record) => {
                let workspace: Workspace = record.decode()?;
                if workspace.authority != access.authority {
                    return Err(Error::Access);
                }
                self.store.state().record(
                    Collection::Session,
                    access.session.as_str(),
                    &access.workspace,
                )?;
            }
            Err(_)
                if access.bootstrap
                    && access.write
                    && access.authority == AuthorityRevision::ZERO => {}
            Err(_) => return Err(Error::Access),
        }
        Ok(())
    }
    /// Interactive and headless adapters call this same handler. Neither owns a
    /// scheduler or receives authority from fields supplied in serialized input.
    pub async fn jsonl(
        &mut self,
        bytes: &[u8],
        access: &Access,
        host: &HostFacts,
    ) -> Result<Vec<u8>> {
        self.handle(CommandEnvelope::parse_jsonl(bytes)?, access, host)
            .await?
            .jsonl()
            .map_err(Error::from)
    }
    pub async fn handle(
        &mut self,
        command: CommandEnvelope,
        access: &Access,
        host: &HostFacts,
    ) -> Result<CommandReceipt> {
        self.handle_with_digest(command, access, host, None).await
    }

    /// Only trusted adapters select a digest domain. Public reconnect identity
    /// excludes transient engine ownership; internal callers retain the exact
    /// historical envelope digest. This is not a wire-supplied digest.
    pub(crate) async fn handle_with_digest(
        &mut self,
        command: CommandEnvelope,
        access: &Access,
        host: &HostFacts,
        digest: Option<String>,
    ) -> Result<CommandReceipt> {
        command.validate_version()?;
        if vcp_protocol::canonical_bytes(&command)?.len() > vcp_protocol::version::MAX_COMMAND_BYTES
        {
            return Err(vcp_protocol::version::Error::Limit.into());
        }
        self.authorize(access)?;
        if command.caller != access.actor
            || command.workspace != access.workspace
            || command.session != access.session
        {
            return Err(Error::Access);
        }
        if !access.write && !matches!(command.payload, Command::Inspect) {
            return Err(Error::Access);
        }
        let digest = match digest {
            Some(digest) => digest,
            None => command.digest()?,
        };
        // A retry is authenticated under current access, then resolves the old
        // receipt before stale state/owner checks. Restart cannot duplicate work.
        if let Some(receipt) =
            self.store
                .state()
                .command(&command.workspace, &command.id, &digest)?
        {
            return Ok(receipt);
        }
        if command.controller != self.controller || command.owner_epoch != self.owner {
            return Err(Error::Owner);
        }
        let event_id = EventId::new();
        let mut mutations = Vec::new();
        let mut artifacts = Vec::new();
        let mut accepted = command.expected;
        let mut result = None;
        let state = self.store.state();
        let scope = || -> Result<Scope> {
            Ok(Scope {
                workspace: command.workspace.clone(),
                session: command.session.clone(),
                task: command.task.clone().ok_or(Error::Target)?,
            })
        };
        let task = || -> Result<Task> {
            let scope = scope()?;
            let task: Task = state
                .record(Collection::Task, scope.task.as_str(), &scope.workspace)?
                .decode()?;
            if task.scope != scope {
                return Err(Error::Access);
            }
            Ok(task)
        };
        let mut put = |collection: Collection,
                       id: String,
                       revision: Revision,
                       value: serde_json::Value,
                       expected: Option<Revision>|
         -> Result<()> {
            let record = Record {
                collection,
                id,
                workspace: command.workspace.clone(),
                revision,
                value,
                references: Default::default(),
            };
            record.validate_shape()?;
            mutations.push(Mutation::Put { expected, record });
            accepted = revision;
            Ok(())
        };
        let kind = match &command.payload {
            Command::Initialize { binding } => {
                if !access.bootstrap
                    || command.task.is_some()
                    || command.expected != Revision::ZERO
                    || command.steering != SteeringRevision::ZERO
                {
                    return Err(Error::Access);
                }
                let workspace = Workspace {
                    id: command.workspace.clone(),
                    binding: binding.clone(),
                    trust: Trust::Untrusted,
                    revision: Revision::ZERO,
                    authority: AuthorityRevision::ZERO,
                    deletion: DeletionEpoch::ZERO,
                };
                workspace.validate()?;
                let session = Session {
                    id: command.session.clone(),
                    workspace: command.workspace.clone(),
                    revision: Revision::ZERO,
                    configuration: Revision::ZERO,
                    fork_origin: None,
                    fork_through: None,
                };
                put(
                    Collection::Workspace,
                    workspace.id.to_string(),
                    workspace.revision,
                    serde_json::to_value(workspace)?,
                    None,
                )?;
                put(
                    Collection::Session,
                    session.id.to_string(),
                    session.revision,
                    serde_json::to_value(session)?,
                    None,
                )?;
                EventKind::SessionStarted
            }
            Command::CreateSession { id, fork_through } => {
                if command.task.is_some()
                    || command.expected != Revision::ZERO
                    || command.steering != SteeringRevision::ZERO
                    || id == &command.session
                {
                    return Err(Error::Target);
                }
                let source: Session = state
                    .record(
                        Collection::Session,
                        command.session.as_str(),
                        &command.workspace,
                    )?
                    .decode()?;
                if let Some(turn) = fork_through {
                    let turn: Turn = state
                        .record(Collection::Turn, turn.as_str(), &command.workspace)?
                        .decode()?;
                    if turn.scope.session != command.session || turn.state != TurnState::Completed {
                        return Err(Error::Target);
                    }
                    // A stored row alone cannot claim a retained history boundary.
                    if !state.events.iter().any(|event| {
                        event.event.id == turn.cause
                            && event.event.session == command.session
                            && matches!(
                                event.event.kind,
                                EventKind::TurnTransition | EventKind::TaskTransition
                            )
                    }) {
                        return Err(Error::Target);
                    }
                }
                let session = Session {
                    id: id.clone(),
                    workspace: command.workspace.clone(),
                    revision: Revision::ZERO,
                    configuration: source.configuration,
                    fork_origin: fork_through.as_ref().map(|_| command.session.clone()),
                    fork_through: fork_through.clone(),
                };
                put(
                    Collection::Session,
                    id.to_string(),
                    Revision::ZERO,
                    serde_json::to_value(session)?,
                    None,
                )?;
                EventKind::SessionStarted
            }
            Command::CreateTask {
                root,
                parent,
                fork_origin,
                objective,
                fingerprint,
                editing,
                required_checks,
            } => {
                if parent.is_some() && crate::agents::graph(state, &scope()?, root)?.is_some() {
                    return Err(Error::Target);
                }
                if command.expected != Revision::ZERO || command.steering != SteeringRevision::ZERO
                {
                    return Err(vcp_domain::Error::Stale.into());
                }
                let mut objective = objective.clone();
                objective.source = event_id.clone();
                objective.steering = SteeringRevision::ZERO;
                let task = Task {
                    scope: scope()?,
                    root: root.clone(),
                    parent: parent.clone(),
                    fork_origin: fork_origin.clone(),
                    revision: Revision::ZERO,
                    steering: SteeringRevision::ZERO,
                    objectives: vec![objective],
                    state: TaskState::Pending,
                    fingerprint: fingerprint.clone(),
                    editing: *editing,
                    required_checks: required_checks.clone(),
                    cause: event_id.clone(),
                    reason: "accepted objective".into(),
                    redaction: None,
                };
                task.validate()?;
                put(
                    Collection::Task,
                    task.scope.task.to_string(),
                    task.revision,
                    serde_json::to_value(task)?,
                    None,
                )?;
                EventKind::TaskCreated
            }
            Command::CreateChild {
                id,
                objective,
                fingerprint,
                required_checks,
                spec,
                limits,
                expected_graph,
                expected_ledger,
            } => {
                let parent = task()?;
                if parent.revision != command.expected
                    || parent.steering != command.steering
                    || spec.actor != command.caller
                {
                    return Err(vcp_domain::Error::Stale.into());
                }
                if objective.acceptance.is_empty()
                    || objective.acceptance.iter().any(|s| s.trim().is_empty())
                {
                    return Err(vcp_domain::Error::Invalid("child acceptance criteria").into());
                }
                let (graph, ledger) = crate::agents::create(
                    state,
                    &parent,
                    id,
                    spec,
                    limits,
                    *expected_graph,
                    *expected_ledger,
                    host.now,
                )?;
                let mut objective = objective.clone();
                objective.source = event_id.clone();
                objective.steering = SteeringRevision::ZERO;
                let child = Task {
                    scope: Scope {
                        workspace: command.workspace.clone(),
                        session: command.session.clone(),
                        task: id.clone(),
                    },
                    root: parent.root.clone(),
                    parent: Some(parent.scope.task),
                    fork_origin: None,
                    revision: Revision::ZERO,
                    steering: SteeringRevision::ZERO,
                    objectives: vec![objective],
                    state: TaskState::Pending,
                    fingerprint: fingerprint.clone(),
                    editing: spec.mode == vcp_domain::agents::ChildMode::IsolatedWrite,
                    required_checks: required_checks.clone(),
                    cause: event_id.clone(),
                    reason: "bounded child assignment".into(),
                    redaction: None,
                };
                child.validate()?;
                put(
                    Collection::Task,
                    id.to_string(),
                    child.revision,
                    serde_json::to_value(child)?,
                    None,
                )?;
                put(
                    Collection::Ledger,
                    parent.root.to_string(),
                    ledger.revision,
                    serde_json::to_value(ledger)?,
                    Some(*expected_ledger),
                )?;
                put(
                    Collection::Projection,
                    vcp_domain::agents::graph_id(&parent.root),
                    graph.revision,
                    serde_json::to_value(graph)?,
                    *expected_graph,
                )?;
                EventKind::ChildGraphChanged
            }
            Command::SubmitChildResult {
                child,
                result: child_result,
                expected_graph,
            } => {
                let parent = task()?;
                if parent.revision != command.expected || parent.steering != command.steering {
                    return Err(vcp_domain::Error::Stale.into());
                }
                let mut graph = crate::agents::graph(state, &parent.scope, &parent.root)?
                    .ok_or(Error::Target)?;
                if graph.revision != *expected_graph {
                    return Err(vcp_domain::Error::Stale.into());
                }
                if graph
                    .children
                    .get(child)
                    .is_none_or(|spec| spec.parent != parent.scope.task)
                {
                    return Err(Error::Target);
                }
                graph
                    .results
                    .entry(child.clone())
                    .or_default()
                    .push(child_result.clone());
                graph.revision = graph.revision.next()?;
                graph.validate()?;
                put(
                    Collection::Projection,
                    vcp_domain::agents::graph_id(&parent.root),
                    graph.revision,
                    serde_json::to_value(graph)?,
                    Some(*expected_graph),
                )?;
                EventKind::ChildGraphChanged
            }
            Command::SetChildDependencies {
                child,
                dependencies,
                expected_graph,
            } => {
                let parent = task()?;
                if parent.revision != command.expected
                    || parent.steering != command.steering
                    || parent.state != TaskState::Running
                {
                    return Err(vcp_domain::Error::Stale.into());
                }
                let mut graph = crate::agents::graph(state, &parent.scope, &parent.root)?
                    .ok_or(Error::Target)?;
                if graph.revision != *expected_graph {
                    return Err(vcp_domain::Error::Stale.into());
                }
                let spec = graph.children.get_mut(child).ok_or(Error::Target)?;
                let current: Task = state
                    .record(Collection::Task, child.as_str(), &command.workspace)?
                    .decode()?;
                if spec.parent != parent.scope.task || current.state != TaskState::Pending {
                    return Err(Error::Target);
                }
                spec.dependencies = dependencies.clone();
                graph.revision = graph.revision.next()?;
                graph.validate()?;
                put(
                    Collection::Projection,
                    vcp_domain::agents::graph_id(&parent.root),
                    graph.revision,
                    serde_json::to_value(graph)?,
                    Some(*expected_graph),
                )?;
                EventKind::ChildGraphChanged
            }
            Command::Transition {
                next: TaskState::Paused,
                reason,
                verification: None,
            } if task()?.state == TaskState::Paused => {
                let current = task()?;
                if current.revision != command.expected || current.steering != command.steering {
                    return Err(vcp_domain::Error::Stale.into());
                }
                if reason.trim().is_empty() || reason.len() > 4096 {
                    return Err(vcp_domain::Error::Invalid("transition reason").into());
                }
                // Repeated explicit pause has its own durable acknowledgement,
                // but does not change the task revision or resume descendants.
                EventKind::TaskTransition
            }
            Command::Transition {
                next,
                reason,
                verification,
            } => {
                let current = task()?;
                let evidence = verification
                    .as_ref()
                    .map(|id| {
                        state
                            .record(Collection::Verification, id.as_str(), &command.workspace)?
                            .decode::<Verification>()
                    })
                    .transpose()?;
                if *next == TaskState::Running && !host.may_execute {
                    return Err(Error::Host);
                }
                if *next == TaskState::Running
                    && current.parent.is_some()
                    && crate::agents::graph(state, &current.scope, &current.root)?.is_some()
                    && !(if current.state == TaskState::Pending {
                        crate::agents::eligibility(state, &current, host.now, true)?
                    } else {
                        crate::agents::eligibility_for_resume(state, &current, host.now, true)?
                    })
                    .is_empty()
                {
                    return Err(Error::Host);
                }
                if *next == TaskState::Completed {
                    for record in state.records.values().filter(|r| {
                        r.workspace == command.workspace && r.collection == Collection::Task
                    }) {
                        let child: Task = record.decode()?;
                        if child.parent.as_ref() == Some(&current.scope.task)
                            && !child.state.terminal()
                        {
                            return Err(vcp_domain::Error::Evidence.into());
                        }
                    }
                    // An evidence author cannot hide a canonical unknown effect.
                    for record in state.records.values().filter(|r| {
                        r.workspace == command.workspace && r.collection == Collection::Effect
                    }) {
                        let effect: Effect = record.decode()?;
                        if effect.scope.task == current.scope.task
                            && !matches!(
                                effect.state,
                                EffectState::Succeeded
                                    | EffectState::Failed
                                    | EffectState::Cancelled
                            )
                        {
                            return Err(vcp_domain::Error::Evidence.into());
                        }
                    }
                    if let Some(evidence) = &evidence {
                        for id in evidence
                            .outputs
                            .iter()
                            .chain(evidence.checks.iter().map(|c| &c.output))
                        {
                            let artifact: ArtifactDescriptor = state
                                .record(Collection::Artifact, id.as_str(), &command.workspace)?
                                .decode()?;
                            if artifact.state != CaptureState::Complete {
                                return Err(vcp_domain::Error::Evidence.into());
                            }
                        }
                    }
                }
                let next = current.transition(
                    &scope()?,
                    command.expected,
                    command.steering,
                    *next,
                    event_id.clone(),
                    reason.clone(),
                    evidence.as_ref(),
                    host.resume.as_ref(),
                )?;
                if next.state == TaskState::Completed {
                    // The verified task and its final turn boundary must share
                    // a commit; a crash cannot strand a completed conversation
                    // in Verifying with no usable fork boundary.
                    for row in state.records.values().filter(|row| {
                        row.collection == Collection::Turn && row.workspace == command.workspace
                    }) {
                        let turn: Turn = row.decode()?;
                        if turn.scope != current.scope
                            || turn.steering != current.steering
                            || turn.state != TurnState::Verifying
                        {
                            continue;
                        }
                        let completed = turn.transition(
                            turn.revision,
                            current.steering,
                            TurnState::Completed,
                            event_id.clone(),
                            "owning task verification accepted".into(),
                            None,
                        )?;
                        put(
                            Collection::Turn,
                            turn.id.to_string(),
                            completed.revision,
                            serde_json::to_value(completed)?,
                            Some(turn.revision),
                        )?;
                    }
                }
                if next.state == TaskState::Cancelled {
                    for row in state.records.values().filter(|r| {
                        r.workspace == command.workspace && r.collection == Collection::Task
                    }) {
                        let child: Task = row.decode()?;
                        if child.state.terminal() || child.scope.task == current.scope.task {
                            continue;
                        }
                        let mut ancestor = child.parent.clone();
                        let mut descendant = false;
                        while let Some(id) = ancestor {
                            if id == current.scope.task {
                                descendant = true;
                                break;
                            }
                            ancestor = state
                                .record(Collection::Task, id.as_str(), &command.workspace)?
                                .decode::<Task>()?
                                .parent;
                        }
                        if descendant {
                            let cancelled = child.transition(
                                &child.scope,
                                child.revision,
                                child.steering,
                                TaskState::Cancelled,
                                event_id.clone(),
                                "ancestor cancelled; liabilities retained".into(),
                                None,
                                None,
                            )?;
                            put(
                                Collection::Task,
                                child.scope.task.to_string(),
                                cancelled.revision,
                                serde_json::to_value(cancelled)?,
                                Some(child.revision),
                            )?;
                        }
                    }
                }
                put(
                    Collection::Task,
                    next.scope.task.to_string(),
                    next.revision,
                    serde_json::to_value(next)?,
                    Some(current.revision),
                )?;
                EventKind::TaskTransition
            }
            Command::Steer { objective } => {
                let current = task()?;
                if current.steering != command.steering {
                    return Err(vcp_domain::Error::Steering.into());
                }
                let mut objective = objective.clone();
                objective.source = event_id.clone();
                let next = current.steer(command.expected, objective)?;
                // A superseded turn retains its old steering identity. Pause
                // it in the same commit before accepting the new objective;
                // later work must start a turn under the new revision.
                for row in state.records.values().filter(|row| {
                    row.collection == Collection::Turn && row.workspace == command.workspace
                }) {
                    let turn: Turn = row.decode()?;
                    if turn.scope != current.scope
                        || turn.steering != current.steering
                        || matches!(
                            turn.state,
                            TurnState::Completed
                                | TurnState::Failed
                                | TurnState::Cancelled
                                | TurnState::Paused
                                | TurnState::Blocked
                                | TurnState::WaitingForInput
                                | TurnState::BudgetExhausted
                                | TurnState::Cancelling
                        )
                    {
                        continue;
                    }
                    let paused = turn.transition(
                        turn.revision,
                        current.steering,
                        TurnState::Paused,
                        event_id.clone(),
                        "superseded by explicit steering".into(),
                        None,
                    )?;
                    put(
                        Collection::Turn,
                        turn.id.to_string(),
                        paused.revision,
                        serde_json::to_value(paused)?,
                        Some(turn.revision),
                    )?;
                }
                put(
                    Collection::Task,
                    next.scope.task.to_string(),
                    next.revision,
                    serde_json::to_value(next)?,
                    Some(current.revision),
                )?;
                EventKind::ObjectiveChanged
            }
            Command::ObserveFingerprint { fingerprint } => {
                let current = task()?;
                if current.steering != command.steering {
                    return Err(vcp_domain::Error::Steering.into());
                }
                let next = current.observe_fingerprint(
                    command.expected,
                    fingerprint.clone(),
                    event_id.clone(),
                )?;
                put(
                    Collection::Task,
                    next.scope.task.to_string(),
                    next.revision,
                    serde_json::to_value(next)?,
                    Some(current.revision),
                )?;
                EventKind::FingerprintObserved
            }
            Command::StartTurn { id, trigger } => {
                let current = task()?;
                if current.revision != command.expected || current.steering != command.steering {
                    return Err(vcp_domain::Error::Stale.into());
                }
                let turn = Turn {
                    redaction: None,
                    id: id.clone(),
                    scope: scope()?,
                    revision: Revision::ZERO,
                    steering: current.steering,
                    state: TurnState::Queued,
                    trigger: trigger.clone(),
                    cause: event_id.clone(),
                    reason: "accepted turn".into(),
                };
                artifacts.push(trigger.clone());
                put(
                    Collection::Turn,
                    id.to_string(),
                    turn.revision,
                    serde_json::to_value(turn)?,
                    None,
                )?;
                EventKind::TurnTransition
            }
            Command::AdvanceTurn { id, next, reason } => {
                let task = task()?;
                let current: Turn = state
                    .record(Collection::Turn, id.as_str(), &command.workspace)?
                    .decode()?;
                if current.scope != scope()? || command.steering != task.steering {
                    return Err(Error::Target);
                }
                if matches!(next, TurnState::RequestingModel | TurnState::ExecutingTools)
                    && (!host.may_execute || !dispatchable(state, &task)?)
                {
                    return Err(Error::Host);
                }
                let turn = current.transition(
                    command.expected,
                    task.steering,
                    *next,
                    event_id.clone(),
                    reason.clone(),
                    host.resume.as_ref(),
                )?;
                put(
                    Collection::Turn,
                    id.to_string(),
                    turn.revision,
                    serde_json::to_value(turn)?,
                    Some(current.revision),
                )?;
                EventKind::TurnTransition
            }
            Command::ProposeEffect {
                id,
                operation_digest,
            } => {
                let task = task()?;
                if task.revision != command.expected
                    || task.steering != command.steering
                    || !hash(operation_digest)
                {
                    return Err(Error::Target);
                }
                let effect = Effect {
                    redaction: None,
                    id: id.clone(),
                    scope: scope()?,
                    revision: Revision::ZERO,
                    steering: task.steering,
                    state: EffectState::Proposed,
                    operation_digest: operation_digest.clone(),
                    execution: None,
                    exit_code: None,
                    observed_changes: vec![],
                    cause: event_id.clone(),
                    reason: "prepared proposal".into(),
                };
                put(
                    Collection::Effect,
                    id.to_string(),
                    effect.revision,
                    serde_json::to_value(effect)?,
                    None,
                )?;
                EventKind::EffectTransition
            }
            Command::AdvanceEffect {
                id,
                next,
                reason,
                execution,
                exit_code,
                observed_changes,
            } => {
                let task = task()?;
                let current: Effect = state
                    .record(Collection::Effect, id.as_str(), &command.workspace)?
                    .decode()?;
                if current.scope != scope()? {
                    return Err(Error::Target);
                }
                if matches!(
                    next,
                    EffectState::Authorized | EffectState::DispatchRecorded | EffectState::Running
                ) && (!host.may_execute
                    || command.steering != task.steering
                    || !dispatchable(state, &task)?)
                {
                    return Err(Error::Host);
                }
                let mut effect = current.transition(
                    command.expected,
                    command.steering,
                    *next,
                    event_id.clone(),
                    reason.clone(),
                )?;
                if matches!(next, EffectState::DispatchRecorded | EffectState::Running)
                    && execution.is_none()
                {
                    return Err(Error::Target);
                }
                if let Some(existing) = &current.execution {
                    if execution.as_ref() != Some(existing) {
                        return Err(Error::Target);
                    }
                }
                effect.execution = execution.clone();
                effect.exit_code = *exit_code;
                effect.observed_changes = observed_changes.clone();
                artifacts.extend(observed_changes.clone());
                put(
                    Collection::Effect,
                    id.to_string(),
                    effect.revision,
                    serde_json::to_value(effect)?,
                    Some(current.revision),
                )?;
                EventKind::EffectTransition
            }
            Command::RecordVerification { verification } => {
                let task = task()?;
                if task.revision != command.expected || verification.scope != scope()? {
                    return Err(Error::Target);
                }
                // Stale observations remain attributable; completion separately
                // checks their fingerprint and steering against current state.
                artifacts.extend(verification.outputs.clone());
                artifacts.extend(verification.checks.iter().map(|c| c.output.clone()));
                put(
                    Collection::Verification,
                    verification.id.to_string(),
                    Revision::ZERO,
                    serde_json::to_value(verification)?,
                    None,
                )?;
                EventKind::VerificationRecorded
            }
            Command::AttachArtifact { descriptor } => {
                if descriptor.spec.scope != scope()? {
                    return Err(Error::Target);
                }
                let key = key(Collection::Artifact, descriptor.spec.id.as_str());
                let previous = state.records.get(&key).map(|r| r.revision);
                if previous.unwrap_or_default() != command.expected {
                    return Err(vcp_domain::Error::Stale.into());
                }
                let revision = previous
                    .map(Revision::next)
                    .transpose()?
                    .unwrap_or_default();
                put(
                    Collection::Artifact,
                    descriptor.spec.id.to_string(),
                    revision,
                    serde_json::to_value(descriptor)?,
                    previous,
                )?;
                artifacts.push(descriptor.spec.id.clone());
                EventKind::ArtifactAttached
            }
            Command::Rebind { binding } => {
                if command.task.is_some() {
                    return Err(Error::Target);
                }
                let current: Workspace = state
                    .record(
                        Collection::Workspace,
                        command.workspace.as_str(),
                        &command.workspace,
                    )?
                    .decode()?;
                let next = current.rebind(command.expected, binding.clone())?;
                put(
                    Collection::Workspace,
                    next.id.to_string(),
                    next.revision,
                    serde_json::to_value(next)?,
                    Some(current.revision),
                )?;
                EventKind::WorkspaceBound
            }
            Command::SetWorkspaceTrust { trust } => {
                if command.task.is_some() {
                    return Err(Error::Target);
                }
                let mut workspace: Workspace = state
                    .record(
                        Collection::Workspace,
                        command.workspace.as_str(),
                        &command.workspace,
                    )?
                    .decode()?;
                if command.expected != workspace.revision {
                    return Err(Error::Target);
                }
                let previous = workspace.revision;
                workspace.revision = workspace.revision.next()?;
                workspace.authority = workspace.authority.next()?;
                workspace.trust = *trust;
                put(
                    Collection::Workspace,
                    workspace.id.to_string(),
                    workspace.revision,
                    serde_json::to_value(workspace)?,
                    Some(previous),
                )?;
                EventKind::AccessChanged
            }
            Command::SetPolicy { policy } => {
                vcp_policy::validate_policy(policy)?;
                if command.task.is_some()
                    || policy.workspace != command.workspace
                    || policy.denials.iter().any(|r| r.origin != RuleOrigin::User)
                {
                    return Err(Error::Target);
                }
                if let Some(row) = state
                    .records
                    .get(&key(Collection::Access, command.workspace.as_str()))
                {
                    let old: AuthorityDocument = row.decode()?;
                    if !matches!(old.data, AuthorityData::Policy { .. }) {
                        return Err(Error::Target);
                    }
                }
                let previous = state
                    .records
                    .get(&key(Collection::Access, command.workspace.as_str()))
                    .map(|r| r.revision);
                let next = previous.map_or(Ok(Revision::ZERO), Revision::next)?;
                if command.expected != previous.unwrap_or(Revision::ZERO)
                    || policy.revision.get() != next.get()
                {
                    return Err(Error::Target);
                }
                let document = AuthorityDocument {
                    document_type: AuthorityFormat::VcpAuthorityV1,
                    schema_version: 1,
                    id: AuthorityId::parse(command.workspace.as_str())?,
                    workspace: command.workspace.clone(),
                    revision: next,
                    data: AuthorityData::Policy {
                        policy: policy.clone(),
                    },
                };
                put(
                    Collection::Access,
                    document.id.to_string(),
                    next,
                    serde_json::to_value(document)?,
                    previous,
                )?;
                // Initial policy revision zero must also invalidate a seal made
                // before any policy existed. Authority epochs cover that change
                // as well as later explicit policy replacements.
                let mut workspace: Workspace = state
                    .record(
                        Collection::Workspace,
                        command.workspace.as_str(),
                        &command.workspace,
                    )?
                    .decode()?;
                let previous = workspace.revision;
                workspace.revision = workspace.revision.next()?;
                workspace.authority = workspace.authority.next()?;
                put(
                    Collection::Workspace,
                    workspace.id.to_string(),
                    workspace.revision,
                    serde_json::to_value(workspace)?,
                    Some(previous),
                )?;
                result = Some(CommandResult::Accepted { revision: next });
                EventKind::AccessChanged
            }
            Command::SetGrant { grant } => {
                vcp_policy::validate_grant(grant)?;
                let policy = crate::policy::current(state, &command.workspace)?;
                let workspace: Workspace = state
                    .record(
                        Collection::Workspace,
                        command.workspace.as_str(),
                        &command.workspace,
                    )?
                    .decode()?;
                if grant.actor != access.actor
                    || grant.scope.workspace() != &command.workspace
                    || grant.origin != RuleOrigin::User
                    || grant.policy != policy.revision
                    || grant.host != workspace.binding.host
                    || grant.binding != workspace.binding.revision
                    || grant.authority != workspace.authority
                    || (!grant.revoked && grant.expires_at <= host.now)
                    || grant.id.as_str() == command.workspace.as_str()
                {
                    return Err(Error::Target);
                }
                match &grant.scope {
                    GrantScope::Session { session, .. } if session != &command.session => {
                        return Err(Error::Target)
                    }
                    GrantScope::Task { scope } if scope.session != command.session => {
                        return Err(Error::Target)
                    }
                    _ => (),
                }
                let previous = state
                    .records
                    .get(&key(Collection::Access, grant.id.as_str()))
                    .map(|r| r.revision);
                if let Some(row) = state
                    .records
                    .get(&key(Collection::Access, grant.id.as_str()))
                {
                    let old: AuthorityDocument = row.decode()?;
                    if !matches!(old.data, AuthorityData::Grant { .. }) {
                        return Err(Error::Target);
                    }
                }
                let next = previous.map_or(Ok(Revision::ZERO), Revision::next)?;
                if command.expected != previous.unwrap_or(Revision::ZERO) || grant.revision != next
                {
                    return Err(Error::Target);
                }
                let document = AuthorityDocument {
                    document_type: AuthorityFormat::VcpAuthorityV1,
                    schema_version: 1,
                    id: AuthorityId::parse(grant.id.as_str())?,
                    workspace: command.workspace.clone(),
                    revision: next,
                    data: AuthorityData::Grant {
                        grant: grant.clone(),
                    },
                };
                put(
                    Collection::Access,
                    document.id.to_string(),
                    next,
                    serde_json::to_value(document)?,
                    previous,
                )?;
                EventKind::AccessChanged
            }
            Command::Ask { approval } => {
                let task = task()?;
                let workspace: Workspace = state
                    .record(
                        Collection::Workspace,
                        command.workspace.as_str(),
                        &command.workspace,
                    )?
                    .decode()?;
                let effect: Effect = state
                    .record(
                        Collection::Effect,
                        approval.effect.as_str(),
                        &command.workspace,
                    )?
                    .decode()?;
                if approval.scope != scope()?
                    || approval.scope != effect.scope
                    || approval.revision != Revision::ZERO
                    || approval.state != ApprovalState::Pending
                    || approval.actor != access.actor
                    || approval.effect_revision != effect.revision
                    || approval.operation_digest != effect.operation_digest
                    || approval.steering != task.steering
                    || command.steering != task.steering
                    || command.expected != effect.revision
                    || approval.policy != host.policy
                    || approval.expires_at <= host.now
                    || approval.controller.as_ref() != Some(&self.controller)
                    || approval.owner_epoch != Some(self.owner)
                    || approval.authority != Some(workspace.authority)
                    || approval.binding != Some(workspace.binding.revision)
                {
                    return Err(Error::Target);
                }
                put(
                    Collection::Approval,
                    approval.id.to_string(),
                    approval.revision,
                    serde_json::to_value(approval)?,
                    None,
                )?;
                if !matches!(task.state, TaskState::Paused | TaskState::WaitingForInput) {
                    let waiting = task.transition(
                        &task.scope,
                        task.revision,
                        task.steering,
                        TaskState::WaitingForInput,
                        event_id.clone(),
                        "prepared operation requires user input".into(),
                        None,
                        None,
                    )?;
                    put(
                        Collection::Task,
                        waiting.scope.task.to_string(),
                        waiting.revision,
                        serde_json::to_value(waiting)?,
                        Some(task.revision),
                    )?;
                }
                result = Some(CommandResult::Accepted {
                    revision: approval.revision,
                });
                EventKind::ApprovalRequested
            }
            Command::Decide {
                id,
                operation_digest,
                effect_revision,
                allow,
            } => {
                let task = task()?;
                let current: Approval = state
                    .record(Collection::Approval, id.as_str(), &command.workspace)?
                    .decode()?;
                let effect: Effect = state
                    .record(
                        Collection::Effect,
                        current.effect.as_str(),
                        &command.workspace,
                    )?
                    .decode()?;
                if current.scope != scope()?
                    || current.actor != access.actor
                    || current.operation_digest != *operation_digest
                    || current.effect_revision != *effect_revision
                    || current.controller.as_ref() != Some(&self.controller)
                    || current.owner_epoch != Some(self.owner)
                {
                    return Err(Error::Target);
                }
                if current.state != ApprovalState::Pending {
                    if (*allow && current.state != ApprovalState::Allowed)
                        || (!*allow && current.state != ApprovalState::Denied)
                    {
                        return Err(Error::Target);
                    }
                    result = Some(CommandResult::Accepted {
                        revision: current.revision,
                    });
                    EventKind::CommandInspected
                } else {
                    let workspace: Workspace = state
                        .record(
                            Collection::Workspace,
                            command.workspace.as_str(),
                            &command.workspace,
                        )?
                        .decode()?;
                    if current.revision != command.expected
                        || current.expires_at <= host.now
                        || current.policy != host.policy
                        || current.steering != task.steering
                        || command.steering != task.steering
                        || effect.operation_digest != *operation_digest
                        || effect.revision != *effect_revision
                        || current.authority != Some(workspace.authority)
                        || current.binding != Some(workspace.binding.revision)
                    {
                        return Err(Error::Target);
                    }
                    if *allow {
                        let policy = crate::policy::current(state, &command.workspace)?;
                        let workspace: Workspace = state
                            .record(
                                Collection::Workspace,
                                command.workspace.as_str(),
                                &command.workspace,
                            )?
                            .decode()?;
                        if policy.revision != current.policy || workspace.trust != Trust::Trusted {
                            return Err(Error::Target);
                        }
                        let grant = Grant {
                            id: GrantId::parse(current.id.as_str())?,
                            actor: current.actor.clone(),
                            scope: GrantScope::Task {
                                scope: current.scope.clone(),
                            },
                            host: workspace.binding.host,
                            binding: workspace.binding.revision,
                            authority: workspace.authority,
                            policy: current.policy,
                            expires_at: current.expires_at,
                            target: GrantTarget::Exact {
                                digest: current.operation_digest.clone(),
                            },
                            origin: RuleOrigin::User,
                            reason: "explicit prepared-operation approval".into(),
                            revoked: false,
                            revision: Revision::ZERO,
                            approval: Some(current.id.clone()),
                        };
                        let document = AuthorityDocument {
                            document_type: AuthorityFormat::VcpAuthorityV1,
                            schema_version: 1,
                            id: AuthorityId::parse(grant.id.as_str())?,
                            workspace: command.workspace.clone(),
                            revision: Revision::ZERO,
                            data: AuthorityData::Grant { grant },
                        };
                        put(
                            Collection::Access,
                            document.id.to_string(),
                            document.revision,
                            serde_json::to_value(document)?,
                            None,
                        )?;
                    }
                    let mut next = current.clone();
                    next.revision = next.revision.next()?;
                    next.state = if *allow {
                        ApprovalState::Allowed
                    } else {
                        ApprovalState::Denied
                    };
                    put(
                        Collection::Approval,
                        id.to_string(),
                        next.revision,
                        serde_json::to_value(next)?,
                        Some(current.revision),
                    )?;
                    EventKind::ApprovalResolved
                }
            }
            Command::Inspect => {
                result = Some(CommandResult::Inspection {
                    task: command.task.as_ref().map(|_| task()).transpose()?,
                });
                EventKind::CommandInspected
            }
        };
        let result = result.unwrap_or(CommandResult::Accepted { revision: accepted });
        let facts=mutations.iter().filter_map(|m|match m{Mutation::Put{record,..}=>Some(serde_json::json!({"collection":record.collection,"id":record.id,"revision":record.revision,"value":record.value})),_=>None}).collect::<Vec<_>>();
        let mut data = serde_json::json!({"schema_version":1,"facts":facts});
        if kind == EventKind::VerificationRecorded {
            // The verification snapshot already binds steering and fingerprints.
            // Preserve the accepted task revision as the remaining typed input
            // identity needed for exact, non-prose failure signatures.
            data["observed_task_revision"] = serde_json::to_value(command.expected)?;
        }
        let event = EventInput {
            metadata: None,
            id: event_id,
            workspace: command.workspace.clone(),
            session: command.session.clone(),
            task: command.task.clone(),
            actor: command.caller.clone(),
            correlation: command.id.clone(),
            causation: None,
            timestamp: host.now,
            kind,
            artifacts,
            data,
        };
        let transaction = Transaction {
            id: TransactionId::new(),
            expected_watermark: state.watermark,
            mutations,
            events: vec![event],
            command: Some(ReceiptInput {
                command: command.id,
                workspace: command.workspace,
                session: command.session,
                digest,
                result,
            }),
        };
        self.store
            .transact(transaction)
            .await?
            .command
            .ok_or(Error::Target)
    }
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
pub fn dispatchable(state: &State, task: &Task) -> Result<bool> {
    if !task.can_dispatch(&task.scope, task.steering, true) {
        return Ok(false);
    }
    let mut parent = task.parent.clone();
    while let Some(id) = parent {
        let ancestor: Task = state
            .record(Collection::Task, id.as_str(), &task.scope.workspace)?
            .decode()?;
        if ancestor.state != TaskState::Running {
            return Ok(false);
        }
        parent = ancestor.parent;
    }
    Ok(true)
}
