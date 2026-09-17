# Qualification, evaluation and release design

Status: proposed implementation design for P0 harness work, P5-08, P6-04 and P8. It expands [architecture testing](vcp-what.md#20-testing-evaluation-and-performance), [ADR-018](../adr/018-release-acceptance.md), [plan 15](../plan/15-integration-and-release.md) and [plan 16](../plan/16-test-fixtures-and-acceptance.md). The [delivery harness](../development/delivery-harness.md) implements the initial registry, recorder and deterministic checks. No VCP package or product qualification result exists yet.

## Harness architecture

Use a small orchestration layer under `scripts/` to resolve the checkout, validate a requested suite/case/backend, identify prerequisites and invoke registered test targets. Reusable fixture builders, graders and oracles live under `src/tests/` and `src/evals/`, attached to the selected workspace. The test code must not depend on a developer's current shell directory.

The proposed registry maps each suite to concrete commands, supported backend variants, required host/assets/network class and produced evidence. Unknown suite/case/backend fails before creating a run. `fast` never pulls model assets or spends money; live evaluation has explicit cap, role/model policy, concurrency and deadline inputs.

A run moves through prepared, running and a terminal outcome. Preparation failures still produce an outcome if the evidence root is writable. If the recorder fails, print an actionable failure and propagate a failing process status; do not return success for unrecorded required tests.

## Result and attempt records

These are field contracts, not an implemented JSON schema:

| Record | Required fields |
|---|---|
| Run | Run ID, task/test IDs, suite version, source commit plus dirty identity, command, fixture hash/seed, start/end, outcome |
| Environment | Native host/OS/architecture/filesystem/terminal, compiler/runtime, CPU/RAM, prerequisite state |
| Configuration | Backend, canonical/index/schema versions, model asset and embedding spec, catalog/policy revisions, limits |
| Attempt | Attempt ID and predecessor, case/variant/repetition, executed command, exit status, fault barrier and timing |
| Observation | Independent oracle identity, expected property, actual value, artifact reference and digest |
| Spend | Root evaluation cap, admitted/settled/unresolved totals, all model/support/child attempts and attribution |
| Limitation | Missing prerequisites, untested environments, redaction/gaps and claims the result cannot establish |
| Summary | Pass/fail/not-run per required case and aggregate counts with original failure denominators |

Use explicit `not_run` for missing assets or host, and `fail` for failed assertions/setup that should have worked. A user-cancelled campaign retains completed attempts and remaining not-run cases. A required case with no result prevents an overall passing gate.

Raw artifacts are local/private as appropriate. A reviewed public summary is a separate output with a field allowlist and synthetic/public fixtures. Do not publish private owner paths, transcript contents, keys or provider headers. Sanitization failures block public report preparation without deleting the local evidence.

## Fixture construction and isolation

Create a fixture under a unique harness-owned root and store its ownership marker outside product-controlled state. Record the intended base and expected effects independently. For dirty repositories, preserve staged, unstaged and untracked sets separately; a final tree hash alone cannot detect an accidental change to staging.

The fixture manifest declares required resources, preconditions, action schedule and assertions. Reject unknown assertions rather than silently skipping them. A deterministic seed controls synthetic data and scheduling only; cryptographic identities/nonces use the selected library's real randomness.

Use two workspaces with colliding paths/symbol names for scope tests. Maintain an independent truth set for retrieval and seeded defects that is never included in model-visible context. Private owner fixture references resolve only in the authorized local environment.

Cleanup validates resolved absolute paths against the recorded fixture root. A test failure must not delete the data root needed to diagnose recovery. Preserve evidence first; clean up only resources the harness created.

## Fault supervisor and acknowledgement oracle

Place explicit test barriers before and after durable and external-effect boundaries. The product child signals its barrier identity and waits; a separate supervisor records that signal durably, kills the child for the selected case, and launches a fresh process on the same state. A timeout is an inconclusive/failed attempt, not proof that the intended barrier was reached.

Maintain independent ledgers of:

- Commands whose acknowledgement the supervisor actually received.
- Effects observed by marker files, a controlled remote endpoint or process-tree observation.
- Bytes observed entering the vault, including partially copied objects.
- Canonical/export state and query eligibility after reopen.

For acknowledged commits, all declared records and finalized references must survive within the tested envelope. If the supervisor positively observes the barrier after the qualified durable commit but before reply, reopening must return the original receipt, result and sequence; retry must create no new mutation or replacement receipt. If termination occurs before or within an uncertain commit boundary, recover the original committed result if present, otherwise permit one idempotent commit. Record which boundary was actually observed. For dispatch-without-outcome, preserve the uncertain effect; an absent success event is not a retry instruction.

Run returned-error injections for breadth and real process termination for durability/ownership claims. Process kills do not establish hardware power-loss, device-cache or media-corruption guarantees. State the exact envelope in every report.

## Integrated pause experiment

U06 contains an explicit same-process scenario:

1. Start a root model request and a child process with a non-idempotent marker at a known barrier.
2. Submit `/pause` while the terminal stays open. Observe the committed stop boundary and verify no new root/child model or tool dispatch occurs after it.
3. Allow already-started receipt reconciliation; preserve partial artifacts and unresolved charges. Display pausing/paused and residual unknown effects honestly.
4. Inspect history, cost, children and full evidence without resuming. Submit another pause and ensure no duplicate transition/effect.
5. Record new steering while paused; it must not trigger a request. Change a relevant file/policy and submit `/resume`.
6. Revalidate scope/revisions and reconcile unknown effects before new work. Independently paused/cancelled children remain so until deliberately selected.

Repeat with waiting input, streaming output, capture failure and budget exhaustion. Then run actual console close, lost owning pipe and forced kill as separate variants. Inspection availability and owner-loss behavior are different assertions.

## Quality and performance experiments

Freeze training/tuning versus held-out fixture membership before choosing routing or retrieval thresholds. Use the same task revisions, tools, permission ceiling, evaluation cap and context limits across comparative strategies where feasible. Record unavoidable differences as experimental variables.

For memory compare no semantic memory, versioned Markdown recall and governed memory. For routing compare fixed economical, fixed stronger and routed strategies. Record actual served model/provider, cache conditions, repeated attempts, support roles and child integration costs. A failed task remains in the denominator even if a later retry succeeds.

Compute retrieval precision/recall against the authorized relevant set; evaluate lexical, vector and fused retrieval separately. Compare ANN to exact distance search on the same vectors. Report narrow-scope recall and forbidden-content count independently; access violations cannot be traded for recall.

Measure acknowledgement, cancellation admission, worker termination, cold startup, local query embedding, index query and total task latency separately. Report sample count and distribution; a p95 from a tiny sample is descriptive, not a stable support promise. Track resident memory, mapped working set, peak build disk and retained snapshot growth without conflating configured cache size with total resource use.

Quality thresholds and hardware/corpus envelopes are owner/engineering choices recorded before final runs. Numerical architecture targets remain provisional until measurements support them.

## Package and compatibility manifest

P8-04 produces an inventory tied to the actual distribution bytes: artifact/build identity, target host, CLI/worker versions, runtime dependencies, canonical/config/index compatibility, built-in skill catalog, upstream selection and patch identities, notice files, local model provisioning digests and applicable signing/checksum information.

Test from a clean profile with no developer cache or checkout dependencies. Verify startup, missing model diagnosis, data-root selection, explicit pause/resume, upgrade, failed migration, rollback refusal when incompatible, uninstall data preservation and encrypted second-machine recovery. A signed package still needs behavior tests; a build artifact is not an installed package test.

Models may be provisioned separately only with an explicit digest/license/setup contract. Startup reports download/indexing work and resource implications. Do not silently fetch a large model to make an install check pass.

## Release evidence and operations handoff

The proposed release matrix is keyed by requirement/invariant, task, acceptance case, required variant and evidence ID. Check dependency closure and freshness before the owner review. Missing or stale evidence remains a blocker; an averaged score cannot waive correctness invariants.

Prepare these concrete documents with measured information at P8:

| Output | Required content |
|---|---|
| Support matrix | Tested Windows/CPU/filesystem/terminal and local inference limits, unsupported capabilities |
| Installation | Exact artifact, verification, setup, model provisioning and first-run commands |
| Operations | Data roots, inspection, in-app pause/resume, recovery, migration, backups and key recovery |
| Known limitations | Unknown-effect handling, measured scale, retained cloud copies and untested environments |
| Release scorecard | U01–U09, FR/I coverage, all attempts, quality/cost/latency and owner review record |
| Source/notice inventory | Actual shipped components/assets, immutable origins, patches and notices |

P8-05 reviews the final package and evidence; authorization to publish is a separate action. Changed package bytes, relevant source, policy, fixtures or dependencies invalidate affected checks and trigger a scoped rerun. Preserve prior reports rather than relabelling them as current.
