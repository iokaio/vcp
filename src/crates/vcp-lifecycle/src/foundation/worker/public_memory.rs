// SPDX-License-Identifier: Apache-2.0
//! Read-only claim history using current governed source access and retention.
use super::public_connection::PublicConnection;
use super::*;
use std::{collections::BTreeSet, time::Instant};
use vcp_protocol::{
    errors::{ApplicationError, Code, Retry},
    jsonrpc::RpcError,
    memory as wire,
    methods::{self, Call},
};

fn error(code: Code) -> RpcError {
    ApplicationError {
        code,
        retry: Retry::AfterRevalidation,
        operation: None,
        explanation: "governed memory inspection unavailable".into(),
        reconciliation: None,
    }
    .into_rpc()
}
fn unavailable() -> RpcError {
    error(Code::StoreUnavailable)
}
fn id(value: &str) -> std::result::Result<methods::Id, RpcError> {
    value.to_owned().try_into().map_err(|_| unavailable())
}
fn memory_error(value: vcp_memory::Error) -> RpcError {
    match value {
        vcp_memory::Error::Access => error(Code::PolicyDenied),
        _ => unavailable(),
    }
}

impl PublicConnection {
    pub fn memory_inspect(
        &self,
        request: &methods::MemoryInspect,
        current: &Access,
    ) -> std::result::Result<methods::MemoryPage, RpcError> {
        let (host, access, connection, token) = self
            .rpc_context(current)
            .map_err(|_| error(Code::PolicyDenied))?;
        let connected = self.connected.clone();
        let request = request.clone();
        let worker = host.worker.clone();
        host.worker
            .run_cleanup(move |context| {
                Ok((|| {
                    context
                        .public_authorize(&access, &connection, token.as_ref(), false)
                        .map_err(|_| error(Code::PolicyDenied))?;
                    Call::MemoryInspect(request.clone())
                        .validate()
                        .map_err(|_| RpcError::invalid_params())?;
                    let start = Instant::now();
                    let check = || {
                        if !connected.load(Ordering::SeqCst)
                            || worker.fenced()
                            || start.elapsed() >= Duration::from_secs(2)
                        {
                            Err(vcp_memory::Error::Conflict("memory inspection interrupted"))
                        } else {
                            Ok(())
                        }
                    };
                    inspect(context.engine.store(), &access, &request, &check)
                })())
            })
            .map_err(|_| unavailable())?
    }
}

