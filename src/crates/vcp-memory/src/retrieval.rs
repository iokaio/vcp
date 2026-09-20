// SPDX-License-Identifier: Apache-2.0
//! Bounded hybrid selection. Persisted index text is never a returned passage.
//! The owner pins a View; current canonical authorization supplies every byte.
use crate::{
    access::{self, Access},
    lexical,
    publication::View,
    search_record::{self, ChunkerSpec, SearchRecord, SourceBinding, TextSource},
    vector, Error, Result,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::{Duration, Instant},
};
use vcp_domain::{
    artifact::Range,
    memory::{Applicability, EvidenceStatus, Outcome, Version},
    workspace::Scope,
    *,
};
use vcp_store::{contract::Collection, Store};

pub const FUSION_VERSION: &str = "rrf-equal-k60-id-ascending/1";
pub const TOKEN_ACCOUNTING: &str = "serialized-passage-array-utf8-byte-upper-bound/1";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub workspace: WorkspaceId,
    pub tasks: Option<Vec<TaskId>>,
    pub roots: Option<Vec<RootId>>,
    pub paths: Option<Vec<String>>,
    pub symbols: Option<Vec<String>>,
    pub text: String,
    pub historical: Option<MemorySeq>,
    pub minimum_sequence: Option<MemorySeq>,
    pub timeout_ms: u64,
    pub results: usize,
    pub tokens: usize,
    pub bytes: usize,
}
impl Request {
    pub fn validate(&self) -> Result<()> {
        if self.text.len() > 4096
            || self.timeout_ms == 0
            || self.timeout_ms > 30_000
            || self.results == 0
            || self.results > 64
            || self.tokens < 2
            || self.tokens > 16384
            || self.bytes < 2
            || self.bytes > 65536
            || self.tasks.as_ref().is_some_and(|v| v.len() > 256)
            || self.roots.as_ref().is_some_and(|v| v.len() > 64)
            || self.paths.as_ref().is_some_and(|v| {
                v.len() > 64
                    || v.iter().any(|p| {
                        p.is_empty()
                            || p.len() > 4096
                            || p.starts_with('/')
                            || p.contains(['\\', ':', '\0'])
                            || p.split('/').any(|s| matches!(s, "" | "." | ".."))
                    })
            })
            || self.symbols.as_ref().is_some_and(|v| {
                v.len() > 64
                    || v.iter()
                        .any(|s| s.is_empty() || s.len() > 1024 || s.contains('\0'))
            })
        {
            return Err(Error::Invalid("bounded retrieval request".into()));
        }
        Ok(())
    }
    fn contains(&self, record: &SearchRecord) -> bool {
        record.scope.workspace == self.workspace
            && self
                .tasks
                .as_ref()
                .is_none_or(|v| v.contains(&record.scope.task))
            && self.roots.as_ref().is_none_or(|v| v.contains(&record.root))
            && self
                .paths
                .as_ref()
                .is_none_or(|v| v.iter().any(|p| record.paths.contains(p)))
            && self
                .symbols
                .as_ref()
                .is_none_or(|v| v.iter().any(|p| record.symbols.contains(p)))
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Ranked {
    pub id: String,
    pub lexical_rank: Option<usize>,
    pub vector_rank: Option<usize>,
    pub lexical_score: Option<f32>,
    pub vector_distance: Option<f32>,
    pub fused_score: f64,
}
/// One contribution per source record per component, even if many embedding
/// subchunks match. Component ordinals are never used as evidence identities.
pub fn fuse(lexical: &[lexical::Candidate], vectors: &[vector::Candidate]) -> Result<Vec<Ranked>> {
    if lexical.len() > 100 || vectors.len() > 64 {
        return Err(Error::Invalid("retrieval candidate bound".into()));
    }
    let mut rows = BTreeMap::<String, Ranked>::new();
    for (rank, (id, score, from_lexical)) in lexical
        .iter()
        .map(|r| (r.rank, (&r.id, r.score, true)))
        .chain(
            vectors
                .iter()
                .map(|r| (&r.source, r.distance, false))
                .enumerate()
                .map(|(position, value)| (position + 1, value)),
        )
    {
        if id.len() > 512 || !score.is_finite() || rank == 0 || rank > 100 {
            return Err(Error::Invalid("invalid retrieval candidate".into()));
        }
        let row = rows.entry(id.clone()).or_insert_with(|| Ranked {
            id: id.clone(),
            lexical_rank: None,
            vector_rank: None,
            lexical_score: None,
            vector_distance: None,
            fused_score: 0.0,
        });
        if from_lexical && row.lexical_rank.is_none() {
            row.lexical_rank = Some(rank);
            row.lexical_score = Some(score);
            row.fused_score += 1.0 / (60 + rank) as f64;
        } else if !from_lexical && row.vector_rank.is_none() {
            row.vector_rank = Some(rank);
            row.vector_distance = Some(score);
            row.fused_score += 1.0 / (60 + rank) as f64;
        }
    }
    let mut result: Vec<_> = rows.into_values().collect();
    result.sort_by(|a, b| {
        b.fused_score
            .total_cmp(&a.fused_score)
            .then(a.id.cmp(&b.id))
    });
    Ok(result)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFence {
    pub record_id: String,
    pub scope: Scope,
    pub root: RootId,
    pub paths: Vec<String>,
    pub symbols: Vec<String>,
    pub source: TextSource,
    pub digest: String,
    pub span: Range,
    pub applicability: Option<Applicability>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fence {
    pub workspace: WorkspaceId,
    pub authority: AuthorityRevision,
    pub deletion: DeletionEpoch,
    pub binding: Revision,
    pub canonical_watermark: Watermark,
    pub generation: GenerationId,
    pub chunker: ChunkerSpec,
    pub bindings: Vec<SourceBinding>,
    pub sources: Vec<SourceFence>,
}
fn source_fence(record: &SearchRecord) -> SourceFence {
    SourceFence {
        record_id: record.id.clone(),
        scope: record.scope.clone(),
        root: record.root.clone(),
        paths: record.paths.clone(),
        symbols: record.symbols.clone(),
        source: record.source.clone(),
        digest: record.source_digest.clone(),
        span: record.span.clone(),
        applicability: record.applicability.clone(),
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Passage {
    pub record_id: String,
    pub scope: Scope,
    pub root: RootId,
    pub paths: Vec<String>,
    pub symbols: Vec<String>,
    pub source: TextSource,
    pub source_digest: String,
    pub span: Range,
    pub status: Outcome,
    pub evidence_status: EvidenceStatus,
    pub evidence: Vec<ArtifactId>,
    pub rank: Ranked,
    pub text: String,
    pub trimmed: bool,
}
#[derive(Debug, Serialize)]
pub struct Response {
    pub fusion: &'static str,
    pub token_accounting: &'static str,
    pub canonical_watermark: Watermark,
    pub generation: Option<GenerationId>,
    pub generation_watermark: Option<Watermark>,
    pub indexed_sequence: MemorySeq,
    pub rebuild_required: bool,
    /// Fixed codes only: denied names, IDs and text never become diagnostics.
    pub degraded: Vec<&'static str>,
    pub passages: Vec<Passage>,
    pub token_upper_bound: usize,
    pub fence: Option<Fence>,
}
fn checkpoint(start: Instant, request: &Request, cancelled: &dyn Fn() -> bool) -> Result<()> {
    if cancelled() {
        return Err(Error::Conflict("retrieval cancelled"));
    }
    if start.elapsed() >= Duration::from_millis(request.timeout_ms) {
        return Err(Error::Conflict("retrieval deadline"));
    }
    Ok(())
}
fn fits(mut passage: Passage, available: usize) -> Result<Option<(Passage, usize)>> {
    let original = passage.text.clone();
    let mut end = original.len().min(available);
    loop {
        while !original.is_char_boundary(end) {
            end -= 1;
        }
        passage.text = original[..end].into();
        passage.trimmed = end < original.len();
        passage.span.end = ByteCount::new(passage.span.start.get() + end as u64);
        let bytes = serde_json::to_vec(&passage)?.len();
        if bytes <= available {
            return Ok((end > 0).then_some((passage, bytes)));
        }
        if end == 0 {
            return Ok(None);
        }
        end = end.saturating_sub(bytes - available);
    }
}
fn overlaps(left: &Passage, right: &SearchRecord) -> bool {
    if left.source != right.source {
        return false;
    }
    matches!(right.source, TextSource::Claim { .. })
        || (left.span.start < right.span.end && right.span.start < left.span.end)
}
/// Query vectors must already have been computed locally under host admission.
/// This function never dispatches a model call or repairs an index on a read.
#[derive(Clone, Copy, Debug)]
pub struct QueryVector<'a> {
    pub specification: &'a str,
    pub values: &'a [f32],
}

/// Canonical authorization and retained inputs captured by the owner. The root
/// snapshot lease survives native search and prevents concurrent physical purge.
pub struct Capture {
    start: Instant,
    request: Request,
    bindings: Vec<SourceBinding>,
    chunker: ChunkerSpec,
    workspace: vcp_domain::workspace::Workspace,
    actor: ActorId,
    tasks: Option<BTreeSet<TaskId>>,
    inventory: Option<search_record::Inventory>,
    _snapshot: vcp_store::Snapshot,
}
/// Opaque candidate IDs and scores; only finish() can turn these into passages.
/// The captured access scope can be narrowed later but cannot be expanded.
pub struct Selection {
    capture: Capture,
    response: Response,
    ranked: Vec<Ranked>,
    eligible: BTreeMap<String, SourceFence>,
    materialize: bool,
}
fn narrow_tasks(access: &Access, request: &Request) -> Option<BTreeSet<TaskId>> {
    match (&access.tasks, &request.tasks) {
        (Some(allowed), Some(requested)) => Some(
            requested
                .iter()
                .filter(|id| allowed.contains(*id))
                .cloned()
                .collect(),
        ),
        (Some(allowed), None) => Some(allowed.clone()),
        (None, Some(requested)) => Some(requested.iter().cloned().collect()),
        (None, None) => None,
    }
}
/// Owner-only stage: resolve present-day access and bounded retained source bytes.
/// No component lookup, lexical search, vector search or model work occurs here.
pub fn capture(
    store: &Store,
    access: &Access,
    request: &Request,
    bindings: &[SourceBinding],
    chunker: &ChunkerSpec,
    cancelled: &dyn Fn() -> bool,
) -> Result<Capture> {
    let start = Instant::now();
    request.validate()?;
    chunker.validate()?;
    checkpoint(start, request, cancelled)?;
    let workspace = access::authorize(store.state(), access, false)?;
    if request.workspace != access.workspace {
        return Err(Error::Access);
    }
    if bindings.len() > 64 {
        return Err(Error::Invalid("retrieval source binding limit".into()));
    }
    let tasks = narrow_tasks(access, request);
    let narrowed = Access {
        workspace: access.workspace.clone(),
        actor: access.actor.clone(),
        authority: access.authority,
        read: access.read,
        write: false,
        tasks: tasks.clone(),
    };
    let snapshot = store.snapshot()?;
    let inventory = if request.historical.is_some() {
        None
    } else {
        Some(search_record::inventory_with_check(
            store,
            &narrowed,
            bindings,
            chunker,
            search_record::Limits::default(),
            &|| checkpoint(start, request, cancelled),
        )?)
    };
    checkpoint(start, request, cancelled)?;
    Ok(Capture {
        start,
        request: request.clone(),
        bindings: bindings.to_vec(),
        chunker: chunker.clone(),
        workspace,
        actor: access.actor.clone(),
        tasks,
        inventory,
        _snapshot: snapshot,
    })
}
/// Off-owner stage: all native component search runs against the pinned view and
/// IDs authorized by the capture. A queued stage shares the original deadline.
pub fn search(
    capture: Capture,
    view: Option<&View>,
    query_vector: Option<QueryVector<'_>>,
    cancelled: &dyn Fn() -> bool,
) -> Result<Selection> {
    let request = &capture.request;
    let start = capture.start;
    checkpoint(start, request, cancelled)?;
    let mut response = Response {
        fusion: FUSION_VERSION,
        token_accounting: TOKEN_ACCOUNTING,
        canonical_watermark: capture._snapshot.state().watermark,
        generation: None,
        generation_watermark: None,
        indexed_sequence: MemorySeq::ZERO,
        rebuild_required: false,
        degraded: vec![],
        passages: vec![],
        token_upper_bound: 2,
        fence: None,
    };
    if request.historical.is_some() {
        response.rebuild_required = true;
        response.degraded.push("historical_generation_unavailable");
        return Ok(Selection {
            capture,
            response,
            ranked: vec![],
            eligible: BTreeMap::new(),
            materialize: false,
        });
    }
    let Some(view) = view else {
        response.rebuild_required = true;
        response.degraded.push("generation_unavailable");
        return Ok(Selection {
            capture,
            response,
            ranked: vec![],
            eligible: BTreeMap::new(),
            materialize: false,
        });
    };
    if view.manifest.scope.workspace != capture.workspace.id
        || view.inventory.workspace != capture.workspace.id
    {
        return Err(Error::Access);
    }
    response.generation = Some(view.manifest.id.clone());
    response.generation_watermark = Some(view.manifest.canonical_watermark);
    response.indexed_sequence = view.manifest.memory_seq;
    if view.inventory.chunker_digest != capture.chunker.digest()? {
        response.rebuild_required = true;
        response.degraded.push("chunker_incompatible");
        return Ok(Selection {
            capture,
            response,
            ranked: vec![],
            eligible: BTreeMap::new(),
            materialize: false,
        });
    }
    if request
        .minimum_sequence
        .is_some_and(|minimum| minimum > view.manifest.memory_seq)
    {
        response.degraded.push("minimum_sequence_unsatisfied");
    }
    if view.manifest.canonical_watermark < response.canonical_watermark {
        response.degraded.push("generation_lag");
    }
    let current = capture
        .inventory
        .as_ref()
        .ok_or(Error::Conflict("captured inventory unavailable"))?;
    if current
        .exclusions
        .iter()
        .any(|e| e.reason.contains("limit"))
    {
        response.degraded.push("canonical_inventory_bounded");
        response.rebuild_required = true;
    }
    let published: BTreeSet<_> = view
        .inventory
        .records
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    let eligible: BTreeMap<_, _> = current
        .records
        .iter()
        .filter(|r| request.contains(r) && published.contains(r.id.as_str()))
        .map(|r| (r.id.clone(), source_fence(r)))
        .collect();
    let lexical = view.lexical.search_authorized(
        &lexical::Query {
            workspace: capture.workspace.id.clone(),
            tasks: capture.tasks.as_ref().map(|v| v.iter().cloned().collect()),
            roots: request.roots.clone(),
            paths: request.paths.clone(),
            symbols: request.symbols.clone(),
            kind: None,
            claim_kind: None,
            status: None,
            text: request.text.clone(),
            phrase: false,
            limit: 100,
        },
        &eligible.keys().cloned().collect(),
    )?;
    if lexical.len() == 100 {
        response.degraded.push("lexical_candidate_bound");
    }
    let lexical: Vec<_> = lexical
        .into_iter()
        .filter(|candidate| eligible.contains_key(&candidate.id))
        .collect();
    checkpoint(start, request, cancelled)?;
    let vectors = if let (Some(component), Some(vector)) = (&view.vector, query_vector) {
        if vector.specification != view.manifest.embedding_specification {
            return Err(Error::Conflict("query embedding specification mismatch"));
        }
        let allowed = component
            .rows()
            .iter()
            .filter(|row| eligible.contains_key(&row.identity.source))
            .map(|row| row.identity.id.clone())
            .collect();
        let stop_native =
            || cancelled() || start.elapsed() >= Duration::from_millis(request.timeout_ms);
        let candidates = component.query(
            &capture.workspace.id,
            &allowed,
            vector.values,
            64,
            &stop_native,
        )?;
        match candidates.mode {
            vector::Mode::Ann => (),
            vector::Mode::ExactAuthorizedSubset => response.degraded.push("bounded_exact_subset"),
            vector::Mode::ReducedRecall => response.degraded.push("vector_reduced_recall"),
        }
        candidates.rows
    } else {
        response.degraded.push("lexical_only");
        vec![]
    };
    let ranked = fuse(&lexical, &vectors)?;
    checkpoint(start, request, cancelled)?;
    Ok(Selection {
        capture,
        response,
        ranked,
        eligible,
        materialize: true,
    })
}
/// Owner-only return fence. Recompute authorized bytes from current canonical
/// state after native search; changed/hidden sources cannot reuse old snippets.
pub fn finish(
    store: &Store,
    access: &Access,
    selection: Selection,
    cancelled: &dyn Fn() -> bool,
) -> Result<Response> {
    let Selection {
        capture,
        mut response,
        ranked,
        eligible,
        materialize,
    } = selection;
    let start = capture.start;
    let request = &capture.request;
    let bindings = &capture.bindings;
    let chunker = &capture.chunker;
    let workspace = access::authorize(store.state(), access, false)?;
    checkpoint(start, request, cancelled)?;
    if workspace.id != capture.workspace.id
        || access.actor != capture.actor
        || workspace.authority != capture.workspace.authority
        || workspace.deletion != capture.workspace.deletion
        || workspace.binding.revision != capture.workspace.binding.revision
    {
        return Err(Error::Access);
    }
    response.canonical_watermark = store.state().watermark;
    if !materialize {
        return Ok(response);
    }
    if response
        .generation_watermark
        .is_some_and(|watermark| watermark < store.state().watermark)
        && !response.degraded.contains(&"generation_lag")
    {
        response.degraded.push("generation_lag");
    }
    let mut tasks = narrow_tasks(access, request);
    if let Some(captured) = &capture.tasks {
        tasks = Some(match tasks {
            Some(current) => current.intersection(captured).cloned().collect(),
            None => captured.clone(),
        });
    }
    let narrowed = Access {
        workspace: access.workspace.clone(),
        actor: access.actor.clone(),
        authority: access.authority,
        read: access.read,
        write: false,
        tasks,
    };
    let fresh = search_record::inventory_with_check(
        store,
        &narrowed,
        bindings,
        chunker,
        search_record::Limits::default(),
        &|| checkpoint(start, request, cancelled),
    )?;
    if fresh.exclusions.iter().any(|e| e.reason.contains("limit")) {
        response.rebuild_required = true;
        if !response.degraded.contains(&"canonical_inventory_bounded") {
            response.degraded.push("canonical_inventory_bounded");
        }
    }
    let records: BTreeMap<_, _> = fresh
        .records
        .iter()
        .filter(|record| {
            request.contains(record)
                && eligible
                    .get(&record.id)
                    .is_some_and(|saved| saved == &source_fence(record))
        })
        .map(|record| (record.id.clone(), record))
        .collect();
    if ranked.iter().any(|rank| !records.contains_key(&rank.id)) {
        response.degraded.push("canonical_sources_changed");
    }
    let mut selected = Vec::new();
    let mut used = 2; // JSON array brackets; each later passage adds a comma.
    for rank in ranked {
        checkpoint(start, request, cancelled)?;
        if response.passages.len() >= request.results {
            break;
        }
        let Some(record) = records.get(&rank.id) else {
            continue;
        };
        if response.passages.iter().any(|left| overlaps(left, record)) {
            continue;
        }
        let evidence = match &record.source {
            TextSource::Artifact { id } => vec![id.clone()],
            TextSource::Claim { version, .. } => {
                let value: Version = store
                    .state()
                    .record(Collection::Claim, version.as_str(), &access.workspace)?
                    .decode()?;
                access::proposal_scope(store.state(), &narrowed, &value.proposal)?;
                value
                    .proposal
                    .evidence
                    .iter()
                    .map(|e| e.artifact.clone())
                    .collect()
            }
        };
        let passage = Passage {
            record_id: record.id.clone(),
            scope: record.scope.clone(),
            root: record.root.clone(),
            paths: record.paths.clone(),
            symbols: record.symbols.clone(),
            source: record.source.clone(),
            source_digest: record.source_digest.clone(),
            span: record.span.clone(),
            status: record.status,
            evidence_status: record.evidence_status,
            evidence,
            rank,
            text: record.text.clone(),
            trimmed: false,
        };
        let separator = usize::from(!response.passages.is_empty());
        if let Some((passage, bytes)) = fits(
            passage,
            request
                .bytes
                .min(request.tokens)
                .saturating_sub(used + separator),
        )? {
            used += bytes + separator;
            selected.push(source_fence(record));
            response.passages.push(passage);
        }
    }
    checkpoint(start, request, cancelled)?;
    let latest = access::authorize(store.state(), access, false)?;
    if latest.authority != workspace.authority
        || latest.deletion != workspace.deletion
        || latest.binding.revision != workspace.binding.revision
    {
        return Err(Error::Access);
    }
    response.token_upper_bound = used;
    let bindings: Vec<_> = bindings
        .iter()
        .filter(|binding| {
            selected.iter().any(|source| {
                matches!(&source.source,TextSource::Artifact{id} if id==&binding.artifact)
                    && source.root == binding.root
                    && source.paths.contains(&binding.path)
                    && source.symbols == binding.symbols
            })
        })
        .cloned()
        .collect();
    if bindings.len() > 64 {
        return Err(Error::Invalid("retrieval fence binding limit".into()));
    }
    response.fence = Some(Fence {
        workspace: workspace.id,
        authority: workspace.authority,
        deletion: workspace.deletion,
        binding: workspace.binding.revision,
        canonical_watermark: store.state().watermark,
        generation: response
            .generation
            .clone()
            .ok_or(Error::Conflict("selected generation unavailable"))?,
        chunker: chunker.clone(),
        bindings,
        sources: selected,
    });
    Ok(response)
}

/// Synchronous qualification/inspection wrapper. Production hosts run search()
/// on an admitted blocking worker and finish() on their canonical owner.
#[allow(clippy::too_many_arguments)]
pub fn query(
    store: &Store,
    access: &Access,
    view: Option<&View>,
    request: &Request,
    bindings: &[SourceBinding],
    chunker: &ChunkerSpec,
    query_vector: Option<QueryVector<'_>>,
    cancelled: &dyn Fn() -> bool,
) -> Result<Response> {
    let captured = capture(store, access, request, bindings, chunker, cancelled)?;
    finish(
        store,
        access,
        search(captured, view, query_vector, cancelled)?,
        cancelled,
    )
}
/// Invoke again at the controller's send-admission fence after refreshing local
/// source observations. Epoch equality alone is insufficient: recompute retained
/// bytes, source bindings, claim eligibility and every selected chunk identity.
pub fn revalidate_fence(store: &Store, access: &Access, fence: &Fence) -> Result<()> {
    let workspace = access::authorize(store.state(), access, false)?;
    if fence.workspace != workspace.id
        || fence.authority != workspace.authority
        || fence.deletion != workspace.deletion
        || fence.binding != workspace.binding.revision
        || fence.sources.len() > 64
        || fence.bindings.len() > 64
        || fence.canonical_watermark > store.state().watermark
    {
        return Err(Error::Access);
    }
    let inventory = search_record::inventory(
        store,
        access,
        &fence.bindings,
        &fence.chunker,
        search_record::Limits::default(),
    )?;
    for selected in &fence.sources {
        if !inventory
            .records
            .iter()
            .any(|record| source_fence(record) == *selected)
        {
            return Err(Error::Access);
        }
    }
    Ok(())
}

/// Read-only discovery from retained native manifests. Completeness is explicit;
/// this is current canonical provenance, not a refreshed filesystem observation.
#[derive(Debug, Serialize)]
pub struct SourceBindings {
    pub bindings: Vec<SourceBinding>,
    pub complete: bool,
    pub degraded: Vec<&'static str>,
}
impl SourceBindings {
    fn deficit(&mut self, reason: &'static str) {
        self.complete = false;
        if !self.degraded.contains(&reason) {
            self.degraded.push(reason);
        }
    }
}
pub fn source_bindings(store: &Store, access: &Access) -> Result<SourceBindings> {
    source_bindings_with_check(store, access, &|| Ok(()))
}
/// All bounds cover authorized inputs only. No hidden source IDs/paths are
/// returned in deficit explanations, and the helper never opens workspace files.
pub fn source_bindings_with_check(
    store: &Store,
    access: &Access,
    check: &dyn Fn() -> Result<()>,
) -> Result<SourceBindings> {
    use vcp_domain::{artifact::ArtifactDescriptor, task::Task};
    const ARTIFACT_BYTES: u64 = 256 * 1024;
    const READ_BYTES: u64 = 4 * 1024 * 1024;
    fn read(
        store: &Store,
        access: &Access,
        artifact: &ArtifactDescriptor,
        total: &mut u64,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<Option<Vec<u8>>> {
        check()?;
        if artifact.spec.scope.workspace != access.workspace
            || !access.allows_task(&artifact.spec.scope.task)
            || !crate::proof::complete_capture(artifact)
            || artifact.length.get() > ARTIFACT_BYTES
        {
            return Ok(None);
        }
        if total.saturating_add(artifact.length.get()) > READ_BYTES {
            return Err(Error::Conflict("source discovery read limit"));
        }
        *total += artifact.length.get();
        let mut bytes = Vec::with_capacity(artifact.length.get() as usize);
        let read = vcp_audit::history::History::read_artifact(
            store,
            &access.history(),
            &artifact.spec.id,
            &mut bytes,
        );
        check()?;
        match read {
            Ok(_) => Ok(Some(bytes)),
            Err(_) => Ok(None),
        }
    }
    check()?;
    let workspace = access::authorize(store.state(), access, false)?;
    let mut result = SourceBindings {
        bindings: vec![],
        complete: true,
        degraded: vec![],
    };
    let mut total = 0u64;
    let mut manifests = 0usize;
    let mut unique = BTreeSet::new();
    'manifests: for row in store.state().records.values() {
        check()?;
        if row.workspace != access.workspace || row.collection != Collection::Artifact {
            continue;
        }
        let manifest: ArtifactDescriptor = row.decode()?;
        if !access.allows_task(&manifest.spec.scope.task) {
            continue;
        }
        if !matches!(
            manifest.spec.schema.as_str(),
            "verification-baseline/1"
                | "verification-plan/1"
                | "verification-result/1"
                | "vcp-memory-change/1"
        ) {
            continue;
        }
        if manifests == 128 {
            result.deficit("source_manifest_limit");
            break;
        }
        manifests += 1;
        let bytes = match read(store, access, &manifest, &mut total, check) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => {
                result.deficit("source_manifest_unavailable");
                continue;
            }
            Err(Error::Conflict("source discovery read limit")) => {
                result.deficit("source_read_limit");
                break;
            }
            Err(error) => return Err(error),
        };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            result.deficit("source_manifest_invalid");
            continue;
        };
        let (snapshot, references) = match manifest.spec.schema.as_str() {
            "verification-plan/1" => (&value["before"], &value["source_artifacts"]),
            "verification-baseline/1" => (&value["manifest"], &value["sources"]),
            "verification-result/1" if value["applicability"] == "current" => {
                (&value["before"], &value["source_artifacts"])
            }
            "vcp-memory-change/1" => (&value["current"], &value["sources"]),
            _ => continue,
        };
        let task: Task = store
            .state()
            .record(
                Collection::Task,
                manifest.spec.scope.task.as_str(),
                &workspace.id,
            )?
            .decode()?;
        if snapshot["bounded_scan_complete"] != true
            || snapshot["identity"]["workspace"].as_str() != Some(workspace.id.as_str())
            || snapshot["identity"]["repository"].as_str()
                != Some(workspace.binding.repository.as_str())
            || snapshot["identity"]["worktree"].as_str()
                != Some(workspace.binding.worktree.as_str())
            || snapshot["identity"]["binding"] != serde_json::to_value(workspace.binding.revision)?
            || vcp_protocol::digest_bytes(&vcp_protocol::canonical_bytes(snapshot)?)
                != task.fingerprint.repository
        {
            continue;
        }
        let Some(root) = snapshot["identity"]["root"]
            .as_str()
            .and_then(|root| RootId::parse(root).ok())
        else {
            result.deficit("source_manifest_invalid");
            continue;
        };
        let (Some(files), Some(references)) = (snapshot["files"].as_array(), references.as_array())
        else {
            result.deficit("source_manifest_invalid");
            continue;
        };
        if files.len() > 4096 || references.len() > 4096 {
            result.deficit("source_manifest_entry_limit");
            continue;
        }
        for reference in references {
            check()?;
            let id = if manifest.spec.schema == "vcp-memory-change/1" {
                reference["artifact"].as_str()
            } else {
                reference.as_str()
            };
            let Some(id) = id.and_then(|id| ArtifactId::parse(id).ok()) else {
                result.deficit("source_manifest_invalid");
                continue;
            };
            let Some(row) = store
                .state()
                .records
                .get(&vcp_store::contract::key(Collection::Artifact, id.as_str()))
            else {
                result.deficit("source_unavailable");
                continue;
            };
            if row.workspace != workspace.id {
                result.deficit("source_unavailable");
                continue;
            }
            let source: ArtifactDescriptor = row.decode()?;
            if source.spec.scope != manifest.spec.scope
                || !access.allows_task(&source.spec.scope.task)
            {
                result.deficit("source_unavailable");
                continue;
            }
            let mut associated = false;
            for file in files {
                check()?;
                if file["root"].as_str() != Some(root.as_str())
                    || file["sha256"] != source.sha256
                    || file["bytes"] != serde_json::json!(source.length)
                {
                    continue;
                }
                let Some(path) = file["path"].as_str() else {
                    result.deficit("source_manifest_invalid");
                    continue;
                };
                let binding = SourceBinding {
                    artifact: id.clone(),
                    manifest: manifest.spec.id.clone(),
                    root: root.clone(),
                    path: path.into(),
                    symbols: vec![],
                    fingerprint: task.fingerprint.clone(),
                };
                if !search_record::source_current(
                    store, &workspace, &binding, &source, &manifest, &bytes,
                )? {
                    result.deficit("source_manifest_invalid");
                    continue;
                }
                associated = true;
                let identity = vcp_protocol::canonical_bytes(&(
                    &binding.artifact,
                    &binding.root,
                    &binding.path,
                    &binding.fingerprint,
                ))?;
                if unique.contains(&identity) {
                    continue;
                }
                if result.bindings.len() == 64 {
                    result.deficit("source_binding_limit");
                    break 'manifests;
                }
                match read(store, access, &source, &mut total, check) {
                    Ok(Some(_)) => (),
                    Ok(None) => {
                        result.deficit("source_unavailable");
                        continue;
                    }
                    Err(Error::Conflict("source discovery read limit")) => {
                        result.deficit("source_read_limit");
                        break 'manifests;
                    }
                    Err(error) => return Err(error),
                }
                unique.insert(identity);
                result.bindings.push(binding);
            }
            if !associated {
                result.deficit("source_unavailable");
            }
        }
    }
    result.bindings.sort_by(|a, b| {
        (&a.artifact, &a.root, &a.path, &a.manifest).cmp(&(
            &b.artifact,
            &b.root,
            &b.path,
            &b.manifest,
        ))
    });
    check()?;
    Ok(result)
}
