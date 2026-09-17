# 05 — OpenRouter adapter, coding loop and completion

Status: planned. Owns P2-02, P2-05 and P2-06. P2-02 needs P1-05 and P0-09. The loop also needs context, policy and tools from segments 04/06. Architecture sections 4, 7–9 and 14 define behavior.

## Code organization and interfaces

Under `vcp-models`, implement `request`, `content`, `capabilities`, `stream`, `usage`, `errors`, `catalog`, `openrouter` and `fake`. Under the retained engine, isolate `turn_driver`, `dispatch`, `steering`, `verification` and `completion`. Provider-native types stay inside the adapter.

Proposed normalized contracts:

- `ModelRequest`: attempt/task IDs, role/model/provider constraints, context manifest, bounded output/effort settings, tool schemas and reservation receipt.
- `ModelEvent`: content delta, partial/complete tool call, usage, served-model/provider attribution, terminal result or classified error.
- `ModelResult`: normalized content, finish reason, usage certainty, provider request identity and raw-response artifact reference.
- `VerificationResult`: check definition, input fingerprint, output artifact, exit/result state, timing and explicit skipped reason.

The adapter accepts only an admitted attempt; a provider SDK helper cannot create its own unaccounted model call. Revalidate current OpenRouter API/model capabilities during P2-02 implementation using primary documentation and captured fixtures; the research workbook is not a frozen API contract.

## P2-02 — Provider path

1. Serialize normalized messages/content/tools from the exact context manifest. Record supported parameter conversions and reject unknown required capabilities before dispatch.
2. Implement incremental stream parsing with bounded buffering, partial JSON/tool fragments, multiple tool IDs and cancellation. Keep arrival order separate from execution eligibility.
3. Normalize served model/provider, usage, errors and termination. Do not replace an unknown value with a requested-model assumption. Persist enough raw evidence to audit normalization without credentials.
4. Implement bounded retries/backoff/deadlines with a new attempt/reservation per billable retry. Pre-send failures and ambiguous post-send failures have distinct accounting transitions. Honor pinned-model/fallback and provider-data restrictions.
5. Load dated catalog snapshots and tested compatibility records. Stale/missing price or capability states are visible; refresh cannot mutate an active attempt's meaning.

Tests in `src/tests/contracts/provider/`: malformed/truncated stream, interleaved tool fragments, duplicate terminal event, rate limit, timeout before and after send, cancellation during usage delivery, unsupported role/tool schema, fallback to smaller context and missing cost fields. Assert no tool dispatch from an incomplete call and no request without a durable reservation. Use scripted local transport before a separately budgeted live smoke test. E11/E12/R05 apply.

## P2-05 — Retained Codex loop with VCP boundaries

Implement in observable increments:

1. Read-only loop: assemble versioned context, choose a fixed qualified model for the internal scaffold, reserve, stream, capture and report evidence.
2. Prepared tool loop: validate complete calls, resolve tool/resource identity, obtain effective policy, record dispatch intent and execute through the broker. Commit results before rebuilding context.
3. Multi-call scheduling: use selected G02 patterns for independent reads and conflicting effects. Cancellation/steering invalidates queued work before dispatch. Keep budget admission and resource locks outside model control.
4. Loop limits: explicit step/output/deadline bounds and wait/input/budget-exhausted states. A model repeating a failed action does not authorize unbounded retries.
5. Memory/routing seams: inject interfaces and visible not-ready states during scaffolding. Integrate real memory/groups in later owning segments; do not add a second loop to enable them.

Tests in `src/tests/end-to-end/coding_loop/` drive a deterministic read/change/test sequence, a user correction during streaming, out-of-order tool completion and a process with partial effects. An independent effect observer checks actual filesystem changes and request counts. Both storage backends must recover the same acknowledged state.

## P2-06 — Verification and honest completion

Discover project checks from real configuration/instructions, bind each to the tested workspace fingerprint and record passed/failed/not-run. Relevant changes invalidate old results. Explanatory analysis can finish with cited evidence; edited code needs proportionate checks and explicit limitations.

Completion is a controller decision supported by the current diff, applicable checks, outstanding issues and known/uncertain spend. A model saying “done,” a successful child branch or a zero exit code for an unrelated check is insufficient. Preserve failed checks and partial artifacts when the task fails or blocks.

Test seeded defects, wrong test directory, missing toolchain, stale passing result after a later edit, failed integrated change and a valid analysis-only result. U01–U03 scaffolds exercise architecture-aware behavior; final quality evaluation belongs in segment 15/16.

## Exit

Run `provider`, `tools`, `recovery` and the deterministic coding-loop cases. Produce a complete attempt/effect/verification trace, current diff and settled/unknown ledger report. P2 provides an internal coding baseline; owner usability still requires memory, grouped routing, extensions and delegation.
