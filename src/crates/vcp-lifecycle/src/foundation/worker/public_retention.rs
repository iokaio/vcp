// SPDX-License-Identifier: Apache-2.0
//! Connection-scoped exact retention previews and owned accepted-job cleanup.
use super::{public_connection::PublicConnection, *};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};
use vcp_memory::{
    retention::{Action, PruneReceipt, Target},
    retention_public as service,
};
use vcp_protocol::{
    errors::{ApplicationError, Code, Retry},
    jsonrpc::RpcError,
    memory_retention as wire,
    methods::{self, Call, ResultValue},
};
type RpcResult<T> = std::result::Result<T, RpcError>;
const CACHE_BYTES: usize = 1024 * 1024;
const CACHE_ENTRIES: usize = 2;
#[derive(Default)]
pub(super) struct Previews(BTreeMap<String, Cached>);
#[derive(Clone)]
struct Cached {
    scope: Scope,
    preview: service::Preview,
    expires: Instant,
}
impl Previews {
    pub(super) fn clear(&mut self) {
        self.0.clear();
    }
    fn expire(&mut self) {
        let now = Instant::now();
        self.0.retain(|_, v| v.expires > now);
    }
    fn get(&mut self, id: &str, scope: &Scope, call: &Call) -> RpcResult<Cached> {
        self.expire();
        self.0
            .get(id)
            .filter(|entry| entry.scope == *scope)
            .cloned()
            .ok_or_else(|| failure(Code::CursorGap, call))
    }
    fn insert(&mut self, value: Cached) {
        self.expire();
        if self.0.len() == CACHE_ENTRIES && !self.0.contains_key(value.preview.id()) {
            if let Some(oldest) = self
                .0
                .iter()
                .min_by_key(|(_, v)| v.expires)
                .map(|(key, _)| key.clone())
            {
                self.0.remove(&oldest);
            }
        }
        self.0.insert(value.preview.id().to_owned(), value);
    }
}
fn failure(code: Code, call: &Call) -> RpcError {
    ApplicationError {
        code,
        retry: Retry::AfterRevalidation,
        operation: call.command_id().cloned(),
        explanation: "scoped retention unavailable".into(),
        reconciliation: None,
    }
    .into_rpc()
}
fn unknown(call: &Call) -> RpcError {
    ApplicationError {
        code: Code::OutcomeUnknown,
        retry: Retry::ReconcileOriginal,
        operation: call.command_id().cloned(),
        explanation: "reconcile the original retention command".into(),
        reconciliation: None,
    }
    .into_rpc()
}
fn memory_error(error: vcp_memory::Error, call: &Call) -> RpcError {
    let code = match error {
        vcp_memory::Error::Access => Code::PolicyDenied,
        vcp_memory::Error::Conflict(
            "retention command payload conflict" | "retention job command identity",
        ) => Code::CommandConflict,
        vcp_memory::Error::Conflict("retention preview cache limit" | "retention source limit") => {
            Code::ResourceLimit
        }
        vcp_memory::Error::Conflict(_) => Code::VersionConflict,
        vcp_memory::Error::Store(vcp_store::Error::Limit(_)) => Code::ResourceLimit,
        vcp_memory::Error::Store(
            vcp_store::Error::Io(_)
            | vcp_store::Error::Database(_)
            | vcp_store::Error::Unavailable(_),
        ) if call.is_mutation() => return unknown(call),
        _ => Code::StoreUnavailable,
    };
    failure(code, call)
}
fn id(value: &str) -> RpcResult<methods::Id> {
    value
        .to_owned()
        .try_into()
        .map_err(|_| RpcError::internal_error())
}
fn scope(value: &methods::Scope, task: &methods::Id) -> RpcResult<Scope> {
    Ok(Scope {
        workspace: WorkspaceId::parse(value.workspace.as_str())
            .map_err(|_| RpcError::invalid_params())?,
        session: SessionId::parse(value.session.as_str())
            .map_err(|_| RpcError::invalid_params())?,
        task: TaskId::parse(task.as_str()).map_err(|_| RpcError::invalid_params())?,
    })
}
fn action(value: wire::Action) -> Action {
    match value {
        wire::Action::Exclude => Action::Exclude,
        wire::Action::RestoreRecall => Action::RestoreRecall,
        wire::Action::Compact => Action::Compact,
        wire::Action::Purge => Action::Purge,
    }
}
fn public_action(value: Action) -> wire::Action {
    match value {
        Action::Exclude => wire::Action::Exclude,
        Action::RestoreRecall => wire::Action::RestoreRecall,
        Action::Compact => wire::Action::Compact,
        Action::Purge => wire::Action::Purge,
    }
}
fn scoped_access(
    context: &Context,
    access: &Access,
    own: &Scope,
    write: bool,
    call: &Call,
) -> RpcResult<vcp_memory::access::Access> {
    if !access.read
        || (write && !access.write)
        || own.workspace != access.workspace
        || own.session != access.session
    {
        return Err(failure(Code::PolicyDenied, call));
    }
    let mut tasks = BTreeSet::new();
    for row in context
        .engine
        .store()
        .state()
        .records
        .values()
        .filter(|row| row.workspace == access.workspace && row.collection == Collection::Task)
    {
        let task: Task = row
            .decode()
            .map_err(|_| failure(Code::StoreUnavailable, call))?;
        if task.scope.workspace != row.workspace
            || task.scope.task.as_str() != row.id
            || task.revision != row.revision
        {
            return Err(failure(Code::StoreUnavailable, call));
        }
        if task.scope.session == access.session {
            tasks.insert(task.scope.task);
        }
    }
    if !tasks.contains(&own.task) {
        return Err(failure(Code::PolicyDenied, call));
    }
    // Redacted task identities remain scoped evidence for accepted-job replay;
    // shared fresh-apply admission rejects a redacted anchor itself.
    Ok(vcp_memory::access::Access {
        workspace: access.workspace.clone(),
        actor: access.actor.clone(),
        authority: access.authority,
        read: access.read,
        write,
        tasks: Some(tasks),
    })
}
fn page(cached: &Cached, offset: u32, limit: u32, call: &Call) -> RpcResult<wire::PreviewPage> {
    let selected = cached.preview.selection();
    let closure: BTreeSet<_> = selected.selected.union(&selected.dependent).collect();
    let total = closure.len();
    let mut protected: BTreeMap<&Target, BTreeSet<&str>> = BTreeMap::new();
    for entry in &selected.protected {
        if !closure.contains(&entry.target) {
            return Err(failure(Code::StoreUnavailable, call));
        }
        protected
            .entry(&entry.target)
            .or_default()
            .insert(&entry.reason);
    }
    if offset as usize > total || limit == 0 || limit > wire::MAX_TARGETS {
        return Err(RpcError::invalid_params());
    }
    let mut targets = Vec::new();
    for target in closure
        .iter()
        .copied()
        .skip(offset as usize)
        .take(limit as usize)
    {
        targets.push(wire::PreviewTarget {
            target: match target {
                Target::Record(key) => wire::Target::Record(key.clone()),
                Target::Event(event) => wire::Target::Event(id(event.as_str())?),
            },
            selected: selected.selected.contains(target),
            protected_reason: protected
                .get(target)
                .map(|reasons| reasons.iter().copied().collect::<Vec<_>>().join("; ")),
        });
    }
    let next = offset as usize + targets.len();
    let result = wire::PreviewPage {
        scope: methods::Scope {
            workspace: id(cached.scope.workspace.as_str())?,
            session: id(cached.scope.session.as_str())?,
        },
        task: id(cached.scope.task.as_str())?,
        preview: id(cached.preview.id())?,
        digest: cached
            .preview
            .digest()
            .map_err(|e| memory_error(e, call))?
            .try_into()
            .map_err(|_| RpcError::internal_error())?,
        action: public_action(selected.action),
        watermark: selected.watermark.get().into(),
        authority: selected.authority.get().into(),
        deletion: selected.deletion.get().into(),
        selected_count: (selected.selected.len() as u64).into(),
        dependent_count: (selected.dependent.len() as u64).into(),
        protected_count: (protected.len() as u64).into(),
        retained_bytes: selected.retained_bytes.into(),
        bytes_are_exact: selected.bytes_are_exact,
        offset,
        next_offset: (next < total).then_some(next as u32),
        targets,
        expires_in_ms: cached
            .expires
            .saturating_duration_since(Instant::now())
            .as_millis()
            .min(u128::from(wire::PREVIEW_TTL_MS)) as u32,
    };
    result
        .validate()
        .map_err(|_| failure(Code::ResourceLimit, call))?;
    // Cache admission bounded the complete private selection to one MiB.
    // Leave room for the RPC envelope in the public 256 KiB frame profile.
    if serde_json::to_vec(&result)
        .map_err(|_| failure(Code::StoreUnavailable, call))?
        .len()
        > 240 * 1024
    {
        return Err(failure(Code::ResourceLimit, call));
    }
    Ok(result)
}
fn job_view(own: &Scope, job: &PruneReceipt) -> RpcResult<wire::JobView> {
    Ok(wire::JobView {
        scope: methods::Scope {
            workspace: id(own.workspace.as_str())?,
            session: id(own.session.as_str())?,
        },
        task: id(own.task.as_str())?,
        job: id(&job.id)?,
        action: public_action(job.preview.action),
        deletion: job.deletion.get().into(),
        logical_unavailable: job.logical_unavailable,
        rewrite_complete: job.rewrite_complete,
        local_cleanup_complete: job.local_cleanup_complete,
        pending_generations_count: (job.pending_generations.len() as u64).into(),
        backup_copies_count: (job.backup_copies.len() as u64).into(),
        cleanup_required: !job.rewrite_complete
            || !job.local_cleanup_complete
            || !job.pending_generations.is_empty()
            || !job.backup_copies.is_empty(),
    })
}
impl PublicConnection {
    pub(super) async fn retention_call(
        &mut self,
        call: Call,
        current: &Access,
    ) -> RpcResult<ResultValue> {
        call.validate().map_err(|_| RpcError::invalid_params())?;
        if matches!(call, Call::MemoryForget(_)) {
            return self.forget_owned(call, current).await;
        }
        let (host, access, connection, token) = self
            .rpc_context(current)
            .map_err(|_| failure(Code::PolicyDenied, &call))?;
        let previews = self.retention_previews.clone();
        let connected = self.connected.clone();
        let fallback = call.clone();
        let worker = host.worker.clone();
        host.worker
            .run_cleanup(move |context| {
                Ok((|| -> RpcResult<_> {
                    if !connected.load(Ordering::SeqCst) || worker.fenced() {
                        return Err(failure(Code::PolicyDenied, &call));
                    }
                    context
                        .public_authorize(&access, &connection, token.as_ref(), false)
                        .map_err(|_| failure(Code::PolicyDenied, &call))?;
                    let result = match &call {
                        Call::MemoryForgetPreview(request) => {
                            let own = scope(&request.scope, &request.task)?;
                            let memory = scoped_access(context, &access, &own, false, &call)?;
                            let preview = service::preview(
                                context.engine.store(),
                                &memory,
                                own.clone(),
                                request
                                    .selector
                                    .normalized()
                                    .map_err(|_| RpcError::invalid_params())?,
                                action(request.action),
                                now(),
                            )
                            .map_err(|e| memory_error(e, &call))?;
                            let selection = preview.selection();
                            if selection.selected.union(&selection.dependent).count()
                                > wire::MAX_OFFSET as usize
                            {
                                return Err(failure(Code::ResourceLimit, &call));
                            }
                            preview
                                .cache_bytes(CACHE_BYTES)
                                .map_err(|e| memory_error(e, &call))?;
                            let cached = Cached {
                                scope: own,
                                preview,
                                expires: Instant::now()
                                    + Duration::from_millis(u64::from(wire::PREVIEW_TTL_MS)),
                            };
                            let result = page(&cached, 0, request.limit, &call)?;
                            previews
                                .lock()
                                .map_err(|_| failure(Code::StoreUnavailable, &call))?
                                .insert(cached);
                            Ok(ResultValue::RetentionPreview(result))
                        }
                        Call::MemoryForgetPreviewRead(request) => {
                            let own = scope(&request.scope, &request.task)?;
                            let memory = scoped_access(context, &access, &own, false, &call)?;
                            let cached = previews
                                .lock()
                                .map_err(|_| failure(Code::StoreUnavailable, &call))?
                                .get(request.preview.as_str(), &own, &call)?;
                            service::validate_preview(
                                context.engine.store(),
                                &memory,
                                &own,
                                &cached.preview,
                            )
                            .map_err(|e| memory_error(e, &call))?;
                            Ok(ResultValue::RetentionPreview(page(
                                &cached,
                                request.offset,
                                request.limit,
                                &call,
                            )?))
                        }
                        Call::MemoryForgetRead(request) => {
                            let own = scope(&request.scope, &request.task)?;
                            let memory = scoped_access(context, &access, &own, false, &call)?;
                            let job = service::read_job(
                                context.engine.store(),
                                &memory,
                                &own,
                                request.job.as_str(),
                            )
                            .map_err(|e| memory_error(e, &call))?;
                            Ok(ResultValue::Retention(job_view(&own, &job)?))
                        }
                        _ => Err(RpcError::invalid_params()),
                    }?;
                    if !connected.load(Ordering::SeqCst) || worker.fenced() {
                        return Err(failure(Code::PolicyDenied, &call));
                    }
                    context
                        .public_authorize(&access, &connection, token.as_ref(), false)
                        .map_err(|_| failure(Code::PolicyDenied, &call))?;
                    Ok(result)
                })())
            })
            .map_err(|_| failure(Code::StoreUnavailable, &fallback))?
    }

