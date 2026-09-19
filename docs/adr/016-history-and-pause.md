# ADR-016 — Full history, retention and pause lifecycle

Status: P2 pause/recovery lifecycle qualified; history navigation, pruning and installed-CLI qualification remain pending.
Decision gate: the P2-07 internal baseline is complete; P3-04/05, P5-07 and final U05/U06 campaigns remain open. Bounded P0/P2 evidence is recorded below.

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

P0-03 [native evidence](../evaluations/p0-03-recovery-execution.md) qualifies private CLI pause/status/resume, durable root/child restoration, command idempotency and unresolved-effect reconciliation in a synthetic owner. Product CLI, history/pruning and the supported console matrix remain at the production gates above.

The [consolidated P2 qualification](../evaluations/p2-completion.md) carries that
behavior into the canonical coding host: pause fences new root/child/model/tool
work, interrupts owned streams/process trees, preserves late usage and unresolved
effects, survives exclusive reopen and requires deliberate revision-checked
resume. The private owner-control trace remains an internal acceptance surface;
P3 owns the installed CLI and P5 owns retention/pruning.

U05 tests date/timezone edges, active references, changed preview, crash during cleanup and stale indexes/snapshots. U06 requires `/pause`, inspection and `/resume` in one live CLI process, repeated pause and steering while paused with no new dispatch, plus real console close, process trees and post-reopen dispatch counts. Projection rebuild never executes effects.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

Protected recovery and unsettled-accounting facts can delay physical deletion; report what remains and why. A crash may leave unknown remote effects, requiring reconciliation rather than an assumed rollback.

Update this record with the selected mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when its decision gate runs. Reopen an engineering choice when those assumptions fail; changes to confirmed product scope need an explicit owner decision.
