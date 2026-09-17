# ADR-004 — Prepared edits and execution receipts

Status: confirmed product direction recorded; engineering design proposed and qualification pending.
Decision gate: P2-04/07; later P4-03. No implementation, runtime result or owner sign-off is recorded here.

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

E05/E07/E08/R02/R04 include CRLF, encoding, locked files, case changes, junction replacement, partial application and grandchild cancellation. A separate marker observer detects duplicate non-idempotent effects after restart. Editor version guarantees are qualified later with real APIs.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

Some external effects cannot be undone or proven absent. Keep uncertainty visible and block blind replay. A compensating edit is a new checked operation; rollback must not overwrite later human changes.

Update this record with the selected mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when its decision gate runs. Reopen an engineering choice when those assumptions fail; changes to confirmed product scope need an explicit owner decision.
