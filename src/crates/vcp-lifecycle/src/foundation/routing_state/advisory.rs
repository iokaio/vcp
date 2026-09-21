// SPDX-License-Identifier: Apache-2.0
//! Immutable canonical records for bounded escalation-advisory requests/results.
//! A stale result remains inspectable historical evidence and is never accepted
//! as current advice.
use super::{commit, read, row, Result};
use serde::{Deserialize, Serialize};
use vcp_domain::{
    accounting::{Attempt, CostQuote, RequestRole, ReservationState},
    artifact::{ArtifactDescriptor, CaptureState, Channel},
    task::Task,
    workspace::Workspace,
    *,
};
use vcp_memory::access::Access;
use vcp_models::decision::{Binding, Outcome, Prepared, Purpose, QualifiedEvaluator, Request};
use vcp_protocol::{canonical_bytes, digest_bytes};
use vcp_store::{
    contract::{key, Collection, Mutation},
    Store,
};

const REQUEST_DOCUMENT: &str = "vcp_escalation_advisory_request_v1";
const RESULT_DOCUMENT: &str = "vcp_escalation_advisory_result_v1";
const SCHEDULE_DOCUMENT: &str = "vcp_escalation_advisory_schedule_v1";
const ACCOUNTING_DOCUMENT: &str = "vcp_escalation_advisory_accounting_v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestRecord {
    pub document_type: String,
    pub document_version: u32,
    pub id: String,
    pub workspace: WorkspaceId,
    pub task: TaskId,
    pub recorded_at: Timestamp,
    pub request_digest: String,
    pub transport_commitment: String,
    pub request: Request,
    pub evaluator: QualifiedEvaluator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    AcceptedCurrent,
    HistoricalStale,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultRecord {
    pub document_type: String,
    pub document_version: u32,
    pub id: String,
    pub request_id: String,
    pub workspace: WorkspaceId,
    pub task: TaskId,
    pub recorded_at: Timestamp,
    pub request_digest: String,
    pub transport_commitment: String,
    pub outcome_digest: String,
    pub outcome: serde_json::Value,
    pub disposition: Disposition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cancellation {
    Paused,
    TaskNotRunning,
    StaleInput,
    Deadline,
    CallerCancelled,
    Interrupted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScheduleState {
    Pending,
    Claimed {
        claimant: CommandId,
        claimed_at: Timestamp,
    },
    Cancelled {
        reason: Cancellation,
        cancelled_at: Timestamp,
    },
    Completed {
        result_id: String,
        completed_at: Timestamp,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleRecord {
    pub document_type: String,
    pub document_version: u32,
    pub id: String,
    pub revision: Revision,
    pub request_id: String,
    pub workspace: WorkspaceId,
    pub task: TaskId,
    pub request_digest: String,
    pub transport_commitment: String,
    pub deadline: Timestamp,
    pub created_at: Timestamp,
    pub state: ScheduleState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Claim {
    Updated(ScheduleRecord),
    Existing(ScheduleRecord),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountingRecord {
    pub document_type: String,
    pub document_version: u32,
    pub id: String,
    pub request_id: String,
    pub workspace: WorkspaceId,
    pub task: TaskId,
    pub claimant: CommandId,
    pub schedule_revision: Revision,
    pub attempt: AttemptId,
    pub reservation: ReservationId,
    pub request_artifact: ArtifactId,
    pub request_artifact_digest: String,
    pub quote: CostQuote,
    pub bound_at: Timestamp,
}

/// Persist the exact prepared advisory input before a caller schedules transport.
/// The ID derives from the qualified transport commitment, so retrying the same
/// input is idempotent while a different preparation cannot reuse its identity.
pub async fn record_request(
    store: &mut Store,
    access: &Access,
    command: CommandId,
    prepared: &Prepared,
    current: &Binding,
    now: Timestamp,
) -> Result<RequestRecord> {
    let request = prepared.request();
    if request.purpose != Purpose::Escalation || prepared.evaluator().purpose != Purpose::Escalation
    {
        return Err("canonical advisory records accept only escalation requests".into());
    }
    request.validate(now).map_err(super::err)?;
    validate_current(store, access, current)?;
    if &request.binding != current {
        return Err("advisory request input is no longer current".into());
    }
    let transport_commitment = prepared.digest().to_owned();
    if !valid_digest(&transport_commitment) {
        return Err("invalid advisory transport commitment".into());
    }
    let request_digest = digest_bytes(&canonical_bytes(request).map_err(super::err)?);
    let id = request_name(&access.workspace, &transport_commitment);
    if let Some(existing) = read::<RequestRecord>(store, access, &id)? {
        authorize_request(access, &existing)?;
        validate_request_record(&existing)?;
        if existing.request_digest != request_digest
            || existing.transport_commitment != transport_commitment
        {
            return Err("advisory request identity was reused with different input".into());
        }
        return Ok(existing);
    }
    if !access.write {
        return Err("advisory request write access denied".into());
    }
    let record = RequestRecord {
        document_type: REQUEST_DOCUMENT.into(),
        document_version: 1,
        id: id.clone(),
        workspace: request.binding.scope.workspace.clone(),
        task: request.binding.scope.task.clone(),
        recorded_at: now,
        request_digest,
        transport_commitment,
        request: request.clone(),
        evaluator: prepared.evaluator().clone(),
    };
    validate_request_record(&record)?;
    let mut stored = row(access, id, Revision::ZERO, &record)?;
    stored
        .references
        .insert(key(Collection::Task, record.task.as_str()));
    commit(
        store,
        access,
        vec![Mutation::Put {
            record: stored,
            expected: None,
        }],
        command,
        now,
    )
    .await?;
    Ok(record)
}

/// Persist one decoded outcome for a canonical request. Results arriving after a
/// deadline or input change are retained with `HistoricalStale` disposition.
/// They must not be passed to the advisory consumer as current advice.
pub async fn record_result(
    store: &mut Store,
    access: &Access,
    command: CommandId,
    request_id: &str,
    outcome: &Outcome,
    current: &Binding,
    now: Timestamp,
) -> Result<ResultRecord> {
    validate_current(store, access, current)?;
    let request = load_request(store, access, request_id)?;
    let outcome_value = serde_json::to_value(outcome).map_err(super::err)?;
    let outcome_digest = digest_bytes(&canonical_bytes(&outcome_value).map_err(super::err)?);
    validate_outcome(&request, outcome)?;
    let disposition = if current == &request.request.binding && now < request.request.deadline {
        Disposition::AcceptedCurrent
    } else {
        Disposition::HistoricalStale
    };
    let id = result_name(&request.id);
    if let Some(existing) = read::<ResultRecord>(store, access, &id)? {
        authorize_result(access, &existing)?;
        validate_result_record(&existing)?;
        if existing.request_id != request.id
            || existing.workspace != request.workspace
            || existing.task != request.task
            || existing.request_digest != request.request_digest
            || existing.transport_commitment != request.transport_commitment
            || existing.outcome_digest != outcome_digest
        {
            return Err("advisory result identity was reused with different output".into());
        }
        return Ok(existing);
    }
    if !access.write {
        return Err("advisory result write access denied".into());
    }
    let result = ResultRecord {
        document_type: RESULT_DOCUMENT.into(),
        document_version: 1,
        id: id.clone(),
        request_id: request.id.clone(),
        workspace: request.workspace.clone(),
        task: request.task.clone(),
        recorded_at: now,
        request_digest: request.request_digest.clone(),
        transport_commitment: request.transport_commitment.clone(),
        outcome_digest,
        outcome: outcome_value,
        disposition,
    };
    validate_result_record(&result)?;
    let mut stored = row(access, id, Revision::ZERO, &result)?;
    stored
        .references
        .insert(key(Collection::Task, result.task.as_str()));
    stored
        .references
        .insert(key(Collection::Projection, &result.request_id));
    commit(
        store,
        access,
        vec![Mutation::Put {
            record: stored,
            expected: None,
        }],
        command,
        now,
    )
    .await?;
    Ok(result)
}

pub fn load_request(store: &Store, access: &Access, id: &str) -> Result<RequestRecord> {
    let record =
        read::<RequestRecord>(store, access, id)?.ok_or("canonical advisory request not found")?;
    authorize_request(access, &record)?;
    validate_request_record(&record)?;
    Ok(record)
}

pub fn load_result(store: &Store, access: &Access, request_id: &str) -> Result<ResultRecord> {
    let request = load_request(store, access, request_id)?;
    let record = read::<ResultRecord>(store, access, &result_name(request_id))?
        .ok_or("canonical advisory result not found")?;
    authorize_result(access, &record)?;
    validate_result_record(&record)?;
    if record.request_id != request_id
        || record.workspace != request.workspace
        || record.task != request.task
        || record.request_digest != request.request_digest
        || record.transport_commitment != request.transport_commitment
    {
        return Err("invalid canonical advisory result".into());
    }
    Ok(record)
}

/// Create the single caller-owned scheduling lease for a canonical request.
/// This records no provider attempt and performs no transport.
pub async fn schedule(
    store: &mut Store,
    access: &Access,
    command: CommandId,
    request_id: &str,
    current: &Binding,
    now: Timestamp,
) -> Result<ScheduleRecord> {
    let request = load_request(store, access, request_id)?;
    let task = current_task(store, access, current)?;
    if current != &request.request.binding {
        return Err("advisory schedule input is no longer current".into());
    }
    if now >= request.request.deadline {
        return Err("advisory schedule deadline passed".into());
    }
    if task.state != vcp_domain::task::TaskState::Running {
        return Err("advisory schedule requires a running task".into());
    }
    let id = schedule_name(&request.id);
    if let Some(existing) = read::<ScheduleRecord>(store, access, &id)? {
        validate_schedule(&existing, &request)?;
        return Ok(existing);
    }
    if !access.write {
        return Err("advisory schedule write access denied".into());
    }
    let schedule = ScheduleRecord {
        document_type: SCHEDULE_DOCUMENT.into(),
        document_version: 1,
        id: id.clone(),
        revision: Revision::ZERO,
        request_id: request.id.clone(),
        workspace: request.workspace.clone(),
        task: request.task.clone(),
        request_digest: request.request_digest.clone(),
        transport_commitment: request.transport_commitment.clone(),
        deadline: request.request.deadline,
        created_at: now,
        state: ScheduleState::Pending,
    };
    put_schedule(store, access, command, schedule, None, now).await
}

/// Claim a pending lease. `Existing` never authorizes another send: recovery must
/// settle or explicitly interrupt a prior claim before creating a new request.
pub async fn claim(
    store: &mut Store,
    access: &Access,
    command: CommandId,
    request_id: &str,
    claimant: CommandId,
    current: &Binding,
    now: Timestamp,
) -> Result<Claim> {
    let request = load_request(store, access, request_id)?;
    let mut schedule = load_schedule(store, access, request_id)?;
    validate_schedule(&schedule, &request)?;
    if schedule.state != ScheduleState::Pending {
        return Ok(Claim::Existing(schedule));
    }
    let task = current_task(store, access, current)?;
    let cancellation = cancellation(&request, current, &task, now);
    schedule.state = match cancellation {
        Some(reason) => ScheduleState::Cancelled {
            reason,
            cancelled_at: now,
        },
        None => ScheduleState::Claimed {
            claimant,
            claimed_at: now,
        },
    };
    let expected = schedule.revision;
    schedule.revision = schedule.revision.next().map_err(super::err)?;
    let schedule = put_schedule(store, access, command, schedule, Some(expected), now).await?;
    Ok(Claim::Updated(schedule))
}

/// Recheck the canonical task immediately before transport. A pause, non-running
/// state, changed input or deadline atomically closes the lease without a send.
pub async fn revalidate_dispatch(
    store: &mut Store,
    access: &Access,
    command: CommandId,
    request_id: &str,
    claimant: &CommandId,
    current: &Binding,
    now: Timestamp,
) -> Result<ScheduleRecord> {
    let request = load_request(store, access, request_id)?;
    let mut schedule = load_schedule(store, access, request_id)?;
    validate_schedule(&schedule, &request)?;
    match &schedule.state {
        ScheduleState::Claimed {
            claimant: owner, ..
        } if owner == claimant => {}
        ScheduleState::Claimed { .. } => {
            return Err("advisory dispatch requires its exact active claim".into())
        }
        _ => return Ok(schedule),
    }
    let task = current_task(store, access, current)?;
    let Some(reason) = cancellation(&request, current, &task, now) else {
        return Ok(schedule);
    };
    schedule.state = ScheduleState::Cancelled {
        reason,
        cancelled_at: now,
    };
    let expected = schedule.revision;
    schedule.revision = schedule.revision.next().map_err(super::err)?;
    put_schedule(store, access, command, schedule, Some(expected), now).await
}

/// Close a claimed lease when the caller-owned future is cancelled or interrupted.
pub async fn interrupt(
    store: &mut Store,
    access: &Access,
    command: CommandId,
    request_id: &str,
    claimant: &CommandId,
    interrupted: bool,
    now: Timestamp,
) -> Result<ScheduleRecord> {
    let request = load_request(store, access, request_id)?;
    let mut schedule = load_schedule(store, access, request_id)?;
    validate_schedule(&schedule, &request)?;
    let reason = if interrupted {
        Cancellation::Interrupted
    } else {
        Cancellation::CallerCancelled
    };
    match &schedule.state {
        ScheduleState::Claimed {
            claimant: owner, ..
        } if owner == claimant => {}
        ScheduleState::Cancelled {
            reason: existing, ..
        } if *existing == reason => return Ok(schedule),
        _ => return Err("only the active advisory claimant can interrupt its lease".into()),
    }
    schedule.state = ScheduleState::Cancelled {
        reason,
        cancelled_at: now,
    };
    let expected = schedule.revision;
    schedule.revision = schedule.revision.next().map_err(super::err)?;
    put_schedule(store, access, command, schedule, Some(expected), now).await
}

/// Complete a claim only from its accepted-current canonical result, rechecking
/// current inputs at completion. A stored disposition is historical evidence,
/// not permission to consume advice after the task or its inputs change.
pub async fn complete(
    store: &mut Store,
    access: &Access,
    command: CommandId,
    request_id: &str,
    claimant: &CommandId,
    current: &Binding,
    now: Timestamp,
) -> Result<ScheduleRecord> {
    let request = load_request(store, access, request_id)?;
    let result = load_result(store, access, request_id)?;
    if result.disposition != Disposition::AcceptedCurrent {
        return Err("historical advisory result cannot complete a schedule".into());
    }
    let task = current_task(store, access, current)?;
    if cancellation(&request, current, &task, now).is_some() || now >= request.evaluator.valid_until
    {
        return Err("advisory completion inputs are no longer current".into());
    }
    let mut schedule = load_schedule(store, access, request_id)?;
    validate_schedule(&schedule, &request)?;
    if matches!(
        &schedule.state,
        ScheduleState::Completed { result_id, .. } if result_id == &result.id
    ) {
        return Ok(schedule);
    }
    if !matches!(
        &schedule.state,
        ScheduleState::Claimed {
            claimant: owner,
            ..
        } if owner == claimant
    ) {
        return Err("advisory completion requires its exact active claim".into());
    }
    schedule.state = ScheduleState::Completed {
        result_id: result.id,
        completed_at: now,
    };
    let expected = schedule.revision;
    schedule.revision = schedule.revision.next().map_err(super::err)?;
    put_schedule(store, access, command, schedule, Some(expected), now).await
}

pub fn load_schedule(store: &Store, access: &Access, request_id: &str) -> Result<ScheduleRecord> {
    let request = load_request(store, access, request_id)?;
    let record = read::<ScheduleRecord>(store, access, &schedule_name(request_id))?
        .ok_or("canonical advisory schedule not found")?;
    validate_schedule(&record, &request)?;
    Ok(record)
}

/// Bind a newly claimed lease to an ordinary canonical helper reservation. The
/// budget service remains the sole owner of submission, charge and settlement.
pub async fn bind_attempt(
    store: &mut Store,
    access: &Access,
    command: CommandId,
    request_id: &str,
    claimant: &CommandId,
    attempt_id: &AttemptId,
    now: Timestamp,
) -> Result<AccountingRecord> {
    let request = load_request(store, access, request_id)?;
    let schedule = load_schedule(store, access, request_id)?;
    if !matches!(
        &schedule.state,
        ScheduleState::Claimed {
            claimant: owner,
            ..
        } if owner == claimant
    ) {
        return Err("advisory accounting requires its exact active claim".into());
    }
    let attempt = canonical_attempt(store, &request, attempt_id)?;
    if attempt.phase != ReservationState::Created || attempt.send_intent.is_some() {
        return Err("advisory accounting must bind before submission".into());
    }
    let artifact = canonical_request_artifact(store, &attempt)?;
    let id = accounting_name(request_id);
    if let Some(existing) = read::<AccountingRecord>(store, access, &id)? {
        validate_accounting(&existing, &request, &schedule, &attempt, &artifact)?;
        if existing.claimant != *claimant {
            return Err("advisory accounting claimant changed".into());
        }
        return Ok(existing);
    }
    if !access.write {
        return Err("advisory accounting write access denied".into());
    }
    let accounting = AccountingRecord {
        document_type: ACCOUNTING_DOCUMENT.into(),
        document_version: 1,
        id: id.clone(),
        request_id: request.id.clone(),
        workspace: request.workspace.clone(),
        task: request.task.clone(),
        claimant: claimant.clone(),
        schedule_revision: schedule.revision,
        attempt: attempt.id.clone(),
        reservation: attempt.reservation.clone(),
        request_artifact: artifact.spec.id.clone(),
        request_artifact_digest: artifact.sha256.clone(),
        quote: attempt.quote.clone(),
        bound_at: now,
    };
    validate_accounting(&accounting, &request, &schedule, &attempt, &artifact)?;
    let mut stored = row(access, id, Revision::ZERO, &accounting)?;
    for reference in [
        key(Collection::Task, accounting.task.as_str()),
        key(Collection::Projection, &accounting.request_id),
        key(Collection::Projection, &schedule.id),
        key(Collection::Attempt, accounting.attempt.as_str()),
        key(Collection::Reservation, accounting.reservation.as_str()),
        key(Collection::Artifact, accounting.request_artifact.as_str()),
    ] {
        stored.references.insert(reference);
    }
    commit(
        store,
        access,
        vec![Mutation::Put {
            record: stored,
            expected: None,
        }],
        command,
        now,
    )
    .await?;
    Ok(accounting)
}

/// Return the current canonical attempt after rechecking its immutable advisory
/// binding. Charge state is read from the budget ledger, never copied here.
pub fn accounting_attempt(store: &Store, access: &Access, request_id: &str) -> Result<Attempt> {
    let request = load_request(store, access, request_id)?;
    let schedule = load_schedule(store, access, request_id)?;
    let accounting = read::<AccountingRecord>(store, access, &accounting_name(request_id))?
        .ok_or("canonical advisory accounting binding not found")?;
    let attempt = canonical_attempt(store, &request, &accounting.attempt)?;
    let artifact = canonical_request_artifact(store, &attempt)?;
    validate_accounting(&accounting, &request, &schedule, &attempt, &artifact)?;
    Ok(attempt)
}

fn validate_current(store: &Store, access: &Access, binding: &Binding) -> Result<()> {
    current_task(store, access, binding).map(|_| ())
}

fn current_task(store: &Store, access: &Access, binding: &Binding) -> Result<Task> {
    if binding.scope.workspace != access.workspace || !access.allows_task(&binding.scope.task) {
        return Err("advisory binding scope denied".into());
    }
    let workspace: Workspace = store
        .state()
        .records
        .get(&key(Collection::Workspace, access.workspace.as_str()))
        .ok_or("advisory workspace absent")?
        .decode()
        .map_err(super::err)?;
    let task: Task = store
        .state()
        .records
        .get(&key(Collection::Task, binding.scope.task.as_str()))
        .ok_or("advisory task absent")?
        .decode()
        .map_err(super::err)?;
    if workspace.authority != access.authority
        || workspace.authority != binding.authority
        || workspace.deletion != binding.deletion
        || task.scope != binding.scope
        || task.root != binding.root
        || task.revision != binding.step
        || task.steering != binding.steering
    {
        return Err("advisory canonical input revisions changed".into());
    }
    Ok(task)
}

fn cancellation(
    request: &RequestRecord,
    current: &Binding,
    task: &Task,
    now: Timestamp,
) -> Option<Cancellation> {
    if task.state == vcp_domain::task::TaskState::Paused {
        Some(Cancellation::Paused)
    } else if task.state != vcp_domain::task::TaskState::Running {
        Some(Cancellation::TaskNotRunning)
    } else if current != &request.request.binding {
        Some(Cancellation::StaleInput)
    } else if now >= request.request.deadline {
        Some(Cancellation::Deadline)
    } else {
        None
    }
}

async fn put_schedule(
    store: &mut Store,
    access: &Access,
    command: CommandId,
    schedule: ScheduleRecord,
    expected: Option<Revision>,
    now: Timestamp,
) -> Result<ScheduleRecord> {
    validate_schedule_shape(&schedule)?;
    let mut stored = row(access, schedule.id.clone(), schedule.revision, &schedule)?;
    stored
        .references
        .insert(key(Collection::Task, schedule.task.as_str()));
    stored
        .references
        .insert(key(Collection::Projection, &schedule.request_id));
    if let ScheduleState::Completed { result_id, .. } = &schedule.state {
        stored
            .references
            .insert(key(Collection::Projection, result_id));
    }
    commit(
        store,
        access,
        vec![Mutation::Put {
            record: stored,
            expected,
        }],
        command,
        now,
    )
    .await?;
    Ok(schedule)
}

fn validate_schedule(schedule: &ScheduleRecord, request: &RequestRecord) -> Result<()> {
    validate_schedule_shape(schedule)?;
    if schedule.request_id != request.id
        || schedule.workspace != request.workspace
        || schedule.task != request.task
        || schedule.request_digest != request.request_digest
        || schedule.transport_commitment != request.transport_commitment
        || schedule.deadline != request.request.deadline
    {
        return Err("canonical advisory schedule does not match its request".into());
    }
    Ok(())
}

fn validate_schedule_shape(schedule: &ScheduleRecord) -> Result<()> {
    if schedule.document_type != SCHEDULE_DOCUMENT
        || schedule.document_version != 1
        || schedule.id != schedule_name(&schedule.request_id)
        || !valid_digest(&schedule.request_digest)
        || !valid_digest(&schedule.transport_commitment)
        || schedule.created_at >= schedule.deadline
    {
        return Err("invalid canonical advisory schedule".into());
    }
    Ok(())
}

fn canonical_attempt(
    store: &Store,
    request: &RequestRecord,
    attempt_id: &AttemptId,
) -> Result<Attempt> {
    let attempt: Attempt = store
        .state()
        .records
        .get(&key(Collection::Attempt, attempt_id.as_str()))
        .ok_or("advisory helper attempt absent")?
        .decode()
        .map_err(super::err)?;
    if attempt.scope != request.request.binding.scope
        || attempt.root != request.request.binding.root
        || attempt.steering != request.request.binding.steering
        || attempt.role != RequestRole::Helper
    {
        return Err("attempt is not the canonical advisory helper reservation".into());
    }
    Ok(attempt)
}

fn canonical_request_artifact(store: &Store, attempt: &Attempt) -> Result<ArtifactDescriptor> {
    let artifact: ArtifactDescriptor = store
        .state()
        .records
        .get(&key(Collection::Artifact, attempt.request.as_str()))
        .ok_or("advisory request artifact absent")?
        .decode()
        .map_err(super::err)?;
    artifact.validate().map_err(super::err)?;
    if artifact.spec.scope != attempt.scope
        || artifact.state != CaptureState::Complete
        || artifact.spec.channel != Channel::RequestBody
        || artifact.spec.schema != "vcp-escalation-advisory-request-v1"
        || artifact.spec.source != "vcp-lifecycle/advisory"
        || artifact.sha256 != attempt.request_digest
    {
        return Err("attempt request is not a retained canonical advisory body".into());
    }
    Ok(artifact)
}

fn validate_accounting(
    accounting: &AccountingRecord,
    request: &RequestRecord,
    schedule: &ScheduleRecord,
    attempt: &Attempt,
    artifact: &ArtifactDescriptor,
) -> Result<()> {
    if accounting.document_type != ACCOUNTING_DOCUMENT
        || accounting.document_version != 1
        || accounting.id != accounting_name(&accounting.request_id)
        || accounting.request_id != request.id
        || accounting.workspace != request.workspace
        || accounting.task != request.task
        || accounting.schedule_revision.get() == 0
        || accounting.schedule_revision.get() > schedule.revision.get()
        || accounting.attempt != attempt.id
        || accounting.reservation != attempt.reservation
        || accounting.request_artifact != artifact.spec.id
        || accounting.request_artifact_digest != artifact.sha256
        || accounting.quote != attempt.quote
    {
        return Err("invalid canonical advisory accounting binding".into());
    }
    Ok(())
}

fn validate_outcome(request: &RequestRecord, outcome: &Outcome) -> Result<()> {
    if let Outcome::Advice {
        binding,
        purpose,
        question_revision,
        request_digest,
        evaluator,
        mode,
        ..
    } = outcome
    {
        let evaluator_changed = canonical_bytes(evaluator).map_err(super::err)?
            != canonical_bytes(&request.evaluator).map_err(super::err)?;
        if binding != &request.request.binding
            || *purpose != Purpose::Escalation
            || question_revision != &request.request.question_revision
            || request_digest != &request.transport_commitment
            || evaluator_changed
            || *mode != request.evaluator.mode
        {
            return Err("advisory outcome does not match its canonical request".into());
        }
    }
    Ok(())
}

fn authorize_request(access: &Access, record: &RequestRecord) -> Result<()> {
    if record.workspace != access.workspace || !access.allows_task(&record.task) {
        return Err("advisory request scope denied".into());
    }
    Ok(())
}

fn authorize_result(access: &Access, record: &ResultRecord) -> Result<()> {
    if record.workspace != access.workspace || !access.allows_task(&record.task) {
        return Err("advisory result scope denied".into());
    }
    Ok(())
}

fn validate_request_record(record: &RequestRecord) -> Result<()> {
    record
        .request
        .validate(record.recorded_at)
        .map_err(super::err)?;
    let request_digest = digest_bytes(&canonical_bytes(&record.request).map_err(super::err)?);
    if record.document_type != REQUEST_DOCUMENT
        || record.document_version != 1
        || record.workspace != record.request.binding.scope.workspace
        || record.task != record.request.binding.scope.task
        || record.request.purpose != Purpose::Escalation
        || record.evaluator.purpose != Purpose::Escalation
        || record.evaluator.mode != vcp_models::decision::Mode::Advisory
        || record.request_digest != request_digest
        || !valid_digest(&record.transport_commitment)
        || record.id != request_name(&record.workspace, &record.transport_commitment)
    {
        return Err("invalid canonical advisory request".into());
    }
    Ok(())
}

fn validate_result_record(record: &ResultRecord) -> Result<()> {
    let outcome_digest = digest_bytes(&canonical_bytes(&record.outcome).map_err(super::err)?);
    if record.document_type != RESULT_DOCUMENT
        || record.document_version != 1
        || record.id != result_name(&record.request_id)
        || !valid_digest(&record.request_digest)
        || !valid_digest(&record.transport_commitment)
        || record.outcome_digest != outcome_digest
    {
        return Err("invalid canonical advisory result".into());
    }
    Ok(())
}

fn request_name(workspace: &WorkspaceId, commitment: &str) -> String {
    format!(
        "routing-advisory-request-{}",
        digest_bytes(format!("{workspace}:{commitment}").as_bytes())
    )
}

fn result_name(request_id: &str) -> String {
    format!(
        "routing-advisory-result-{}",
        digest_bytes(request_id.as_bytes())
    )
}

fn schedule_name(request_id: &str) -> String {
    format!(
        "routing-advisory-schedule-{}",
        digest_bytes(request_id.as_bytes())
    )
}

fn accounting_name(request_id: &str) -> String {
    format!(
        "routing-advisory-accounting-{}",
        digest_bytes(request_id.as_bytes())
    )
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
