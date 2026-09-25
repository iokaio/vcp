# 05 — OpenRouter adapter, coding loop and completion

Status: P2-02, P2-05 and P2-06 complete with [consolidated P2 acceptance](../evaluations/p2-completion.md), including provider contracts and live compatibility, the retained coding loop, and current-result verification/completion on both canonical stores. P2-02 needs P1-05 and P0-09. The loop also needs context, policy and tools from segments 04/06. Architecture sections 4, 7–9 and 14 define behavior.

## Implementation references

Use [OpenRouter adapter requirements](../architecture/vcp-what.md#71-adapter-boundary),
[routing/handoff](../architecture/vcp-what.md#75-routing-algorithm-and-explanation)
and [completion](../architecture/vcp-what.md#44-completion-contract) as authority.
The proposed [context/provider design](../architecture/context-provider-design.md)
specifies request sealing, parsing, uncertainty and retries; the
[engine design](../architecture/engine-execution-design.md#controller-and-command-transactions)
specifies state/effect transactions. [ADR-006](../adr/006-model-gateway-and-groups.md)
tracks gateway qualification. No current OpenRouter SDK, endpoint parameter or
provider behavior is declared supported by this document alone.

The later [bounded decision design](../architecture/decision-evaluation-design.md)
reuses this admitted provider path for optional P6 judgments. Preserve a normalized
structured-result boundary and attempt attribution; do not add a second provider
client or hidden repair calls. P2-06 completion and mandatory checks remain based
on observed verification, never a classifier score. P2 implementation does not
depend on the future evaluator. P6-02 later qualifies actual Jev's
[OpenRouter decision operation](../architecture/decision-evaluation-design.md#jev-through-openrouter-qualification)
and conventional LLM comparison through these shared facilities. Do not assume
native decisions use the same wire schema as chat or manufacture chat events for
unsupported fields. A distinct request codec still uses the same admission and
receipt boundaries; no LangChain runtime or direct TypeSafe transport is needed.

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

Build separate layers for normalized request validation, provider conversion,
transport framing, payload parsing, normalized events and controller consumption.
The transport gets credentials from a separate trusted facility; a serializable
request contains credential references only where needed, never header values.
Persist the exact request body and visibility manifest before transport dispatch.
The gateway checks that its attempt/reservation, model/provider constraints and
body identity match what admission approved.

Maintain a conversion table per qualified compatibility record:

| Entry | Required content |
|---|---|
| Messages/roles | Supported roles, logical trust conversion and incompatible cases |
| Tools | Schema subset, call ID handling, argument limits and result pairing |
| Output/effort | Supported bounds, normalized defaults and rejected required settings |
| Streaming | Framing/termination behavior, ordering and partial-event interpretation |
| Attribution/usage | Requested versus served identity, charge units, cumulative fields and unknown states |
| Provider policy | Enforced allowed-provider/data constraints and fallback behavior |

Test framing independently from JSON: split chunks inside UTF-8 characters,
escaped strings and delimiters; combine several events in one read; inject
keepalives, premature EOF and duplicate terminal messages. Accumulate each tool
call by attempt and provider call ID with bounded bytes. Only complete validated
arguments with a known schema become proposed tool calls. A fragment temporarily
forming valid JSON is not proof that the provider finished the call. Initially
wait for a qualified response boundary before tool admission; early tool dispatch
needs explicit interruption/retry qualification.

Disable implicit client-library retries unless every underlying request is
observable and covered by VCP's attempt policy. VCP-controlled retry means a new
attempt/reservation with a predecessor link. Bound total retries, delayed retry
time and deadline; pause cancels retry timers. Gateway-side fallback is eligible
only when all possible candidates fit the admitted capability/context/data and
accounting envelope. Otherwise perform a VCP-controlled reassembly/re-admission
or reject fallback. A user model pin does not silently become a fallback pool.

Normalize cumulative usage updates without summing overlapping totals. Persist
raw usage source fields, normalization version and certainty; duplicate messages
settle once, while a corrected final observation becomes an explicit adjustment.
Unknown served identity remains unknown. Cancellation after submission retains
liability even if no final answer or request ID arrived. Separately test a
positively pre-send failure that can release its reserve.

Tests in `src/tests/contracts/provider/`: malformed/truncated stream, interleaved tool fragments, duplicate terminal event, rate limit, timeout before and after send, cancellation during usage delivery, unsupported role/tool schema, fallback to smaller context and missing cost fields. Assert no tool dispatch from an incomplete call and no request without a durable reservation. Use scripted local transport before a separately budgeted live smoke test. E11/E12/R05 apply.

## P2-05 — Retained Codex loop with VCP boundaries

Owner-selected canonical tool ceilings and their retained-loop binding are
documented in [canonical model tool ceiling](../development/p2-canonical-tool-ceiling.md).

Implement in observable increments:

1. Read-only loop: assemble versioned context, choose a fixed qualified model for the internal scaffold, reserve, stream, capture and report evidence.
2. Prepared tool loop: validate complete calls, resolve tool/resource identity, obtain effective policy, record dispatch intent and execute through the broker. Commit results before rebuilding context.
3. Multi-call scheduling: use selected G02 patterns for independent reads and conflicting effects. Cancellation/steering invalidates queued work before dispatch. Keep budget admission and resource locks outside model control.
4. Loop limits: explicit step/output/deadline bounds and wait/input/budget-exhausted states. A model repeating a failed action does not authorize unbounded retries.
5. Memory/routing seams: inject interfaces and visible not-ready states during scaffolding. Integrate real memory/groups in later owning segments; do not add a second loop to enable them.

Map every retained upstream request path before enabling the loop: main response,
compaction, review, retry, routing helpers and memory extraction. Replace or wrap
each with the same admitted gateway. Instrument request dispatch in the fixture
transport so an unaccounted hidden helper call fails independently of ledger
events. Retaining upstream types is useful only when their lifecycle meaning
matches VCP; annotate deliberate differences in the P0 source map and patches.

Implement each turn as controller decisions around async messages:

1. Snapshot current task/steering and dependency revisions; assemble/seal context.
2. Obtain capability and root-budget admission for the actual serialized request.
3. Commit submission intent, stream outside store/controller locks, and capture
   normalized results plus usage observations.
4. Recheck current steering before admitting proposed tool operations. Validate,
   prepare and authorize each tool through segment 06; incomplete calls never
   enter the scheduler.
5. Schedule independent operations by trusted resource conflicts and global/task
   limits. Preserve original call correlation when results complete out of order.
6. Commit each observed outcome/artifact before constructing its tool-result
   context. Continue, verify, wait, pause or stop according to typed task state.

The scheduler owns concurrency, not a model field. Commands with unknown effects
claim conservative conflict scope; overlapping writes serialize, while disjoint
bounded reads may batch. A slow output stream cannot block cancellation/steering
admission. Queue and timer callbacks carry an owner/admission generation so a
late callback after pause cannot start work. Late output/usage remains recordable
without restarting scheduling.

Define explicit failure exits for empty/unusable responses, repeated identical
invalid tools, exhausted context, budget denial, missing authority and store/capture
failure. Count bounded repair/escalation attempts at the root. An unavailable
memory service in the early scaffold is visible capability state, not invented
evidence or a substitute remote embedding request.

Tests in `src/tests/end-to-end/coding_loop/` drive a deterministic read/change/test sequence, a user correction during streaming, out-of-order tool completion and a process with partial effects. An independent effect observer checks actual filesystem changes and request counts. Both storage backends must recover the same acknowledged state.

## P2-06 — Verification and honest completion

**Planned follow-up to completed acceptance:** [M6 verification ordering](21-markov-integration.md#m6--verification-order)
separates required check discovery/dependencies from execution order. Collect
check-specific cost/failure evidence and evaluate fail-fast ordering only among
ready checks; preserve the full applicable set, current fingerprints, not-run
visibility and completion gates. Missing evidence retains baseline order.

Current [retained integration](../development/p2-loop-verification.md) connects
owner-configured checks to `vcp_verify` and final-cost completion evidence. The
remaining contracts below continue to define task completion.

Discover project checks from real configuration/instructions, bind each to the tested workspace fingerprint and record passed/failed/not-run. Relevant changes invalidate old results. Explanatory analysis can finish with cited evidence; edited code needs proportionate checks and explicit limitations.

Completion is a controller decision supported by the current diff, applicable checks, outstanding issues and known/uncertain spend. A model saying “done,” a successful child branch or a zero exit code for an unrelated check is insufficient. Preserve failed checks and partial artifacts when the task fails or blocks.

Implement a proposed `VerificationPlan` containing check origin/specification,
execution directory, relevant source/configuration/toolchain dependencies,
expected evidence and rationale for scope. Discover commands from actual project
manifests and instructions; do not run a command merely because a generic skill
names it. Commands still require policy and execution receipts.

Fingerprint relevant inputs before the check and again after it. If an editor or
tool changed them mid-check, retain the observed result with a stale/inapplicable
marker and plan proportionate fresh verification. Include instruction/config
and environment changes where they affect the check. A command starting in the
wrong subdirectory or finding zero relevant tests cannot silently satisfy the
intended check; fixtures should assert the expected target was exercised.

Construct a completion candidate with current acceptance revision, result
fingerprint, artifact/diff references, applicable checks, limitations, unresolved
effects and known/uncertain spend. Atomically compare current task/source
revisions before committing completion. If those changed, recompute the candidate
instead of preserving a stale success. Analysis-only completion uses cited
evidence; documentation edits need applicable documentation checks rather than
an invented full application test requirement.

Keep `failed`, `not_run`, `stale` applicability and `passed` outcome separately
where necessary. Missing toolchains and unavailable fixtures produce explicit
not-run reasons, and known failures remain visible even after a later narrower
check passes. The CLI's exit code summarizes this richer record; it is not the
record itself.

Test seeded defects, wrong test directory, missing toolchain, stale passing result after a later edit, failed integrated change and a valid analysis-only result. U01–U03 scaffolds exercise architecture-aware behavior; final quality evaluation belongs in segment 15/16.

## Exit

Run `provider`, `tools`, `recovery` and the deterministic coding-loop cases. Produce a complete attempt/effect/verification trace, current diff and settled/unknown ledger report. P2 provides an internal coding baseline; owner usability still requires memory, grouped routing, extensions and delegation.
