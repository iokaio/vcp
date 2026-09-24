// SPDX-License-Identifier: Apache-2.0
//! Read-only claim history using current governed source access and retention.
use super::public_connection::PublicConnection;
use super::*;
use std::{collections::BTreeSet, time::Instant};
use vcp_protocol::{
    errors::{ApplicationError, Code, Retry},
    jsonrpc::RpcError,
    memory as wire, memory_history as history_wire,
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
fn visible_origins(
    state: &vcp_store::contract::State,
    access: &Access,
    origins: &[EventId],
) -> std::result::Result<Vec<methods::Id>, RpcError> {
    origins
        .iter()
        .filter(|id| {
            state.events.iter().any(|event| {
                event.event.id == **id
                    && event.event.workspace == access.workspace
                    && event.event.session == access.session
            })
        })
        .map(|origin| id(origin.as_str()))
        .collect()
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    query: String,
    authority: u64,
    deletion: u64,
    at: u64,
    after: u64,
}

impl PublicConnection {
    pub fn memory_history(
        &self,
        request: &history_wire::Request,
        current: &Access,
    ) -> std::result::Result<history_wire::Page, RpcError> {
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
                    Call::MemoryHistory(request.clone())
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
    request: &history_wire::Request,
    check: &dyn Fn() -> vcp_memory::Result<()>,
) -> std::result::Result<history_wire::Page, RpcError> {
    history_wire::validate_request(request).map_err(|_| RpcError::invalid_params())?;
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
        if source.scope.session == access.session && source.redaction.is_none() {
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
    let workspace: vcp_domain::workspace::Workspace = store
        .state()
        .record(
            Collection::Workspace,
            access.workspace.as_str(),
            &access.workspace,
        )
        .map_err(|_| unavailable())?
        .decode()
        .map_err(|_| unavailable())?;
    let query = vcp_protocol::digest_bytes(
        &vcp_protocol::canonical_bytes(&(
            &request.scope,
            &request.task,
            &request.claim,
            request.limit,
            &access.actor,
            &memory_access.tasks,
            access.read,
            &task.fingerprint,
        ))
        .map_err(|_| unavailable())?,
    );
    let cursor: Option<Cursor> = request
        .cursor
        .as_ref()
        .map(|text| serde_json::from_str(text).map_err(|_| RpcError::invalid_params()))
        .transpose()?;
    if cursor.as_ref().is_some_and(|c| {
        c.query != query
            || c.authority != workspace.authority.get()
            || c.deletion != workspace.deletion.get()
            || c.after > c.at
    }) {
        return Err(error(Code::VersionConflict));
    }
    let (history, at, more) = vcp_memory::history::window_with_check(
        store,
        &memory_access,
        &claim,
        cursor.as_ref().map(|c| MemorySeq::new(c.at)),
        cursor
            .as_ref()
            .map_or(MemorySeq::ZERO, |c| MemorySeq::new(c.after)),
        request.limit as usize,
        Some(&task.fingerprint),
        check,
    )
    .map_err(memory_error)?;
    let watermark = history.watermark.get();
    let available = history.versions.len();
    if available == 0 {
        return Err(unavailable());
    }
    let mut after = cursor.as_ref().map_or(0, |c| c.after);
    let mut bytes = 0;
    let mut findings = Vec::new();
    for row in history.versions {
        check().map_err(memory_error)?;
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
        let origins = row
            .version
            .as_ref()
            .map(|version| visible_origins(store.state(), access, &version.proposal.origins))
            .transpose()?
            .unwrap_or_default();
        let (mut content, resolution) = if let Some(version) = row.version {
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
        if evidence.len() > 64 || states.len() > 64 {
            return Err(error(Code::ResourceLimit));
        }
        let content_truncated = content.len() > history_wire::MAX_SUMMARY_BYTES;
        if content_truncated {
            let mut end = history_wire::MAX_SUMMARY_BYTES;
            while !content.is_char_boundary(end) {
                end -= 1;
            }
            content.truncate(end);
        }
        let decision = vcp_memory::retention::decision(
            store.state(),
            &access.workspace,
            &vcp_memory::retention::Target::Record(vcp_store::contract::key(
                Collection::Claim,
                row.id.as_str(),
            )),
        )
        .map_err(memory_error)?;
        let projected = history_wire::Version {
            origins,
            content_truncated,
            presentation_compacted: decision.as_ref().is_some_and(|d| d.compacted),
            recall_excluded: decision.as_ref().is_some_and(|d| d.recall_excluded),
            finding: methods::MemoryFinding {
                claim: request.claim.clone(),
                version: id(row.id.as_str())?,
                evidence,
                content,
                state: Some(wire::InspectionState {
                    memory_sequence: row.memory_seq.get().into(),
                    canonical_watermark: watermark.into(),
                    visibility,
                    applicable: row.applicable,
                    current: row.current,
                    resolution,
                    evidence: states,
                }),
            },
        };
        let size = serde_json::to_vec(&projected)
            .map_err(|_| unavailable())?
            .len();
        if size > 60000 {
            return Err(error(Code::ResourceLimit));
        }
        if bytes + size > 60000 {
            break;
        }
        bytes += size;
        after = row.memory_seq.get();
        findings.push(projected);
    }
    let more = more || findings.len() < available;
    let next_cursor = more
        .then(|| {
            serde_json::to_string(&Cursor {
                query,
                authority: workspace.authority.get(),
                deletion: workspace.deletion.get(),
                at: at.get(),
                after,
            })
        })
        .transpose()
        .map_err(|_| unavailable())?;
    let page = history_wire::Page {
        scope: request.scope.clone(),
        task: request.task.clone(),
        claim: request.claim.clone(),
        watermark: watermark.into(),
        at: at.get().into(),
        versions: findings,
        next_cursor,
        complete: !more,
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
#[path = "public_memory_history/tests.rs"]
mod tests;
