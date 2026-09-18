# ADR-008 — Local governed memory and retrieval

Status: confirmed product direction; bounded P0 integration candidate qualified.
Decision gate: P0-02/07 feasibility complete; P5-01 through P5-08 production integration and acceptance remain open.

## Context and authority

This record expands the [architecture contract](../architecture/vcp-what.md#11-governed-memory) and its [ADR register](../architecture/vcp-what.md#221-adr-register). The architecture remains the product authority. Proposed mechanisms below must be qualified at the named gate before support is advertised.

## Confirmed direction

Adapt selected Munarium governance locally with immutable claim versions, evidence, disputes and supersession. Automatically process all supported claim classes with evidence labels. Tantivy, DiskANN and embeddings run locally.

## Implementation proposal

Commit proposal resolution, accepted version and indexing intent atomically. Stable source/chunk/version IDs link claims to observed artifacts. Publish compatible lexical/vector generations at a common canonical watermark. Recheck current access, tombstones and applicability before exposing passage text, including historical views.

Detailed contracts and failure ordering are in the [supporting design](../architecture/memory-retrieval-design.md). The [task ledger](../plan/20-traceability.md) preserves exact implementation dependencies; referencing a later integration test does not add a new task dependency.

## Alternatives and unresolved choices

Qualify Munarium seams and exact local model/runtime and DiskANN provider at P0. Start with coherent immutable generations and a bounded overlay; incremental improvements must preserve the same snapshot contract. Model-assisted claim extraction uses admitted OpenRouter calls, separately from local embeddings.

## Qualification evidence

[Native Munarium baseline results](../evaluations/p0-07-munarium-datastore.md)
establish Windows API/artifact feasibility for Tantivy 0.22.1 and DiskANN 0.56.0
in an unmodified library candidate, with 200 tests passed and one benchmark
ignored. This does not yet select a VCP embedding runtime, memory backend or
production retrieval envelope.

The [local CPU embedding result](../evaluations/p0-07-local-embeddings.md) adds
verified MiniLM assets and a Candle adapter with independent PyTorch reference
vectors. That earlier gate alone did not qualify network denial, corpus/index
reopening or resource/recall; the subsequent evidence below covers those gates.

The [corpus prototype](../development/local-memory-spike.md) subsequently joins
real embeddings with persisted lexical/vector indexes. Its
[governance adapter](../development/local-governance-spike.md) now exercises
Munarium gates, retained disputed evidence and historical supersession through
separate workspace-scoped in-memory backends. A blocked correction records its
proposed predecessor as evidence without attaching an effective replacement
edge. This preserves accepted visibility under the retained resolver's semantics.
Volatile replay does not qualify durable canonical transactions or recovery;
[offline CPU qualification](../evaluations/p0-02-offline-embeddings.md) adds
observed Windows network denial and real missing/corrupt asset failures.
The [declared resource experiment](../evaluations/p0-02-local-resources.md)
passes 100/1,000/10,000-record build and two fresh query processes, 324 exact-oracle
query rows and native resident/private/mapped plus disk observations. This does
not qualify full index containment or select the product sandbox.

### Bounded engineering selection

Use the pinned Munarium governance interfaces and Tantivy 0.22.1/DiskANN 0.56.0
with the verified MiniLM/Candle 0.11/Tokenizers 0.22.2 CPU adapter as the P0
integration candidate. The 384-dimensional normalized vectors and cosine metric,
asset digest, preprocessing and library identity are part of index compatibility;
changes require a new generation and qualification, not reuse of stale vectors.
Source pins, manifests and maintenance procedures are recorded in the
[Munarium source map](../development/munarium-source.md) and
[embedding source map](../development/local-embeddings.md).

Retain scoped adapters around governance rather than exposing the volatile
backend as a product store. At 10,000 records replay takes 252–259 seconds and
lookup/filtering scans the whole scoped candidate set. P0-04/P1/P5 must replace
volatile replay with durable, revision-bound projections and implement bounded
candidate refill while preserving current authority and historical evidence.
The fixture's flags are an independent oracle, never the source of authorization.

The experiment needs about 91.6 MB of model assets and 25.3 MB of retained index
data at 10,000 synthetic records; observed native peak resident memory is about
203 MB. These overlapping and partial component costs are not an installed
package or minimum hardware promise. Production durability, concurrent deletion,
generation migration, broader recall and performance acceptance remain P5 gates.

M01–M07/E13/E14/E20/U09 cover contradictory claims, origin replay, narrow scopes, exact-vector oracle, real CPU embeddings, reopen, deletion during lag and model changes. Fake vectors cannot qualify recall or offline compute.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

Governance acceptance does not certify a claim true; preserve inference and evidence status separately. Canonical durability can precede search visibility. Retaining vectors aids restore but increases snapshot size; asset licenses and resource measurements govern provisioning.

Update this record with the selected mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when its decision gate runs. Reopen an engineering choice when those assumptions fail; changes to confirmed product scope need an explicit owner decision.
