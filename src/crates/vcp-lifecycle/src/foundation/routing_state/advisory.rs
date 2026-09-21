// SPDX-License-Identifier: Apache-2.0
//! Immutable canonical records for bounded escalation-advisory requests/results.
//! A stale result remains inspectable historical evidence and is never accepted
//! as current advice.
use super::{commit, read, row, Result};
use serde::{Deserialize, Serialize};
use vcp_domain::{task::Task, workspace::Workspace, *};
use vcp_memory::access::Access;
use vcp_models::decision::{Binding, Outcome, Prepared, Purpose, QualifiedEvaluator, Request};
use vcp_protocol::{canonical_bytes, digest_bytes};
use vcp_store::{
    contract::{key, Collection, Mutation},
    Store,
};

const REQUEST_DOCUMENT: &str = "vcp_escalation_advisory_request_v1";
const RESULT_DOCUMENT: &str = "vcp_escalation_advisory_result_v1";

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

fn validate_current(store: &Store, access: &Access, binding: &Binding) -> Result<()> {
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

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
