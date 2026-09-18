# ADR-015 — Portable snapshots, backend choice and handoff

Status: confirmed product direction; bounded P0 portability comparison complete. Production snapshot format and activation remain unqualified.
Decision gate: P0-04, P5-09/10, U04. Bounded P0 evidence is recorded below; production qualification and owner sign-off remain pending.

## Context and authority

This record expands the [architecture contract](../architecture/vcp-what.md#128-portable-environment-and-cloud-folder-transport) and its [ADR register](../architecture/vcp-what.md#221-adr-register). The architecture remains the product authority. Proposed mechanisms below must be qualified at the named gate before support is advertised.

## Confirmed direction

A complete validated portable environment transfers history, context, memory, search inputs, policies and liabilities between machines. Local active data is plaintext; cloud-bound snapshots are encrypted. Handoff is sequential, with divergent descendants preserved.

## Implementation proposal

Pin canonical view and referenced artifacts/generations before packaging. Include neutral records, ancestry, deletion epochs and compatibility metadata in the encrypted inner manifest. Restore into isolated local staging, validate every reference, rebind roots/credentials/authority, then activate through a crash-tested switch.

Detailed contracts and failure ordering are in the [supporting design](../architecture/storage-portability-design.md). The [task ledger](../plan/20-traceability.md) preserves exact implementation dependencies; referencing a later integration test does not add a new task dependency.

## Alternatives and unresolved choices

Compare full and incremental immutable packages using encrypted transfer churn, peak disk and restore time. Test SQLite/files conversion through the neutral record contract. Treat a cloud sync folder as unreliable object transport, not a shared live database or distributed writer lock.

## Qualification evidence

P0-04 [prototype evidence](../evaluations/p0-04-portable-storage.md) compares full and incremental authenticated ciphertext, conversion in both backend directions, retained liabilities/deletion history and real Tantivy/DiskANN rebuild, including second-Windows restore. Select immutable encrypted artifact reuse for the next integration; replace the bounded JSON envelope and implement crash-tested activation at P5.

M08/U04 run A-to-B-to-A with two Windows environments, partial/out-of-order hydration, old indexes, corrupt or missing bytes, open liabilities and offline descendants. Restore must preserve newer deletion intent and never silently merge conflicts.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

Snapshots can retain pruned bytes until explicit retention cleanup; third-party versions may remain outside VCP control. Restored settings cannot replace recipient pins or grant new-host authority. Exact format and packaging choice remain qualified engineering work.

Update this record with the selected mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when its decision gate runs. Reopen an engineering choice when those assumptions fail; changes to confirmed product scope need an explicit owner decision.