    async fn forget_owned(&self, call: Call, current: &Access) -> RpcResult<ResultValue> {
        let (host, access, connection, token) = self
            .rpc_context(current)
            .map_err(|_| failure(Code::PolicyDenied, &call))?;
        let token = token.ok_or_else(|| failure(Code::PolicyDenied, &call))?;
        let Call::MemoryForget(request) = &call else {
            return Err(RpcError::invalid_params());
        };
        let request = request.clone();
        let own = scope(&request.scope, &request.task)?;
        let operation = service::Apply {
            command: CommandId::parse(request.mutation.command_id.as_str())
                .map_err(|_| RpcError::invalid_params())?,
            command_digest: call
                .digest(access.actor.as_str())
                .map_err(|_| RpcError::invalid_params())?,
            expected_revision: Revision::new(
                request
                    .mutation
                    .expected_revision
                    .as_str()
                    .parse()
                    .map_err(|_| RpcError::invalid_params())?,
            ),
            steering: SteeringRevision::new(
                request
                    .mutation
                    .steering_revision
                    .as_str()
                    .parse()
                    .map_err(|_| RpcError::invalid_params())?,
            ),
        };
        let connected = self.connected.clone();
        let previews = self.retention_previews.clone();
        let fallback = call.clone();
        let held = Arc::new(AtomicBool::new(false));
        let hold_marker = held.clone();
        // Dropping this JoinHandle only abandons the response. The host-owned
        // apply/drain/cleanup sequence retains its runtime and original intent.
        self.reactor
            .spawn(async move {
                struct Guard {
                    worker: Worker,
                    held: Arc<AtomicBool>,
                    complete: bool,
                }
                impl Drop for Guard {
                    fn drop(&mut self) {
                        if self.held.load(Ordering::SeqCst) && !self.complete {
                            self.worker.fence();
                        }
                    }
                }
                let mut guard = Guard {
                    worker: host.worker.clone(),
                    held,
                    complete: false,
                };
                let first_host = host.clone();
                let first_access = access.clone();
                let first_connection = connection.clone();
                let first_token = token.clone();
                let first_connected = connected.clone();
                let first_call = call.clone();
                let first_scope = own.clone();
                let first = tokio::task::spawn_blocking(move || {
                    let worker = first_host.worker.clone();
                    let runner = worker.clone();
                    let fallback = first_call.clone();
                    runner
                        .run_cleanup(move |context| {
                            Ok((|| -> RpcResult<_> {
                                if !first_connected.load(Ordering::SeqCst) || worker.fenced() {
                                    return Err(failure(Code::PolicyDenied, &first_call));
                                }
                                context
                                    .public_authorize(
                                        &first_access,
                                        &first_connection,
                                        Some(&first_token),
                                        true,
                                    )
                                    .map_err(|_| failure(Code::PolicyDenied, &first_call))?;
                                let memory = scoped_access(
                                    context,
                                    &first_access,
                                    &first_scope,
                                    true,
                                    &first_call,
                                )?;
                                // Durable intent wins over expired/evicted connection cache.
                                let replay = service::replay(
                                    context.engine.store(),
                                    &memory,
                                    &first_scope,
                                    request.preview.as_str(),
                                    &operation,
                                )
                                .map_err(|e| memory_error(e, &first_call))?;
                                let cached = if replay.is_none() {
                                    let value = previews
                                        .lock()
                                        .map_err(|_| failure(Code::StoreUnavailable, &first_call))?
                                        .get(request.preview.as_str(), &first_scope, &first_call)?;
                                    service::validate_preview(
                                        context.engine.store(),
                                        &memory,
                                        &first_scope,
                                        &value.preview,
                                    )
                                    .map_err(|e| memory_error(e, &first_call))?;
                                    if value
                                        .preview
                                        .digest()
                                        .map_err(|e| memory_error(e, &first_call))?
                                        != request.preview_digest
                                    {
                                        return Err(failure(Code::VersionConflict, &first_call));
                                    }
                                    let task: Task = context
                                        .engine
                                        .store()
                                        .state()
                                        .record(
                                            Collection::Task,
                                            first_scope.task.as_str(),
                                            &first_scope.workspace,
                                        )
                                        .and_then(Record::decode)
                                        .map_err(|_| {
                                            failure(Code::StoreUnavailable, &first_call)
                                        })?;
                                    if task.redaction.is_some()
                                        || task.revision != operation.expected_revision
                                        || task.steering != operation.steering
                                    {
                                        return Err(failure(Code::VersionConflict, &first_call));
                                    }
                                    Some(value)
                                } else {
                                    None
                                };
                                if !first_connected.load(Ordering::SeqCst) || worker.fenced() {
                                    return Err(failure(Code::PolicyDenied, &first_call));
                                }
                                context
                                    .public_authorize(
                                        &first_access,
                                        &first_connection,
                                        Some(&first_token),
                                        true,
                                    )
                                    .map_err(|_| failure(Code::PolicyDenied, &first_call))?;
                                #[cfg(windows)]
                                let needs_hold = first_host.mcp_connections_present();
                                #[cfg(not(windows))]
                                let needs_hold = false;
                                let hold = if needs_hold {
                                    hold_marker.store(true, Ordering::SeqCst);
                                    Some(
                                        first_host
                                            .runtime
                                            .hold_owner()
                                            .map_err(|_| unknown(&first_call))?,
                                    )
                                } else {
                                    None
                                };
                                let mut result = match replay {
                                    Some(commit) => Ok(commit),
                                    None => match cached {
                                        Some(cached) => context
                                            .runtime
                                            .block_on(service::apply(
                                                context.engine.store_mut(),
                                                &memory,
                                                &cached.preview,
                                                &request.preview_digest,
                                                &operation,
                                                now(),
                                            ))
                                            .map_err(|e| {
                                                if context
                                                    .engine
                                                    .store()
                                                    .state()
                                                    .commands
                                                    .contains_key(
                                                        &vcp_store::contract::command_key(
                                                            &first_scope.workspace,
                                                            &operation.command,
                                                        ),
                                                    )
                                                    || matches!(
                                                        &e,
                                                        vcp_memory::Error::Conflict(
                                                            "retention receipt unavailable"
                                                        )
                                                    )
                                                {
                                                    unknown(&first_call)
                                                } else {
                                                    memory_error(e, &first_call)
                                                }
                                            }),
                                        None => Err(RpcError::internal_error()),
                                    },
                                };
                                // The generation hold precedes deletion. Canonical pauses
                                // follow exact-preview commit, never invalidate it first.
                                if needs_hold {
                                    let paused = (|| -> RpcResult<()> {
                                        let tasks = context
                                            .engine
                                            .store()
                                            .state()
                                            .records
                                            .values()
                                            .filter(|row| {
                                                row.collection == Collection::Task
                                                    && row.workspace == first_scope.workspace
                                            })
                                            .map(Record::decode::<Task>)
                                            .collect::<std::result::Result<Vec<_>, _>>()
                                            .map_err(|_| unknown(&first_call))?;
                                        for task in tasks {
                                            if task.scope.session == first_scope.session
                                                && !task.state.terminal()
                                                && task.state != TaskState::Paused
                                            {
                                                context
                        .command(
                            Command::Transition {
                                next: TaskState::Paused,
                                reason:
                                    "public retention interrupted MCP; deliberate resume required"
                                        .into(),
                                verification: None,
                            },
                            Some(task.scope.task),
                            task.revision,
                        )
                        .map_err(|_| unknown(&first_call))?;
                                            }
                                        }
                                        Ok(())
                                    })();
                                    if let Err(error) = paused {
                                        result = Err(error);
                                    }
                                }
                                Ok((result, hold))
                            })())
                        })
                        .map_err(|_| unknown(&fallback))?
                })
                .await
                .map_err(|_| unknown(&call))??;
                let (committed, hold) = first;
                if let Some(waiter) = hold {
                    super::public_connection::drain_owned_dependencies(
                        &host,
                        Duration::from_secs(5),
                    )
                    .await
                    .map_err(|_| unknown(&call))?;
                    tokio::time::timeout(Duration::from_secs(5), waiter.wait())
                        .await
                        .map_err(|_| unknown(&call))?
                        .map_err(|_| unknown(&call))?;
                }
                // The owned interruption is now complete; authorization loss may
                // leave cleanup pending, but must not fence fresh-controller retry.
                guard.complete = true;
                let committed = committed?;
                let second_call = call.clone();
                let result = tokio::task::spawn_blocking(move || {
                    let worker = host.worker.clone();
                    let fallback = second_call.clone();
                    host.worker
                        .run_cleanup(move |context| {
                            Ok((|| -> RpcResult<_> {
                                if !connected.load(Ordering::SeqCst) || worker.fenced() {
                                    return Err(unknown(&second_call));
                                }
                                context
                                    .public_authorize(&access, &connection, Some(&token), true)
                                    .map_err(|_| unknown(&second_call))?;
                                let memory =
                                    scoped_access(context, &access, &own, true, &second_call)
                                        .map_err(|_| unknown(&second_call))?;
                                let job = context
                                    .runtime
                                    .block_on(service::cleanup(
                                        context.engine.store_mut(),
                                        &memory,
                                        &own,
                                        &committed.job.id,
                                        now(),
                                    ))
                                    .map_err(|_| unknown(&second_call))?;
                                if !connected.load(Ordering::SeqCst) || worker.fenced() {
                                    return Err(unknown(&second_call));
                                }
                                context
                                    .public_authorize(&access, &connection, Some(&token), true)
                                    .map_err(|_| unknown(&second_call))?;
                                let receipt = committed
                                    .receipt
                                    .command
                                    .as_ref()
                                    .ok_or_else(|| unknown(&second_call))?;
                                let ResultValue::Acceptance(acceptance) =
                                    vcp_engine::rpc::acceptance(&context.engine, &access, receipt)
                                        .map_err(|_| unknown(&second_call))?
                                else {
                                    return Err(unknown(&second_call));
                                };
                                Ok(ResultValue::Forgotten(wire::ForgetResult {
                                    acceptance,
                                    job: job_view(&own, &job).map_err(|_| unknown(&second_call))?,
                                }))
                            })())
                        })
                        .map_err(|_| unknown(&fallback))?
                })
                .await
                .map_err(|_| unknown(&call))?;
                result
            })
            .await
            .map_err(|_| unknown(&fallback))?
    }
}
