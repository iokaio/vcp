# ADR-006 — Model gateway, catalog and capability groups

Status: confirmed product direction recorded; engineering design proposed and qualification pending.
Decision gate: P2-02, P6-01. No implementation, runtime result or owner sign-off is recorded here.

## Context and authority

This record expands the [architecture contract](../architecture/vcp-what.md#7-openrouter-and-model-strategy) and its [ADR register](../architecture/vcp-what.md#221-adr-register). The architecture remains the product authority. Proposed mechanisms below must be qualified at the named gate before support is advertised.

## Confirmed direction

Coding and model-assisted work use OpenRouter. Frontier/High/Medium/Low describe versioned capability groups; low/med/high are separate user policies. Local memory embeddings never use a remote fallback.

## Implementation proposal

Normalize requests, streamed content/tool fragments, served model/provider, errors and usage behind the gateway. Every attempt receives an atomic reservation for the actual request. Keep requested and served identities separate; unknown usage and price are explicit. Pin catalog/compatibility revisions to the attempt.

Detailed contracts and failure ordering are in the [supporting design](../architecture/context-provider-design.md). The [task ledger](../plan/20-traceability.md) preserves exact implementation dependencies; referencing a later integration test does not add a new task dependency.

## Alternatives and unresolved choices

Use current primary provider documentation and captured fixtures during implementation; research tables nominate candidates only. Qualify required roles, tool schemas, context/output limits, fallback and provider-data constraints per model/provider combination.

## Qualification evidence

E11/E12/R05 exercise truncated streams, malformed fragments, duplicate terminal events, rate limits, cancellation and ambiguous sends. A separately capped live smoke run confirms advertised compatibility; mocks prove control flow only.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

Catalogs and prices change. Refresh creates new revisions without altering in-flight decisions. Hidden SDK retry/helper paths violate accounting and must be disabled or routed through new admitted attempts.

Update this record with the selected mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when its decision gate runs. Reopen an engineering choice when those assumptions fail; changes to confirmed product scope need an explicit owner decision.
