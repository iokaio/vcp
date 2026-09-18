# ADR-008 — Local governed memory and retrieval

Status: confirmed product direction recorded; engineering design proposed and qualification pending.
Decision gate: P0-02/07, P5-01 through P5-08. Bounded qualification evidence is recorded below; production integration and final engineering selection remain open.

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
vectors. It does not complete OS network denial, corpus/index reopening or the
P0-02 resource/recall experiment, so the broader decision gate remains open.

The [corpus prototype](../development/local-memory-spike.md) subsequently joins
real embeddings with persisted lexical/vector indexes. Its
[governance adapter](../development/local-governance-spike.md) now exercises
Munarium gates, retained disputed evidence and historical supersession through
separate workspace-scoped in-memory backends. A blocked correction records its
proposed predecessor as evidence without attaching an effective replacement
edge. This preserves accepted visibility under the retained resolver's semantics.
Volatile replay does not qualify durable canonical transactions or recovery;
network denial and broader resource measurements remain P0-02 gates.

M01–M07/E13/E14/E20/U09 cover contradictory claims, origin replay, narrow scopes, exact-vector oracle, real CPU embeddings, reopen, deletion during lag and model changes. Fake vectors cannot qualify recall or offline compute.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

Governance acceptance does not certify a claim true; preserve inference and evidence status separately. Canonical durability can precede search visibility. Retaining vectors aids restore but increases snapshot size; asset licenses and resource measurements govern provisioning.

Update this record with the selected mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when its decision gate runs. Reopen an engineering choice when those assumptions fail; changes to confirmed product scope need an explicit owner decision.
