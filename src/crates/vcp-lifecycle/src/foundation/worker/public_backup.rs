// SPDX-License-Identifier: Apache-2.0
//! Scoped public control of the existing native encrypted publisher.
use super::{public_connection::PublicConnection, *};
use crate::foundation::{
    backup_manager::{CapabilityIdentity, ManagerSnapshot, Phase},
    backup_run::PublicFence,
};
use vcp_protocol::{
    backup_publisher as wire,
    errors::{ApplicationError, Code, Retry},
    jsonrpc::RpcError,
    methods::{self, Call, ResultValue},
};
use vcp_store::snapshot_jobs::{Job as SnapshotJob, Jobs, Stage};
mod durable;
#[cfg(test)]
mod tests;
type RpcResult<T> = std::result::Result<T, RpcError>;
fn failure(code: Code) -> RpcError {
    ApplicationError {
        code,
        retry: Retry::AfterRevalidation,
        operation: None,
        explanation: "encrypted publisher unavailable; revalidate current state".into(),
        reconciliation: None,
    }
    .into_rpc()
}
fn unknown(call: &Call) -> RpcError {
    ApplicationError {
        code: Code::OutcomeUnknown,
        retry: Retry::ReconcileOriginal,
        operation: call.command_id().cloned(),
        explanation: "reconcile the original publisher command".into(),
        reconciliation: None,
    }
    .into_rpc()
}
fn id(value: &str) -> RpcResult<methods::Id> {
    value
        .to_owned()
        .try_into()
        .map_err(|_| failure(Code::StoreUnavailable))
}
fn counter(value: &methods::Counter) -> RpcResult<u64> {
    value
        .as_str()
        .parse()
        .map_err(|_| RpcError::invalid_params())
}
fn command(call: &Call) -> RpcResult<CommandId> {
    CommandId::parse(
        call.command_id()
            .ok_or_else(RpcError::invalid_params)?
            .as_str(),
    )
    .map_err(|_| RpcError::invalid_params())
}
fn workspace(context: &Context, access: &Access) -> RpcResult<Workspace> {
    let value: Workspace = context
        .engine
        .store()
        .state()
        .record(
            Collection::Workspace,
            access.workspace.as_str(),
            &access.workspace,
        )
        .map_err(|_| failure(Code::StoreUnavailable))?
        .decode()
        .map_err(|_| failure(Code::StoreUnavailable))?;
    if value.id != access.workspace || value.authority != access.authority {
        return Err(failure(Code::PolicyDenied));
    }
    Ok(value)
}
fn snapshot_job(
    store: &Store,
    access: &Access,
    operation: &CommandId,
) -> RpcResult<Option<SnapshotJob>> {
    if !store
        .state()
        .records
        .contains_key(&key(Collection::SnapshotPin, operation.as_str()))
    {
        return Ok(None);
    }
    Jobs::inspect(store, operation, &access.workspace)
        .map(Some)
        .map_err(|_| failure(Code::StoreUnavailable))
}
fn scope(call: &Call) -> RpcResult<&methods::Scope> {
    match call {
        Call::BackupStatus(p) => Ok(&p.scope),
        Call::BackupCreate(p) => Ok(&p.scope),
        Call::BackupRead(p) => Ok(&p.scope),
        Call::BackupRetry(p) => Ok(&p.scope),
        Call::BackupCancel(p) => Ok(&p.scope),
        _ => Err(RpcError::invalid_params()),
    }
}
fn capability(
    snapshot: &ManagerSnapshot,
    reference: &methods::Id,
    generation: &methods::Counter,
) -> RpcResult<CapabilityIdentity> {
    snapshot
        .capability
        .as_ref()
        .filter(|cap| {
            cap.reference == reference.as_str() && cap.generation.to_string() == generation.as_str()
        })
        .cloned()
        .ok_or_else(|| failure(Code::CapabilityUnavailable))
}
fn view(
    context: &Context,
    access: &Access,
    request: &wire::Read,
    snapshot: &ManagerSnapshot,
) -> RpcResult<wire::JobView> {
    let operation =
        CommandId::parse(request.operation.as_str()).map_err(|_| RpcError::invalid_params())?;
    let intent = durable::read(context.engine.store(), access, &operation)?;
    let job = snapshot_job(context.engine.store(), access, &operation)?;
    let progress = snapshot
        .progress
        .as_ref()
        .filter(|p| p.operation == operation);
    let running = progress.is_some_and(|p| p.running());
    let (phase, publication, checkpoint, cleanup, pins) = match job.as_ref() {
        Some(j) => {
            let phase = match j.stage {
                Stage::Captured => wire::Phase::Captured,
                Stage::ArchiveReady => wire::Phase::ArchiveReady,
                Stage::CiphertextReady => wire::Phase::CiphertextReady,
                Stage::Admitted => wire::Phase::Admitted,
                Stage::Published => wire::Phase::Published,
                Stage::Cancelled => wire::Phase::Cancelled,
            };
            let publication = match j.stage {
                Stage::Published => wire::LocalPublication::Published,
                Stage::Admitted if running => wire::LocalPublication::InProgress,
                Stage::Admitted => wire::LocalPublication::Unknown,
                _ => wire::LocalPublication::NotObserved,
            };
            // Released published jobs passed independent checkpoint reconciliation.
            let checkpoint = if j.stage == Stage::Published {
                if j.active {
                    wire::Checkpoint::Pending
                } else {
                    wire::Checkpoint::Matched
                }
            } else {
                wire::Checkpoint::NotObserved
            };
            let cleanup = if !j.active {
                wire::Cleanup::Complete
            } else if running {
                wire::Cleanup::Pending
            } else {
                wire::Cleanup::ReconciliationRequired
            };
            (
                phase,
                publication,
                checkpoint,
                cleanup,
                if j.pins.is_empty() && !j.active {
                    wire::SourcePins::Released
                } else {
                    wire::SourcePins::Held
                },
            )
        }
        None => (
            if running {
                wire::Phase::Preparing
            } else if intent.cancel_requested {
                wire::Phase::Cancelled
            } else {
                wire::Phase::Interrupted
            },
            wire::LocalPublication::NotObserved,
            wire::Checkpoint::NotObserved,
            if !running {
                wire::Cleanup::ReconciliationRequired
            } else {
                wire::Cleanup::Pending
            },
            wire::SourcePins::NotObserved,
        ),
    };
    let failed = progress.is_some_and(|p| p.phase == Phase::Failed || p.phase == Phase::Cancelled)
        || (!running && job.as_ref().is_none_or(|j| j.active) && !intent.cancel_requested);
    let value = wire::JobView {
        scope: request.scope.clone(),
        operation: request.operation.clone(),
        revision: intent.revision.get().into(),
        job_revision: job.as_ref().map(|j| j.revision.get().into()),
        watermark: context.engine.store().state().watermark.get().into(),
        phase,
        cancel_requested: intent.cancel_requested,
        snapshot_watermark: job.as_ref().map(|j| j.watermark.get().into()),
        deletion_revision: job
            .as_ref()
            .map_or(intent.deletion.get(), |j| j.deletion)
            .into(),
        authority_revision: job
            .as_ref()
            .map_or(intent.authority.get(), |j| j.authority)
            .into(),
        source_pins: pins,
        local_publication: publication,
        checkpoint,
        cleanup,
        failure: failed.then_some(wire::FailureReason::Interrupted),
        cloud_transfer: wire::CloudTransfer::Unknown,
        restore_verification: wire::RestoreVerification::NotObserved,
    };
    if canonical_bytes(&value)
        .map_err(|_| failure(Code::StoreUnavailable))?
        .len()
        > wire::MAX_RESPONSE_BYTES
    {
        return Err(failure(Code::ResourceLimit));
    }
    Ok(value)
}
enum Action {
    Start {
        intent: durable::Intent,
        capability: CapabilityIdentity,
        retry: bool,
    },
    Cancel(CommandId),
}
impl PublicConnection {
    pub(super) fn backup_call(&self, call: Call, current: &Access) -> RpcResult<ResultValue> {
        call.validate().map_err(|_| RpcError::invalid_params())?;
        let (host, access, connection, token) = self
            .rpc_context(current)
            .map_err(|_| failure(Code::PolicyDenied))?;
        let selected = scope(&call)?;
        if selected.workspace.as_str() != access.workspace.as_str()
            || selected.session.as_str() != access.session.as_str()
        {
            return Err(failure(Code::PolicyDenied));
        }
        let fallback = call.clone();
        let connected = self.connected.clone();
        let manager = host.clone();
        let owned_access = access.clone();
        let lease = token.clone();
        let caller = connection.clone();
        let (result, action) = host
            .worker
            .run_cleanup(move |context| {
                Ok((|| -> RpcResult<(ResultValue, Option<Action>)> {
                    if !connected.load(Ordering::SeqCst) {
                        return Err(failure(Code::PolicyDenied));
                    }
                    let write = !matches!(call, Call::BackupStatus(_) | Call::BackupRead(_));
                    context
                        .public_authorize(&access, &connection, token.as_ref(), write)
                        .map_err(|_| failure(Code::PolicyDenied))?;
                    let current_workspace = workspace(context, &access)?;
                    if write {
                        if let Some(receipt) =
                            durable::replay(context.engine.store(), &access, &call)?
                        {
                            return Ok((
                                vcp_engine::rpc::acceptance(&context.engine, &access, &receipt)?,
                                None,
                            ));
                        }
                    }
                    let snapshot = manager
                        .backup_manager_snapshot()
                        .map_err(|_| failure(Code::StoreUnavailable))?;
                    match &call {
                        Call::BackupStatus(request) => {
                            let active_operation = snapshot
                                .progress
                                .as_ref()
                                .filter(|p| p.running())
                                .and_then(|p| {
                                    durable::read(context.engine.store(), &access, &p.operation)
                                        .ok()
                                        .map(|_| p.operation.clone())
                                })
                                .map(|op| id(op.as_str()))
                                .transpose()?;
                            let capability = match snapshot.capability {
                                Some(cap) => wire::Capability::Loaded {
                                    reference: id(&cap.reference)?,
                                    generation: cap.generation.into(),
                                    configuration_revision: cap.configuration_revision.into(),
                                },
                                None => wire::Capability::Unavailable {
                                    reason: wire::CapabilityReason::NotLoaded,
                                },
                            };
                            return Ok((
                                ResultValue::BackupStatus(wire::StatusView {
                                    scope: request.scope.clone(),
                                    watermark: context
                                        .engine
                                        .store()
                                        .state()
                                        .watermark
                                        .get()
                                        .into(),
                                    capability,
                                    busy: snapshot.busy,
                                    active_operation,
                                    destination: wire::Destination::LocalEncryptedVault,
                                    cloud_transfer: wire::CloudTransfer::Unknown,
                                    restore_verification: wire::RestoreVerification::NotObserved,
                                }),
                                None,
                            ));
                        }
                        Call::BackupRead(request) => {
                            return Ok((
                                ResultValue::BackupJob(view(context, &access, request, &snapshot)?),
                                None,
                            ))
                        }
                        _ => {}
                    }
                    let mutation = call.mutation().ok_or_else(RpcError::invalid_params)?;
                    let expected_binding = match &call {
                        Call::BackupCreate(p) => &p.expected_binding_revision,
                        Call::BackupRetry(p) => &p.expected_binding_revision,
                        Call::BackupCancel(p) => &p.expected_binding_revision,
                        _ => return Err(RpcError::invalid_params()),
                    };
                    if current_workspace.revision.get() != counter(&mutation.expected_revision)?
                        || current_workspace.binding.revision.get() != counter(expected_binding)?
                    {
                        return Err(failure(Code::VersionConflict));
                    }
                    if context.authority_pending || context.capture_admission_blocked() {
                        return Err(failure(Code::AuthorityStale));
                    }
                    let (intent, expected, action) = match &call {
                        Call::BackupCreate(p) => {
                            if current_workspace.trust != Trust::Trusted {
                                return Err(failure(Code::PolicyDenied));
                            }
                            context
                                .backup_cut()
                                .map_err(|_| failure(Code::PolicyDenied))?;
                            if snapshot.busy {
                                return Err(failure(Code::ResourceLimit));
                            }
                            let capability = capability(
                                &snapshot,
                                &p.capability,
                                &p.expected_capability_generation,
                            )?;
                            if snapshot_job(context.engine.store(), &access, &command(&call)?)?
                                .is_some()
                            {
                                return Err(failure(Code::CommandConflict));
                            }
                            let intent = durable::Intent {
                                schema_version: 1,
                                operation: command(&call)?,
                                workspace: access.workspace.clone(),
                                session: access.session.clone(),
                                actor: access.actor.clone(),
                                revision: Revision::ZERO,
                                authority: current_workspace.authority,
                                deletion: current_workspace.deletion,
                                binding: current_workspace.binding.revision,
                                capability: capability.reference.clone(),
                                generation: capability.generation,
                                configuration_revision: capability.configuration_revision,
                                cancel_requested: false,
                            };
                            (
                                intent.clone(),
                                None,
                                Action::Start {
                                    intent,
                                    capability,
                                    retry: false,
                                },
                            )
                        }
                        Call::BackupRetry(p) => {
                            if current_workspace.trust != Trust::Trusted {
                                return Err(failure(Code::PolicyDenied));
                            }
                            context
                                .backup_cut()
                                .map_err(|_| failure(Code::PolicyDenied))?;
                            if snapshot.busy {
                                return Err(failure(Code::ResourceLimit));
                            }
                            let operation = CommandId::parse(p.operation.as_str())
                                .map_err(|_| RpcError::invalid_params())?;
                            let mut intent =
                                durable::read(context.engine.store(), &access, &operation)?;
                            let job = snapshot_job(context.engine.store(), &access, &operation)?
                                .ok_or_else(|| failure(Code::VersionConflict))?;
                            if intent.revision.get() != counter(&p.expected_operation_revision)?
                                || job.revision.get() != counter(&p.expected_job_revision)?
                                || job.stage == Stage::Cancelled
                            {
                                return Err(failure(Code::VersionConflict));
                            }
                            let capability = capability(
                                &snapshot,
                                &p.capability,
                                &p.expected_capability_generation,
                            )?;
                            let expected = intent.revision;
                            intent.revision = intent
                                .revision
                                .next()
                                .map_err(|_| failure(Code::ResourceLimit))?;
                            intent.authority = current_workspace.authority;
                            intent.deletion = current_workspace.deletion;
                            intent.binding = current_workspace.binding.revision;
                            intent.capability = capability.reference.clone();
                            intent.generation = capability.generation;
                            intent.configuration_revision = capability.configuration_revision;
                            intent.cancel_requested = false;
                            (
                                intent.clone(),
                                Some(expected),
                                Action::Start {
                                    intent,
                                    capability,
                                    retry: true,
                                },
                            )
                        }
                        Call::BackupCancel(p) => {
                            let operation = CommandId::parse(p.operation.as_str())
                                .map_err(|_| RpcError::invalid_params())?;
                            let mut intent =
                                durable::read(context.engine.store(), &access, &operation)?;
                            let job = snapshot_job(context.engine.store(), &access, &operation)?;
                            if intent.revision.get() != counter(&p.expected_operation_revision)?
                                || job.as_ref().map(|j| j.revision.get())
                                    != p.expected_job_revision.as_ref().map(counter).transpose()?
                            {
                                return Err(failure(Code::VersionConflict));
                            }
                            let expected = intent.revision;
                            intent.revision = intent
                                .revision
                                .next()
                                .map_err(|_| failure(Code::ResourceLimit))?;
                            intent.cancel_requested = true;
                            (intent, Some(expected), Action::Cancel(operation))
                        }
                        _ => return Err(RpcError::invalid_params()),
                    };
                    let receipt = context.runtime.block_on(durable::commit(
                        context.engine.store_mut(),
                        &access,
                        &call,
                        &intent,
                        expected,
                    ))?;
                    if let Action::Cancel(operation) = &action {
                        let _stopped = manager.request_backup_stop(operation);
                    }
                    let result = vcp_engine::rpc::acceptance(&context.engine, &access, &receipt)
                        .map_err(|_| unknown(&call))?;
                    Ok((result, Some(action)))
                })())
            })
            .map_err(|_| {
                if fallback.command_id().is_some() {
                    unknown(&fallback)
                } else {
                    failure(Code::StoreUnavailable)
                }
            })??;
        // Once accepted, failures scheduling native work remain visible as an
        // interrupted intent. Replaying the receipt never schedules a second job.
        match action {
            Some(Action::Start {
                intent,
                capability,
                retry,
            }) => {
                let manager = host.clone();
                let connected = self.connected.clone();
                let signal = connected.clone();
                let operation = intent.operation.clone();
                let fence = PublicFence::new(
                    move |context| {
                        if !connected.load(Ordering::SeqCst) {
                            return Err("publisher connection lost".into());
                        }
                        context
                            .public_authorize(&owned_access, &caller, lease.as_ref(), true)
                            .map_err(|_| "publisher authority changed".to_owned())?;
                        if context.authority_pending || context.capture_admission_blocked() {
                            return Err("publisher admission fenced".into());
                        }
                        let workspace = workspace(context, &owned_access)
                            .map_err(|_| "publisher workspace unavailable".to_owned())?;
                        if workspace.trust != Trust::Trusted
                            || workspace.authority != intent.authority
                            || workspace.deletion != intent.deletion
                            || workspace.binding.revision != intent.binding
                        {
                            return Err("publisher workspace changed".into());
                        }
                        let current =
                            durable::read(context.engine.store(), &owned_access, &intent.operation)
                                .map_err(|_| "publisher intent unavailable".to_owned())?;
                        if current.revision != intent.revision || current.cancel_requested {
                            return Err("publisher intent changed".into());
                        }
                        let snapshot = manager.backup_manager_snapshot()?;
                        if snapshot.capability.as_ref().is_none_or(|cap| {
                            cap.reference != intent.capability
                                || cap.generation != intent.generation
                                || cap.configuration_revision != intent.configuration_revision
                        }) {
                            return Err("publisher capability changed".into());
                        }
                        Ok(())
                    },
                    move || !signal.load(Ordering::SeqCst),
                );
                let _runtime = self.reactor.enter();
                let _scheduled = host.start_backup_fenced(operation, retry, &capability, fence);
            }
            Some(Action::Cancel(operation)) => {
                let _ = operation; // Stop was signalled on the serialized worker.
            }
            None => {}
        }
        Ok(result)
    }
}
