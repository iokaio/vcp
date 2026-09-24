// SPDX-License-Identifier: Apache-2.0
//! Public dispatch through the existing serialized host and owned lifecycle.
use super::public_connection::{await_release, PublicConnection};
use super::*;
use vcp_engine::{
    controller::ControllerError,
    public::{PreparedPublicCommand, PublicAdmission, PublicPauseProof},
    rpc::{acceptance, public_error, EngineRpcHost, RpcHost},
};
use vcp_protocol::{
    errors::{ApplicationError, Code, Retry},
    jsonrpc::RpcError,
    methods::{self, Call, ResultValue},
};

const METHODS: &[&str] = &[
    "workspace/open",
    "workspace/setTrust",
    "session/create",
    "session/fork",
    "session/read",
    "session/snapshot",
    "session/list",
    "session/export",
    "task/read",
    "task/presentation",
    "usage/read",
    "context/inspect",
    "routing/explain",
    "policy/read",
    "routing/status",
    "routing/reportCapture",
    "routing/reportRead",
    "routing/preview",
    "routing/apply",
    "routing/rollback",
    "artifact/read",
    "diff/read",
    #[cfg(windows)]
    "editor/context",
    #[cfg(windows)]
    "editor/prepare",
    #[cfg(windows)]
    "editor/changeRead",
    #[cfg(windows)]
    "editor/dispatch",
    #[cfg(windows)]
    "editor/changeResult",
    "memory/inspect",
    "history/query",
    "memory/history",
    "memory/query",
    "memory/propose",
    "memory/resolve",
    "memory/review",
    "memory/forgetPreview",
    "memory/forgetPreviewRead",
    "memory/forgetRead",
    "memory/forget",
    "task/cancel",
    "turn/pause",
    "turn/cancel",
    "turn/steer",
    "approval/respond",
    "command/read",
    "controller/read",
    "controller/acquire",
    "controller/release",
    "controller/recover",
    "events/subscribe",
    "events/next",
    "events/unsubscribe",
];

fn failure(code: Code, retry: Retry, call: &Call, explanation: &str) -> RpcError {
    ApplicationError {
        code,
        retry,
        operation: call.command_id().cloned(),
        explanation: explanation.into(),
        reconciliation: None,
    }
    .into_rpc()
}

fn facts(context: &Context) -> Result<HostFacts> {
    Ok(HostFacts {
        now: now(),
        policy: vcp_engine::policy::optional(
            context.engine.store().state(),
            &context.config.workspace,
        )?
        .map_or(PolicyRevision::ZERO, |policy| policy.revision),
        resume: None,
        may_execute: context.owner_alive,
    })
}

enum Admission {
    Reply(ResultValue),
    Drain {
        prepared: PreparedPublicCommand,
        proof: Option<PublicPauseProof>,
        waiter: Option<crate::HoldWaiter>,
    },
}

fn authority_task(
    record: &Record,
    access: &Access,
    task_id: &TaskId,
) -> std::result::Result<Task, RpcError> {
    let task: Task = record.decode().map_err(|_| RpcError::internal_error())?;
    if record.collection != Collection::Task
        || record.workspace != access.workspace
        || record.id != task_id.as_str()
        || task.scope.workspace != access.workspace
        || task.scope.session != access.session
        || task.scope.task != *task_id
        || task.revision != record.revision
    {
        return Err(RpcError::internal_error());
    }
    Ok(task)
}

