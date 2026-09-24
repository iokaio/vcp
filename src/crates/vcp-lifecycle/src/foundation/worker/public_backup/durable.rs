// SPDX-License-Identifier: Apache-2.0
//! Public intent is committed before checkpoint capture can mutate the store.
use super::*;
use serde::{Deserialize, Serialize};
use vcp_protocol::command::{CommandReceipt, CommandResult};
use vcp_protocol::event::{EventInput, EventKind};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Intent {
    pub schema_version: u32,
    pub operation: CommandId,
    pub workspace: WorkspaceId,
    pub session: SessionId,
    pub actor: ActorId,
    pub revision: Revision,
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
    pub binding: Revision,
    pub capability: String,
    pub generation: u64,
    pub configuration_revision: u64,
    pub cancel_requested: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptBinding {
    schema_version: u32,
    actor: ActorId,
    session: SessionId,
    digest: String,
    operation: CommandId,
    revision: Revision,
}
fn intent_id(workspace: &WorkspaceId, operation: &CommandId) -> String {
    format!(
        "publisher-intent-{}",
        vcp_protocol::digest_bytes(format!("{workspace}:{operation}").as_bytes())
    )
}
fn receipt_id(workspace: &WorkspaceId, command: &CommandId) -> String {
    format!(
        "publisher-command-{}",
        vcp_protocol::digest_bytes(format!("{workspace}:{command}").as_bytes())
    )
}
pub(super) fn read(store: &Store, access: &Access, operation: &CommandId) -> RpcResult<Intent> {
    let row = store
        .state()
        .record(
            Collection::Projection,
            &intent_id(&access.workspace, operation),
            &access.workspace,
        )
        .map_err(|_| failure(Code::PolicyDenied))?;
    let value: Intent = row.decode().map_err(|_| failure(Code::StoreUnavailable))?;
    if value.schema_version != 1
        || value.operation != *operation
        || value.workspace != access.workspace
        || value.session != access.session
        || value.actor != access.actor
        || value.revision != row.revision
    {
        return Err(failure(Code::PolicyDenied));
    }
    Ok(value)
}
pub(super) fn replay(
    store: &Store,
    access: &Access,
    call: &Call,
) -> RpcResult<Option<CommandReceipt>> {
    let command = command(call)?;
    let digest = call
        .digest(access.actor.as_str())
        .map_err(|_| RpcError::invalid_params())?;
    let Some(receipt) = store
        .state()
        .command(&access.workspace, &command, &digest)
        .map_err(|_| failure(Code::CommandConflict))?
    else {
        return Ok(None);
    };
    let row = store
        .state()
        .record(
            Collection::Projection,
            &receipt_id(&access.workspace, &command),
            &access.workspace,
        )
        .map_err(|_| failure(Code::CommandConflict))?;
    let binding: ReceiptBinding = row.decode().map_err(|_| failure(Code::StoreUnavailable))?;
    if binding.schema_version != 1
        || binding.actor != access.actor
        || binding.session != access.session
        || binding.digest != digest
    {
        return Err(failure(Code::CommandConflict));
    }
    if receipt.result
        != (CommandResult::Accepted {
            revision: binding.revision,
        })
        || !store.state().events.iter().any(|event| {
            event.watermark == receipt.watermark
                && event.event.workspace == access.workspace
                && event.event.session == access.session
                && event.event.actor == access.actor
                && event.event.correlation == command
        })
    {
        return Err(failure(Code::StoreUnavailable));
    }
    let transaction = store
        .state()
        .transactions
        .get(&receipt.transaction)
        .ok_or_else(|| failure(Code::StoreUnavailable))?;
    if transaction.command.as_ref() != Some(&receipt) {
        return Err(failure(Code::StoreUnavailable));
    }
    Ok(Some(receipt))
}
pub(super) async fn commit(
    store: &mut Store,
    access: &Access,
    call: &Call,
    intent: &Intent,
    expected: Option<Revision>,
) -> RpcResult<CommandReceipt> {
    let command = command(call)?;
    let digest = call
        .digest(access.actor.as_str())
        .map_err(|_| RpcError::invalid_params())?;
    let binding = ReceiptBinding {
        schema_version: 1,
        actor: access.actor.clone(),
        session: access.session.clone(),
        digest: digest.clone(),
        operation: intent.operation.clone(),
        revision: intent.revision,
    };
    let record = |id, revision, value| Record {
        collection: Collection::Projection,
        id,
        workspace: access.workspace.clone(),
        revision,
        value,
        references: std::collections::BTreeSet::from([
            key(Collection::Workspace, access.workspace.as_str()),
            key(Collection::Session, access.session.as_str()),
        ]),
    };
    let mutations = vec![
        Mutation::Put {
            record: record(
                intent_id(&access.workspace, &intent.operation),
                intent.revision,
                serde_json::to_value(intent).map_err(|_| failure(Code::StoreUnavailable))?,
            ),
            expected,
        },
        Mutation::Put {
            record: record(
                receipt_id(&access.workspace, &command),
                Revision::ZERO,
                serde_json::to_value(binding).map_err(|_| failure(Code::StoreUnavailable))?,
            ),
            expected: None,
        },
    ];
    store
        .transact(Transaction {
            id: TransactionId::new(),
            expected_watermark: store.state().watermark,
            mutations,
            events: vec![EventInput {
                id: EventId::new(),
                workspace: access.workspace.clone(),
                session: access.session.clone(),
                task: None,
                actor: access.actor.clone(),
                correlation: command.clone(),
                causation: None,
                timestamp: now(),
                kind: EventKind::Diagnostic,
                artifacts: vec![],
                data: serde_json::json!({"version":1,"publisher_intent":true}),
                metadata: None,
            }],
            command: Some(ReceiptInput {
                command,
                workspace: access.workspace.clone(),
                session: access.session.clone(),
                digest,
                result: CommandResult::Accepted {
                    revision: intent.revision,
                },
            }),
        })
        .await
        .map_err(|_| unknown(call))?;
    replay(store, access, call)
        .map_err(|_| unknown(call))?
        .ok_or_else(|| unknown(call))
}
