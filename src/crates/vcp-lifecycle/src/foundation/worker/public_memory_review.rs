// SPDX-License-Identifier: Apache-2.0
//! Explicit manual review under a current public controller. No automatic grants.
use super::{public_connection::PublicConnection, *};
use serde::{de::DeserializeOwned, Serialize};
use std::collections::BTreeSet;
use vcp_memory::review;
use vcp_protocol::{
    errors::{ApplicationError, Code, Retry},
    jsonrpc::RpcError,
    memory_governance as wire,
    methods::{self, Call, ResultValue},
};
type RpcResult<T> = std::result::Result<T, RpcError>;
fn failure(code: Code, call: &Call) -> RpcError {
    ApplicationError {
        code,
        retry: Retry::AfterRevalidation,
        operation: call.command_id().cloned(),
        explanation: "manual memory review unavailable".into(),
        reconciliation: None,
    }
    .into_rpc()
}
fn unavailable(call: &Call) -> RpcError {
    failure(Code::StoreUnavailable, call)
}
fn unknown(call: &Call) -> RpcError {
    ApplicationError {
        code: Code::OutcomeUnknown,
        retry: Retry::ReconcileOriginal,
        operation: call.command_id().cloned(),
        explanation: "reconcile the original memory command".into(),
        reconciliation: None,
    }
    .into_rpc()
}
fn memory_error(error: vcp_memory::Error, call: &Call) -> RpcError {
    if call.is_mutation()
        && matches!(
            &error,
            vcp_memory::Error::Store(
                vcp_store::Error::Io(_)
                    | vcp_store::Error::Unavailable(_)
                    | vcp_store::Error::Database(_)
            )
        )
    {
        return unknown(call);
    }
    let code = match error {
        vcp_memory::Error::Access => Code::PolicyDenied,
        vcp_memory::Error::Conflict("manual memory command payload conflict") => {
            Code::CommandConflict
        }
        vcp_memory::Error::Conflict(_) => Code::VersionConflict,
        vcp_memory::Error::Invalid(_) | vcp_memory::Error::Domain(_) => Code::PolicyDenied,
        _ => Code::StoreUnavailable,
    };
    failure(code, call)
}
fn convert<T: DeserializeOwned>(value: &impl Serialize) -> RpcResult<T> {
    serde_json::from_value(serde_json::to_value(value).map_err(|_| RpcError::invalid_params())?)
        .map_err(|_| RpcError::invalid_params())
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
fn scoped_access(
    store: &Store,
    access: &Access,
    scope: &Scope,
    write: bool,
    call: &Call,
) -> RpcResult<vcp_memory::access::Access> {
    if !access.read
        || (write && !access.write)
        || scope.workspace != access.workspace
        || scope.session != access.session
    {
        return Err(failure(Code::PolicyDenied, call));
    }
    let task: Task = store
        .state()
        .record(Collection::Task, scope.task.as_str(), &access.workspace)
        .map_err(|_| unavailable(call))?
        .decode()
        .map_err(|_| unavailable(call))?;
    if task.scope != *scope || task.redaction.is_some() {
        return Err(failure(Code::PolicyDenied, call));
    }
    let mut tasks = BTreeSet::new();
    for row in store
        .state()
        .records
        .values()
        .filter(|row| row.collection == Collection::Task && row.workspace == access.workspace)
    {
        let other: Task = row.decode().map_err(|_| unavailable(call))?;
        if other.scope.session == access.session && other.redaction.is_none() {
            tasks.insert(other.scope.task);
        }
    }
    Ok(vcp_memory::access::Access {
        workspace: access.workspace.clone(),
        actor: access.actor.clone(),
        authority: access.authority,
        read: true,
        write,
        tasks: Some(tasks),
    })
}
fn proposal(
    request: &wire::TypedPropose,
    access: &Access,
) -> RpcResult<vcp_domain::memory::Proposal> {
    let p = &request.candidate;
    Ok(vcp_domain::memory::Proposal {
        id: convert(&request.submission)?,
        command: convert(&request.mutation.command_id)?,
        claim: convert(&p.claim)?,
        scope: scope(&request.scope, &request.task)?,
        actor: access.actor.clone(),
        epochs: vcp_domain::memory::Epochs {
            authority: access.authority,
            policy: convert(&request.guards.policy_revision)?,
            deletion: convert(&request.guards.deletion_epoch)?,
        },
        registry_version: vcp_domain::memory::REGISTRY_VERSION,
        extractor: vcp_domain::memory_review::MANUAL_EXTRACTOR.into(),
        output_key: request.submission.as_str().into(),
        origins: convert(&p.origins)?,
        subject: p.subject.clone(),
        predicate: p.predicate.clone(),
        statement: p.statement.clone(),
        value: convert(&p.value)?,
        applicability: convert(&p.applicability)?,
        evidence: convert(&p.evidence)?,
        predecessor: convert(&p.predecessor)?,
        correction_reason: p.correction_reason.clone(),
        retention: p.retention.clone(),
    })
}
impl PublicConnection {
    pub(super) async fn memory_review_call(
        &self,
        call: Call,
        current: &Access,
    ) -> RpcResult<ResultValue> {
        call.validate().map_err(|_| RpcError::invalid_params())?;
        let (host, access, connection, token) = self
            .rpc_context(current)
            .map_err(|_| failure(Code::PolicyDenied, &call))?;
        // The legacy prose shape never implied a typed claim. Refuse it rather
        // than infer governance semantics or silently change its digest.
        if matches!(
            &call,
            Call::MemoryPropose(wire::ProposeParams::Legacy(_))
                | Call::MemoryResolve(wire::ResolveParams::Legacy(_))
        ) {
            return Err(failure(Code::CapabilityUnavailable, &call));
        }
        let connected = self.connected.clone();
        let worker = host.worker.clone();
        let fallback = call.clone();
        host.worker
            .run_cleanup(move |context| {
                Ok((|| -> RpcResult<_> {
                    if !connected.load(Ordering::SeqCst) || worker.fenced() {
                        return Err(unavailable(&call));
                    }
                    context
                        .public_authorize(&access, &connection, token.as_ref(), call.is_mutation())
                        .map_err(|_| failure(Code::PolicyDenied, &call))?;
                    let (requested_scope, task_id) = match &call {
                        Call::MemoryPropose(p) => (p.scope(), p.task()),
                        Call::MemoryResolve(p) => (p.scope(), p.task()),
                        Call::MemoryReview(p) => (&p.scope, &p.task),
                        _ => return Err(RpcError::invalid_params()),
                    };
                    let own = scope(requested_scope, task_id)?;
                    let memory_access = scoped_access(
                        context.engine.store(),
                        &access,
                        &own,
                        call.is_mutation(),
                        &call,
                    )?;
                    let command_digest = call
                        .digest(access.actor.as_str())
                        .map_err(|_| unavailable(&call))?;
                    let committed = match &call {
                        Call::MemoryReview(p) => {
                            let state = review::read(
                                context.engine.store(),
                                &memory_access,
                                &own,
                                &convert(&p.submission)?,
                            )
                            .map_err(|e| memory_error(e, &call))?;
                            let result =
                                bounded(ResultValue::MemoryReview(project(&state)?), &call)?;
                            if !connected.load(Ordering::SeqCst) || worker.fenced() {
                                return Err(unavailable(&call));
                            }
                            context
                                .public_authorize(&access, &connection, token.as_ref(), false)
                                .map_err(|_| failure(Code::PolicyDenied, &call))?;
                            return Ok(result);
                        }
                        Call::MemoryPropose(wire::ProposeParams::Typed(p)) => {
                            let candidate = proposal(p, &access)?;
                            let candidate_digest = vcp_protocol::digest_bytes(
                                &vcp_protocol::canonical_bytes(&candidate)
                                    .map_err(|_| unavailable(&call))?,
                            );
                            let submission = vcp_domain::memory_review::Submission {
                                document_type: vcp_domain::memory_review::SUBMISSION.into(),
                                schema_version: 1,
                                id: candidate.id.clone(),
                                scope: own,
                                revision: Revision::ZERO,
                                candidate,
                                candidate_digest,
                                command_digest,
                                task_revision: convert(&p.mutation.expected_revision)?,
                                steering: convert(&p.mutation.steering_revision)?,
                                expected_head: convert(&p.guards.expected_head)?,
                                recorded_at: now(),
                            };
                            context.runtime.block_on(review::submit(
                                context.engine.store_mut(),
                                &memory_access,
                                submission,
                            ))
                        }
                        Call::MemoryResolve(wire::ResolveParams::Typed(p)) => {
                            let request = review::Resolve {
                                scope: own,
                                submission: convert(&p.submission)?,
                                submission_digest: p.submission_digest.as_str().into(),
                                command: convert(&p.mutation.command_id)?,
                                command_digest,
                                task_revision: convert(&p.mutation.expected_revision)?,
                                steering: convert(&p.mutation.steering_revision)?,
                                epochs: vcp_domain::memory::Epochs {
                                    authority: access.authority,
                                    policy: convert(&p.guards.policy_revision)?,
                                    deletion: convert(&p.guards.deletion_epoch)?,
                                },
                                expected_head: convert(&p.guards.expected_head)?,
                                choice: match p.decision {
                                    methods::MemoryDecision::Accept => {
                                        vcp_domain::memory_review::Choice::Accept
                                    }
                                    methods::MemoryDecision::Reject => {
                                        vcp_domain::memory_review::Choice::Reject
                                    }
                                },
                                reason: p.reason.clone(),
                                now: now(),
                            };
                            context.runtime.block_on(review::resolve(
                                context.engine.store_mut(),
                                &memory_access,
                                request,
                            ))
                        }
                        _ => return Err(RpcError::invalid_params()),
                    }
                    .map_err(|e| memory_error(e, &call))?;
                    // A lost response leaves the same durable command for reconciliation.
                    if !connected.load(Ordering::SeqCst) || worker.fenced() {
                        return Err(unknown(&call));
                    }
                    context
                        .public_authorize(&access, &connection, token.as_ref(), true)
                        .map_err(|_| unknown(&call))?;
                    let receipt = committed
                        .receipt
                        .command
                        .as_ref()
                        .ok_or_else(|| unknown(&call))?;
                    let ResultValue::Acceptance(acceptance) =
                        vcp_engine::rpc::acceptance(&context.engine, &access, receipt)
                            .map_err(|_| unknown(&call))?
                    else {
                        return Err(unknown(&call));
                    };
                    let view = project(&committed.review).map_err(|_| unknown(&call))?;
                    bounded(
                        ResultValue::MemoryReviewed(wire::ReviewResult {
                            acceptance,
                            submission: view.submission,
                            candidate_digest: view.candidate_digest,
                            disposition: view.disposition,
                            governed_proposal: view.governed_proposal,
                            version: view.version,
                            resolution: view.resolution,
                            indexing: view.indexing,
                        }),
                        &call,
                    )
                    .map_err(|_| unknown(&call))
                })())
            })
            .map_err(|_| {
                if fallback.is_mutation() {
                    unknown(&fallback)
                } else {
                    unavailable(&fallback)
                }
            })?
    }
}
fn bounded(value: ResultValue, call: &Call) -> RpcResult<ResultValue> {
    if serde_json::to_vec(&value)
        .map_err(|_| unavailable(call))?
        .len()
        > methods::MAX_METHOD_BYTES
    {
        return Err(failure(Code::ResourceLimit, call));
    }
    Ok(value)
}
fn project(state: &review::ReviewState) -> RpcResult<wire::ReviewView> {
    let s = &state.submission;
    let p = &s.candidate;
    let candidate = wire::Candidate {
        claim: id(p.claim.as_str())?,
        origins: convert(&p.origins)?,
        subject: p.subject.clone(),
        predicate: p.predicate.clone(),
        statement: p.statement.clone(),
        value: convert(&p.value)?,
        applicability: convert(&p.applicability)?,
        evidence: convert(&p.evidence)?,
        predecessor: convert(&p.predecessor)?,
        correction_reason: p.correction_reason.clone(),
        retention: p.retention.clone(),
    };
    let resolution = state
        .decision
        .as_ref()
        .map(|d| -> RpcResult<_> {
            Ok(vcp_protocol::memory::Resolution {
                outcome: convert(&d.resolution.outcome)?,
                evidence_status: convert(&d.resolution.evidence_status)?,
                conflicts: convert(&d.resolution.conflicts)?,
            })
        })
        .transpose()?;
    Ok(wire::ReviewView {
        scope: methods::Scope {
            workspace: id(s.scope.workspace.as_str())?,
            session: id(s.scope.session.as_str())?,
        },
        task: id(s.scope.task.as_str())?,
        submission: id(s.id.as_str())?,
        candidate_digest: s
            .candidate_digest
            .clone()
            .try_into()
            .map_err(|_| RpcError::internal_error())?,
        candidate,
        disposition: if state.decision.is_some() {
            wire::ReviewDisposition::Resolved
        } else {
            wire::ReviewDisposition::AwaitingReview
        },
        submission_command: id(p.command.as_str())?,
        decision_command: state
            .decision
            .as_ref()
            .map(|d| id(d.command.as_str()))
            .transpose()?,
        governed_proposal: state
            .decision
            .as_ref()
            .and_then(|d| d.governed_proposal.as_ref())
            .map(|p| id(p.as_str()))
            .transpose()?,
        version: state
            .result
            .as_ref()
            .and_then(|r| r.version.as_ref())
            .map(|v| id(v.as_str()))
            .transpose()?,
        resolution,
        indexing: state.indexing.as_ref().map(convert).transpose()?,
        guards: wire::Guards {
            policy_revision: p.epochs.policy.get().into(),
            deletion_epoch: p.epochs.deletion.get().into(),
            expected_head: convert(&s.expected_head)?,
        },
        decision: state
            .decision
            .as_ref()
            .map(|d| convert(&d.choice))
            .transpose()?,
        reason: state.decision.as_ref().map(|d| d.reason.clone()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uncertain_commit_preserves_original_command_and_reconciliation_retry() {
        let call = Call::decode("memory/resolve", serde_json::json!({
            "scope":{"workspace":"workspace","session":"session"},
            "mutation":{"command_id":"original","expected_revision":"0","steering_revision":"0"},
            "task":"task","proposal":"proposal","decision":"accept"
        })).unwrap();
        let uncertain = memory_error(
            vcp_memory::Error::Store(vcp_store::Error::Unavailable("writer fenced")),
            &call,
        );
        let expected = unknown(&call);
        assert_eq!(
            serde_json::to_value(&uncertain).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
        let data = uncertain.data.unwrap().details;
        assert_eq!(data["code"], "OUTCOME_UNKNOWN");
        assert_eq!(data["operation"], "original");
        assert_eq!(data["retry"], "reconcile_original");
        let stale = memory_error(
            vcp_memory::Error::Conflict("manual memory preconditions changed"),
            &call,
        );
        assert_eq!(stale.data.unwrap().details["code"], "VERSION_CONFLICT");
    }
}
