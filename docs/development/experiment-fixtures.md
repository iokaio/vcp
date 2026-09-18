# Reproducible feasibility fixtures

P0-01 establishes public synthetic inputs and deterministic test helpers before
upstream experiments. Run `pwsh -NoProfile -File scripts/test.ps1 -Suite experiments`
or include it through `-Suite fast`. The suite checks the fixtures and their
oracles; it does not claim that VCP or a model completed these workloads.

## Source map and ownership

| Source | Purpose |
|---|---|
| [experiments.cjs](../../src/tests/support/experiments.cjs) | Manual clock, finite scripted provider, owned temporary roots and append/sync observation log |
| [workloads.json](../../src/tests/fixtures/workloads.json) | Versioned workloads, deterministic seed, visible source root and U01–U03 mappings |
| [checkout fixture](../../src/tests/fixtures/repositories/checkout/README.md) | Small original repository with documented domain contracts |
| [workload-graders.cjs](../../src/tests/support/workload-graders.cjs) | Independent truth set and quality calculations, outside the visible repository |
| [experiments.test.cjs](../../src/tests/contracts/experiments.test.cjs) | Fixture, helper and grader regression tests |

Every file is original Apache-2.0 material. There are no owner transcripts,
external services, real credentials, borrowed fixtures or model assets. The
declared seed identifies fixture versioning; it must never seed cryptographic
keys or nonces. The current fixed input does not need random generation.

The clock advances only when explicitly requested, orders equal-time callbacks
by registration, supports cancellation, and rejects runaway zero-delay loops.
The provider checks the complete expected request, consumes a finite response
script, rejects unexpected calls, and reports request mismatches without payload
contents. Neither helper uses a network service or wall-clock sleeping.

An owned fixture has a workspace directory and a sibling ownership marker.
Cleanup checks real paths and the ownership token before deleting only its
container. Keep failed experiment roots until evidence has been collected;
call cleanup explicitly after review. These checks prevent accidental deletion,
not hostile same-user filesystem races. The observation log lives outside the
workspace, records sequential barriers/effects/acknowledgements, and syncs each
record. A future product child must signal barriers to its supervisor; the
current log test does not prove a product crash or process isolation boundary.

## Workload protocols and independent answers

Only copy `model_visible_root` into a model-visible workspace. The workload
request is visible, while the grader and this document stay outside it.

| Workload | Known observation | Initial deterministic criterion |
|---|---|---|
| Analysis | Receipt composition depends on the shipping domain function; the domain has no I/O | Correct module set, dependency direction and I/O boundary; source bytes preserved |
| Review | At subtotal 50 the documented fee is 0 but the implementation returns 5 | One relevant boundary finding, correct path/trigger/expected/actual result, no unrelated finding |
| Generation | New `normalizeWorkspaceLabel` trims edges and preserves Unicode/case/internal whitespace; invalid inputs are rejected | Pass the independent valid/invalid input matrix, preserve existing files and keep domain code free of I/O |

`gradeAnalysis` checks structured findings; `gradeReview` reports true positives,
false positives, duplicate findings, precision and recall; `gradeLabel` checks
behavior through an injected function. Their tests include deliberately wrong
answers. The grader's reference implementation is harness-only. Run untrusted
generated code through the eventual execution broker, not in a supervisor;
these current tests invoke only trusted synthetic functions.

For this tiny fixture all assertions must pass. That is fixture qualification,
not the final product quality threshold. Before model comparisons, freeze task
revisions and declare separate minimum analysis coverage, review precision and
recall, behavioral generation pass rate, architecture-fit rubric, and source
preservation checks. P8-05 selects final owner thresholds before scoring. A
model score or fixed-model coding loop cannot waive required memory, routing,
extensions, delegation, pause or encrypted portability.

## Proposed experiment sizes and measurements

Start with these three tiny workloads for API and fixture correctness. Propose
10, 100 and 1,000 synthetic files for context/discovery scaling, and 100, 1,000
and 10,000 scoped records for local memory/search experiments. Generate and
version larger corpora with their owning experiments; these sizes are not
implemented fixtures or support limits. Keep at least two workspace identities
with colliding symbols in retrieval experiments.

Measure cold and warm runs separately, with five warm repetitions initially;
retain every attempt and report samples rather than an unsupported percentile
promise. Record native OS, CPU model/count, RAM, filesystem, compiler/runtime,
asset digests, peak resident memory/disk, startup/query/termination times and
network observations where relevant. No minimum machine or dollar cap is
selected by P0-01. Paid evaluations remain unavailable until explicit input
scope, model policy and a spend cap are authorized.

## Requirement and decision coverage

The [ledger](../plan/20-traceability.md) maps all 17 owner answers, 17 functional
requirements and 19 invariants to owning plans and observations. Initial
invariant cases use existing acceptance IDs below; they remain planned product
checks, not aliases for the passing harness tests.

| Invariants | Initial product case IDs |
|---|---|
| I-01, I-02 | E03, E06, E16, R03 |
| I-03 | E01, E10 |
| I-04, I-05 | E12, U07 |
| I-06 | E05, E15, U03 |
| I-07, I-08 | M01, M02, M05, M07 |
| I-09 | E17, U05 |
| I-10, I-11 | E08, E10, U06 |
| I-12 | E09, U03 |
| I-13 | M01, M02, M08 |
| I-14 | E11, E17, U09 |
| I-15 | M08, U04 |
| I-16 | M03, M04, U09 |
| I-17 | M05, U05 |
| I-18 | E15, U02, U06 |
| I-19 | M08, U04 |

All [20 ADRs](../adr/README.md) distinguish confirmed direction from proposed
mechanisms and unresolved engineering qualification. ADR-013 additionally fixes
the repository's copied-source convention. P0-01 does not accept a runtime,
database, encryption format, inference model or sandbox capability. P0-07 pins
and builds the candidates; P0-06 accepts the completed feasibility evidence.

ADR-020 was added during later P6 planning. Its presence does not extend the
historical P0-01 result into evidence for a decision evaluator.
