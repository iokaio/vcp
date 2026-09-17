# Engine, accounting and execution design

Status: proposed implementation design. This document expands existing product
contracts; it is not an accepted ADR, imported implementation, or test result.
Type/field names and module assignments below are proposed. P0 maps logical
modules to the retained Codex workspace; do not build a second controller merely
to reproduce these names. [ADR-013](../adr/013-upstream-reuse-and-vendoring.md)
governs source ownership.

## Authority and delivery map

| Contract | Authoritative requirement | Implementation owners |
|---|---|---|
| Entity identity and state | [Domain lifecycle](vcp-what.md#4-domain-model-and-lifecycle) | P1-01/P1-02 |
| Durable capture and projections | [Observability and recovery](vcp-what.md#14-observability-durability-and-recovery) | P1-03/P1-06 |
| Canonical transactions | [Backend-neutral contract](vcp-what.md#122-backend-neutral-contract) | P1-04 |
| Root money accounting | [Budget enforcement](vcp-what.md#8-cost-accounting-and-budget-enforcement) | P1-05 |
| Prepared tools and authority | [Tools](vcp-what.md#9-tools-and-execution), [permissions](vcp-what.md#10-permissions-and-isolation) | P2-03/P2-04 |
| Completion and close/reopen | [Completion](vcp-what.md#44-completion-contract), [pause](vcp-what.md#45-terminal-close-pause-and-workspace-resume) | P2-05/P2-06/P2-07 |
| CLI control and inspection | [CLI contract](vcp-what.md#17-cli-and-programmatic-clients) | P3-01–04 |

[Context/provider design](context-provider-design.md) specifies the inputs to a
model attempt. [Storage and portability design](storage-portability-design.md)
specifies physical backend, snapshot and migration work. Neither design relaxes
the canonical transaction or effect boundaries below.

## Identity, revisions and ownership

Use distinct opaque types for workspace, session, task, turn, step, attempt,
tool-run, execution, artifact, approval and verification IDs. Entity access
always checks stored workspace/scope membership; an opaque ID is a locator, not
an authorization token. Root bindings contain host identity and normalized
absolute paths separately from the portable workspace ID. Rebinding changes a
binding revision, not every historical event or artifact reference.

Keep the following counters distinct:

| Proposed counter | Meaning and invalidation consequence |
|---|---|
| `entity_revision` | Optimistic concurrency of one canonical entity |
| `session_seq` | Ordered durable events within a session; not wall-clock order |
| `memory_seq` | Canonical memory watermark; not a session cursor |
| `steering_revision` | Current objective/constraint interpretation for a task |
| `policy_revision` | Effective authority rules used for a decision |
| `authority_revision`, `deletion_epoch` | Workspace access/retention changes relevant to evidence reuse |
| `owner_epoch` | Current local controller incarnation; fences stale worker requests |
| `projection_watermark` | Last canonical event included in a derived view |

Serialize large counters and monetary values as decimal strings; reject overflow
and wrong counter domains. A controller incarnation may finish receiving late
evidence from an older execution without granting that execution new authority.
Persist controller/execution nonces and host/process identity where relevant;
PID equality alone cannot establish ownership after restart.

## Controller and command transactions

One session controller owns transition decisions. Async work returns typed
messages containing operation IDs, starting revisions and artifact references;
it cannot directly mutate task state. Schedule bounded independent reads and
model/worker activity outside controller locks and store transactions. Give
pause, cancellation and steering a responsive admission path even when a model
stream or output reader is backpressured.

Implement pure transition functions that consume current state and a typed
command/outcome and return proposed mutations, events and scheduling intents.
The task/turn/effect state names remain those in
[sections 4.2–4.3](vcp-what.md#42-task-and-turn-semantics). A scheduler intent is
not an executed effect. After store acknowledgement, the controller may dispatch
eligible newly admitted work; reconstruction consumes committed records without
dispatching those intents automatically.

For a state-changing command, use this sequence:

1. Validate the envelope version, caller identity, scope and payload shape.
2. Resolve the idempotency key in its authenticated scope. Compare a canonical
   payload digest that includes operation type, target, expected revisions and
   effective arguments. Document the serialization/digest version; arbitrary
   JSON object field ordering cannot change semantic identity.
3. For an existing matching command, return its recorded outcome without applying
   it again. A reused ID with a changed payload is a conflict. Recheck current
   access before returning historical result content.
4. Read a coherent state revision, apply the pure transition, and durably stage
   any required immutable payloads. Never wait for approval or a model here.
5. In one canonical transaction, verify expected revisions, append events,
   mutate authoritative state, add any indexing intents, and record the command
   result. Allocate event sequences in the commit boundary.
6. Acknowledge only after the durable receipt. Publish notifications from the
   committed sequence. A subscriber can recover a missed notification by cursor.

A lost acknowledgement is resolved by retrying the same command ID, not by
creating a new task. For a conflict, re-read and re-plan explicitly; generic
automatic retry must not apply old authority to changed inputs. A transaction
failure leaves no acknowledged partial canonical state. Staged unreferenced
artifacts are later reclaimable after proving no pending/live reference exists.

## Canonical records and projections

The minimal first slice persists workspace bindings, session/task/turn state,
command receipts, events, artifacts, attempts/reservations, tool runs/approvals
and verification records. Add memory/generation/retention collections through
their owning work items using the same transaction interface. Unique constraints
cover command ID and digest, event ID and session sequence, one reservation per
attempt, and one effective decision per approval. Never infer referential
integrity merely from a path existing on disk.

Choose whether a small projection update belongs in the source transaction or
an idempotent projector transaction. For asynchronous projections, commit the
projection changes and its input watermark together. Process an event at most
once logically even if delivery repeats. Rebuild into a new projection version,
compare it at a common watermark, then activate it; do not erase live recovery
state to repair a summary.

Query pages bind scope, filters, projection/schema version, snapshot watermark
and final cursor. A later page either uses the same coherent snapshot or reports
that it expired. Return explicit access/retention gaps instead of silently
joining unrelated surviving ranges. Summaries include uncertain cost and
unknown effects even when those records are inconvenient to display. Trace
rendering, projection rebuild and task re-execution are separate operations.

## Artifact staging and capture

Proposed artifact descriptors contain `artifact_id`, owning scope, media/schema
type, capture mode, byte length, integrity algorithm/digest, retained ranges,
omission reasons, source operation and completion state. A streaming writer
returns bounded chunk receipts or a pending reference until finalization; it
cannot label an interrupted stream complete. Prefer immutable sealed chunks
plus a manifest when that simplifies durable partial output. Choose the actual
layout under P1-03/P1-04 after crash qualification.

The recording order is:

1. Strip authentication material before it enters the capture interface. Network
   credentials are supplied through a separate trusted transport facility.
2. Write request bodies, response/output bytes or source versions to local
   staging with bounded buffers and explicit offsets.
3. Finalize and verify immutable content through the backend's tested durability
   boundary, then commit its descriptor/reference. A pending descriptor may
   refer only to its positively observed retained extent.
4. Derive model/UI tails and redacted exports from recorded content. Every view
   states whether it is full, partial, redacted, unavailable or metadata-only.

The default captures full observed local work, not hidden model reasoning or
unexposed provider internals. An exact-request inspector means the body bytes
VCP actually serialized under that capture policy. Request headers containing
credentials are never part of that view. A derived redacted body cannot claim
byte identity with the transmitted body.

On capture failure, stop new effects that require recording and request bounded
cancellation of affected active work. Preserve the last durable extent; report
unknown effects if the worker may have continued. A storage failure may prevent
a final failure event from being saved, so reopen must detect unfinished writers
and dispatch intents independently. Do not drop output silently when a spool
limit or disk capacity is reached. Active artifacts remain local plaintext;
cloud-bound copies use the encrypted publisher owned by P5-09.

## Atomic budget admission and settlement

Proposed `ReservationRequest` carries root/task/attempt/role IDs, ledger revision,
currency, immutable price/capability snapshots, actual request size, maximum
output, charge categories, estimation method and applicable child/session/daily
limits. Pricing units are explicit. Use checked fixed-precision arithmetic;
round required reserves upward at the selected currency precision. The precision,
overflow behavior and provider-category mapping need P1-05 evidence.

Under one transaction, compute:

```text
available = cap - settled - active_reservations
            - unresolved_reserves - protected_verification_reserve
```

Admit only when the bounded estimate fits every applicable limit; atomically
write reservation, attempt, root/child aggregate revisions and admission event.
Use constraints or revision conflicts to force racing children to recalculate.
Child allocations constrain eligibility and remaining share; do not add them to
the root's available funds or count them as charges. Protected verification
funds require an explicit eligible-purpose draw/transfer so a verification
reservation is not also subtracted as protected money.

Use the architecture's durable reservation lifecycle:

| State/change | Required evidence and accounting |
|---|---|
| `created` | Admitted reserve; request not yet submitted |
| `created -> released` | Positive proof no transport submission could occur |
| `created -> submitted` | Commit send intent before transport may send; a crash at this boundary may create uncertainty |
| `submitted -> settled` | Durable normalized final usage with source evidence |
| `submitted -> reconciliation_pending` | Cancellation/disconnect/crash without reliable final usage; reserve remains |
| `reconciliation_pending -> settled` | Idempotent reliable reconciliation |
| `reconciliation_pending -> explicitly_resolved` | Explicit accounting policy/decision preserving its uncertainty and audit history |

Transport progress can refine whether bytes were sent, but absence of a remote
request ID is not proof of no charge. Every retry has a new attempt and reserve;
the failed predecessor's liability does not vanish. Store provider usage fields
with their source identity; totals and constituent subtotals are not additive.
Duplicate observations do not add charges twice. Late corrected final usage
produces a versioned reconciliation/adjustment rather than rewriting evidence.

If actual spend exceeds the reservation, record actual spend and a visible
overrun, then block unaffordable admission. Do not clamp charges to make the cap
appear respected. Pause, cap changes, restore and backend conversion preserve
liabilities. Local embeddings record resource use separately and never create a
remote embedding charge. Model-assisted memory, compaction and optimization
enter this same root or explicitly configured maintenance ledger.

## Prepared effects and authorization

Proposed `PreparedInvocation` contains tool-run ID, tool/schema revision,
canonical arguments, resource keys, effect/retry class, expected source versions,
workspace/root binding, steering revision and required capabilities. Compute an
operation digest over the meaning of those fields, including host and execution
mode. Approval applies to that immutable meaning; rewriting arguments, changing
an MCP schema or resolving a path to another target requires re-preparation.

Evaluate policy in the [specified decision order](vcp-what.md#102-decision-order).
A proposed `PolicyDecision` records allow/deny/question, rule origins, applicable
grants, required isolation and checked revisions. A question contains operation
digest, scope, authorized actor/controller, expiry and policy revision. Resolution
is a compare-and-set from pending to a single decision. A duplicate identical
answer is idempotent; a different, expired or stale answer grants nothing.

Acquire trusted resource conflicts in a stable order before the final freshness
check. Known disjoint reads can run concurrently; writes on intersecting paths
serialize. Unknown process/MCP effects use conservative workspace or service
conflict scope. Model-supplied read-only labels cannot weaken this classification.
Do not hold store locks while waiting for resource availability or user input.

After rechecking ownership, steering relevance, policy, roots, file versions and
isolation support, commit authorized dispatch intent and issue a broker-only
capability for that execution identity. Capabilities bind effect scope,
executable/arguments or explicit shell, environment, handle inheritance, time and
resource limits. Their representation and IPC authentication are P0-05/P2-04
choices; an in-process struct alone is not OS isolation. The worker accepts one
admitted execution, and the model cannot call its execution endpoint directly.

## File and process outcome contracts

Preparation computes all before/after bytes and complete proposed changes before
writing. Model patch text only feeds the parser. Resolve root containment and
link/reparse targets, reject ambiguous matches, preserve encoding/line endings,
and bind expected versions to the prepared change. Preserve existing staged,
unstaged and untracked work; index changes are separate authorized Git effects.

For each file, stage candidate content, repeat path/identity/version checks as
close to replacement as the qualified Windows primitive allows, perform the
write, observe the result, and persist a per-file receipt. Receipts record before,
intended after, observed after, operation/execution IDs and certainty. A later
failure yields a partial change set. Recovery compares current bytes and identity
to receipts; it never restores old bytes over a newer human edit. Advisory locks
and hashes do not promise a filesystem compare-and-swap against arbitrary editors.

Processes capture stdout/stderr separately with ordered offsets and full spooled
artifacts. The worker identity includes an execution nonce and validated process
identity, not only PID. Test child creation, job assignment, inherited handles,
PTY behavior and forced cancellation at the selected Windows implementation.
Report enforced isolation capabilities and unsupported requests. A surviving
descendant or a process exit without observable effect certainty is recorded
separately from exit code; failed and cancelled tools can have real effects.

## Pause, crash reconciliation and resume

Pause begins by fencing new scheduling at the controller: change lifecycle state
and owner/admission generation, reject queued work for the old generation, and
propagate cancellation through all children, model calls, tools and task-scoped
extraction/observer maintenance. Local receipt reconciliation and a configured
pause checkpoint may finish without admitting model calls or resuming task work.
Do not introduce a new persisted task state named `stopping`; it can be an
internal shutdown phase while the task transitions to the specified paused state.
Accept late outcomes and usage as evidence without letting them enqueue new work.

Record why each descendant is paused: inherited parent pause, explicit child
pause or another blocker. A parent resume removes only its inherited scheduling
hold; an independently paused or cancelled child does not become runnable. Keep
the child's own expected revision and current authority in that decision.

An explicit `/pause` does not close the owning CLI. Keep it open for status,
cost, history and evidence inspection; display paused children, pending questions
and unresolved effects. Repeated pause is idempotent. The user can deliberately
`/resume` in that same process after revalidation; answering a question or
changing guidance while paused cannot restart scheduling implicitly. Pause,
cancel and exit remain distinct user actions. Route `vcp tasks pause <task-id>`
and structured input through the same command using a qualified private
owner-control path; authenticate ownership and never open a competing writer.
An unreachable owner is explicit, and public API/SDK attachment remains deferred.

Checkpoint during normal execution. At graceful close, request cancellation,
wait bounded grace, escalate where allowed, finalize partial artifacts and commit
pause/unknown liabilities. A portable checkpoint is attempted only if configured
and time permits; an unavailable vault does not justify ongoing task scheduling.
Hard termination can bypass this sequence and is recovered from durable intents.

| Last durable boundary | Restart action |
|---|---|
| Prepared only | Revalidate before any new authorized execution |
| Dispatch intent, no worker result | Query matching execution identity; classify unknown if proof is unavailable |
| File intent and some per-file receipts | Compare actual identities/hashes; report applied/unapplied/conflicted/unknown per file |
| Worker result not yet canonical | Reconcile trusted retained evidence; never infer success from process disappearance |
| Model submission without final usage | Preserve unresolved reserve; use supported provider reconciliation or explicit unknown state |
| Outcome committed, reply lost | Return same recorded result by command/operation ID |

No row licenses blind replay of a non-idempotent operation. Allow unrelated reads
within current policy while unknown effects block mutations whose assumptions
they could invalidate. Explicit resume is a new command validating workspace
bindings, instructions, authority, current files, pending effects, model/catalog
and budget. A restored environment obtains destination authority separately.

## Verification, CLI and inspection

Proposed `VerificationRecord` includes check specification and origin, command
and working directory, environment/toolchain identity, input fingerprint,
affected artifacts, timings, exit status, findings and raw output references.
Compute relevant fingerprints before and after the check. If relevant inputs
change during execution, retain the observed result but mark it inapplicable to
the current result. A missing executable is `not_run`, not a failed test or pass.
Generated outputs may be part of the check's declared output set; any treatment
of them as irrelevant must be explicit in the check specification.

A completion candidate reads current task acceptance, diff/artifacts, applicable
checks, outstanding work/effects and settled/uncertain money in a coherent view.
Commit completion with expected revisions so concurrent edits or steering cannot
attach stale evidence. Analysis-only tasks may finish with citations; changed
code needs proportionate applicable checks. A model's finish reason is input to
this decision, not authorization to mark the task complete.

The CLI issues the same command envelopes in interactive and JSONL modes. It
derives display state from projections and owns task lifetime; a renderer cannot
launch a worker. Preserve durable command/task IDs on required-input, pause,
budget and unresolved-effect outcomes. Follow the exact exit-code precedence in
[section 17.3](vcp-what.md#173-headless-contract). JSONL stdout has versioned
envelopes only; diagnostics use stderr. Escape untrusted terminal controls in
display views while preserving recorded raw bytes.

Inspectors traverse task → context/attempt → policy/dispatch → outcome → checks
and costs through scoped references. Check current access at each dereference,
including artifact ranges and historical views. Bounded event queues either
apply backpressure or return an explicit cursor gap and snapshot recovery path.
Do not make a terminal-size limit into a history-retention limit.

## Acceptance and unresolved engineering selections

| Owning work | Required independent observation |
|---|---|
| P1-01/P1-02 | Generated command schedules preserve revision/scope/idempotency; event observers agree after lost acknowledgement |
| P1-03/P1-04/P1-06 | Kill at stage/commit/reply/projector boundaries; fresh process reconstructs acknowledged state and exact retained ranges |
| P1-05 | Racing main/helper/child requests and late usage keep one balanced root ledger across restart/conversion |
| P2-03/P2-04 | Outside-root marker remains untouched; stale approval does not dispatch; actual Windows process tree and edited bytes match receipts |
| P2-05/P2-06 | Scripted read/edit/check workflow completes only with current evidence; hidden helper request count stays zero |
| P2-07/P3-04 | Non-idempotent external marker increments once despite intent/result crashes and reopen; owner loss stops descendant scheduling |
| P2-07/P3-02/P3-04 | Pause without closing during model/tool/input/child work, inspect while paused, repeat pause and deliberately resume in the same CLI; no background admission bypasses the fence |
| P3-01–03 | Independent JSONL parser and inspector queries recover ordered evidence without secrets or workspace leaks |

Use the [fixture/fault harness](../plan/16-test-fixtures-and-acceptance.md#harness-contracts-to-code-first)
and original E/R/U mappings in the owning plans. Physical flush guarantees,
artifact chunk layout, lock mechanisms and migration format remain P0-04/P1-04
decisions. Process isolation/capability transport remain P0-05/P2-04 decisions;
autonomy defaults remain P2-03/P8-01. Record them in the corresponding
[ADR register](vcp-what.md#221-adr-register) with measured evidence before making
support claims. Forced-process survival does not establish power-loss safety.
