# ADR-016 — Full history, retention and pause lifecycle

Status: confirmed product direction recorded; engineering design proposed and qualification pending.
Decision gate: P3-04/05, P5-07, U05/U06. No implementation, runtime result or owner sign-off is recorded here.

## Context and authority

This record expands the [architecture contract](../architecture/vcp-what.md#118-history-exploration-aging-and-pruning) and its [ADR register](../architecture/vcp-what.md#221-adr-register). The architecture remains the product authority. Proposed mechanisms below must be qualified at the named gate before support is advertised.

## Confirmed direction

Retain full observed history, notify beyond 30 days and prune only by user command or saved policy. Explicit `/pause` pauses root and children while the CLI stays open for inspection; `/resume` deliberately continues in that same process after revalidation. Closing the owning CLI also pauses its task tree; reopening reconciles before deliberate continuation.

## Implementation proposal

Separate exclude-from-recall, presentation compaction and content purge. Compute revision-bound exact selectors and protected dependency sets. Commit tombstones/deletion epoch before asynchronous cleanup and invalidate pending context. On owner loss, stop admission, cancel bounded work and preserve incomplete artifacts/effects/liabilities.

Detailed contracts and failure ordering are in the [memory design](../architecture/memory-retrieval-design.md), [prune transaction](../architecture/storage-portability-design.md#retention-and-prune-transaction) and [pause/recovery design](../architecture/engine-execution-design.md#pause-crash-reconciliation-and-resume). The [task ledger](../plan/20-traceability.md) preserves exact implementation dependencies; referencing a later integration test does not add a new task dependency.

## Alternatives and unresolved choices

Use canonical history and derived paged views rather than UI-tail logs as authority. Notice repeat cadence, additional retention defaults and historical vector availability remain explicit decisions. Saved policy must identify scope and action instead of an implicit global age cutoff.

## Qualification evidence

U05 tests date/timezone edges, active references, changed preview, crash during cleanup and stale indexes/snapshots. U06 requires `/pause`, inspection and `/resume` in one live CLI process, repeated pause and steering while paused with no new dispatch, plus real console close, process trees and post-reopen dispatch counts. Projection rebuild never executes effects.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

Protected recovery and unsettled-accounting facts can delay physical deletion; report what remains and why. A crash may leave unknown remote effects, requiring reconciliation rather than an assumed rollback.

Update this record with the selected mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when its decision gate runs. Reopen an engineering choice when those assumptions fail; changes to confirmed product scope need an explicit owner decision.
