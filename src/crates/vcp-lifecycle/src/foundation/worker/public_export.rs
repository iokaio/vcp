// SPDX-License-Identifier: Apache-2.0
//! Local derived captures under the authenticated reader's existing disclosure ceiling.
use super::{public_connection::PublicConnection, *};
use vcp_engine::{
    public_export::{ExportDisclosure, PublicExportAdmission, PublicExportCommitError},
    rpc::public_error,
};
use vcp_protocol::{
    errors::{ApplicationError, Code, Retry},
    jsonrpc::RpcError,
    methods::{self, Call},
};
type RpcResult<T> = std::result::Result<T, RpcError>;
fn error(code: Code, request: &methods::SessionExport) -> RpcError {
    ApplicationError {
        code,
        retry: Retry::AfterRevalidation,
        operation: Some(request.mutation.command_id.clone()),
        explanation: "local session export unavailable".into(),
        reconciliation: None,
    }
    .into_rpc()
}
fn unknown(request: &methods::SessionExport) -> RpcError {
    ApplicationError {
        code: Code::OutcomeUnknown,
        retry: Retry::ReconcileOriginal,
        operation: Some(request.mutation.command_id.clone()),
        explanation: "reconcile the original export command".into(),
        reconciliation: None,
    }
    .into_rpc()
}
impl PublicConnection {
    pub(super) fn session_export(
        &self,
        request: &methods::SessionExport,
        current: &Access,
    ) -> RpcResult<methods::ExportView> {
        Call::SessionExport(request.clone())
            .validate()
            .map_err(|_| RpcError::invalid_params())?;
        let (host, access, connection, token) = self
            .rpc_context(current)
            .map_err(|_| error(Code::PolicyDenied, request))?;
        let token = token.ok_or_else(|| error(Code::PolicyDenied, request))?;
        let request = request.clone();
        let fallback = request.clone();
        let connected = self.connected.clone();
        let worker = host.worker.clone();
        let lifecycle = host.runtime.clone();
        host.worker
            .run_cleanup(move |context| {
                Ok((|| -> RpcResult<_> {
                    let stopped = || !connected.load(Ordering::SeqCst) || worker.fenced();
                    if stopped() {
                        return Err(error(Code::StoreUnavailable, &request));
                    }
                    context
                        .public_authorize(&access, &connection, Some(&token), true)
                        .map_err(|_| error(Code::PolicyDenied, &request))?;
                    if context.config.host_tool_denials.iter().any(|denial| {
                        denial
                            .tool
                            .as_deref()
                            .is_none_or(|tool| tool == "session/export")
                            && (denial.roots.is_empty()
                                || denial
                                    .roots
                                    .iter()
                                    .any(|root| root.as_str() == access.workspace.as_str()))
                            && (denial.effects.is_empty()
                                || denial
                                    .effects
                                    .contains(&vcp_domain::policy::EffectClass::Read)
                                || denial
                                    .effects
                                    .contains(&vcp_domain::policy::EffectClass::Write))
                        // An aggregate capture cannot prove a path-specific export
                        // denial irrelevant; conservatively deny the matching tool.
                    }) {
                        return Err(error(Code::PolicyDenied, &request));
                    }
                    // This fixed local profile permits the same retained source bytes
                    // as artifact/read. It does not publish or write a client path.
                    let disclosure = ExportDisclosure {
                        policy: vcp_engine::policy::optional(
                            context.engine.store().state(),
                            &access.workspace,
                        )
                        .map_err(|_| error(Code::StoreUnavailable, &request))?
                        .map_or(PolicyRevision::ZERO, |p| p.revision),
                        history: access.read,
                        artifacts: access.read,
                    };
                    let admission = context
                        .engine
                        .prepare_public_export(
                            request.clone(),
                            &access,
                            &connection,
                            &token,
                            &disclosure,
                            &context.config.root_task,
                        )
                        .map_err(|e| {
                            public_error(e, Some(request.mutation.command_id.clone()), false)
                        })?;
                    let prepared = match admission {
                        PublicExportAdmission::Replay(outcome) => {
                            if stopped() {
                                return Err(unknown(&request));
                            }
                            return Ok(outcome.view);
                        }
                        PublicExportAdmission::Ready(prepared) => prepared,
                    };
                    let history = vcp_audit::history::Access {
                        workspace: access.workspace.clone(),
                        authority: access.authority,
                        read: access.read,
                        tasks: Some(prepared.sources().tasks().clone()),
                    };
                    let rendered = vcp_audit::session_export::render(
                        context.engine.store(),
                        &history,
                        prepared.sources(),
                        prepared.capture(),
                    )
                    .map_err(|e| match e {
                        vcp_audit::Error::Access => error(Code::PolicyDenied, &request),
                        vcp_audit::Error::Limit => error(Code::ResourceLimit, &request),
                        _ => error(Code::StoreUnavailable, &request),
                    })?;
                    if stopped() {
                        return Err(error(Code::StoreUnavailable, &request));
                    }
                    context
                        .public_authorize(&access, &connection, Some(&token), true)
                        .map_err(|_| error(Code::PolicyDenied, &request))?;
                    let outcome = context
                        .runtime
                        .block_on(context.engine.commit_public_export(
                            prepared,
                            &access,
                            &disclosure,
                            rendered,
                            now(),
                        ))
                        .map_err(|e| {
                            match e {
                                PublicExportCommitError::Public(error) => public_error(
                                    error,
                                    Some(request.mutation.command_id.clone()),
                                    false,
                                ),
                                PublicExportCommitError::CaptureFault => {
                                    context.interrupted_capture = true;
                                    // Admission closes synchronously. The lifecycle owns
                                    // the interruption; dropping its waiter cannot abort it.
                                    if lifecycle.hold_owner().is_err() {
                                        worker.fence();
                                    }
                                    if context.pause_root("session export capture failed").is_err()
                                    {
                                        worker.fence();
                                    }
                                    error(Code::StoreUnavailable, &request)
                                }
                            }
                        })?;
                    if stopped() {
                        return Err(unknown(&request));
                    }
                    context
                        .public_authorize(&access, &connection, Some(&token), true)
                        .map_err(|_| unknown(&request))?;
                    Ok(outcome.view)
                })())
            })
            .map_err(|_| unknown(&fallback))?
    }
}
