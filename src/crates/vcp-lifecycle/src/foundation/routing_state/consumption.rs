// SPDX-License-Identifier: Apache-2.0
//! Immutable receipts for exact values actually consumed by local analysis.
//! Replay returns the recorded scalar and never rebuilds a fitted artifact.
use super::{commit, observations, read, rewards, row, HistoryWindow, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use vcp_domain::*;
use vcp_memory::access::Access;
use vcp_protocol::{canonical_bytes, digest_bytes};
use vcp_store::{
    contract::{key, Collection, Mutation},
    Store,
};

const PRODUCER: &str = "reward-map-consumer/1";
const MAX_REFERENCES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    OptimizationInspection,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RewardSelection {
    pub cohort: rewards::Cohort,
    pub currency: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub document_type: String,
    pub document_version: u32,
    pub id: String,
    pub workspace: WorkspaceId,
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
    pub consumer_decision: CommandId,
    pub purpose: Purpose,
    pub recorded_at: Timestamp,
    pub producer: String,
    pub producer_artifact: String,
    pub source_evidence: String,
    pub source_digest: String,
    pub source_window: HistoryWindow,
    pub source_cutoff: Watermark,
    pub source_tasks: BTreeSet<TaskId>,
    pub source_attempts: u64,
    pub source_attempts_digest: String,
    pub selection: RewardSelection,
    pub selection_digest: String,
    pub input_digest: String,
    pub consumed_micros: u64,
    pub currency: String,
    pub policy: Option<String>,
    pub catalog: Option<String>,
    pub historical_replay_only: bool,
    pub serving_qualified: bool,
}

#[derive(Serialize)]
struct InputIdentity<'a> {
    producer: &'a str,
    producer_artifact: &'a str,
    source_evidence: &'a str,
    source_digest: &'a str,
    selection_digest: &'a str,
    source_attempts_digest: &'a str,
    consumed_micros: u64,
    currency: &'a str,
}

/// Persist the exact reward mean selected by a local optimization inspection.
/// The supplied artifact must equal a fresh authorized rebuild. Reusing the same
/// decision ID is idempotent only for the same artifact and selection.
pub async fn consume_reward(
    store: &mut Store,
    access: &Access,
    decision: CommandId,
    artifact: &rewards::Artifact,
    selection: RewardSelection,
    now: Timestamp,
) -> Result<Receipt> {
    let id = receipt_name(&access.workspace, &decision);
    if let Some(existing) = read::<Receipt>(store, access, &id)? {
        authorize_receipt(access, &existing)?;
        validate_receipt(&existing)?;
        if existing.consumer_decision != decision
            || existing.producer_artifact != artifact.id
            || existing.selection != selection
        {
            return Err("consumed-value decision identity was reused with different inputs".into());
        }
        return Ok(existing);
    }
    if !access.write {
        return Err("consumed-value write access denied".into());
    }
    let fresh = rewards::map(store, access, artifact.source_window.clone())?;
    if &fresh != artifact {
        return Err("reward artifact changed; rebuild before consumption".into());
    }
    let cells = match &artifact.status {
        rewards::Status::Mapped { cells } => cells,
        rewards::Status::Abstained { .. } => {
            return Err("an abstained reward artifact has no consumable value".into())
        }
    };
    let cell = cells
        .iter()
        .find(|cell| cell.cohort == selection.cohort && cell.currency == selection.currency)
        .ok_or("reward selection is absent from the producer artifact")?;
    let consumed_micros = cell
        .point_estimate_micros
        .ok_or("reward selection has an unknown remainder")?;
    let evidence = observations::observe(store, access, artifact.source_window.clone())?;
    if evidence.id != artifact.source_evidence {
        return Err("reward source changed during consumption".into());
    }
    let mut source_tasks = BTreeSet::new();
    let mut source_attempts = Vec::new();
    let mut references = BTreeSet::new();
    for trace in &evidence.attempts {
        if matches_selection(trace, &selection) {
            if trace.charge.final_charge_micros.is_none() {
                return Err("reward source became incomplete during consumption".into());
            }
            source_tasks.insert(trace.task.clone());
            source_attempts.push(trace.attempt.clone());
            references.insert(key(Collection::Task, trace.task.as_str()));
            references.insert(key(Collection::Attempt, trace.attempt.as_str()));
            for settlement in &trace.charge.settlements {
                references.insert(key(Collection::Settlement, settlement.settlement.as_str()));
            }
        }
    }
    if source_attempts.len() as u64 != cell.attempts || source_attempts.is_empty() {
        return Err("reward source cohort does not match its artifact count".into());
    }
    if references.len() > MAX_REFERENCES {
        return Err("consumed reward exceeds 4096 canonical references; narrow the source".into());
    }
    source_attempts.sort();
    let source_attempts_digest =
        digest_bytes(&canonical_bytes(&source_attempts).map_err(super::err)?);
    let selection_digest = digest_bytes(&canonical_bytes(&selection).map_err(super::err)?);
    let input_digest = digest_bytes(
        &canonical_bytes(&InputIdentity {
            producer: PRODUCER,
            producer_artifact: &artifact.id,
            source_evidence: &artifact.source_evidence,
            source_digest: &artifact.source_digest,
            selection_digest: &selection_digest,
            source_attempts_digest: &source_attempts_digest,
            consumed_micros,
            currency: &selection.currency,
        })
        .map_err(super::err)?,
    );
    let receipt = Receipt {
        document_type: "vcp_consumed_reward_v1".into(),
        document_version: 1,
        id: id.clone(),
        workspace: artifact.workspace.clone(),
        authority: artifact.authority,
        deletion: artifact.deletion,
        consumer_decision: decision.clone(),
        purpose: Purpose::OptimizationInspection,
        recorded_at: now,
        producer: PRODUCER.into(),
        producer_artifact: artifact.id.clone(),
        source_evidence: artifact.source_evidence.clone(),
        source_digest: artifact.source_digest.clone(),
        source_window: artifact.source_window.clone(),
        source_cutoff: artifact.source_cutoff,
        source_tasks,
        source_attempts: source_attempts.len() as u64,
        source_attempts_digest,
        selection,
        selection_digest,
        input_digest,
        consumed_micros,
        currency: cell.currency.clone(),
        policy: artifact.policy.clone(),
        catalog: artifact.catalog.clone(),
        historical_replay_only: true,
        serving_qualified: false,
    };
    let mut record = row(access, id, Revision::ZERO, &receipt)?;
    record.references.extend(references);
    commit(
        store,
        access,
        vec![Mutation::Put {
            record,
            expected: None,
        }],
        decision,
        now,
    )
    .await?;
    Ok(receipt)
}