fn inspect(
    store: &Store,
    access: &Access,
    request: &methods::MemoryInspect,
    check: &dyn Fn() -> vcp_memory::Result<()>,
) -> std::result::Result<methods::MemoryPage, RpcError> {
    check().map_err(memory_error)?;
    if request.scope.workspace.as_str() != access.workspace.as_str()
        || request.scope.session.as_str() != access.session.as_str()
        || !access.read
    {
        return Err(error(Code::PolicyDenied));
    }
    let task: Task = store
        .state()
        .record(Collection::Task, request.task.as_str(), &access.workspace)
        .map_err(|_| unavailable())?
        .decode()
        .map_err(|_| unavailable())?;
    if task.scope.session != access.session
        || task.scope.workspace != access.workspace
        || task.scope.task.as_str() != request.task.as_str()
        || task.redaction.is_some()
    {
        return Err(error(Code::PolicyDenied));
    }
    let mut tasks = BTreeSet::new();
    for row in store
        .state()
        .records
        .values()
        .filter(|row| row.collection == Collection::Task && row.workspace == access.workspace)
    {
        check().map_err(memory_error)?;
        let source: Task = row.decode().map_err(|_| unavailable())?;
        if source.scope.workspace != row.workspace || source.scope.task.as_str() != row.id {
            return Err(unavailable());
        }
        if source.scope.session == access.session {
            tasks.insert(source.scope.task);
        }
    }
    let memory_access = vcp_memory::access::Access {
        workspace: access.workspace.clone(),
        actor: access.actor.clone(),
        authority: access.authority,
        read: access.read,
        write: false,
        tasks: Some(tasks),
    };
    let claim = ClaimId::parse(request.claim.as_str()).map_err(|_| RpcError::invalid_params())?;
    let history = vcp_memory::history::query_with_check(
        store,
        &memory_access,
        &claim,
        None,
        Some(&task.fingerprint),
        check,
    )
    .map_err(memory_error)?;
    let sequence = history
        .versions
        .iter()
        .map(|row| row.memory_seq.get())
        .max()
        .unwrap_or(0);
    let mut findings = Vec::new();
    for row in history.versions {
        check().map_err(memory_error)?;
        if request
            .version
            .as_ref()
            .is_some_and(|version| version.as_str() != row.id.as_str())
        {
            continue;
        }
        let visibility = match row.visibility {
            "retained" => wire::Visibility::Retained,
            "pruned" => wire::Visibility::Pruned,
            "purged" => wire::Visibility::Purged,
            _ => return Err(unavailable()),
        };
        let mut evidence = Vec::new();
        let mut states = Vec::new();
        for observed in &row.evidence {
            let availability = match observed.availability {
                "available" => wire::EvidenceAvailability::Available,
                "unavailable" => wire::EvidenceAvailability::Unavailable,
                "missing" => wire::EvidenceAvailability::Missing,
                "integrity_failure" => wire::EvidenceAvailability::IntegrityFailure,
                // Governed history rejects unauthorized sources before projection.
                _ => return Err(error(Code::PolicyDenied)),
            };
            states.push(wire::EvidenceState {
                artifact: id(observed.artifact.as_str())?,
                availability,
                verification_current: observed.verification_current,
            });
            if observed.availability == "available" {
                let version = row.version.as_ref().ok_or_else(unavailable)?;
                let source = version
                    .proposal
                    .evidence
                    .iter()
                    .find(|source| source.artifact == observed.artifact)
                    .ok_or_else(unavailable)?;
                let descriptor: ArtifactDescriptor = store
                    .state()
                    .record(
                        Collection::Artifact,
                        observed.artifact.as_str(),
                        &access.workspace,
                    )
                    .map_err(|_| unavailable())?
                    .decode()
                    .map_err(|_| unavailable())?;
                let (offset, end) = source
                    .range
                    .as_ref()
                    .map_or((0, descriptor.length.get()), |range| {
                        (range.start.get(), range.end.get())
                    });
                evidence.push(methods::EvidenceReference {
                    artifact: id(observed.artifact.as_str())?,
                    offset: offset.into(),
                    length: end.checked_sub(offset).ok_or_else(unavailable)?.into(),
                    sha256: source.sha256.clone(),
                });
            }
        }
        let (content, resolution) = if let Some(version) = row.version {
            let resolution = version.resolution;
            let outcome = match resolution.outcome {
                vcp_domain::memory::Outcome::Accepted => wire::Outcome::Accepted,
                vcp_domain::memory::Outcome::Disputed => wire::Outcome::Disputed,
                vcp_domain::memory::Outcome::Rejected => wire::Outcome::Rejected,
                vcp_domain::memory::Outcome::AwaitingReview => wire::Outcome::AwaitingReview,
            };
            let evidence_status = match resolution.evidence_status {
                vcp_domain::memory::EvidenceStatus::Verified => wire::EvidenceStatus::Verified,
                vcp_domain::memory::EvidenceStatus::Observed => wire::EvidenceStatus::Observed,
                vcp_domain::memory::EvidenceStatus::Inferred => wire::EvidenceStatus::Inferred,
                vcp_domain::memory::EvidenceStatus::Unverified => wire::EvidenceStatus::Unverified,
            };
            (
                version.proposal.statement,
                Some(wire::Resolution {
                    outcome,
                    evidence_status,
                    conflicts: resolution
                        .conflicts
                        .iter()
                        .map(|id_| id(id_.as_str()))
                        .collect::<std::result::Result<_, _>>()?,
                }),
            )
        } else {
            (String::new(), None)
        };
        if content.len() > 65536 || evidence.len() > 64 || states.len() > 64 {
            return Err(error(Code::ResourceLimit));
        }
        findings.push(methods::MemoryFinding {
            claim: request.claim.clone(),
            version: id(row.id.as_str())?,
            evidence,
            content,
            state: Some(wire::InspectionState {
                memory_sequence: row.memory_seq.get().into(),
                canonical_watermark: history.watermark.get().into(),
                visibility,
                applicable: row.applicable,
                current: row.current,
                resolution,
                evidence: states,
            }),
        });
        if findings.len() > methods::MAX_PAGE as usize {
            return Err(error(Code::ResourceLimit));
        }
    }
    if findings.is_empty() {
        return Err(unavailable());
    }
    let page = methods::MemoryPage {
        scope: request.scope.clone(),
        task: request.task.clone(),
        generation: None,
        sequence: sequence.into(),
        findings,
        complete: true,
    };
    // A complete inspection describes all selected versions, not their truth or
    // evidence availability. No search generation or retrieval is started.
    if serde_json::to_vec(&page).map_err(|_| unavailable())?.len() > methods::MAX_METHOD_BYTES {
        return Err(error(Code::ResourceLimit));
    }
    check().map_err(memory_error)?;
    Ok(page)
}

#[cfg(test)]
mod tests;
