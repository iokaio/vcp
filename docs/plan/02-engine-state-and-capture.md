# 02 — Engine state, internal events and full capture

Use the [concrete P0 handoff](../development/p0-handoff.md#concrete-segment-02-starting-points) for the first edit locations and retained integration seams. Keep the existing controller/build graph while replacing prototype state and persistence; create each new package only with its first working slice.

Status: P1-01/02/03 in progress through the [durable foundation](../development/p1-foundation.md); retained integration and P1-06 remain. Owns P1-01, P1-02, P1-03 and P1-06. P1-01 begins after P0-06; P1-06 also requires P1-04 from [storage](03-storage-and-budget.md). Architecture sections 4, 14 and 3.8 govern this work.

## Implementation references

Read [entity/lifecycle requirements](../architecture/vcp-what.md#4-domain-model-and-lifecycle),
[capture truthfulness](../architecture/vcp-what.md#143-capture-policy-and-truthfulness)
and [borrowed-component boundaries](../architecture/vcp-what.md#38-boundaries-around-borrowed-components).
The proposed [engine execution design](../architecture/engine-execution-design.md)
provides revision domains, command transaction ordering, artifact staging and
projection contracts. [ADR-002](../adr/002-internal-and-public-protocol.md) and
[ADR-009](../adr/009-context-and-capture.md) track internal protocol/capture
decisions; their existence does not establish implementation qualification.

## Code organization

| Logical module | Proposed files/modules | Responsibility |
|---|---|---|
| `vcp-domain` | `ids`, `workspace`, `session`, `task`, `turn`, `effect`, `verification`, `error` | Typed entities, revision checks and pure transitions |
| Internal `vcp-protocol` | `command`, `event`, `content`, `version` | CLI-neutral envelopes and versioned JSONL content |
| `vcp-store` artifact interface | `artifact`, `capture`, `integrity` | Full payload staging and durable content references |
| `vcp-audit` | `projection`, `cursor`, `redaction`, `inspection` | Queryable state/history derived from canonical records |
| `vcp-engine` | `controller_state`, `command_handler` | Transition orchestration through injected services |

Reuse the selected Codex lifecycle/event types where their meaning fits. Domain types cannot import CLI widgets, a database connection or a model SDK. Keep serialization adapters separate from invariant checks.

## P1-01 — Domain state

1. Introduce typed workspace/session/task/turn/agent/attempt/tool/artifact IDs. Store host roots separately from durable workspace identity. Use distinct session and memory sequences; wire-safe counters serialize as decimal strings.
2. Model the architecture's task, turn and effect states explicitly. A transition accepts expected revision, causative event and reason; invalid or stale transitions return structured errors without mutation.
3. Define completion evidence as current repository fingerprint, output artifacts, checks with passed/failed/not-run status, unresolved effects and known/uncertain cost. A tool exit code alone cannot complete a task.
4. Add steering revision to model steps and prepared effects. Resume requests revalidate current state instead of re-entering saved execution frames.

Implement the smallest coherent domain slice before adding every future entity:
workspace binding → session → task → turn → event/artifact references. Represent
the root-task relationship explicitly so later children cannot accidentally own
a separate money total. Add attempt/tool/verification entities when their first
consumer lands, preserving typed identities at the seams.

| Proposed input to a transition | Required check |
|---|---|
| Entity ID and workspace scope | Record belongs to the caller's authorized workspace |
| Expected entity revision | Equal to current revision; conflict has no side effects |
| Steering revision and cause | Older observations may be recorded but cannot overwrite current instructions |
| Requested state and reason | Legal lifecycle transition with structured diagnostic on rejection |
| Completion evidence | Applicable to current objective, relevant files/environment and unresolved effects |

Implement predicates separately from serialization: `can_dispatch`,
`verification_applies` and `can_complete` are proposed names for pure decisions.
Use exhaustive enums for state and certainty instead of booleans such as `done`
or `success`. Keep process exit, external-effect certainty and task outcome
distinct. A cancelled tool may have applied a file change, and a paused task may
still need effect/cost reconciliation.

Persist accepted objective changes with their source and revision; a progress
question does not replace the task objective. New relevant edits invalidate
verification through dependency/fingerprint comparison, not by deleting the old
result. Forks retain origin references but receive distinct mutable task state.
Choose ID generation and wire-version details in P1-01/P1-02; this plan does not
declare a UUID variant, SQL schema or retained upstream type already suitable.

Tests: generated transition sequences reject completion without applicable evidence; new edits invalidate prior verification; paused tasks retain objective/children; duplicate IDs and stale revisions cannot alter another workspace. Round-trip counters above JavaScript's exact-integer limit. Test behavior, not a second copy of the transition table.

## P1-02 — Internal commands and events

Define `CommandEnvelope` with command ID, workspace/session/task scope, expected revision, caller/controller identity and payload. Define `EventEnvelope` with event ID, correlation/causation, scoped sequence, schema version, timestamp and artifact references. Separate user-facing commentary from private implementation diagnostics.

Implement command admission, idempotent lookup and deterministic handler dispatch against the store interface. Matching IDs/payloads return the original result; reused IDs with different payloads fail. Bind questions/decisions to immutable operation hashes and revisions so stale responses cannot authorize changed effects.

Provide bounded event subscriptions for interactive CLI and JSONL consumers. Slow readers receive backpressure or an explicit cursor gap plus snapshot route; do not silently lose final results. Keep public JSON-RPC negotiation and TypeScript exports deferred.

Follow the [command transaction algorithm](../architecture/engine-execution-design.md#controller-and-command-transactions):
validate/authenticate → resolve matching command receipt → evaluate revisions →
stage payloads → atomically commit state/events/result → acknowledge → notify.
Define the command digest version and included fields before implementing the
receipt lookup. Identical JSON keys in a different order cannot create a second
semantic operation; changed target/arguments/revisions under the same ID fail.
Perform receipt lookup before stale-revision rejection for a legitimate retry of
an already committed command, while still checking current caller access.

For subscriptions, define cursor scope, inclusive/exclusive boundary, page limit,
retention-gap response and terminal-result discovery. Proposed responses include
`events`, `next_cursor`, `snapshot_watermark` and `gap`; names remain internal.
Persisted event sequence determines order, not callback arrival or timestamps.
Separate command acknowledgement from eventual task completion so a streaming
CLI can display accepted work without claiming it finished.

Exercise faults after commit but before acknowledgement and notification.
Reissuing the original command must return its original receipt; re-subscribing
from the previous cursor must expose the event once logically. Add an observer
that counts effects independently of the command-handler's reported status.

Tests: replayed commands create one task/effect, cursor reconnect returns ordered events, slow/closed consumers cannot grow memory without limit, unsupported versions return an actionable error, and identical commands through interactive/headless adapters resolve to the same domain behavior. E01/E09 and the internal subset of R01 apply.

## P1-03 — Full capture and artifact staging

Expose an `ArtifactWriter` contract with open/write/finalize/abort and an immutable descriptor containing media/schema type, integrity information, byte count, retention scope and completion state. Store full model-visible request/response content, complete tool output and child transcripts locally; prompt/UI truncation points reference the full artifact.

Stage referenced payloads durably before committing canonical references, or record a clearly pending artifact. Stream large outputs with bounded buffers and backpressure. An interrupted writer cannot expose a complete descriptor. Keep raw secrets out of serialized capture inputs; record categories omitted and why without storing secret values in the omission record.

Local artifact files remain plaintext with OS access controls. Recovery keys and provider credentials never enter capture. Cloud export goes through segment 11's encrypted publisher; the artifact writer itself does not write into a sync folder.

Implement a writer test double first, then a real local spool using the same
open/write/finalize/abort interface. A proposed write result reports confirmed
offset and bytes retained; finalization returns an immutable digest/length
descriptor. Define exact handling for zero-byte artifacts, invalid UTF-8,
separate stdout/stderr channels, partial writes and an interrupted final chunk.
Text rendering is a derived view; binary or undecodable bytes must remain
retrievable through a declared media/encoding type.

Use the [artifact staging protocol](../architecture/engine-execution-design.md#artifact-staging-and-capture)
to distinguish staged data, finalized content and canonical references. Test a
successful payload write followed by failed transaction separately from failed
payload write: the former can leave an orphan, the latter cannot create a
complete reference. Garbage collection must prove an artifact is unreferenced
and outside active capture/recovery before removing it; retention scheduling is
owned by P5-07.

Request-body capture occurs before dispatch and excludes credential headers by
construction. Capture response/output incrementally so bounded UI tails never
become stored-output truncation. On spool failure, stop new dependent effects,
request bounded cancellation and expose the last known extent. Reopen must find
unfinished writers even if disk failure prevented recording a final error event.
Test synthetic secret separation at transport/configuration inputs as well as
redaction; do not depend solely on pattern matching after capture.

Tests: output larger than the UI limit is fully retrievable; interrupted streams show exact retained/omitted ranges; disk-full or finalize failure pauses new effects; an acknowledged event never references a missing supposedly finalized payload. Use synthetic provider headers, passwords and recovery identities to test exclusion. Do not claim heuristic detection guarantees removal of every possible secret. U05/E17 apply.

## P1-06 — Projections and history

After canonical storage is available, implement projector offsets, task/child summaries, event cursor scans and workspace lookup. Rebuilding projections must not execute tools or issue model requests. Preserve unknown effects and liabilities in both live and rebuilt views.

Implement stable filter metadata for time, workspace, task, agent, model/provider, path and event type; content search can integrate later. Detect missing retained ranges and return explicit gaps. Use snapshot reads so a page does not mix incompatible revisions. Do not use a filename or current machine path as the sole identity.

Write each projector as a deterministic fold over typed events with an explicit
schema version. Commit projection updates and input watermark together; replay
of a delivered event must not increment totals twice. For a changed projection
schema, build beside the current projection and compare both at the same durable
watermark before activation. Projection failure leaves canonical data available
for repair and never authorizes replay of a worker or model request.

Define query filters and cursor encoding in the audit service, not the terminal.
Return the snapshot watermark with every page; reject an expired snapshot or
return a declared restart path instead of mixing revisions. Apply current access
and retention scope when dereferencing artifacts, including from historical
events. A workspace rename updates its binding while old event references remain
resolvable by identity.

Build one fixture with a late model charge, a paused child and an unknown process
effect. Render it live, drop only disposable projections, rebuild in a fresh
process and compare all three facts plus the final cursor. This is stronger than
comparing a task's single status string.

Tests: drop and rebuild projections, duplicate input events, interrupted projector commits, renamed workspace root, forked task history and deleted cursor ranges. Live and rebuilt summaries must agree after the same durable sequence, including paused children and uncertain spend.

## Verification and exit

Run proposed suites `fast`, `store` for artifact/reference integration, and `cli` once consumers exist. Retain event fixtures under `src/tests/fixtures/events/`, artifact cases under `src/tests/contracts/artifacts/`, and projector recovery under `src/tests/recovery/projections/` in the selected workspace.

Done when downstream modules can operate using typed commands/state and complete artifact references; capture failure cannot produce a false success; and a fresh process reconstructs the same acknowledged task state without replaying external work.
