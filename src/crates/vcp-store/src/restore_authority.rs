// SPDX-License-Identifier: Apache-2.0
//! Fixed destination sanitization. This is a new canonical fact following the
//! immutable imported history, never a rewrite of historical grants or receipts.
use crate::{
    contract::{Collection, Mutation, Record, State, Transaction},
    Error, Result,
};
use vcp_domain::{
    policy::{AuthorityData, AuthorityDocument, Autonomy},
    task::{Task, TaskState},
    workspace::Workspace,
    ActorId, CommandId, EventId, SessionId, Timestamp, TransactionId, WorkspaceId,
};
use vcp_protocol::{
    canonical_bytes, digest_bytes,
    event::{EventInput, EventKind},
};

pub(crate) fn transaction(
    state: &State,
    workspace: &WorkspaceId,
    operation: &CommandId,
    actor: &ActorId,
    timestamp: Timestamp,
) -> Result<Transaction> {
    let workspace_row = state.record(Collection::Workspace, workspace.as_str(), workspace)?;
    let old: Workspace = workspace_row.decode()?;
    // The old path is retained solely for reconciliation. Advancing both
    // authority and binding makes old permits and source applicability stale.
    let next = old.rebind(old.revision, old.binding.clone())?;
    let mut mutations = vec![Mutation::Put {
        expected: Some(old.revision),
        record: Record::typed(
            Collection::Workspace,
            workspace.as_str(),
            workspace.clone(),
            next.revision,
            &next,
        )?,
    }];
    let session = state
        .records
        .values()
        .find(|r| r.collection == Collection::Session && &r.workspace == workspace)
        .map(|r| SessionId::parse(r.id.clone()))
        .transpose()?
        .ok_or(Error::Corruption("restore workspace has no session"))?;
    let event = |kind: EventKind,
                 task: Option<&Task>,
                 label: &str,
                 data: serde_json::Value|
     -> Result<EventInput> {
        Ok(EventInput {
            id: EventId::parse(digest_bytes(&canonical_bytes(&(
                "vcp-restore-authority/1",
                operation,
                label,
            ))?))?,
            workspace: workspace.clone(),
            session: task
                .map(|t| t.scope.session.clone())
                .unwrap_or_else(|| session.clone()),
            task: task.map(|t| t.scope.task.clone()),
            actor: actor.clone(),
            correlation: operation.clone(),
            causation: None,
            timestamp,
            kind,
            artifacts: vec![],
            data,
            metadata: None,
        })
    };
    let mut events = vec![event(
        EventKind::WorkspaceBound,
        None,
        "workspace",
        serde_json::json!({"schema_version":1,"reason":"restored history requires destination rebind","authority":next.authority,"binding":next.binding.revision}),
    )?];
    for row in state.records.values().filter(|r| &r.workspace == workspace) {
        if row.collection == Collection::Task {
            let old: Task = row.decode()?;
            if old.state.terminal() || old.state == TaskState::Paused || old.redaction.is_some() {
                continue;
            }
            let fact = event(
                EventKind::TaskTransition,
                Some(&old),
                old.scope.task.as_str(),
                serde_json::json!({"schema_version":1,"reason":"restored task remains paused pending destination reconciliation"}),
            )?;
            let next = old.transition(
                &old.scope,
                old.revision,
                old.steering,
                TaskState::Paused,
                fact.id.clone(),
                "restored task remains paused pending destination reconciliation".into(),
                None,
                None,
            )?;
            let mut record = Record::typed(
                Collection::Task,
                row.id.clone(),
                workspace.clone(),
                next.revision,
                &next,
            )?;
            record.references = row.references.clone();
            mutations.push(Mutation::Put {
                expected: Some(row.revision),
                record,
            });
            events.push(fact);
        } else if row.collection == Collection::Access
            && row.value["document_type"] == "vcp_authority_v1"
        {
            let mut document: AuthorityDocument = row.decode()?;
            match &mut document.data {
                AuthorityData::Grant { grant } => {
                    if grant.revoked {
                        continue;
                    }
                    grant.revoked = true;
                    grant.revision = grant.revision.next()?;
                }
                AuthorityData::Policy { policy } => {
                    policy.mode = Autonomy::Plan;
                    policy.automatic_effects.clear();
                    policy.workspace_roots.clear();
                    policy.revision = policy.revision.next()?;
                }
            }
            document.revision = document.revision.next()?;
            let mut record = Record::typed(
                Collection::Access,
                row.id.clone(),
                workspace.clone(),
                document.revision,
                &document,
            )?;
            record.references = row.references.clone();
            mutations.push(Mutation::Put {
                expected: Some(row.revision),
                record,
            });
        }
    }
    events.push(event(EventKind::AccessChanged,None,"access",serde_json::json!({"schema_version":1,"reason":"restored grants revoked; local execution authority must be enrolled independently"}))?);
    Ok(Transaction {
        id: TransactionId::parse(operation.as_str())?,
        expected_watermark: state.watermark,
        mutations,
        events,
        command: None,
    })
}
