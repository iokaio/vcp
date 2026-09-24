// SPDX-License-Identifier: Apache-2.0
//! Authenticated optimizer commands; previews are disposable, receipts are durable.
use super::{public_connection::PublicConnection, *};
use crate::foundation::routing_state::{self, public_optimizer as service};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};
use vcp_domain::workspace::{Trust, Workspace};
use vcp_protocol::{
    errors::{ApplicationError, Code, Retry},
    jsonrpc::RpcError,
    methods::{self, Call, ResultValue},
    routing_optimizer as wire,
};
mod preview;
mod read;
type RpcResult<T> = std::result::Result<T, RpcError>;

fn failure(code: Code) -> RpcError {
    ApplicationError {
        code,
        retry: Retry::AfterRevalidation,
        operation: None,
        explanation: "optimizer operation unavailable; revalidate current state".into(),
        reconciliation: None,
    }
    .into_rpc()
}
fn unknown(call: &Call) -> RpcError {
    ApplicationError {
        code: Code::OutcomeUnknown,
        retry: Retry::ReconcileOriginal,
        operation: call.command_id().cloned(),
        explanation: "reconcile the original optimizer command".into(),
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
fn workspace(store: &Store, access: &Access) -> RpcResult<Workspace> {
    let value: Workspace = store
        .state()
        .record(
            Collection::Workspace,
            access.workspace.as_str(),
            &access.workspace,
        )
        .map_err(|_| failure(Code::StoreUnavailable))?
        .decode()
        .map_err(|_| failure(Code::StoreUnavailable))?;
    if value.id != access.workspace || value.authority != access.authority || !access.read {
        return Err(failure(Code::PolicyDenied));
    }
    Ok(value)
}
fn memory_access(
    store: &Store,
    access: &Access,
    global: bool,
    write: bool,
    check: &dyn Fn() -> RpcResult<()>,
) -> RpcResult<vcp_memory::access::Access> {
    check()?;
    workspace(store, access)?;
    if write && !access.write {
        return Err(failure(Code::PolicyDenied));
    }
    let tasks = if global {
        None
    } else {
        let mut tasks = BTreeSet::new();
        for row in store.state().records.values() {
            check()?;
            if row.collection != Collection::Task || row.workspace != access.workspace {
                continue;
            }
            let task: Task = row.decode().map_err(|_| failure(Code::StoreUnavailable))?;
            if task.scope.workspace != access.workspace
                || task.scope.task.as_str() != row.id
                || task.revision != row.revision
            {
                return Err(failure(Code::StoreUnavailable));
            }
            if task.scope.session == access.session && task.redaction.is_none() {
                tasks.insert(task.scope.task);
            }
        }
        Some(tasks)
    };
    Ok(vcp_memory::access::Access {
        workspace: access.workspace.clone(),
        actor: access.actor.clone(),
        authority: access.authority,
        read: true,
        write,
        tasks,
    })
}

#[derive(Clone, serde::Serialize)]
enum Proposal {
    Apply(routing_state::Preview),
    Rollback(service::RollbackPreview),
}
#[derive(Clone)]
struct Cached {
    id: String,
    sha256: String,
    scope: methods::Scope,
    report: Option<methods::Id>,
    proposal: Proposal,
    workspace: Workspace,
    expires: Instant,
}
#[derive(Default)]
pub(super) struct Previews(BTreeMap<String, Cached>);
impl Previews {
    pub(super) fn clear(&mut self) {
        self.0.clear();
    }
    fn expire(&mut self) {
        self.0.retain(|_, value| value.expires > Instant::now());
    }
    fn insert(&mut self, value: Cached) {
        self.expire();
        if self.0.len() >= 2 {
            if let Some(oldest) = self
                .0
                .iter()
                .min_by_key(|(_, v)| v.expires)
                .map(|(k, _)| k.clone())
            {
                self.0.remove(&oldest);
            }
        }
        self.0.insert(value.id.clone(), value);
    }
    fn get(&mut self, id: &str, sha: &str, scope: &methods::Scope) -> RpcResult<Cached> {
        self.expire();
        self.0
            .get(id)
            .filter(|value| value.sha256 == sha && &value.scope == scope)
            .cloned()
            .ok_or_else(|| failure(Code::CursorGap))
    }
}

fn service_error(error: service::Error) -> RpcError {
    failure(match error {
        service::Error::Access => Code::PolicyDenied,
        service::Error::Invalid => return RpcError::invalid_params(),
        service::Error::CommandConflict => Code::CommandConflict,
        service::Error::Limit => Code::ResourceLimit,
        service::Error::Stale => Code::VersionConflict,
        service::Error::Cancelled => Code::CursorGap,
        service::Error::Unavailable | service::Error::OutcomeUnknown => Code::StoreUnavailable,
    })
}
fn command(
    call: &Call,
    access: &Access,
    binding: &methods::Counter,
) -> RpcResult<service::Command> {
    let mutation = call.mutation().ok_or_else(RpcError::invalid_params)?;
    if counter(&mutation.steering_revision)? != 0 {
        return Err(RpcError::invalid_params());
    }
    Ok(service::Command {
        workspace: access.workspace.clone(),
        session: access.session.clone(),
        actor: access.actor.clone(),
        id: CommandId::parse(mutation.command_id.as_str())
            .map_err(|_| RpcError::invalid_params())?,
        digest: call
            .digest(access.actor.as_str())
            .map_err(|_| RpcError::invalid_params())?,
        expected_revision: Revision::new(counter(&mutation.expected_revision)?),
        expected_binding_revision: Revision::new(counter(binding)?),
    })
}
fn scope(call: &Call) -> RpcResult<&methods::Scope> {
    match call {
        Call::RoutingReportCapture(p) => Ok(&p.scope),
        Call::RoutingReportRead(p) => Ok(&p.scope),
        Call::RoutingPreview(p) => Ok(&p.scope),
        Call::RoutingApply(p) => Ok(&p.scope),
        Call::RoutingRollback(p) => Ok(&p.scope),
        _ => Err(RpcError::invalid_params()),
    }
}
impl PublicConnection {
    pub(super) fn optimizer_call(&self, call: Call, current: &Access) -> RpcResult<ResultValue> {
        call.validate().map_err(|_| RpcError::invalid_params())?;
        let (host, access, connection, token) = self
            .rpc_context(current)
            .map_err(|_| failure(Code::PolicyDenied))?;
        let own = scope(&call)?;
        if own.workspace.as_str() != access.workspace.as_str()
            || own.session.as_str() != access.session.as_str()
        {
            return Err(failure(Code::PolicyDenied));
        }
        let fallback = call.clone();
        let connected = self.connected.clone();
        let previews = self.optimizer_previews.clone();
        let worker = host.worker.clone();
        host.worker
            .run_cleanup(move |context| {
                Ok((|| -> RpcResult<_> {
                    let start = Instant::now();
                    let check = || {
                        if !connected.load(Ordering::SeqCst)
                            || worker.fenced()
                            || start.elapsed() >= Duration::from_secs(2)
                        {
                            Err(failure(Code::CursorGap))
                        } else {
                            Ok(())
                        }
                    };
                    let service_check = || check().map_err(|_| service::Error::Cancelled);
                    check()?;
                    let is_read = matches!(&call, Call::RoutingReportRead(_));
                    context
                        .public_authorize(&access, &connection, token.as_ref(), !is_read)
                        .map_err(|_| failure(Code::PolicyDenied))?;
                    if let Call::RoutingReportRead(request) = &call {
                        let global = token.is_some()
                            && context
                                .public_authorize(&access, &connection, token.as_ref(), true)
                                .is_ok();
                        let result =
                            read::read(context.engine.store(), &access, request, global, &check)?;
                        check()?;
                        return Ok(ResultValue::RoutingReport(result));
                    }
                    // Only the controller-authorized branch can request global access.
                    let global =
                        memory_access(context.engine.store(), &access, true, true, &check)?;
                    let binding = match &call {
                        Call::RoutingReportCapture(p) => Some(&p.expected_binding_revision),
                        Call::RoutingApply(p) => Some(&p.expected_binding_revision),
                        Call::RoutingRollback(p) => Some(&p.expected_binding_revision),
                        _ => None,
                    };
                    let command = binding
                        .map(|binding| command(&call, &access, binding))
                        .transpose()?;
                    // Receipt lookup precedes workspace/trust preconditions and the volatile
                    // preview cache. Reopening may discard previews, never accepted outcomes.
                    if let Some(command) = &command {
                        if let Some(committed) =
                            service::replay(context.engine.store(), &global, command)
                                .map_err(service_error)?
                        {
                            return vcp_engine::rpc::acceptance(
                                &context.engine,
                                &access,
                                &committed.receipt,
                            );
                        }
                    }
                    if context.authority_pending || context.capture_admission_blocked() {
                        return Err(failure(Code::AuthorityStale));
                    }
                    let current_workspace = workspace(context.engine.store(), &access)?;
                    if !matches!(&call, Call::RoutingReportCapture(_))
                        && current_workspace.trust != Trust::Trusted
                    {
                        return Err(failure(Code::PolicyDenied));
                    }
                    match &call {
                        Call::RoutingReportCapture(request) => {
                            let command = command.as_ref().ok_or_else(RpcError::internal_error)?;
                            let global_coverage = request.coverage == wire::Coverage::Workspace;
                            let selected = memory_access(
                                context.engine.store(),
                                &access,
                                global_coverage,
                                true,
                                &check,
                            )?;
                            let window = routing_state::HistoryWindow {
                                from: request
                                    .window
                                    .from
                                    .as_ref()
                                    .map(counter)
                                    .transpose()?
                                    .map(Timestamp::new),
                                until: Timestamp::new(counter(&request.window.until)?),
                            };
                            let coverage = if global_coverage {
                                service::Coverage::Workspace
                            } else {
                                service::Coverage::Session
                            };
                            let committed = context
                                .runtime
                                .block_on(service::capture(
                                    context.engine.store_mut(),
                                    &selected,
                                    command,
                                    coverage,
                                    window,
                                    now(),
                                    &service_check,
                                ))
                                .map_err(|e| {
                                    if e == service::Error::OutcomeUnknown {
                                        unknown(&call)
                                    } else {
                                        service_error(e)
                                    }
                                })?;
                            if check().is_err() {
                                return Err(unknown(&call));
                            }
                            vcp_engine::rpc::acceptance(
                                &context.engine,
                                &access,
                                &committed.receipt,
                            )
                            .map_err(|_| unknown(&call))
                        }
                        Call::RoutingPreview(request) => {
                            let ceilings = context
                                .routing_ceilings()
                                .map_err(|_| failure(Code::StoreUnavailable))?
                                .ok_or_else(|| failure(Code::CapabilityUnavailable))?;
                            let current =
                                routing_state::current_policy(context.engine.store(), &global)
                                    .map_err(|_| failure(Code::StoreUnavailable))?
                                    .ok_or_else(|| failure(Code::CapabilityUnavailable))?;
                            if current.revision.get() != counter(&request.expected_policy_revision)?
                            {
                                return Err(failure(Code::VersionConflict));
                            }
                            let (report, proposal) = match &request.proposal {
                                wire::Proposal::Apply { report, edits } => {
                                    let report_command = CommandId::parse(report.as_str())
                                        .map_err(|_| RpcError::invalid_params())?;
                                    let captured = service::read_report(
                                        context.engine.store(),
                                        &global,
                                        &access.session,
                                        &report_command,
                                        &service_check,
                                    )
                                    .map_err(service_error)?;
                                    let proposal = routing_state::preview_with_check(
                                        context.engine.store(),
                                        &global,
                                        &captured.report.id,
                                        preview::edits(edits)?,
                                        &ceilings,
                                        &|| {
                                            service_check()
                                                .map_err(|_| "optimizer interrupted".into())
                                        },
                                    )
                                    .map_err(|_| failure(Code::VersionConflict))?;
                                    (Some(report.clone()), Proposal::Apply(proposal))
                                }
                                wire::Proposal::Rollback { target_revision } => (
                                    None,
                                    Proposal::Rollback(
                                        service::rollback_preview(
                                            context.engine.store(),
                                            &global,
                                            current.revision,
                                            Revision::new(counter(target_revision)?),
                                            &ceilings,
                                            &service_check,
                                        )
                                        .map_err(service_error)?,
                                    ),
                                ),
                            };
                            check()?;
                            let id = CommandId::new().to_string();
                            let bytes = canonical_bytes(&(
                                &id,
                                &access.actor,
                                &request.scope,
                                &current_workspace,
                                &proposal,
                            ))
                            .map_err(|_| failure(Code::StoreUnavailable))?;
                            if bytes.len() > 256 * 1024 {
                                return Err(failure(Code::ResourceLimit));
                            }
                            let cached = Cached {
                                id,
                                sha256: vcp_protocol::digest_bytes(&bytes),
                                scope: request.scope.clone(),
                                report,
                                proposal,
                                workspace: current_workspace,
                                expires: Instant::now() + Duration::from_secs(60),
                            };
                            let view = preview::view(&cached)?;
                            check()?;
                            previews
                                .lock()
                                .map_err(|_| failure(Code::StoreUnavailable))?
                                .insert(cached);
                            Ok(ResultValue::RoutingPreview(view))
                        }
                        Call::RoutingApply(_) | Call::RoutingRollback(_) => {
                            let (preview_id, digest, own) = match &call {
                                Call::RoutingApply(p) => {
                                    (&p.preview_id, &p.preview_sha256, &p.scope)
                                }
                                Call::RoutingRollback(p) => {
                                    (&p.preview_id, &p.preview_sha256, &p.scope)
                                }
                                _ => return Err(RpcError::invalid_params()),
                            };
                            let cached = previews
                                .lock()
                                .map_err(|_| failure(Code::StoreUnavailable))?
                                .get(preview_id.as_str(), digest, own)?;
                            if cached.workspace != current_workspace {
                                return Err(failure(Code::VersionConflict));
                            }
                            let ceilings = context
                                .routing_ceilings()
                                .map_err(|_| failure(Code::StoreUnavailable))?
                                .ok_or_else(|| failure(Code::CapabilityUnavailable))?;
                            let command = command.as_ref().ok_or_else(RpcError::internal_error)?;
                            check()?;
                            let committed = match (&call, &cached.proposal) {
                                (Call::RoutingApply(_), Proposal::Apply(proposal)) => {
                                    context.runtime.block_on(service::apply(
                                        context.engine.store_mut(),
                                        &global,
                                        command,
                                        proposal,
                                        &ceilings,
                                        now(),
                                        &service_check,
                                    ))
                                }
                                (Call::RoutingRollback(_), Proposal::Rollback(proposal)) => {
                                    context.runtime.block_on(service::rollback(
                                        context.engine.store_mut(),
                                        &global,
                                        command,
                                        proposal,
                                        &ceilings,
                                        now(),
                                        &service_check,
                                    ))
                                }
                                _ => return Err(RpcError::invalid_params()),
                            }
                            .map_err(|e| {
                                if e == service::Error::OutcomeUnknown {
                                    unknown(&call)
                                } else {
                                    service_error(e)
                                }
                            })?;
                            if check().is_err() {
                                return Err(unknown(&call));
                            }
                            vcp_engine::rpc::acceptance(
                                &context.engine,
                                &access,
                                &committed.receipt,
                            )
                            .map_err(|_| unknown(&call))
                        }
                        _ => Err(RpcError::invalid_params()),
                    }
                })())
            })
            .map_err(|_| {
                if fallback.is_mutation() {
                    unknown(&fallback)
                } else {
                    failure(Code::StoreUnavailable)
                }
            })?
    }
}
