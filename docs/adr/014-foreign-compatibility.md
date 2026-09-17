# ADR-014 — Explicit foreign compatibility subsets

Status: confirmed product direction recorded; engineering design proposed and qualification pending.
Decision gate: Later P9 and P10-02. No implementation, runtime result or owner sign-off is recorded here.

## Context and authority

This record expands the [architecture contract](../architecture/vcp-what.md#155-gemini-derived-hooks-and-explicit-compatibility) and its [ADR register](../architecture/vcp-what.md#221-adr-register). The architecture remains the product authority. Proposed mechanisms below must be qualified at the named gate before support is advertised.

## Confirmed direction

Borrowing code does not promise drop-in protocol, session, prompt or configuration compatibility. Importers are explicit, versioned and deferred; imported settings cannot grant authority.

## Implementation proposal

Map each supported source field to a typed VCP preference with source/version provenance and a preview diff. Parse commands as data. Report unsupported fields, unknown versions and missing credential references. Apply chosen changes atomically with a retained prior revision and rollback.

Detailed contracts and failure ordering are in the [supporting design](../architecture/deferred-clients-design.md). The [task ledger](../plan/20-traceability.md) preserves exact implementation dependencies; referencing a later integration test does not add a new task dependency.

## Alternatives and unresolved choices

Prefer narrow documented mappings to permissive heuristic import. Native VCP configuration remains authoritative. Credential values, trust grants and executable hooks need distinct handling and cannot be quietly imported with ordinary preferences.

## Qualification evidence

P10-02/R06 fixtures cover golden supported subsets, future fields, path escapes, conflicting identities, interrupted apply and secret-free previews. P9 separately proves public wire compatibility; internal reuse is not that evidence.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

Partial support may surprise users, so disclose unsupported semantics at preview and in version matrices. Additional subsets require explicit tests and source provenance; none block the first CLI release.

Update this record with the selected mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when its decision gate runs. Reopen an engineering choice when those assumptions fail; changes to confirmed product scope need an explicit owner decision.