#[test]
fn workspace_authority_task_rejects_foreign_scope_and_record_identity() {
    let access = Access {
        actor: ActorId::new(),
        workspace: WorkspaceId::new(),
        session: SessionId::new(),
        authority: AuthorityRevision::ZERO,
        read: true,
        write: true,
        bootstrap: false,
    };
    let task_id = TaskId::new();
    let task = Task {
        scope: Scope {
            workspace: access.workspace.clone(),
            session: access.session.clone(),
            task: task_id.clone(),
        },
        root: task_id.clone(),
        parent: None,
        fork_origin: None,
        revision: Revision::ZERO,
        steering: SteeringRevision::ZERO,
        objectives: vec![],
        state: TaskState::Paused,
        fingerprint: vcp_domain::verification::Fingerprint {
            repository: "a".repeat(64),
            buffers: "b".repeat(64),
            environment: "c".repeat(64),
        },
        editing: false,
        required_checks: vec![],
        cause: EventId::new(),
        reason: "fixture".into(),
        redaction: None,
    };
    let record = Record {
        collection: Collection::Task,
        id: task_id.to_string(),
        workspace: access.workspace.clone(),
        revision: Revision::ZERO,
        value: serde_json::to_value(&task).unwrap(),
        references: Default::default(),
    };
    assert!(authority_task(&record, &access, &task_id).is_ok());
    for mismatch in 0..6 {
        let mut wrong = record.clone();
        let mut wrong_task = task.clone();
        match mismatch {
            0 => wrong.workspace = WorkspaceId::new(),
            1 => wrong.id = TaskId::new().to_string(),
            2 => wrong_task.scope.workspace = WorkspaceId::new(),
            3 => wrong_task.scope.session = SessionId::new(),
            4 => wrong_task.scope.task = TaskId::new(),
            _ => wrong_task.revision = Revision::new(1),
        }
        wrong.value = serde_json::to_value(wrong_task).unwrap();
        assert!(authority_task(&wrong, &access, &task_id).is_err());
    }
}

fn controller_error(error: ControllerError, call: &Call) -> RpcError {
    let (code, retry, explanation) = match error {
        ControllerError::Access => (
            Code::PolicyDenied,
            Retry::AfterRevalidation,
            "current controller access denied",
        ),
        ControllerError::Held => (
            Code::VersionConflict,
            Retry::AfterRevalidation,
            "another controller owns the session",
        ),
        ControllerError::Stale => (
            Code::AuthorityStale,
            Retry::AfterRevalidation,
            "controller ownership or revision changed",
        ),
        ControllerError::CommandConflict => (
            Code::CommandConflict,
            Retry::Never,
            "controller command identity conflicts",
        ),
        ControllerError::RunningTasks => (
            Code::VersionConflict,
            Retry::AfterRevalidation,
            "session tasks require an owned pause",
        ),
        ControllerError::InvalidOperation => (
            Code::PolicyDenied,
            Retry::Never,
            "invalid controller operation",
        ),
        ControllerError::InvalidState => (
            Code::StoreUnavailable,
            Retry::AfterRevalidation,
            "controller state unavailable",
        ),
        ControllerError::OutcomeUnknown => (
            Code::OutcomeUnknown,
            Retry::ReconcileOriginal,
            "controller outcome requires reconciliation",
        ),
    };
    failure(code, retry, call, explanation)
}

