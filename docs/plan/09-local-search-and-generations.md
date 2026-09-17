# 09 — Tantivy, local embeddings, DiskANN and retrieval generations

Status: planned. Owns P5-03 through P5-06. Requires P5-02 and local-runtime qualification; P5-06 additionally needs P3-03 inspectors. Architecture section 13 supplies the retrieval contract.

## Code organization

| Area | Proposed modules | Public responsibility |
|---|---|---|
| Tantivy adapter | `schema`, `tokenizer`, `writer`, `reader`, `generation` | Lexical candidate search and immutable index lifecycle |
| Embedding service within memory/search | `model_assets`, `runtime`, `chunker`, `batcher`, `vector_cache` | Local versioned vector production with resource limits |
| DiskANN adapter | `provider`, `id_map`, `build`, `query`, `filter`, `generation` | Vector candidates, persistent reopen and supported filter behavior |
| Memory orchestration | `index_intent`, `generation_manifest`, `publication`, `overlay`, `fusion`, `query`, `inspect` | Coherent views, current authorization and evidence-bearing results |

Keep the local inference runtime behind an interface so model upgrades need no engine rewrite. Qualified real libraries are required for release; fake vectors test control paths only.

## P5-03 — Lexical retrieval

Define stable document/chunk IDs and fields for workspace/root scope, record/source version, paths/symbols, claim type/status, sequence and text. Avoid using internal index document ordinals as canonical identity.

Implement code-aware tokenization and exact path/symbol lookup alongside general text search. Record tokenizer/schema versions in every generation. Build in a private directory, expose queryable readers after publication and retain old readers until references are released.

Tests compare exact symbol/path cases, punctuation/case, short identifiers, scoped duplicates, superseded versions, deletion and reopening. Measure cold and warm queries on the same fixture and report lexical precision/recall separately from fused results.

## P5-04 — Local vectors and DiskANN

1. Provision the pinned embedding artifact with digest, dimensions, normalization, token/chunk limits, license and runtime version. Explain downloads/setup; missing assets are not-ready, never a remote endpoint.
2. Implement bounded CPU batching/cancellation and vector caching keyed by source/chunk and embedding specification. Track resident memory, mapped pages, temporary disk and build peaks independently.
3. Implement the qualified DiskANN storage/provider boundary, stable vector-to-chunk mapping, rebuild/reopen and schema checks. Reject dimension/model mismatches before publishing.
4. Define how narrow scope filters obtain enough candidates without exposing other scopes. Use bounded overfetch or a qualified filtered-search method; an exact small-set fallback must have explicit resource limits and a degraded-mode label.

Use real local embeddings on a CPU-only Windows machine with embedding-network access blocked. Compare ANN neighbors with exhaustive distance search over the same small vector fixture. Repeat close/reopen, cancellation, missing/corrupt vector file and model-version change. Record recall and resource bounds; a successful API return is not retrieval quality evidence.

## P5-05 — Coherent publication and recovery

Implement the state progression `pending intent -> building private components -> validated -> manifest durable -> active pointer published -> old generation eligible for cleanup` with canonical watermark and schema/embedding versions. Freeze a compatible lexical/vector view; do not pair two mutable indexes solely because their timestamps look recent.

Canonical acceptance precedes eventual index visibility. Record pending intents durably and expose lag. A bounded recent-change overlay may supply new content, but every result still passes canonical scope/status/tombstone checks. Garbage collection cannot delete generations pinned by readers, snapshots or recovery.

Inject kills before/after each index component write, manifest finalization and active-pointer switch. On reopen, select only complete compatible generations, replay/rebuild pending work and report lag. Loss of derived indexes is recoverable from retained inputs; corruption of authoritative records is a different failure.

## P5-06 — Hybrid query and inspection

1. Resolve current workspace/scope and requested historical view, capture a compatible generation, then produce lexical/vector candidates from the same declared visibility contract.
2. Recheck current canonical authorization, source applicability and deletion status. Filter revoked or purged content even while indexes lag.
3. Fuse ranked candidates using a recorded, versioned rule such as reciprocal rank fusion; deduplicate claim/source versions and bound total context bytes/tokens. Any reranking/embedding inference remains local under the architecture contract.
4. Return passages with source/claim/version IDs, evidence refs, component scores/ranks, exclusion reasons, freshness and generation watermarks. The inspector can explain why a result was included or why recall degraded.

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

Done when local hybrid retrieval is useful on a declared truth set, every returned passage has current authority/provenance, and crashes/upgrades/deletion never expose an incoherent or unauthorized generation.
