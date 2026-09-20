// SPDX-License-Identifier: Apache-2.0
//! Bind a remembered command to the native check's retained execution plan.
use crate::{access::Access, Result};
use serde::Deserialize;
use vcp_domain::{
    artifact::{ArtifactDescriptor, CaptureState},
    memory::{ClaimValue, CommandPurpose, EvidenceRef, Proposal},
    verification::CheckOutcome,
    ArtifactId,
};
use vcp_store::{contract::Collection, Store};

const MAX_PROOF_BYTES: u64 = 256 * 1024;

// The native receipt contains additional preparation/coverage data. Only the
// fields below attest this command identity; canonical verification separately
// validates the check's freshness, source fingerprint and declared outcome.
#[derive(Deserialize)]
struct CheckReceipt {
    plan: Plan,
    applicability: String,
    outcome: CheckOutcome,
    exit_code: Option<i32>,
}
#[derive(Deserialize)]
struct Plan {
    runner: String,
    origin: Option<Origin>,
    directory: String,
    request: Request,
    not_run: Option<String>,
}
#[derive(Deserialize)]
struct Origin {
    sha256: String,
}
#[derive(Deserialize)]
struct Request {
    arguments: Vec<String>,
    directory: String,
}

fn descriptor(
    store: &Store,
    access: &Access,
    id: &ArtifactId,
) -> Result<Option<ArtifactDescriptor>> {
    let Some(record) = store
        .state()
        .records
        .get(&vcp_store::contract::key(Collection::Artifact, id.as_str()))
    else {
        return Ok(None);
    };
    if record.workspace != access.workspace {
        return Ok(None);
    }
    let value: ArtifactDescriptor = record.decode()?;
    if !access.allows_task(&value.spec.scope.task)
        || value.spec.scope.workspace != access.workspace
        || value.length.get() > MAX_PROOF_BYTES
        || value.state != CaptureState::Complete
        || !value.spec.omissions.is_empty()
    {
        return Ok(None);
    }
    Ok(Some(value))
}

fn matches(receipt: &CheckReceipt, proposal: &Proposal, configuration_sha256: &str) -> bool {
    let ClaimValue::Command {
        purpose: CommandPurpose::Test,
        argv,
        cwd,
        outcome: Some(outcome),
        ..
    } = &proposal.value
    else {
        return false;
    };
    let runner = match receipt.plan.runner.as_str() {
        "node" => "node",
        "cargo" => "cargo",
        _ => return false,
    };
    let directory = if receipt.plan.request.directory.is_empty() {
        "."
    } else {
        &receipt.plan.request.directory
    };
    receipt.applicability == "current"
        && receipt.plan.not_run.is_none()
        && receipt.plan.directory == receipt.plan.request.directory
        && receipt
            .plan
            .origin
            .as_ref()
            .is_some_and(|origin| origin.sha256 == configuration_sha256)
        && argv.first().is_some_and(|first| first == runner)
        && argv.get(1..) == Some(receipt.plan.request.arguments.as_slice())
        && cwd == directory
        && &receipt.outcome == outcome
        && (!matches!(outcome, CheckOutcome::Passed) || receipt.exit_code == Some(0))
}

/// Additional proof for a command check already tied to a current canonical
/// Verification. Unrelated check success cannot establish an arbitrary argv,
/// working directory, or configuration version. Native checks currently prove
/// test commands only; build commands require a separately supported receipt.
pub fn command_matches(
    store: &Store,
    access: &Access,
    proposal: &Proposal,
    reference: &EvidenceRef,
) -> Result<bool> {
    crate::access::authorize(store.state(), access, false)?;
    let ClaimValue::Command {
        purpose: CommandPurpose::Test,
        configuration,
        ..
    } = &proposal.value
    else {
        return Ok(false);
    };
    if proposal.scope.workspace != access.workspace || !access.allows_task(&proposal.scope.task) {
        return Ok(false);
    }
    let Some(check) = descriptor(store, access, &reference.artifact)? else {
        return Ok(false);
    };
    if check.spec.schema != "verification-check/1" || check.sha256 != reference.sha256 {
        return Ok(false);
    }
    let Some(configuration) = descriptor(store, access, configuration)? else {
        return Ok(false);
    };
    // Check declared lengths before allocating or reading either retained
    // object. History rechecks current grants/deletion and spool integrity.
    if vcp_audit::history::History::read_artifact(
        store,
        &access.history(),
        &configuration.spec.id,
        std::io::sink(),
    )
    .is_err()
    {
        return Ok(false);
    }
    let mut bytes = Vec::with_capacity(check.length.get() as usize);
    if vcp_audit::history::History::read_artifact(
        store,
        &access.history(),
        &reference.artifact,
        &mut bytes,
    )
    .is_err()
    {
        return Ok(false);
    }
    let Ok(receipt) = serde_json::from_slice::<CheckReceipt>(&bytes) else {
        return Ok(false);
    };
    Ok(matches(&receipt, proposal, &configuration.sha256))
}

#[cfg(test)]
#[path = "proof_tests.rs"]
mod tests;
