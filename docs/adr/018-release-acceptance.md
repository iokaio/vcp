# ADR-018 — Complete usable-release acceptance

Status: confirmed product direction recorded; engineering design proposed and qualification pending.
Decision gate: P8-05. No implementation, runtime result or owner sign-off is recorded here.

## Context and authority

This record expands the [architecture contract](../architecture/vcp-what.md#207-owner-acceptance-suites) and its [ADR register](../architecture/vcp-what.md#221-adr-register). The architecture remains the product authority. Proposed mechanisms below must be qualified at the named gate before support is advertised.

## Confirmed direction

All 56 first-release tasks and U01–U09 qualify the native Windows release together. A fixed-model loop, high average benchmark score or passing upstream tests cannot substitute for required features and invariants.

## Implementation proposal

Bind a release candidate to source/dirty identity, package digest, dependency/model/fixture versions and a complete evidence ledger. Predeclare quality thresholds and support environments. Run analysis, review and generation with real integrated routing, memory, MCP, skills, delegation, history and portable recovery.

Detailed contracts and failure ordering are in the [supporting design](../architecture/qualification-release-design.md). The [task ledger](../plan/20-traceability.md) preserves exact implementation dependencies; referencing a later integration test does not add a new task dependency.

## Alternatives and unresolved choices

Use deterministic contracts for exact invariants, real native fault tests for OS/durability claims, and held-out live tasks plus owner review for quality. API/editor/hooks/importers/other hosts remain excluded from first-release dependency closure.

## Qualification evidence

P8-05 consumes all U suites plus FR/I mappings, with pass/fail/not-run per required variant. Any unauthorized effect, lost acknowledged record, stale-context disclosure, hidden charge, false completion or plaintext cloud publication blocks release.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

Evidence is invalidated by relevant changes; approval belongs to the owner and must be recorded factually. Publishing requires authorization after a concrete reviewable candidate is ready. This record defines gates, not a sign-off.

Update this record with the selected mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when its decision gate runs. Reopen an engineering choice when those assumptions fail; changes to confirmed product scope need an explicit owner decision.