impl PublicConnection {
    async fn controller_call(
        &mut self,
        call: Call,
        current: &Access,
    ) -> std::result::Result<ResultValue, RpcError> {
        call.validate()
            .map_err(|_| controller_error(ControllerError::InvalidOperation, &call))?;
        let scope = match &call {
            Call::ControllerRead(p) => &p.scope,
            Call::ControllerAcquire(p) => &p.scope,
            Call::ControllerRelease(p) => &p.scope,
            Call::ControllerRecover(p) => &p.scope,
            _ => return Err(RpcError::internal_error()),
        };
        if scope.workspace.as_str() != current.workspace.as_str()
            || scope.session.as_str() != current.session.as_str()
        {
            return Err(controller_error(ControllerError::Access, &call));
        }
        let revision = |value: &methods::Counter| {
            value
                .as_str()
                .parse::<u64>()
                .map(Revision::new)
                .map_err(|_| controller_error(ControllerError::InvalidOperation, &call))
        };
        let access = current.clone();
        let connection = self.connection.clone();
        let receipt = match &call {
            Call::ControllerRead(p) => {
                let scope = p.scope.clone();
                return self
                    .host
                    .worker
                    .run_cleanup(move |context| {
                        Ok((|| -> std::result::Result<_, ControllerError> {
                            let lease = context.engine.read_controller(&access)?;
                            let ownership = match lease
                                .as_ref()
                                .and_then(|lease| lease.holder.as_ref())
                            {
                                None if lease.is_none() => methods::ControllerOwnership::Unclaimed,
                                None => methods::ControllerOwnership::Released,
                                Some(holder)
                                    if holder.process_owner != *context.engine.controller()
                                        || holder.owner_epoch != context.engine.owner_epoch() =>
                                {
                                    methods::ControllerOwnership::PreviousProcess
                                }
                                Some(holder)
                                    if holder.connection == connection
                                        && holder.actor == access.actor =>
                                {
                                    methods::ControllerOwnership::ThisConnection
                                }
                                Some(_) => methods::ControllerOwnership::OtherConnection,
                            };
                            Ok(ResultValue::Controller(methods::ControllerView {
                                scope,
                                revision: lease.as_ref().map(|lease| lease.revision.get().into()),
                                generation: lease
                                    .as_ref()
                                    .map_or(0, |lease| lease.generation.get())
                                    .into(),
                                ownership,
                                watermark: context.engine.store().state().watermark.get().into(),
                            }))
                        })())
                    })
                    .map_err(|_| controller_error(ControllerError::OutcomeUnknown, &call))?
                    .map_err(|error| controller_error(error, &call));
            }
            Call::ControllerAcquire(p) => {
                let command = CommandId::parse(p.command_id.as_str())
                    .map_err(|_| RpcError::internal_error())?;
                let expected = p.expected_revision.as_ref().map(revision).transpose()?;
                self.acquire_current(current, command, expected)
                    .map_err(|error| controller_error(error, &call))?
            }
            Call::ControllerRelease(p) => {
                let command = CommandId::parse(p.command_id.as_str())
                    .map_err(|_| RpcError::internal_error())?;
                let receiver = self
                    .release_current(
                        current,
                        command,
                        revision(&p.expected_revision)?,
                        revision(&p.generation)?,
                    )
                    .map_err(|error| controller_error(error, &call))?;
                await_release(receiver)
                    .await
                    .map_err(|error| controller_error(error, &call))?
            }
            Call::ControllerRecover(p) => {
                let command = CommandId::parse(p.command_id.as_str())
                    .map_err(|_| RpcError::internal_error())?;
                let expected = revision(&p.expected_revision)?;
                let generation = revision(&p.generation)?;
                self.host
                    .worker
                    .run(move |context| {
                        if !access.write {
                            return Ok(Err(ControllerError::Access));
                        }
                        match context.engine.controller_recover_receipt(
                            &access,
                            &connection,
                            &command,
                            expected,
                            generation,
                        ) {
                            Ok(Some(receipt)) => return Ok(Ok(receipt)),
                            Err(error) => return Ok(Err(error)),
                            Ok(None) => (),
                        }
                        if !context.owner_alive
                            || context.authority_pending
                            || context.public_controller.is_some()
                        {
                            return Ok(Err(ControllerError::Held));
                        }
                        Ok(context.runtime.block_on(context.engine.recover_controller(
                            &access,
                            &connection,
                            command,
                            expected,
                            generation,
                            now(),
                        )))
                    })
                    .map_err(|_| controller_error(ControllerError::OutcomeUnknown, &call))?
                    .map_err(|error| controller_error(error, &call))?
            }
            _ => return Err(RpcError::internal_error()),
        };
        let access = current.clone();
        self.host
            .worker
            .run_cleanup(move |context| Ok(acceptance(&context.engine, &access, &receipt)))
            .map_err(|_| controller_error(ControllerError::OutcomeUnknown, &call))?
    }
}

impl RpcHost for PublicConnection {
    fn supported_methods(&self) -> &[&str] {
        METHODS
    }

    fn authorize(&self, current: &Access) -> std::result::Result<(), RpcError> {
        self.rpc_context(current).map(|_| ()).map_err(|_| {
            ApplicationError {
                code: Code::PolicyDenied,
                retry: Retry::AfterRevalidation,
                operation: None,
                explanation: "current public connection access denied".into(),
                reconciliation: None,
            }
            .into_rpc()
        })
    }

