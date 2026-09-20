# 09 — Tantivy, local embeddings, DiskANN and retrieval generations

Status: P5-03 complete; P5-04 through P5-06 planned. Owns P5-03 through P5-06. Requires P5-02 and local-runtime qualification; P5-06 additionally needs P3-03 inspectors. Architecture section 13 supplies the retrieval contract.

## Design references and prerequisite records

Read [retrieval requirements](../architecture/vcp-what.md#13-tantivy-diskann-and-hybrid-retrieval),
[logical records](../architecture/memory-retrieval-design.md#logical-records-and-identity),
[publication/recovery](../architecture/memory-retrieval-design.md#generation-publication-and-recovery)
and [query authorization](../architecture/memory-retrieval-design.md#query-authorization-and-context-handoff).
[ADR-008](../adr/008-local-governed-memory.md) retains the open model/runtime and
index qualification decisions. P0-02 must provide the concrete Tantivy/DiskANN
versions, selected native provider, embedding assets/runtime, licenses and Windows
measurements; this segment does not invent library methods or compatible binaries.

Record task-specific entry evidence: P5-03 needs P0-02/P5-02; P5-04 also needs
P1-05 for optional model-assisted upstream ingestion accounting; P5-05 needs both
adapters and P1-04's publication transaction; P5-06 needs P3-03's common inspection
contract. Local embeddings themselves do not incur an OpenRouter request.

## Code organization

| Area | Proposed modules | Public responsibility |
|---|---|---|
| Tantivy adapter | `schema`, `tokenizer`, `writer`, `reader`, `generation` | Lexical candidate search and immutable index lifecycle |
| Embedding service within memory/search | `model_assets`, `runtime`, `chunker`, `batcher`, `vector_cache` | Local versioned vector production with resource limits |
| DiskANN adapter | `provider`, `id_map`, `build`, `query`, `filter`, `generation` | Vector candidates, persistent reopen and supported filter behavior |
| Memory orchestration | `index_intent`, `generation_manifest`, `publication`, `overlay`, `fusion`, `query`, `inspect` | Coherent views, current authorization and evidence-bearing results |

Keep the local inference runtime behind an interface so model upgrades need no engine rewrite. Qualified real libraries are required for release; fake vectors test control paths only.

## P5-03 — Lexical retrieval

Implemented in `vcp-memory` with the already qualified Tantivy 0.22.1.
[Native qualification](../development/p5-lexical.md) records canonical inventory,
exact scope/tokenization, cancellation/reopen/replacement tests and separate
lexical fixture quality/timings. Canonical activation remains P5-05 work.

Define stable document/chunk IDs and fields for workspace/root scope, record/source version, paths/symbols, claim type/status, sequence and text. Avoid using internal index document ordinals as canonical identity.

Implement code-aware tokenization and exact path/symbol lookup alongside general text search. Record tokenizer/schema versions in every generation. Build in a private directory, expose queryable readers after publication and retain old readers until references are released.

Tests compare exact symbol/path cases, punctuation/case, short identifiers, scoped duplicates, superseded versions, deletion and reopening. Measure cold and warm queries on the same fixture and report lexical precision/recall separately from fused results.

### Lexical adapter increments

First define a backend-independent `SearchRecord` carrying canonical ID/version,
kind, source/span, scope, applicability, searchable text reference and sequence.
The conversion from claims/chunks to this record must produce an inventory with
explicit exclusions; do not derive eligibility from whatever documents happened
to reach the index writer. Use exact fields for workspace/root/ID/path/symbol and
separate analyzed fields for prose and split code tokens. Store source content in
the artifact layer so index document addresses cannot become evidence IDs.

Add a versioned tokenizer fixture table for `snake_case`, camel case, dotted and
qualified names, punctuation-only tokens, Unicode paths, case distinctions and
very short identifiers. Preserve full forms alongside split terms. Scope/root
matching uses canonical workspace identity and qualified path rules, not a
lowercased display string. Record field boosts, stop-word behavior and exact-match
priority as configuration under test; do not tune on the final held-out queries.

Implement private writer creation, bounded document batches, cancellation,
finalize/reopen validation and reader pin release. Expose candidate IDs/ranks with
bounded result limits. If the library returns stored snippets, prevent those
bytes from reaching callers or logs before the canonical authorization step.
Adapter tests must cover deletes/merges and reader reuse while a newer generation
publishes; internal ordinal changes may not alter stable source references.

## P5-04 — Local vectors and DiskANN

1. Provision the pinned embedding artifact with digest, dimensions, normalization, token/chunk limits, license and runtime version. Explain downloads/setup; missing assets are not-ready, never a remote endpoint.
2. Implement bounded CPU batching/cancellation and vector caching keyed by source/chunk and embedding specification. Track resident memory, mapped pages, temporary disk and build peaks independently.
3. Implement the qualified DiskANN storage/provider boundary, stable vector-to-chunk mapping, rebuild/reopen and schema checks. Reject dimension/model mismatches before publishing.
4. Define how narrow scope filters obtain enough candidates without exposing other scopes. Use bounded overfetch or a qualified filtered-search method; an exact small-set fallback must have explicit resource limits and a degraded-mode label.

Use real local embeddings on a CPU-only Windows machine with embedding-network access blocked. Compare ANN neighbors with exhaustive distance search over the same small vector fixture. Repeat close/reopen, cancellation, missing/corrupt vector file and model-version change. Record recall and resource bounds; a successful API return is not retrieval quality evidence.

### Vector preparation and provider boundary

Persist one immutable embedding specification containing artifact digest,
preprocessing/chunker revision, dimension, normalization, metric and runtime
compatibility. Define chunk spans against retained source bytes or an explicit
normalization map. Cache lookup includes the complete specification and content
identity and still enforces source scope. A model upgrade cannot reuse a cache
entry merely because the displayed model name is unchanged.

Implement asset verification before runtime initialization; expose missing,
digest-mismatched, incompatible and ready states. Setup/download is an explicit
provisioning action with provenance/license evidence. Inference receives only
local buffers and cancellation/resource controls. Validate vector dimensions and
finite values before persistence; both query and source vectors must use the
same qualified distance convention.

The DiskANN adapter owns stable vector-to-chunk mapping, graph/vector/provider
storage contracts and generation descriptors. Prove which files must survive a
reopen for the selected provider. Persist source vectors when required for
portable rebuild, and record graph parameters/quantization separately from the
embedding specification. Library/node IDs remain private. Parameter changes that
invalidate compatibility build new components instead of mutating a live reader.

For selective scopes, predeclare a bounded candidate/overfetch policy and measure
the authorized truth set. If filtered ANN cannot produce enough candidates,
report reduced recall or use the qualified bounded exact-subset fallback; never
remove a scope predicate. Test an authorized set with one relevant record among
many tempting records in another workspace and verify both returned IDs and
absence of forbidden text in diagnostics.

## P5-05 — Coherent publication and recovery

Implement the state progression `pending intent -> building private components -> validated -> manifest durable -> active pointer published -> old generation eligible for cleanup` with canonical watermark and schema/embedding versions. Freeze a compatible lexical/vector view; do not pair two mutable indexes solely because their timestamps look recent.

Canonical acceptance precedes eventual index visibility. Record pending intents durably and expose lag. A bounded recent-change overlay may supply new content, but every result still passes canonical scope/status/tombstone checks. Garbage collection cannot delete generations pinned by readers, snapshots or recovery.

Inject kills before/after each index component write, manifest finalization and active-pointer switch. On reopen, select only complete compatible generations, replay/rebuild pending work and report lag. Loss of derived indexes is recoverable from retained inputs; corruption of authoritative records is a different failure.

### Publication transaction and garbage collection

Capture a canonical snapshot N and inventory of eligible records/intents. Build
both components against that inventory and validate stable mapping coverage,
specifications, deletion metadata and actual reopen. A searchable watermark is
the highest fully covered sequence under the declared visibility contract, not
the largest successfully indexed sequence while earlier work remains missing.
Keep any records without vectors visible as an explicit coverage deficit.

After component finalization reaches the tested filesystem durability boundary,
commit the generation manifest, active-generation reference and covered-intent
acknowledgements in a revision-checked canonical transaction. Treat an optional
filesystem pointer as reconstructable convenience state. A competing publisher
that loses the expected-generation comparison must replan; it cannot overwrite
the newer manifest. A lost reply consults the publication receipt.

Reopen verifies only canonically published manifests; incomplete private builds
are resumable work or owned orphans. If a published component is corrupt, retain
canonical records, use a valid compatible retained generation if available and
report its lag, or enter explicit rebuild state. Do not skip corruption and claim
the original generation reopened. Garbage collection must check reader pins,
snapshot pins, historical policy and retention jobs before deleting owned paths.

Overlay publication uses a bounded canonical range above N and its own version.
Record record/byte/time caps and what kinds of retrieval it can supply. When caps
or missing vectors prevent the requested freshness, return the available
watermarks and an unsatisfied freshness reason. Overlay overflow cannot become
unbounded synchronous scanning or a false up-to-date semantic result.

## P5-06 — Hybrid query and inspection

1. Resolve current workspace/scope and requested historical view, capture a compatible generation, then produce lexical/vector candidates from the same declared visibility contract.
2. Recheck current canonical authorization, source applicability and deletion status. Filter revoked or purged content even while indexes lag.
3. Fuse ranked candidates using a recorded, versioned rule such as reciprocal rank fusion; deduplicate claim/source versions and bound total context bytes/tokens. Any reranking/embedding inference remains local under the architecture contract.
4. Return passages with source/claim/version IDs, evidence refs, component scores/ranks, exclusion reasons, freshness and generation watermarks. The inspector can explain why a result was included or why recall degraded.

### Query transaction and explanation contract

Implement a typed request with authorized workspace/root/path scope, current or
historical view, optional minimum sequence, deadline and result/token limits.
Pin compatible components and overlay, generate lexical/vector candidates, batch
check canonical eligibility, then retrieve permitted payloads. Check current
authority/deletion revisions again before returning results. A requested old
canonical view without a matching generation returns a declared canonical-only
view or controlled rebuild requirement; it cannot silently search today's index
and call that historical vector search.

Version the fusion weights/rank constant and deterministic tie-breaker. Deduplicate
overlapping spans and repeated claim versions while preserving evidence links.
Token trimming must retain inference/dispute labels and source references.
Inspection exposes why eligible results were selected, available watermarks,
component ranks and degraded flags without revealing names/text from unauthorized
excluded records. Denied-result diagnostics may require stronger inspect scope
than an ordinary search caller possesses.

Return authority revision, deletion epoch and source/version identities with the
selected context. The controller's send fence revalidates them immediately before
model dispatch, as described in the
[context design](../architecture/context-provider-design.md). Prune/revocation
winning that ordering blocks stale prepared sends; a request already dispatched
is recorded honestly and cannot be retracted by deleting its local context.

## Test matrix

| Test group | Required oracle/result |
|---|---|
| M03/U09 performance | Declared corpus/hardware, build/start/query distributions, local-model identity and no remote embedding traffic |
| M04 quality | Exact symbols, semantic intent and narrow-scope truth set; ANN versus exhaustive search and lexical/vector/fused metrics |
| M02/E14 recovery | Every publication crash point restores a valid reader or explicit rebuilding state |
| M05 access/deletion | Supersede/delete/revoke then query during lag; forbidden text never reaches a returned context passage |
| M06 upgrade | New tokenizer/model/dimensions/library produces a new compatible generation or explicit rejection |
| M07 resource failure | Disk full, missing artifact, memory cap and locked files preserve canonical records and report failed work |

Proposed fixture locations: `src/tests/fixtures/search/`, `src/tests/contracts/search/`, `src/tests/recovery/index_publication/` and `src/evals/fixtures/retrieval/`. Run `search`, `memory`, `recovery` and actual local-model integration before claiming the segment complete.

For each quality run save the corpus manifest, held-out query/relevance IDs,
embedding/graph/tokenizer/fusion specifications, filters and exact-oracle method.
Report lexical/vector/fused metrics separately, cold/warm distributions and build
peak resources. Keep failed/cancelled attempts in the report. Cross-check query
results against the canonical fixture oracle, not an expected set generated by
the index under test. The named suites remain planned until the real runner and
registered test targets exist; missing assets/native prerequisites are not-run.

Done when local hybrid retrieval is useful on a declared truth set, every returned passage has current authority/provenance, and crashes/upgrades/deletion never expose an incoherent or unauthorized generation.
