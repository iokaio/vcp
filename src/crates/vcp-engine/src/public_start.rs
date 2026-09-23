// SPDX-License-Identifier: Apache-2.0
//! Atomic acceptance of a new run; construction and dispatch remain host-owned.
//!
//! Integration prerequisite, not independently advertised as a capability. The
//! live host must pin its selected root/profile/budget and capture fresh native
//! fingerprint evidence before commit. Accepted means Pending task + Queued turn,
//! never a submitted provider request. Only a fresh Accepted outcome may launch.
use crate::{controller::ControllerToken, public::PublicError, Access, Engine};
use serde::Deserialize;
use vcp_domain::{
    accounting::Ledger,
    artifact::{ArtifactDescriptor, CaptureState, Channel, Omission},
    ids::*,
    revision::*,
    task::{Objective, Task, TaskState, Turn, TurnState},
    verification::Fingerprint,
    workspace::{Scope, Workspace},
};
use vcp_protocol::{
    command::{CommandReceipt, CommandResult},
    event::{EventInput, EventKind},
    methods::{Call, Currency, TurnStart},
};
use vcp_store::contract::{
    key, CanonicalStore, Collection, Mutation, ReceiptInput, Record, State, Transaction,
};

/// Trusted native observations/configuration, never deserialized from the wire.
pub struct StartFacts {
    pub fingerprint: Fingerprint,
    pub editing: bool,
    pub required_checks: Vec<String>,
    pub protected: Micros,
    pub policy: PolicyRevision,
}

pub enum PublicStartAdmission {
    Replay(CommandReceipt),
    Ready(PreparedPublicStart),
}

pub struct PreparedPublicStart {
    request: TurnStart,
    actor: ActorId,
    authority: AuthorityRevision,
    connection: ControllerId,
    token: ControllerToken,
}
impl PreparedPublicStart {
    pub fn request(&self) -> &TurnStart {
        &self.request
    }
}

pub enum PublicStartOutcome {
    Accepted(CommandReceipt),
    Replay(CommandReceipt),
}

/// Current canonical proof only, never an execution or constructor capability.
pub struct AcceptedPublicStart {
    pub task: Task,
    pub turn: Turn,
    pub ledger: Ledger,
    pub trigger: ArtifactDescriptor,
}