    async fn call(
        &mut self,
        call: Call,
        current: &Access,
    ) -> std::result::Result<ResultValue, RpcError> {
        let (host, access, connection, token) = self.rpc_context(current).map_err(|_| {
            failure(
                Code::PolicyDenied,
                Retry::AfterRevalidation,
                &call,
                "current public connection access denied",
            )
        })?;
        // The existing selected binding is a read, despite the schema also
        // reserving command identity for future explicit creation semantics.
        if let Call::WorkspaceOpen(request) = &call {
            return self
                .workspace_open(request, current)
                .map(ResultValue::Workspace);
        }
        if matches!(
            call,
            Call::SessionSnapshot(_)
                | Call::EventsSubscribe(_)
                | Call::EventsNext(_)
                | Call::EventsUnsubscribe(_)
        ) {
            return self.event_call(call, current);
        }
        if let Call::MemoryInspect(request) = &call {
            return self
                .memory_inspect(request, current)
                .map(ResultValue::Memory);
        }
        if let Call::HistoryQuery(request) = &call {
            return self
                .history_query(request, current)
                .map(ResultValue::History);
        }
        if let Call::MemoryHistory(request) = &call {
            return self
                .memory_history(request, current)
                .map(ResultValue::MemoryHistory);
        }
        if let Call::PolicyRead(request) = &call {
            return self.policy_read(request, current).map(ResultValue::Policy);
        }
        if let Call::RoutingStatus(request) = &call {
            return self
                .routing_status(request, current)
                .map(ResultValue::RoutingStatus);
        }
        if matches!(call, Call::RoutingReportCapture(_) | Call::RoutingReportRead(_)
            | Call::RoutingPreview(_) | Call::RoutingApply(_) | Call::RoutingRollback(_)) {
            return self.optimizer_call(call, current);
        }
        #[cfg(windows)]
        if matches!(call, Call::EditorContext(_) | Call::EditorPrepare(_) | Call::EditorChangeRead(_)
            | Call::EditorDispatch(_) | Call::EditorChangeResult(_)) {
            return self.editor_call(call, current);
        }
        if let Call::SessionExport(request) = &call {
            return self
                .session_export(request, current)
                .map(ResultValue::Export);
        }
        if let Call::MemoryQuery(request) = &call {
            return self
                .memory_query(request, current)
                .await
                .map(ResultValue::MemoryQuery);
        }
        if matches!(
            call,
            Call::MemoryPropose(_) | Call::MemoryResolve(_) | Call::MemoryReview(_)
        ) {
            return self.memory_review_call(call, current).await;
        }
        if matches!(
            call,
            Call::MemoryForgetPreview(_)
                | Call::MemoryForgetPreviewRead(_)
                | Call::MemoryForgetRead(_)
                | Call::MemoryForget(_)
        ) {
            return self.retention_call(call, current).await;
        }
        if matches!(
            call,
            Call::ControllerRead(_)
                | Call::ControllerAcquire(_)
                | Call::ControllerRelease(_)
                | Call::ControllerRecover(_)
        ) {
            return self.controller_call(call, current).await;
        }
        if !call.is_mutation() {
            let request = call.clone();
            return host
                .worker
                .run_cleanup(move |context| {
                    if let Call::DiffRead(request) = &request {
                        return Ok(context
                            .engine
                            .public_diff(&access, request)
                            .map(ResultValue::Artifact)
                            .map_err(vcp_engine::rpc::query_error));
                    }
                    if let Call::TaskPresentation(request) = &request {
                        return Ok(context
                            .engine
                            .public_presentation_with_content(&access, request, now())
                            .and_then(|mut page| {
                                super::public_presentation::model(
                                    context, &access, request, &mut page,
                                );
                                if serde_json::to_vec(&page)
                                    .map_err(|_| vcp_engine::query::QueryError::InvalidData)?
                                    .len()
                                    > vcp_engine::query::MAX_RESULT_BYTES
                                {
                                    return Err(vcp_engine::query::QueryError::Limit);
                                }
                                Ok(ResultValue::Presentation(page))
                            })
                            .map_err(vcp_engine::rpc::query_error));
                    }
                    if let Call::ArtifactRead(request) = &request {
                        return Ok(context
                            .engine
                            .public_artifact(&access, request)
                            .map(ResultValue::Artifact)
                            .map_err(vcp_engine::rpc::query_error));
                    }
                    let facts = facts(context)?;
                    Ok(context.runtime.block_on(
                        EngineRpcHost {
                            engine: &mut context.engine,
                            facts: &facts,
                        }
                        .call(request, &access),
                    ))
                })
                .map_err(|_| {
                    failure(
                        Code::StoreUnavailable,
                        Retry::AfterRevalidation,
                        &call,
                        "canonical read unavailable",
                    )
                })?;
        }
        let token = token.ok_or_else(|| {
            failure(
                Code::PolicyDenied,
                Retry::AfterRevalidation,
                &call,
                "current controller lease required",
            )
        })?;
        let reactor = tokio::runtime::Handle::try_current().map_err(|_| {
            failure(
                Code::CapabilityUnavailable,
                Retry::AfterRevalidation,
                &call,
                "live host runtime required",
            )
        })?;
        let operation = call.mutation().map(|value| value.command_id.clone());
        let approval = matches!(call, Call::ApprovalRespond(_));
        let runtime = host.runtime.clone();
        let bindings = host.bindings.clone();
        let request = call.clone();
        let admitted_access = access.clone();
        let admitted_connection = connection.clone();
        let admitted_token = token.clone();
        let started = Arc::new(AtomicBool::new(false));
        let marker = started.clone();
        let admission_worker = host.worker.clone();
        let stage = host.worker.run(move |context| {
            // Keep a typed error inside the worker result; filesystem/provider
            // diagnostics never become a public wire explanation.
            let action = (|| -> std::result::Result<Admission, RpcError> {
                if admission_worker.fenced() {
                    return Err(failure(
                        Code::OutcomeUnknown,
                        Retry::ReconcileOriginal,
                        &request,
                        "canonical admission is fenced; reopen and reconcile",
                    ));
                }
                context
                    .check_public_controller(
                        &admitted_access,
                        &admitted_connection,
                        &admitted_token,
                    )
                    .map_err(|_| {
                        failure(
                            Code::AuthorityStale,
                            Retry::AfterRevalidation,
                            &request,
                            "controller connection is no longer current",
                        )
                    })?;
                let facts = facts(context).map_err(|_| {
                    failure(
                        Code::StoreUnavailable,
                        Retry::AfterRevalidation,
                        &request,
                        "current host facts unavailable",
                    )
                })?;
                let prepared = match context
                    .engine
                    .prepare_controlled_public(
                        request.clone(),
                        &admitted_access,
                        &facts,
                        &admitted_connection,
                        &admitted_token,
                    )
                    .map_err(|error| public_error(error, operation.clone(), approval))?
                {
                    PublicAdmission::Replay(receipt) => {
                        return acceptance(&context.engine, &admitted_access, &receipt)
                            .map(Admission::Reply);
                    }
                    PublicAdmission::Ready(prepared) => prepared,
                };
                if !context.owner_alive || context.authority_pending {
                    return Err(failure(
                        Code::OutcomeUnknown,
                        Retry::ReconcileOriginal,
                        &request,
                        "host authority operation is stopping; reconcile original command",
                    ));
                }
                let stopping = matches!(
                    prepared.call(),
                    Call::TaskCancel(_) | Call::TurnPause(_) | Call::TurnCancel(_)
                );
                if stopping {
                    let task = prepared.task().ok_or_else(RpcError::internal_error)?;
                    let selected: Task = context
                        .engine
                        .store()
                        .state()
                        .record(Collection::Task, task.as_str(), &admitted_access.workspace)
                        .and_then(Record::decode)
                        .map_err(|_| RpcError::internal_error())?;
                    // Public preparation validated scope, task/turn chronology and
                    // revisions before any retained work is touched. Acceptance
                    // is durable intent; owned cancellation may still be draining.
                    let attached = bindings
                        .lock()
                        .map_err(|_| RpcError::internal_error())?
                        .values()
                        .any(|binding| binding.scope == selected.scope);
                    if selected.state == TaskState::Running && !attached {
                        return Err(failure(
                            Code::CapabilityUnavailable,
                            Retry::AfterRevalidation,
                            &request,
                            "running task has no retained host owner",
                        ));
                    }
                    marker.store(true, Ordering::SeqCst);
                    super::control::hold_task_stop(
                        &runtime,
                        &bindings,
                        &context.config.root_task,
                        &selected,
                    )
                    .map_err(|_| {
                        failure(
                            Code::OutcomeUnknown,
                            Retry::ReconcileOriginal,
                            &request,
                            "retained task stop requires reconciliation",
                        )
                    })?;
                    context
                        .check_public_controller(
                            &admitted_access,
                            &admitted_connection,
                            &admitted_token,
                        )
                        .map_err(|_| {
                            failure(
                                Code::AuthorityStale,
                                Retry::ReconcileOriginal,
                                &request,
                                "controller lost during task stop",
                            )
                        })?;
                }
                if !matches!(
                    prepared.payload(),
                    Command::Steer { .. } | Command::SetWorkspaceTrust { .. }
                ) {
                    let receipt = context
                        .runtime
                        .block_on(
                            context
                                .engine
                                .commit_public(prepared, &admitted_access, &facts),
                        )
                        .map_err(|error| public_error(error, operation.clone(), approval))?;
                    if stopping {
                        runtime.0.changed.notify_waiters();
                        #[cfg(windows)]
                        context
                            .stop_coding_turns("explicit public task stop")
                            .map_err(|_| {
                                failure(
                                    Code::OutcomeUnknown,
                                    Retry::ReconcileOriginal,
                                    &request,
                                    "coding turn stop requires reconciliation",
                                )
                            })?;
                    }
                    return acceptance(&context.engine, &admitted_access, &receipt)
                        .map(Admission::Reply);
                }
                let task_id = prepared
                    .task()
                    .cloned()
                    .unwrap_or_else(|| context.config.root_task.clone());
                let selected: Option<Task> = context
                    .engine
                    .store()
                    .state()
                    .records
                    .get(&key(Collection::Task, task_id.as_str()))
                    .map(|record| authority_task(record, &admitted_access, &task_id))
                    .transpose()?;
                if selected.is_none() && !matches!(prepared.call(), Call::WorkspaceSetTrust(_)) {
                    return Err(RpcError::internal_error());
                }
                let attached = !bindings
                    .lock()
                    .map_err(|_| RpcError::internal_error())?
                    .is_empty();
                if selected
                    .as_ref()
                    .is_some_and(|task| task.state == TaskState::Running)
                    && !attached
                {
                    return Err(failure(
                        Code::CapabilityUnavailable,
                        Retry::AfterRevalidation,
                        &request,
                        "running task has no retained host owner",
                    ));
                }
                context.mark_authority_pending();
                marker.store(true, Ordering::SeqCst);
                // Seal first. A capture/commit failure cannot leave live work
                // admitted while authority is uncertain.
                let waiter = Some(runtime.hold_owner().map_err(|_| {
                    failure(
                        Code::OutcomeUnknown,
                        Retry::ReconcileOriginal,
                        &request,
                        "retained owner stop requires reconciliation",
                    )
                })?);
                let intent = canonical_bytes(&serde_json::json!({
                    "schema_version":1,"command":prepared.id(),"digest":prepared.digest(),
                    "call":prepared.call(),"state":"stopping; command not applied"
                }))
                .map_err(|_| RpcError::internal_error())?;
                if let Some(selected) = &selected {
                    context
                        .capture(
                            &selected.scope,
                            Channel::Evidence,
                            &intent,
                            "vcp-public-authority-intent/1",
                        )
                        .map_err(|_| {
                            failure(
                                Code::OutcomeUnknown,
                                Retry::ReconcileOriginal,
                                &request,
                                "authority intent capture requires reconciliation",
                            )
                        })?;
                }
                let proof = if matches!(prepared.call(), Call::WorkspaceSetTrust(_)) {
                    None
                } else {
                    Some(
                        context
                            .runtime
                            .block_on(context.engine.pause_public_for_authority(
                                &prepared,
                                &admitted_access,
                                &facts,
                            ))
                            .map_err(|error| public_error(error, operation.clone(), false))?,
                    )
                };
                let tasks = context
                    .engine
                    .store()
                    .state()
                    .records
                    .values()
                    .filter(|row| {
                        row.collection == Collection::Task
                            && row.workspace == admitted_access.workspace
                    })
                    .map(Record::decode::<Task>)
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(|_| RpcError::internal_error())?;
                for task in tasks {
                    if task.scope.session == admitted_access.session
                        && (proof.is_none() || task.scope.task != task_id)
                        && !task.state.terminal()
                        && task.state != TaskState::Paused
                    {
                        context
                            .command(
                                Command::Transition {
                                    next: TaskState::Paused,
                                    reason:
                                        "public authority change requires deliberate continuation"
                                            .into(),
                                    verification: None,
                                },
                                Some(task.scope.task),
                                task.revision,
                            )
                            .map_err(|_| {
                                failure(
                                    Code::OutcomeUnknown,
                                    Retry::ReconcileOriginal,
                                    &request,
                                    "owned task pause requires reconciliation",
                                )
                            })?;
                    }
                }
                #[cfg(windows)]
                context
                    .stop_coding_turns("public authority change")
                    .map_err(|_| {
                        failure(
                            Code::OutcomeUnknown,
                            Retry::ReconcileOriginal,
                            &request,
                            "coding turn pause requires reconciliation",
                        )
                    })?;
                Ok(Admission::Drain {
                    prepared,
                    proof,
                    waiter,
                })
            })();
            Ok(action)
        });
        let stage = match stage {
            Ok(Ok(stage)) => stage,
            error => {
                if started.load(Ordering::SeqCst) {
                    host.worker.fence();
                }
                return Err(match error {
                    Ok(Err(error)) => error,
                    _ => failure(
                        Code::OutcomeUnknown,
                        Retry::ReconcileOriginal,
                        &call,
                        "canonical operation requires reconciliation",
                    ),
                });
            }
        };
        let (prepared, proof, waiter) = match stage {
            Admission::Reply(reply) => return Ok(reply),
            Admission::Drain {
                prepared,
                proof,
                waiter,
            } => (prepared, proof, waiter),
        };
        let worker = host.worker.clone();
        let cleanup_host = host.clone();
        let drain_deadline = host.runtime.0.deadline;
        let request = call.clone();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        // The operation owns draining and completion even when its caller stops
        // awaiting. Connection loss independently invalidates the final commit.
        reactor.spawn(async move {
            let mut drained = match waiter {
                Some(waiter) => waiter.wait().await.is_ok(),
                None => true,
            };
            if drained {
                drained = super::public_connection::drain_owned_dependencies(
                    &cleanup_host,
                    drain_deadline,
                )
                .await
                .is_ok();
            }
            let completion_worker = worker.clone();
            let result = worker.run_cleanup(move |context| {
                let result = (|| -> std::result::Result<ResultValue, RpcError> {
                    if completion_worker.fenced()
                        || !drained
                        || !context.owner_alive
                        || !context.authority_pending
                    {
                        return Err(failure(
                            Code::OutcomeUnknown,
                            Retry::ReconcileOriginal,
                            &request,
                            "authority drain requires reconciliation",
                        ));
                    }
                    context
                        .check_public_controller(&access, &connection, &token)
                        .map_err(|_| {
                            failure(
                                Code::AuthorityStale,
                                Retry::ReconcileOriginal,
                                &request,
                                "controller lost during authority drain",
                            )
                        })?;
                    let facts = facts(context).map_err(|_| RpcError::internal_error())?;
                    let workspace_authority = proof.is_none();
                    let receipt = match proof {
                        Some(proof) => context.runtime.block_on(
                            context
                                .engine
                                .commit_public_after_pause(prepared, proof, &access, &facts),
                        ),
                        None => context.runtime.block_on(
                            context
                                .engine
                                .commit_public_workspace_authority(prepared, &access, &facts),
                        ),
                    }
                    .map_err(|error| {
                        public_error(
                            error,
                            request.mutation().map(|value| value.command_id.clone()),
                            false,
                        )
                    })?;
                    let mut result_access = access.clone();
                    if workspace_authority {
                        let workspace: Workspace = context
                            .engine
                            .store()
                            .state()
                            .record(
                                Collection::Workspace,
                                access.workspace.as_str(),
                                &access.workspace,
                            )
                            .and_then(Record::decode)
                            .map_err(|_| RpcError::internal_error())?;
                        result_access.authority = workspace.authority;
                        // Host maintenance follows durable authority; client
                        // credentials remain stale until explicit refresh.
                        context.access.authority = workspace.authority;
                    }
                    acceptance(&context.engine, &result_access, &receipt)
                })();
                context.clear_authority_pending();
                Ok(result)
            });
            if result.is_err() {
                worker.fence();
            }
            let _ = sender.send(result);
        });
        receiver
            .await
            .map_err(|_| {
                failure(
                    Code::OutcomeUnknown,
                    Retry::ReconcileOriginal,
                    &call,
                    "authority result delivery interrupted",
                )
            })?
            .map_err(|_| {
                failure(
                    Code::OutcomeUnknown,
                    Retry::ReconcileOriginal,
                    &call,
                    "canonical authority result unavailable",
                )
            })?
    }
}
