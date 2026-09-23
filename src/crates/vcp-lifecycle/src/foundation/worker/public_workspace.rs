// SPDX-License-Identifier: Apache-2.0
//! Observe the one binding already selected by authenticated host bootstrap.
//! This facade never resolves a requested path or opens another canonical owner.
use super::public_connection::PublicConnection;
use super::*;
use vcp_protocol::{
    errors::{ApplicationError, Code, Retry},
    jsonrpc::RpcError,
    methods::{self, Call},
};

fn error(code: Code, explanation: &str) -> RpcError {
    ApplicationError {
        code,
        retry: Retry::AfterRevalidation,
        operation: None,
        explanation: explanation.into(),
        reconciliation: None,
    }
    .into_rpc()
}
fn denied() -> RpcError {
    error(
        Code::PolicyDenied,
        "current selected workspace access denied",
    )
}
fn unavailable() -> RpcError {
    error(Code::StoreUnavailable, "selected workspace unavailable")
}
fn id(value: &str) -> std::result::Result<methods::Id, RpcError> {
    value.to_owned().try_into().map_err(|_| unavailable())
}

impl PublicConnection {
    /// Existing authorized binding only, including read-only observers. The
    /// wire command_id is unused here: this read creates no durable command or
    /// receipt. Creating/rebinding a workspace needs separate bootstrap authority.
    /// `root` must exactly match the selected canonical spelling; arbitrary paths,
    /// aliases, UNC names and relative paths are never probed or normalized.
    pub fn workspace_open(
        &self,
        request: &methods::WorkspaceOpen,
        current: &Access,
    ) -> std::result::Result<methods::WorkspaceView, RpcError> {
        let (host, access, connection, token) = self.rpc_context(current).map_err(|_| denied())?;
        let connected = self.connected.clone();
        let request = request.clone();
        host.worker
            .run_cleanup(move |context| {
                Ok((|| -> std::result::Result<_, RpcError> {
                    // Recheck connection loss/current authority inside the same
                    // serialized read that selects the workspace projection.
                    if !connected.load(Ordering::SeqCst) {
                        return Err(denied());
                    }
                    context
                        .public_authorize(&access, &connection, token.as_ref(), false)
                        .map_err(|_| denied())?;
                    Call::WorkspaceOpen(request.clone())
                        .validate()
                        .map_err(|_| RpcError::invalid_params())?;
                    if request.host.as_str() != context.config.binding.host.as_str()
                        || request.root != context.config.binding.root
                    {
                        return Err(denied());
                    }
                    let workspace: Workspace = context
                        .engine
                        .store()
                        .state()
                        .record(
                            Collection::Workspace,
                            access.workspace.as_str(),
                            &access.workspace,
                        )
                        .map_err(|_| unavailable())?
                        .decode()
                        .map_err(|_| unavailable())?;
                    workspace.validate().map_err(|_| unavailable())?;
                    if workspace.id != access.workspace
                        || workspace.binding != context.config.binding
                    {
                        return Err(denied());
                    }
                    Ok(methods::WorkspaceView {
                        workspace: id(workspace.id.as_str())?,
                        host: id(workspace.binding.host.as_str())?,
                        root: workspace.binding.root,
                        trust: match workspace.trust {
                            Trust::Trusted => methods::Trust::Trusted,
                            Trust::Untrusted => methods::Trust::Untrusted,
                        },
                        revision: workspace.revision.get().into(),
                        authority_revision: workspace.authority.get().into(),
                    })
                })())
            })
            .map_err(|_| unavailable())?
    }
}