const ACCEPTED_REASON: &str = "public run accepted; retained construction and dispatch pending";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedStartBudget {
    pub budget: vcp_protocol::methods::Budget,
    pub accepted_at: Timestamp,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StartMarker {
    schema_version: u32,
    task: vcp_protocol::methods::Id,
    turn: vcp_protocol::methods::Id,
    budget: vcp_protocol::methods::Budget,
}

/// Recover the immutable public root ceilings, not the latest task revision or
/// a newly supplied profile. Only a proved legacy genesis returns None. Missing
/// history must never convert a public run into an unlimited legacy run.
pub fn retained_start_budget(
    state: &State,
    scope: &Scope,
) -> Result<Option<RetainedStartBudget>, PublicError> {
    let unavailable = || PublicError::Unavailable;
    let current: Task = state
        .record(Collection::Task, scope.task.as_str(), &scope.workspace)
        .map_err(|_| unavailable())?
        .decode()
        .map_err(|_| unavailable())?;
    if current.scope != *scope
        || current.root != scope.task
        || current.parent.is_some()
        || current.redaction.is_some()
    {
        return Err(unavailable());
    }
    let workspace: Workspace = state
        .record(
            Collection::Workspace,
            scope.workspace.as_str(),
            &scope.workspace,
        )
        .map_err(|_| unavailable())?
        .decode()
        .map_err(|_| unavailable())?;
    let mut genesis = None;
    for event in state.events.iter().filter(|event| {
        event.event.workspace == scope.workspace
            && event.event.session == scope.session
            && event.event.task.as_ref() == Some(&scope.task)
            && event.event.kind == EventKind::TaskCreated
    }) {
        if genesis.is_some() || event.redaction.is_some() || event.event.data["schema_version"] != 1
        {
            return Err(unavailable());
        }
        for row in state.records.values().filter(|row| {
            row.collection == Collection::Tombstone && row.workspace == scope.workspace
        }) {
            let mask: vcp_domain::retention::RetentionMask =
                row.decode().map_err(|_| unavailable())?;
            mask.validate().map_err(|_| unavailable())?;
            if mask.workspace != scope.workspace
                || mask.deletion > workspace.deletion
                || (mask.session == scope.session
                    && mask.first <= event.sequence
                    && event.sequence <= mask.last)
            {
                return Err(unavailable());
            }
        }
        let mut initial = None;
        for fact in event.event.data["facts"]
            .as_array()
            .ok_or_else(unavailable)?
        {
            if fact["collection"] != "task" || fact["id"] != scope.task.as_str() {
                continue;
            }
            let task: Task =
                serde_json::from_value(fact["value"].clone()).map_err(|_| unavailable())?;
            task.validate().map_err(|_| unavailable())?;
            let revision: Revision =
                serde_json::from_value(fact["revision"].clone()).map_err(|_| unavailable())?;
            if initial.is_some()
                || revision != Revision::ZERO
                || task.revision != Revision::ZERO
                || task.scope != *scope
                || task.root != scope.task
                || task.parent.is_some()
                || task.state != TaskState::Pending
                || task.steering != SteeringRevision::ZERO
                || task.redaction.is_some()
                || task.cause != event.event.id
                || task.objectives.len() != 1
                || task.objectives[0].source != event.event.id
            {
                return Err(unavailable());
            }
            initial = Some(task);
        }
        genesis = Some((event, initial.ok_or_else(unavailable)?));
    }
    let (event, initial) = genesis.ok_or_else(unavailable)?;
    let Some(marker) = event.event.data.get("public_start") else {
        // The immutable public acceptance reason is a second discriminator;
        // removing only the marker cannot silently change the run's class.
        return if initial.reason == ACCEPTED_REASON {
            Err(unavailable())
        } else {
            Ok(None)
        };
    };
    let marker: StartMarker = serde_json::from_value(marker.clone()).map_err(|_| unavailable())?;
    if marker.schema_version != 1
        || marker.task.as_str() != scope.task.as_str()
        || initial.reason != ACCEPTED_REASON
        || initial.fork_origin.is_some()
    {
        return Err(unavailable());
    }
    let objective = &initial.objectives[0];
    let request = TurnStart {
        scope: vcp_protocol::methods::Scope {
            workspace: scope.workspace.to_string().try_into().map_err(invalid)?,
            session: scope.session.to_string().try_into().map_err(invalid)?,
        },
        mutation: vcp_protocol::methods::Mutation {
            command_id: event
                .event
                .correlation
                .to_string()
                .try_into()
                .map_err(invalid)?,
            expected_revision: 0.into(),
            steering_revision: 0.into(),
        },
        task: marker.task,
        turn: marker.turn,
        objective: objective.text.clone(),
        constraints: objective.constraints.clone(),
        acceptance: objective.acceptance.clone(),
        budget: marker.budget,
    };
    let call = Call::TurnStart(request.clone());
    call.validate().map_err(|_| unavailable())?;
    let digest = call
        .digest(event.event.actor.as_str())
        .map_err(|_| unavailable())?;
    let receipt = state
        .command(&scope.workspace, &event.event.correlation, &digest)
        .map_err(|_| unavailable())?
        .ok_or_else(unavailable)?;
    if receipt.watermark != event.watermark
        || receipt.first_event > event.sequence
        || receipt.last_event < event.sequence
        || receipt.result
            != (CommandResult::Accepted {
                revision: Revision::ZERO,
            })
    {
        return Err(unavailable());
    }
    Ok(Some(RetainedStartBudget {
        budget: request.budget,
        accepted_at: event.event.timestamp,
    }))
}

fn invalid<T>(_: T) -> PublicError {
    PublicError::InvalidParameters
}
fn scope(request: &TurnStart) -> Result<Scope, PublicError> {
    Ok(Scope {
        workspace: WorkspaceId::parse(request.scope.workspace.as_str()).map_err(invalid)?,
        session: SessionId::parse(request.scope.session.as_str()).map_err(invalid)?,
        task: TaskId::parse(request.task.as_str()).map_err(invalid)?,
    })
}
fn available(state: &State, request: &TurnStart) -> Result<(), PublicError> {
    for (collection, id) in [
        (Collection::Task, request.task.as_str()),
        (Collection::Ledger, request.task.as_str()),
        (Collection::Turn, request.turn.as_str()),
    ] {
        if state.records.contains_key(&key(collection, id)) {
            return Err(PublicError::StaleState);
        }
    }
    Ok(())
}

impl<S: CanonicalStore> Engine<S> {
    /// Revalidate an already accepted, still-pristine run. A lifecycle ticket
    /// must additionally prove that this process owns the fresh acceptance;
    /// calling this on a replay must never mint a constructor capability.
    pub fn check_accepted_public_start(
        &self,
        request: &TurnStart,
        receipt: &CommandReceipt,
        access: &Access,
        connection: &ControllerId,
        token: &ControllerToken,
    ) -> Result<AcceptedPublicStart, PublicError> {
        self.check_controller(access, connection, token)
            .map_err(|_| PublicError::Access)?;
        Call::TurnStart(request.clone())
            .validate()
            .map_err(invalid)?;
        let selected = scope(request)?;
        if selected.workspace != access.workspace || selected.session != access.session {
            return Err(PublicError::Access);
        }
        let command = CommandId::parse(request.mutation.command_id.as_str()).map_err(invalid)?;
        let digest = Call::TurnStart(request.clone())
            .digest(access.actor.as_str())
            .map_err(invalid)?;
        let state = self.store().state();
        if state
            .command(&access.workspace, &command, &digest)
            .map_err(|_| PublicError::CommandConflict)?
            .as_ref()
            != Some(receipt)
        {
            return Err(PublicError::Unavailable);
        }
        let record = |collection, id: &str| {
            state
                .record(collection, id, &access.workspace)
                .map_err(|_| PublicError::Unavailable)
        };
        let task_row = record(Collection::Task, selected.task.as_str())?;
        let turn_row = record(Collection::Turn, request.turn.as_str())?;
        let ledger_row = record(Collection::Ledger, selected.task.as_str())?;
        let task: Task = task_row.decode().map_err(|_| PublicError::Unavailable)?;
        let turn: Turn = turn_row.decode().map_err(|_| PublicError::Unavailable)?;
        let ledger: Ledger = ledger_row.decode().map_err(|_| PublicError::Unavailable)?;
        let trigger_row = record(Collection::Artifact, turn.trigger.as_str())?;
        let trigger: ArtifactDescriptor =
            trigger_row.decode().map_err(|_| PublicError::Unavailable)?;
        task.validate().map_err(|_| PublicError::Unavailable)?;
        ledger.validate().map_err(|_| PublicError::Unavailable)?;
        trigger.validate().map_err(|_| PublicError::Unavailable)?;
        let cap: u64 = request
            .budget
            .cap_micros
            .as_str()
            .parse()
            .map_err(invalid)?;
        if task.scope != selected
            || task.root != selected.task
            || task.parent.is_some()
            || task.fork_origin.is_some()
            || task.state != TaskState::Pending
            || task.revision != Revision::ZERO
            || task.steering != SteeringRevision::ZERO
            || task.redaction.is_some()
            || task.objectives.len() != 1
            || task.objectives[0].text != request.objective
            || task.objectives[0].constraints != request.constraints
            || task.objectives[0].acceptance != request.acceptance
            || task.objectives[0].source != task.cause
            || turn.scope != selected
            || turn.id.as_str() != request.turn.as_str()
            || turn.state != TurnState::Queued
            || turn.revision != Revision::ZERO
            || turn.steering != SteeringRevision::ZERO
            || turn.redaction.is_some()
            || ledger.scope != selected
            || ledger.revision != Revision::ZERO
            || ledger.policy != PolicyRevision::ZERO
            || ledger.currency.code() != "USD"
            || ledger.cap.get() != cap
            || ledger.protected.get() > cap
            || ledger.settled != Micros::ZERO
            || ledger.active != Micros::ZERO
            || ledger.unresolved != Micros::ZERO
            || !ledger.allocations.is_empty()
            || ledger.daily.is_some()
            || ledger.overrun
            || trigger.spec.scope != selected
            || trigger.spec.id != turn.trigger
            || trigger.state != CaptureState::Complete
            || trigger.spec.schema != "coding-turn-input/1"
            || trigger.spec.channel != Channel::Evidence
            || trigger.sha256 != vcp_protocol::digest_bytes(request.objective.as_bytes())
            || trigger.length.get() != request.objective.len() as u64
            || trigger.spec.omissions.iter().any(|omission| {
                !matches!(
                    omission,
                    Omission::AuthenticationHeaders | Omission::RecoveryMaterial
                )
            })
        {
            return Err(PublicError::StaleState);
        }
        for (row, kind, cause) in [
            (task_row, EventKind::TaskCreated, Some(&task.cause)),
            (turn_row, EventKind::TurnTransition, Some(&turn.cause)),
            (ledger_row, EventKind::AccountingResolved, None),
            (trigger_row, EventKind::ArtifactAttached, None),
        ] {
            if row.revision != Revision::ZERO {
                return Err(PublicError::StaleState);
            }
            let collection = serde_json::to_value(row.collection).map_err(invalid)?;
            let revision = serde_json::to_value(Revision::ZERO).map_err(invalid)?;
            if !state.events.iter().any(|event| {
                event.redaction.is_none()
                    && event.watermark == receipt.watermark
                    && event.event.workspace == selected.workspace
                    && event.event.session == selected.session
                    && event.event.task.as_ref() == Some(&selected.task)
                    && event.event.correlation == command
                    && event.event.kind == kind
                    && cause.is_none_or(|cause| event.event.id == *cause)
                    && event.event.data["schema_version"] == 1
                    && event.event.data["facts"].as_array().is_some_and(|facts| {
                        facts.iter().any(|fact| {
                            fact["collection"] == collection
                                && fact["id"] == row.id
                                && fact["value"] == row.value
                                && fact["revision"] == revision
                        })
                    })
            }) {
                return Err(PublicError::Unavailable);
            }
        }
        if crate::public::current_public_turn(state, &selected)?.as_ref() != Some(&turn) {
            return Err(PublicError::StaleState);
        }
        Ok(AcceptedPublicStart {
            task,
            turn,
            ledger,
            trigger,
        })
    }

    /// Current control and scope precede receipt lookup. A duplicate cannot
    /// create a task, capture another trigger or re-enter retained construction.
    pub fn prepare_public_start(
        &self,
        request: TurnStart,
        access: &Access,
        connection: &ControllerId,
        token: &ControllerToken,
    ) -> Result<PublicStartAdmission, PublicError> {
        self.check_controller(access, connection, token)
            .map_err(|_| PublicError::Access)?;
        let call = Call::TurnStart(request.clone());
        call.validate().map_err(invalid)?;
        if vcp_protocol::canonical_bytes(&call).map_err(invalid)?.len()
            > vcp_protocol::methods::MAX_METHOD_BYTES
        {
            return Err(PublicError::InvalidParameters);
        }
        let selected = scope(&request)?;
        if selected.workspace != access.workspace || selected.session != access.session {
            return Err(PublicError::Access);
        }
        let id = CommandId::parse(request.mutation.command_id.as_str()).map_err(invalid)?;
        let digest = call.digest(access.actor.as_str()).map_err(invalid)?;
        if let Some(receipt) = self
            .store()
            .state()
            .command(&access.workspace, &id, &digest)
            .map_err(|_| PublicError::CommandConflict)?
        {
            return Ok(PublicStartAdmission::Replay(receipt));
        }
        if request.mutation.expected_revision.as_str() != "0"
            || request.mutation.steering_revision.as_str() != "0"
        {
            return Err(PublicError::StaleState);
        }
        available(self.store().state(), &request)?;
        Ok(PublicStartAdmission::Ready(PreparedPublicStart {
            request,
            actor: access.actor.clone(),
            authority: access.authority,
            connection: connection.clone(),
            token: token.clone(),
        }))
    }

    /// Capture bytes before this call under the trusted host, then attach their
    /// exact descriptor in the same transaction as all canonical run records.
    /// The spool may retain an unreferenced complete capture if commit fails;
    /// canonical state never contains a partially accepted run.
    pub async fn commit_public_start(
        &mut self,
        prepared: PreparedPublicStart,
        access: &Access,
        facts: &StartFacts,
        trigger: &ArtifactDescriptor,
        now: Timestamp,
    ) -> Result<PublicStartOutcome, PublicError> {
        if prepared.actor != access.actor || prepared.authority != access.authority {
            return Err(PublicError::Access);
        }
        let request = match self.prepare_public_start(
            prepared.request,
            access,
            &prepared.connection,
            &prepared.token,
        )? {
            PublicStartAdmission::Replay(receipt) => {
                return Ok(PublicStartOutcome::Replay(receipt))
            }
            PublicStartAdmission::Ready(current) => current.request,
        };
        let policy = crate::policy::optional(self.store().state(), &access.workspace)
            .map_err(|_| PublicError::Unavailable)?
            .map_or(PolicyRevision::ZERO, |policy| policy.revision);
        if policy != facts.policy {
            return Err(PublicError::StaleState);
        }
        let transaction = transaction(self.store().state(), &request, access, facts, trigger, now)?;
        let receipt = self
            .store_mut()
            .transact(transaction)
            .await
            .map_err(|error| match error {
                vcp_store::Error::Conflict(_) => PublicError::StaleState,
                _ => PublicError::OutcomeUnknown,
            })?
            .command
            .ok_or(PublicError::OutcomeUnknown)?;
        Ok(PublicStartOutcome::Accepted(receipt))
    }
}

fn transaction(
    state: &State,
    request: &TurnStart,
    access: &Access,
    facts: &StartFacts,
    trigger: &ArtifactDescriptor,
    now: Timestamp,
) -> Result<Transaction, PublicError> {
    available(state, request)?;
    let selected = scope(request)?;
    let command = CommandId::parse(request.mutation.command_id.as_str()).map_err(invalid)?;
    let turn_id = TurnId::parse(request.turn.as_str()).map_err(invalid)?;
    let cap: u64 = request
        .budget
        .cap_micros
        .as_str()
        .parse()
        .map_err(invalid)?;
    facts.fingerprint.validate().map_err(invalid)?;
    trigger.validate().map_err(invalid)?;
    if facts.protected.get() > cap
        || request.budget.currency != Currency::Usd
        || trigger.spec.scope != selected
        || trigger.state != CaptureState::Complete
        || trigger.spec.channel != Channel::Evidence
        || trigger.spec.schema != "coding-turn-input/1"
        || trigger.length.get() != request.objective.len() as u64
        || trigger.sha256 != vcp_protocol::digest_bytes(request.objective.as_bytes())
        || trigger.spec.omissions.iter().any(|omission| {
            !matches!(
                omission,
                Omission::AuthenticationHeaders | Omission::RecoveryMaterial
            )
        })
        || state
            .records
            .contains_key(&key(Collection::Artifact, trigger.spec.id.as_str()))
    {
        return Err(PublicError::InvalidParameters);
    }
    let created = EventId::new();
    let queued = EventId::new();
    let task = Task {
        scope: selected.clone(),
        root: selected.task.clone(),
        parent: None,
        fork_origin: None,
        revision: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        objectives: vec![Objective {
            text: request.objective.clone(),
            constraints: request.constraints.clone(),
            acceptance: request.acceptance.clone(),
            source: created.clone(),
            steering: SteeringRevision::ZERO,
        }],
        state: TaskState::Pending,
        fingerprint: facts.fingerprint.clone(),
        editing: facts.editing,
        required_checks: facts.required_checks.clone(),
        cause: created.clone(),
        reason: ACCEPTED_REASON.into(),
        redaction: None,
    };
    task.validate().map_err(invalid)?;
    let turn = Turn {
        redaction: None,
        id: turn_id,
        scope: selected.clone(),
        revision: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        state: TurnState::Queued,
        trigger: trigger.spec.id.clone(),
        cause: queued.clone(),
        reason: "public turn accepted; execution not yet submitted".into(),
    };
    // Same initial accounting values as budget::initialize; no reservation,
    // inherited spend, pending liability or execution allowance is fabricated.
    let ledger = Ledger {
        schema_version: 1,
        scope: selected.clone(),
        revision: Revision::ZERO,
        policy: PolicyRevision::ZERO,
        currency: "USD".to_owned().try_into().map_err(invalid)?,
        cap: Micros::new(cap),
        protected: facts.protected,
        settled: Micros::ZERO,
        active: Micros::ZERO,
        unresolved: Micros::ZERO,
        allocations: Default::default(),
        daily: None,
        overrun: false,
    };
    ledger.validate().map_err(invalid)?;
    let records = vec![
        Record::typed(
            Collection::Task,
            selected.task.as_str(),
            selected.workspace.clone(),
            Revision::ZERO,
            &task,
        )
        .map_err(invalid)?,
        Record::typed(
            Collection::Turn,
            turn.id.as_str(),
            selected.workspace.clone(),
            Revision::ZERO,
            &turn,
        )
        .map_err(invalid)?,
        Record::typed(
            Collection::Ledger,
            selected.task.as_str(),
            selected.workspace.clone(),
            Revision::ZERO,
            &ledger,
        )
        .map_err(invalid)?,
        Record::typed(
            Collection::Artifact,
            trigger.spec.id.as_str(),
            selected.workspace.clone(),
            Revision::ZERO,
            trigger,
        )
        .map_err(invalid)?,
    ];
    let fact = |record: &Record| {
        serde_json::json!({"collection":record.collection,
        "id":record.id,"revision":record.revision,"value":record.value})
    };
    let event = |id, kind, data| EventInput {
        id,
        workspace: selected.workspace.clone(),
        session: selected.session.clone(),
        task: Some(selected.task.clone()),
        actor: access.actor.clone(),
        correlation: command.clone(),
        causation: None,
        timestamp: now,
        kind,
        artifacts: vec![trigger.spec.id.clone()],
        data,
        metadata: None,
    };
    let events = vec![
        event(
            created,
            EventKind::TaskCreated,
            serde_json::json!({"schema_version":1,
            "facts":[fact(&records[0])], "public_start":{"schema_version":1,
                "task":request.task,"turn":request.turn,"budget":request.budget}}),
        ),
        event(
            EventId::new(),
            EventKind::ArtifactAttached,
            serde_json::json!({"schema_version":1,"facts":[fact(&records[3])]}),
        ),
        event(
            EventId::new(),
            EventKind::AccountingResolved,
            serde_json::json!({"schema_version":1,
            "facts":[fact(&records[2])],"ledger":ledger,"reason":"root accounting initialized"}),
        ),
        event(
            queued,
            EventKind::TurnTransition,
            serde_json::json!({"schema_version":1,"facts":[fact(&records[1])]}),
        ),
    ];
    Ok(Transaction {
        id: TransactionId::new(),
        expected_watermark: state.watermark,
        mutations: records
            .into_iter()
            .map(|record| Mutation::Put {
                expected: None,
                record,
            })
            .collect(),
        events,
        command: Some(ReceiptInput {
            command,
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            digest: Call::TurnStart(request.clone())
                .digest(access.actor.as_str())
                .map_err(invalid)?,
            result: CommandResult::Accepted {
                revision: Revision::ZERO,
            },
        }),
    })
}

#[cfg(test)]
mod tests;
