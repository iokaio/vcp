# ADR-003 — Canonical records and artifacts

Status: confirmed product direction; SQLite WAL/FULL selected as the bounded P0 integration candidate. Production schemas/backends remain unqualified.
Decision gate: P0-04/06, P1-04, P5-09/10. Bounded P0 evidence is recorded below; production qualification and owner sign-off remain pending.

## Context and authority

This record expands the [architecture contract](../architecture/vcp-what.md#12-canonical-storage-files-or-sqlite) and its [ADR register](../architecture/vcp-what.md#221-adr-register). The architecture remains the product authority. Proposed mechanisms below must be qualified at the named gate before support is advertised.

## Confirmed direction

Offer storage preference through a shared durable contract. SQLite is the default candidate; files/journal support requires equivalent evidence. Active records/artifacts remain plaintext. Search indexes and UI projections are derived, never alternate authority.

## Implementation proposal

Specify transactional collections, uniqueness, revision checks, event ordering, idempotent command results, indexing intents and artifact descriptors before implementing adapters. Finalize referenced bytes before acknowledging complete references. A failed transaction may leave an unreachable staged object, never an acknowledged dangling artifact.

Detailed contracts and failure ordering are in the [supporting design](../architecture/storage-portability-design.md). The [task ledger](../plan/20-traceability.md) preserves exact implementation dependencies; referencing a later integration test does not add a new task dependency.

## Alternatives and unresolved choices

Compare SQLite transactions and framed append journals using the same workloads and failure barriers. Qualify journal/synchronization settings, checkpoint strategy, artifact placement and writer ownership on Windows. Do not infer power-loss guarantees from a process-kill test.

## Qualification evidence

P0-04 [prototype evidence](../evaluations/p0-04-portable-storage.md) supports SQLite WAL/FULL as the default integration candidate, with a framed-file comparison preserving the same neutral fixtures. Both still require production schemas, bounded replay, migration and power-loss qualification at P1/P8; neither prototype format is a supported backend.

The subsequent [P1 implementation](../development/p1-foundation.md) selects
SQLite WAL/FULL with a 100 ms busy bound and an equivalent versioned, hash-linked
files journal whose durable tip precedes acknowledgement. Both share one owner
lock, canonical constraints, external artifact layout and immutable activation
records. [P1 qualification](../evaluations/p1-completion.md) covers native
transaction kills, corruption, conversion in both directions, activation and
combined ledger/capture receipts. Explicit state/replay and artifact bounds are
documented in the guide. P1 formats replace the feasibility formats; hardware
power-loss and final package qualification remain P8 work.

M01/M02/M07/M08 compare canonical export, reopened records, reservation balances and artifact hashes after identical schedules. Corruption inside committed history fails closed; incomplete uncommitted tails have explicit recovery. Cross-backend conversion validates a new root before activation.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

A custom journal adds replay, corruption and migration burden; SQLite still requires separate artifact/index coordination. Measurements select defaults, while an explicit unsupported preference fails visibly. Neither backend may silently reset history during upgrade.

Update this record with the selected mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when its decision gate runs. Reopen an engineering choice when those assumptions fail; changes to confirmed product scope need an explicit owner decision.
