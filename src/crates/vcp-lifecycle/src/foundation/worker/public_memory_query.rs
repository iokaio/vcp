// SPDX-License-Identifier: Apache-2.0
//! Authenticated retained search under negotiated memory/query-sources/1.
//! No model load, publication, controller acquisition or canonical write.
use super::{public_connection::PublicConnection, *};
use crate::foundation::memory_inspection::{search_captured, SearchError};
use std::{collections::BTreeSet, time::Instant};
use vcp_memory::{retrieval, search_record::ChunkerSpec};
use vcp_protocol::{
    errors::{ApplicationError, Code, Retry},
    jsonrpc::RpcError,
    memory_query as wire,
    methods::{self, Call},
};
type RpcResult<T> = std::result::Result<T, RpcError>;
fn failure(code: Code) -> RpcError {
    ApplicationError {
        code,
        retry: Retry::AfterRevalidation,
        operation: None,
        explanation: "retained memory query unavailable".into(),
        reconciliation: None,
    }
    .into_rpc()
}
fn unavailable() -> RpcError {
    failure(Code::StoreUnavailable)
}
fn memory_error(error: vcp_memory::Error) -> RpcError {
    match error {
        vcp_memory::Error::Access => failure(Code::PolicyDenied),
        _ => unavailable(),
    }
}
fn id(text: &str) -> RpcResult<methods::Id> {
    text.to_owned().try_into().map_err(|_| unavailable())
}
struct Cancel(Arc<AtomicBool>);
impl Drop for Cancel {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

impl PublicConnection {
    /// Dispatch must require negotiated memory/query-sources/1 before calling.
    pub async fn memory_query(
        &self,
        request: &methods::MemoryQuery,
        current: &Access,
    ) -> RpcResult<wire::Page> {
        let (host, access, connection, token) = self
            .rpc_context(current)
            .map_err(|_| failure(Code::PolicyDenied))?;
        Call::MemoryQuery(request.clone())
            .validate()
            .map_err(|_| RpcError::invalid_params())?;
        if request.query.len() > wire::MAX_QUERY_BYTES || request.limit > wire::MAX_RESULTS {
            return Err(failure(Code::ResourceLimit));
        }
        let request = request.clone();
        let connected = self.connected.clone();
        let worker = host.worker.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let _cancel = Cancel(cancelled.clone());
        let start = Instant::now();
        let stopped: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
            cancelled.load(Ordering::SeqCst)
                || !connected.load(Ordering::SeqCst)
                || worker.fenced()
                || start.elapsed() >= Duration::from_secs(30)
        });
        let capture_access = access.clone();
        let capture_connection = connection.clone();
        let capture_token = token.clone();
        let capture_request = request.clone();
        let capture_stop = stopped.clone();
        let (captured, memory_access, directory, sources) = host
            .worker
            .run_cleanup(move |context| {
                Ok((|| -> RpcResult<_> {
                    context
                        .public_authorize(
                            &capture_access,
                            &capture_connection,
                            capture_token.as_ref(),
                            false,
                        )
                        .map_err(|_| failure(Code::PolicyDenied))?;
                    let memory_access =
                        scoped_access(context.engine.store(), &capture_access, &capture_request)?;
                    let check = || {
                        if capture_stop() {
                            Err(vcp_memory::Error::Conflict("memory query interrupted"))
                        } else {
                            Ok(())
                        }
                    };
                    let sources = retrieval::source_bindings_with_check(
                        context.engine.store(),
                        &memory_access,
                        &check,
                    )
                    .map_err(memory_error)?;
                    let internal = retrieval::Request {
                        workspace: memory_access.workspace.clone(),
                        tasks: Some(vec![TaskId::parse(capture_request.task.as_str())
                            .map_err(|_| RpcError::invalid_params())?]),
                        roots: None,
                        paths: None,
                        symbols: None,
                        text: capture_request.query.clone(),
                        historical: None,
                        minimum_sequence: None,
                        timeout_ms: 30_000,
                        results: capture_request.limit as usize,
                        tokens: 16384,
                        bytes: 65536,
                    };
                    let captured = retrieval::capture(
                        context.engine.store(),
                        &memory_access,
                        &internal,
                        &sources.bindings,
                        &ChunkerSpec::default(),
                        &|| capture_stop(),
                    )
                    .map_err(memory_error)?;
                    Ok((
                        captured,
                        memory_access,
                        context.config.canonical_root.join("search-generations"),
                        sources,
                    ))
                })())
            })
            .map_err(|_| unavailable())??;
        let selected = search_captured(captured, memory_access, directory, stopped.clone())
            .await
            .map_err(|error| match error {
                SearchError::Memory(error) => memory_error(error),
                SearchError::Resource => failure(Code::ResourceLimit),
                SearchError::Worker => unavailable(),
            })?;
        host.worker
            .run_cleanup(move |context| {
                Ok((|| -> RpcResult<_> {
                    context
                        .public_authorize(&access, &connection, token.as_ref(), false)
                        .map_err(|_| failure(Code::PolicyDenied))?;
                    let memory_access = scoped_access(context.engine.store(), &access, &request)?;
                    let mut response = retrieval::finish(
                        context.engine.store(),
                        &memory_access,
                        selected,
                        &|| stopped(),
                    )
                    .map_err(memory_error)?;
                    response.rebuild_required |= !sources.complete;
                    response.degraded.extend(sources.degraded);
                    if stopped() {
                        return Err(unavailable());
                    }
                    project(&request, response)
                })())
            })
            .map_err(|_| unavailable())?
    }
}
fn scoped_access(
    store: &Store,
    access: &Access,
    request: &methods::MemoryQuery,
) -> RpcResult<vcp_memory::access::Access> {
    if !access.read
        || request.scope.workspace.as_str() != access.workspace.as_str()
        || request.scope.session.as_str() != access.session.as_str()
    {
        return Err(failure(Code::PolicyDenied));
    }
    let task: Task = store
        .state()
        .record(Collection::Task, request.task.as_str(), &access.workspace)
        .map_err(|_| unavailable())?
        .decode()
        .map_err(|_| unavailable())?;
    if task.scope.workspace != access.workspace
        || task.scope.session != access.session
        || task.scope.task.as_str() != request.task.as_str()
        || task.redaction.is_some()
    {
        return Err(failure(Code::PolicyDenied));
    }
    Ok(vcp_memory::access::Access {
        workspace: access.workspace.clone(),
        actor: access.actor.clone(),
        authority: access.authority,
        read: access.read,
        write: false,
        tasks: Some(BTreeSet::from([task.scope.task])),
    })
}
fn project(request: &methods::MemoryQuery, response: retrieval::Response) -> RpcResult<wire::Page> {
    use vcp_memory::search_record::TextSource;
    let truncated = response.truncated;
    let mut findings = Vec::new();
    for (position, p) in response.passages.into_iter().enumerate() {
        if p.scope.workspace.as_str() != request.scope.workspace.as_str()
            || p.scope.session.as_str() != request.scope.session.as_str()
            || p.scope.task.as_str() != request.task.as_str()
            || p.record_id.is_empty()
            || p.record_id.len() > 512
            || p.text.len() > 16384
            || p.evidence.len() > 64
            || position >= wire::MAX_RESULTS as usize
        {
            return Err(unavailable());
        }
        let source = match p.source {
            TextSource::Artifact { id: artifact } => wire::Source::Artifact {
                artifact: id(artifact.as_str())?,
            },
            TextSource::Claim { claim, version } => wire::Source::Claim {
                claim: id(claim.as_str())?,
                version: id(version.as_str())?,
            },
        };
        let outcome = match p.status {
            vcp_domain::memory::Outcome::Accepted => vcp_protocol::memory::Outcome::Accepted,
            vcp_domain::memory::Outcome::Disputed => vcp_protocol::memory::Outcome::Disputed,
            vcp_domain::memory::Outcome::Rejected => vcp_protocol::memory::Outcome::Rejected,
            vcp_domain::memory::Outcome::AwaitingReview => {
                vcp_protocol::memory::Outcome::AwaitingReview
            }
        };
        let evidence_status = match p.evidence_status {
            vcp_domain::memory::EvidenceStatus::Verified => {
                vcp_protocol::memory::EvidenceStatus::Verified
            }
            vcp_domain::memory::EvidenceStatus::Observed => {
                vcp_protocol::memory::EvidenceStatus::Observed
            }
            vcp_domain::memory::EvidenceStatus::Inferred => {
                vcp_protocol::memory::EvidenceStatus::Inferred
            }
            vcp_domain::memory::EvidenceStatus::Unverified => {
                vcp_protocol::memory::EvidenceStatus::Unverified
            }
        };
        findings.push(wire::Finding {
            record_id: p.record_id,
            source,
            root: id(p.root.as_str())?,
            source_sha256: p.source_digest,
            start: p.span.start.get().into(),
            end: p.span.end.get().into(),
            outcome,
            evidence_status,
            evidence: p
                .evidence
                .iter()
                .map(|e| id(e.as_str()))
                .collect::<RpcResult<_>>()?,
            content: p.text,
            trimmed: p.trimmed,
            rank: position as u32 + 1,
        });
    }
    let mut degraded = Vec::new();
    for code in response.degraded {
        if !matches!(
            code,
            "historical_generation_unavailable"
                | "generation_unavailable"
                | "chunker_incompatible"
                | "minimum_sequence_unsatisfied"
                | "generation_lag"
                | "canonical_inventory_bounded"
                | "recent_overlay_bounded"
                | "recent_overlay_lexical_only"
                | "lexical_candidate_bound"
                | "bounded_exact_subset"
                | "vector_reduced_recall"
                | "lexical_only"
                | "recent_overlay_candidate_bound"
                | "canonical_sources_changed"
                | "source_manifest_limit"
                | "source_manifest_unavailable"
                | "source_read_limit"
                | "source_manifest_invalid"
                | "source_manifest_entry_limit"
                | "source_unavailable"
                | "source_binding_limit"
        ) {
            return Err(unavailable());
        }
        if !degraded.iter().any(|value| value == code) {
            degraded.push(code.to_owned());
        }
    }
    let complete = !truncated
        && !response.rebuild_required
        && degraded.is_empty()
        && !findings.iter().any(|p| p.trimmed);
    let page = wire::Page {
        scope: request.scope.clone(),
        task: request.task.clone(),
        generation: response
            .generation
            .as_ref()
            .map(|g| id(g.as_str()))
            .transpose()?,
        generation_watermark: response.generation_watermark.map(|v| v.get().into()),
        canonical_watermark: response.canonical_watermark.get().into(),
        sequence: response.indexed_sequence.get().into(),
        findings,
        rebuild_required: response.rebuild_required,
        degraded,
        truncated,
        complete,
    };
    if serde_json::to_vec(&page).map_err(|_| unavailable())?.len() > methods::MAX_METHOD_BYTES {
        return Err(failure(Code::ResourceLimit));
    }
    Ok(page)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> methods::MemoryQuery {
        methods::MemoryQuery {
            scope: methods::Scope {
                workspace: id("workspace").unwrap(),
                session: id("session").unwrap(),
            },
            task: id("task").unwrap(),
            query: "needle".into(),
            limit: 8,
        }
    }
    fn response() -> retrieval::Response {
        retrieval::Response {
            fusion: retrieval::FUSION_VERSION,
            token_accounting: retrieval::TOKEN_ACCOUNTING,
            canonical_watermark: Watermark::new(12),
            generation: None,
            generation_watermark: None,
            indexed_sequence: MemorySeq::ZERO,
            rebuild_required: true,
            degraded: vec!["generation_unavailable"],
            passages: vec![],
            token_upper_bound: 2,
            fence: None,
            truncated: false,
        }
    }
    #[test]
    fn missing_generation_and_unknown_diagnostics_never_claim_complete() {
        let page = project(&request(), response()).unwrap();
        assert!(!page.complete);
        assert!(page.rebuild_required);
        assert!(page.findings.is_empty());
        let mut bad = response();
        bad.degraded.push("private-path-or-provider-error");
        assert!(project(&request(), bad).is_err());
    }
    #[test]
    fn tagged_source_projection_preserves_real_identity_and_scope() {
        let mut response = response();
        response.passages.push(retrieval::Passage {
            record_id: "retained:chunk:0".into(),
            scope: Scope {
                workspace: WorkspaceId::parse("workspace").unwrap(),
                session: SessionId::parse("session").unwrap(),
                task: TaskId::parse("task").unwrap(),
            },
            root: RootId::parse("root").unwrap(),
            paths: vec!["file.txt".into()],
            symbols: vec![],
            source: vcp_memory::search_record::TextSource::Artifact {
                id: ArtifactId::parse("artifact").unwrap(),
            },
            source_digest: "a".repeat(64),
            span: vcp_domain::artifact::Range {
                start: ByteCount::ZERO,
                end: ByteCount::new(6),
            },
            status: vcp_domain::memory::Outcome::Accepted,
            evidence_status: vcp_domain::memory::EvidenceStatus::Observed,
            evidence: vec![ArtifactId::parse("artifact").unwrap()],
            rank: retrieval::Ranked {
                id: "retained:chunk:0".into(),
                lexical_rank: Some(1),
                vector_rank: None,
                lexical_score: Some(1.0),
                vector_distance: None,
                overlay_rank: None,
                overlay_score: None,
                fused_score: 1.0,
            },
            text: "needle".into(),
            trimmed: false,
        });
        let page = project(&request(), response).unwrap();
        assert!(
            matches!(&page.findings[0].source,wire::Source::Artifact{artifact} if artifact.as_str()=="artifact")
        );
        assert_eq!(page.findings[0].record_id, "retained:chunk:0");
        assert_eq!(page.findings[0].rank, 1);
    }
}
