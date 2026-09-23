// SPDX-License-Identifier: Apache-2.0
//! Public dispatch through the existing serialized host and owned lifecycle.
use super::public_connection::PublicConnection;
use super::*;
use vcp_engine::{
    public::{PreparedPublicCommand, PublicAdmission, PublicPauseProof},
    rpc::{acceptance, public_error, EngineRpcHost, RpcHost, METHODS},
};
use vcp_protocol::{
    errors::{ApplicationError, Code, Retry},
    jsonrpc::RpcError,
    methods::{Call, ResultValue},
};

fn failure(code: Code, retry: Retry, call: &Call, explanation: &str) -> RpcError {
    ApplicationError {
        code,
        retry,
        operation: call.mutation().map(|mutation| mutation.command_id.clone()),
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
        proof: PublicPauseProof,
        waiter: Option<crate::HoldWaiter>,
    },
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
        if !call.is_mutation() {
            let request = call.clone();
            return host
                .worker
                .run_cleanup(move |context| {
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
                            .map(Admission::Reply)
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
                if !matches!(prepared.payload(), Command::Steer { .. }) {
                    let receipt = context
                        .runtime
                        .block_on(
                            context
                                .engine
                                .commit_public(prepared, &admitted_access, &facts),
                        )
                        .map_err(|error| public_error(error, operation.clone(), approval))?;
                    return acceptance(&context.engine, &admitted_access, &receipt)
                        .map(Admission::Reply);
                }
                let task_id = prepared
                    .task()
                    .cloned()
                    .ok_or_else(RpcError::internal_error)?;
                let selected: Task = context
                    .engine
                    .store()
                    .state()
                    .record(
                        Collection::Task,
                        task_id.as_str(),
                        &admitted_access.workspace,
                    )
                    .and_then(Record::decode)
                    .map_err(|_| RpcError::internal_error())?;
                let attached = !bindings
                    .lock()
                    .map_err(|_| RpcError::internal_error())?
                    .is_empty();
                if selected.state == TaskState::Running && !attached {
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
                let proof = context
                    .runtime
                    .block_on(context.engine.pause_public_for_authority(
                        &prepared,
                        &admitted_access,
                        &facts,
                    ))
                    .map_err(|error| public_error(error, operation.clone(), false))?;
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
                        && task.scope.task != task_id
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
        let scheduler = host.scheduler.clone();
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
            let deadline = tokio::time::Instant::now() + drain_deadline;
            while drained && scheduler.busy() {
                if tokio::time::Instant::now() >= deadline {
                    drained = false;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
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
                    let receipt = context
                        .runtime
                        .block_on(
                            context
                                .engine
                                .commit_public_after_pause(prepared, proof, &access, &facts),
                        )
                        .map_err(|error| {
                            public_error(
                                error,
                                request.mutation().map(|value| value.command_id.clone()),
                                false,
                            )
                        })?;
                    acceptance(&context.engine, &access, &receipt)
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
