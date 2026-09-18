# ADR-001 — Runtime and process topology

Status: confirmed direction with bounded native P0 evidence; production topology remains subject to P1/P2/P5/P8 qualification.
Decision gate: P0-02/03/05/08/06; P2-07. Engineering evidence is linked below; this does not assert human sign-off or release readiness.

## Context and authority

This record expands the [architecture contract](../architecture/vcp-what.md#3-system-topology-and-runtime) and its [ADR register](../architecture/vcp-what.md#221-adr-register). The architecture remains the product authority. Proposed mechanisms below must be qualified at the named gate before support is advertised.

## Confirmed direction

One local Codex-derived controller owns each session; the active data root has one writer. Workers run bounded effects and local inference. The native Windows CLI owns root and child lifetime. There is no hosted VCP engine or memory backend.

## Implementation proposal

Retain cohesive upstream runtime modules and place injected VCP services at the model, store, policy and execution seams. Use typed worker completions carrying task, attempt and steering revisions; discard stale scheduling results while retaining their observed effects and costs. Persist intent before dispatch and keep blocking I/O outside controller/store locks.

Detailed contracts and failure ordering are in the [supporting design](../architecture/engine-execution-design.md). The [task ledger](../plan/20-traceability.md) preserves exact implementation dependencies; referencing a later integration test does not add a new task dependency.

## Alternatives and unresolved choices

Compare in-process services with supervised local workers for crash containment, memory pressure and cancellation. Select the minimum process split that passes native Windows ownership tests. A separate worker must not introduce its own provider credentials, scheduler or canonical database.

## Qualification evidence

The [P0 dossier](../evaluations/p0-06-handoff.md) combines real local memory,
private root/child pause and recovery, native process control, encrypted storage
and the [retained-engine integration](../development/p0-integration.md). Keep the
in-process Codex controller with injected host authority and supervised native
effect processes. No separate engine or hosted memory service is needed for the
qualified envelope. The private journal and bounded locking strategy are
prototype choices; P1 transactions and P2 worker scheduling must still satisfy
the production failure/performance contracts above.

Record process roles, executable paths, ownership tokens, handle inheritance, queue bounds and shutdown deadlines in the P0 source map. Kill the CLI and workers independently; observe grandchildren and dispatch counts. CPU-only inference and index reopen must pass U09, and owner loss must pass U06.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

Local workers add lifecycle and packaging work; in-process inference shares the engine failure domain. New hosts or isolation requirements can reopen placement, but not the one-controller/local-compute contract.

Update this record with the selected mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when its decision gate runs. Reopen an engineering choice when those assumptions fail; changes to confirmed product scope need an explicit owner decision.
