# Local corpus and index prototype

P0-02's `vcp-memory-spike` joins the verified CPU embedding helper to the selected
Munarium datastore in the existing Cargo workspace. It is a qualification
executable using a fixed public synthetic corpus. It is not VCP's production
memory service or an alternative controller/store.

[Executed native results](../evaluations/p0-02-local-corpus.md) record the exact
inputs, commands, observations and remaining P0-02 gates.

The fixture contains 24 documents in two workspaces and seven queries. Both
workspaces contain `pause_session` but describe different behavior. Atlas also
retains an obsolete pause design beside its current revision. Required lexical
and semantic result IDs were specified before running either index. The corpus
and expected results are committed together with their provenance.

## Run and inspect evidence

Install the [native prerequisites](codex-source.md#setup-and-ordinary-build) and
explicitly acquire the [pinned local model](local-embeddings.md). From the
repository root:

```powershell
pwsh -NoProfile -File scripts/test-local-memory.ps1 -AssetsRoot <external-model-directory>
```

This command verifies assets and both imported sources, uses Rust 1.98.0 and MSVC,
tests and builds the qualification binary, and checks its exact native dependency
closure. No acquisition occurs inside compilation or inference. Missing tools or
model files are `not_run`; failed assertions are failures. Output uses a new UUID
directory under ignored `artifacts/local-memory/`. `-TargetRoot` can reuse the
embedding build directory; inputs remain locked.

The Node observer then invokes the native executable in separate, sequential
processes:

1. Build both workspace artifacts from verified CPU vectors and publish a
   content-bound receipt after their manifests.
2. Reopen from the receipt in a fresh process, verify all stored text/provenance,
   and execute the seven queries.
3. Repeat reopen in another process and compare meaningful result IDs and recall.
4. Supply a wrong receipt digest and require the intended identity rejection.
5. Copy only this run's index to a distinct corruption fixture, alter a manifest
   byte, and require the intended integrity rejection.

The observer retains each command, outcome, bounded stdout/stderr and hashes.
It validates the result independently against the fixture, so an executable
printing `pass` with missing queries, stale content, a foreign workspace, changed
model identity or missing relevance does not pass. A timeout, launch failure or
truncated capture cannot count as an expected rejection. The existing harness
owns cancellation and terminates its launched process tree.

## Construction and retrieval contract

Each workspace has a distinct Munarium `ShardWriter` artifact. The original
`BuildSpec` carries workspace identity, source IDs/digests, literal-text extraction,
the embedding asset-specification digest, 384 dimensions, L2 normalization and
cosine metric. Actual Tantivy analyzer version, stop-term digest and library
revision are recorded. The physical plan explicitly selects DiskANN 0.56.0 and
records all graph parameters even for this small corpus; it does not silently
take an exact-search fallback.

The retained writer emits text/provenance, lexical files and serialized DiskANN
vectors/adjacency. The retained reader verifies manifest identity and component
digests before reopening. The prototype additionally compares the full build
specification against its compiled fixture and checks the hydrated records.
An externally supplied receipt digest binds the two phases; the receipt is a
qualification input, not a production authentication mechanism.

Queries analyze text through the same lexical analyzer used by the artifact.
They embed semantic intent locally and collect lexical and vector results
separately. The [governance adapter](local-governance-spike.md) replays the fixture
through retained Munarium gates and scoped in-memory ledgers. Its resolved
workspace/current-version view supplies filtering before the final result limit.
The fixture's `current` flags remain an independent oracle. Index metadata never grants access.
The small experiment fetches the bounded candidate set before filtering, avoiding
result starvation by obsolete entries. Production backfill and efficient filtering
remain P5 work.

A separate three-result ANN query is compared with an independent exhaustive
f64 cosine calculation over freshly computed fixture vectors. The tiny fixture
requires recall@3 of 1; this is an API and persistence check, not a general recall
claim. Required semantic results remain a separate assertion from ANN agreement:
perfect vector recall alone cannot prove useful text relevance.

## What remains open

Measurements distinguish model load, corpus inference, index construction,
artifact reopen, query inference and lookup. They are observations of one small
fixture, not hardware minimums or a production capacity estimate. The subsequent
[resource gate](local-memory-resources.md) measures three declared sizes, native
memory counters, sampled disk growth and repeated queries. The [offline gate](offline-embeddings.md)
qualifies contained CPU inference and missing/corrupt assets. A durable canonical
store adapter remains P0-04/P1/P5 integration. This prototype's volatile governance replay cannot qualify live
deletion, concurrency, crash durability or policy revision races.

Follow [ADR-008](../adr/008-local-governed-memory.md), the
[memory/retrieval design](../architecture/memory-retrieval-design.md) and
[P0-02](../plan/01-upstream-feasibility.md#p0-02--local-munarium-and-search-spike).
The [selected Munarium source](munarium-source.md) remains unchanged; the new
original executable calls its public APIs. The separate workspace-registration
patch and inventory keep ordinary builds and explicit reconstruction aligned.
