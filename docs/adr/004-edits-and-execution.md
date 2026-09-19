# ADR-004 — Prepared edits and execution receipts

Status: confirmed product direction recorded; engineering design proposed and qualification pending.
Decision gate: P2-04/07; later P4-03. Bounded P0 evidence is recorded below; production qualification and owner sign-off remain pending.

## Context and authority

This record expands the [architecture contract](../architecture/vcp-what.md#9-tools-and-execution) and its [ADR register](../architecture/vcp-what.md#221-adr-register). The architecture remains the product authority. Proposed mechanisms below must be qualified at the named gate before support is advertised.

## Confirmed direction

All writes and process effects pass prepared, revision-bound authorization and durable dispatch. Preserve user changes; report partial effects and unknown outcomes.

## Implementation proposal

Prepare immutable argument/resource/schema identities with expected file versions. Immediately before dispatch, validate current authority, steering and resource state. Persist intent, perform the effect, then persist observed before/after receipts. Multi-file edits need per-file outcomes when the host lacks a tested atomic primitive.

Detailed contracts and failure ordering are in the [supporting design](../architecture/engine-execution-design.md). The [task ledger](../plan/20-traceability.md) preserves exact implementation dependencies; referencing a later integration test does not add a new task dependency.

## Alternatives and unresolved choices

Reuse Codex patch parsing and native process primitives, adapting them behind the VCP broker. Compare lock/version strategies against actual Windows races. Avoid claiming a hash check alone provides atomic compare-and-swap or protection from hostile concurrent path replacement.

## Qualification evidence

P0-03/P0-05 [native evidence](../evaluations/p0-03-recovery-execution.md) qualifies durable prototype intent/receipt fencing, retained dispatch denial and Windows Job Object process control. Prepared file revisions and production reconciliation remain P2-04/07.

The P2 [prepared-file increment](../evaluations/p2-tools-increment.md) selects
exact retained patch preparation, parent guards and deny-write/delete source
handles for bounded native file operations. Candidate bytes are staged and
synced; existing files are written through the version-checked handle, preserving
identity. This is not crash-atomic replacement. Per-file intent/outcome capture
preserves partial application without rollback. Native tests cover concurrent
handle conflicts, stale bytes, hard links, junction rejection, long paths and
case-only rename. Memory-mapped writers and hardware power loss remain outside
the qualified envelope. Process dispatch and production reconciliation still
require the full P2-04/07 gates; no store migration is introduced here.

The [prepared-process increment](../evaluations/p2-process-increment.md) adds
explicit executable/environment profiles, pinned script inputs, native argv and
shell conversion, canonical process receipts and bounded output/deadline
observers. Owner close drains those observers before checkpoint closure. It also
strengthens directory guards after a directory-only rename test exposed the
attribute-only limitation. PTY, full reconciliation and coordinated in-flight
revocation remain at the owning gates. No stored format migration is required;
process profiles must be configured again after reopening.

E05/E07/E08/R02/R04 include CRLF, encoding, locked files, case changes, junction replacement, partial application and grandchild cancellation. A separate marker observer detects duplicate non-idempotent effects after restart. Editor version guarantees are qualified later with real APIs.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

Some external effects cannot be undone or proven absent. Keep uncertainty visible and block blind replay. A compensating edit is a new checked operation; rollback must not overwrite later human changes.

Update this record with the selected mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when its decision gate runs. Reopen an engineering choice when those assumptions fail; changes to confirmed product scope need an explicit owner decision.