/// Replay the scalar recorded for a prior consumer decision. This intentionally
/// does not rebuild or compare a current reward artifact.
pub fn replay_reward(store: &Store, access: &Access, decision: &CommandId) -> Result<Receipt> {
    let receipt = read::<Receipt>(store, access, &receipt_name(&access.workspace, decision))?
        .ok_or("consumed reward receipt not found")?;
    authorize_receipt(access, &receipt)?;
    validate_receipt(&receipt)?;
    if receipt.consumer_decision != *decision {
        return Err("invalid consumed reward receipt".into());
    }
    Ok(receipt)
}

fn receipt_name(workspace: &WorkspaceId, decision: &CommandId) -> String {
    format!(
        "routing-consumed-reward-{}",
        digest_bytes(format!("{workspace}:{decision}").as_bytes())
    )
}

fn authorize_receipt(access: &Access, receipt: &Receipt) -> Result<()> {
    if receipt.workspace != access.workspace
        || receipt
            .source_tasks
            .iter()
            .any(|task| !access.allows_task(task))
    {
        return Err("consumed reward scope denied".into());
    }
    Ok(())
}

fn validate_receipt(receipt: &Receipt) -> Result<()> {
    let selection_digest = digest_bytes(&canonical_bytes(&receipt.selection).map_err(super::err)?);
    let input_digest = digest_bytes(
        &canonical_bytes(&InputIdentity {
            producer: PRODUCER,
            producer_artifact: &receipt.producer_artifact,
            source_evidence: &receipt.source_evidence,
            source_digest: &receipt.source_digest,
            selection_digest: &selection_digest,
            source_attempts_digest: &receipt.source_attempts_digest,
            consumed_micros: receipt.consumed_micros,
            currency: &receipt.currency,
        })
        .map_err(super::err)?,
    );
    if receipt.document_type != "vcp_consumed_reward_v1"
        || receipt.document_version != 1
        || receipt.id != receipt_name(&receipt.workspace, &receipt.consumer_decision)
        || receipt.producer != PRODUCER
        || receipt.source_tasks.is_empty()
        || receipt.source_attempts == 0
        || !valid_digest(&receipt.source_digest)
        || !valid_digest(&receipt.source_attempts_digest)
        || receipt.selection_digest != selection_digest
        || receipt.input_digest != input_digest
        || !receipt.historical_replay_only
        || receipt.serving_qualified
        || receipt.currency != receipt.selection.currency
    {
        return Err("invalid consumed reward receipt".into());
    }
    Ok(())
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn matches_selection(trace: &observations::AttemptTrace, selection: &RewardSelection) -> bool {
    trace.charge.currency == selection.currency
        && trace.cohort.role == selection.cohort.role
        && trace.cohort.model == selection.cohort.model
        && trace.cohort.endpoint == selection.cohort.endpoint
        && trace.cohort.authority_policy == selection.cohort.authority_policy
        && trace.cohort.task_class == selection.cohort.task_class
        && trace.cohort.root_task == selection.cohort.root_task
        && trace.cohort.retry_depth == selection.cohort.retry_depth
        && trace.cohort.decomposition_depth == selection.cohort.decomposition_depth
        && trace.cohort.prior_attempts == selection.cohort.prior_attempts
        && trace.cohort.prior_failed_checks == selection.cohort.prior_failed_checks
}
