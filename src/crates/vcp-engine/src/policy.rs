// SPDX-License-Identifier: Apache-2.0
//! Canonical authority projections. The caller still supplies freshly observed
//! native resources and platform capabilities to the pure evaluator.
use crate::{Error, Result};
use vcp_domain::{policy::*, *};
use vcp_protocol::command::{Approval, ApprovalState};
use vcp_store::contract::{Collection, State};

pub fn current(state: &State, workspace: &WorkspaceId) -> Result<Policy> {
    let doc: AuthorityDocument = state
        .record(Collection::Access, workspace.as_str(), workspace)?
        .decode()?;
    match doc.data {
        AuthorityData::Policy { policy } => Ok(policy),
        _ => Err(Error::Target),
    }
}
pub fn optional(state: &State, workspace: &WorkspaceId) -> Result<Option<Policy>> {
    if state.records.contains_key(&vcp_store::contract::key(
        Collection::Access,
        workspace.as_str(),
    )) {
        current(state, workspace).map(Some)
    } else {
        Ok(None)
    }
}
pub fn grants(state: &State, workspace: &WorkspaceId) -> Result<Vec<Grant>> {
    let mut grants = vec![];
    for record in state.records.values().filter(|r| {
        r.collection == Collection::Access
            && &r.workspace == workspace
            && r.value["document_type"] == "vcp_authority_v1"
    }) {
        let document: AuthorityDocument = record.decode()?;
        if let AuthorityData::Grant { grant } = document.data {
            grants.push(grant);
        }
    }
    Ok(grants)
}
pub fn evaluate(
    state: &State,
    prepared: &vcp_policy::Prepared,
    facts: &vcp_policy::Facts<'_>,
) -> Result<vcp_policy::Decision> {
    let operation = prepared.operation();
    let policy = current(state, &operation.scope.workspace)?;
    // A declined exact operation remains declined at this policy/steering
    // revision. A later broad grant cannot silently erase the denial.
    for record in state.records.values().filter(|r| {
        r.collection == Collection::Approval && r.workspace == operation.scope.workspace
    }) {
        let approval: Approval = record.decode()?;
        if approval.state == ApprovalState::Denied
            && approval.scope == operation.scope
            && approval.actor == operation.actor
            && approval.policy == operation.policy
            && approval.steering == operation.steering
            && approval.operation_digest == prepared.digest()
        {
            return Ok(vcp_policy::Decision::Deny {
                origin: approval.id.to_string(),
                reason: "user declined this prepared operation".into(),
            });
        }
    }
    Ok(vcp_policy::evaluate(
        prepared,
        &policy,
        &grants(state, &operation.scope.workspace)?,
        facts,
    )?)
}
