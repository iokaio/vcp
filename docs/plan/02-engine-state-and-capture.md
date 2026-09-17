# 02 — Engine state, internal events and full capture

Status: planned. Owns P1-01, P1-02, P1-03 and P1-06. P1-01 begins after P0-06; P1-06 also requires P1-04 from [storage](03-storage-and-budget.md). Architecture sections 4, 14 and 3.8 govern this work.

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

Tests: generated transition sequences reject completion without applicable evidence; new edits invalidate prior verification; paused tasks retain objective/children; duplicate IDs and stale revisions cannot alter another workspace. Round-trip counters above JavaScript's exact-integer limit. Test behavior, not a second copy of the transition table.

## P1-02 — Internal commands and events

Define `CommandEnvelope` with command ID, workspace/session/task scope, expected revision, caller/controller identity and payload. Define `EventEnvelope` with event ID, correlation/causation, scoped sequence, schema version, timestamp and artifact references. Separate user-facing commentary from private implementation diagnostics.

Implement command admission, idempotent lookup and deterministic handler dispatch against the store interface. Matching IDs/payloads return the original result; reused IDs with different payloads fail. Bind questions/decisions to immutable operation hashes and revisions so stale responses cannot authorize changed effects.

Provide bounded event subscriptions for interactive CLI and JSONL consumers. Slow readers receive backpressure or an explicit cursor gap plus snapshot route; do not silently lose final results. Keep public JSON-RPC negotiation and TypeScript exports deferred.

Tests: replayed commands create one task/effect, cursor reconnect returns ordered events, slow/closed consumers cannot grow memory without limit, unsupported versions return an actionable error, and identical commands through interactive/headless adapters resolve to the same domain behavior. E01/E09 and the internal subset of R01 apply.

## P1-03 — Full capture and artifact staging

Expose an `ArtifactWriter` contract with open/write/finalize/abort and an immutable descriptor containing media/schema type, integrity information, byte count, retention scope and completion state. Store full model-visible request/response content, complete tool output and child transcripts locally; prompt/UI truncation points reference the full artifact.

Stage referenced payloads durably before committing canonical references, or record a clearly pending artifact. Stream large outputs with bounded buffers and backpressure. An interrupted writer cannot expose a complete descriptor. Keep raw secrets out of serialized capture inputs; record categories omitted and why without storing secret values in the omission record.

Local artifact files remain plaintext with OS access controls. Recovery keys and provider credentials never enter capture. Cloud export goes through segment 11's encrypted publisher; the artifact writer itself does not write into a sync folder.

Tests: output larger than the UI limit is fully retrievable; interrupted streams show exact retained/omitted ranges; disk-full or finalize failure pauses new effects; an acknowledged event never references a missing supposedly finalized payload. Use synthetic provider headers, passwords and recovery identities to test exclusion. Do not claim heuristic detection guarantees removal of every possible secret. U05/E17 apply.

## P1-06 — Projections and history

After canonical storage is available, implement projector offsets, task/child summaries, event cursor scans and workspace lookup. Rebuilding projections must not execute tools or issue model requests. Preserve unknown effects and liabilities in both live and rebuilt views.

Implement stable filter metadata for time, workspace, task, agent, model/provider, path and event type; content search can integrate later. Detect missing retained ranges and return explicit gaps. Use snapshot reads so a page does not mix incompatible revisions. Do not use a filename or current machine path as the sole identity.

Tests: drop and rebuild projections, duplicate input events, interrupted projector commits, renamed workspace root, forked task history and deleted cursor ranges. Live and rebuilt summaries must agree after the same durable sequence, including paused children and uncertain spend.

## Verification and exit

Run proposed suites `fast`, `store` for artifact/reference integration, and `cli` once consumers exist. Retain event fixtures under `src/tests/fixtures/events/`, artifact cases under `src/tests/contracts/artifacts/`, and projector recovery under `src/tests/recovery/projections/` in the selected workspace.

Done when downstream modules can operate using typed commands/state and complete artifact references; capture failure cannot produce a false success; and a fresh process reconstructs the same acknowledged task state without replaying external work.
