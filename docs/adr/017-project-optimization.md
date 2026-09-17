# ADR-017 — Interactive project optimization

Status: confirmed product direction recorded; engineering design proposed and qualification pending.
Decision gate: P6-05/04, U07. No implementation, runtime result or owner sign-off is recorded here.

## Context and authority

This record expands the [architecture contract](../architecture/vcp-what.md#78-interactive-project-optimization) and its [ADR register](../architecture/vcp-what.md#221-adr-register). The architecture remains the product authority. Proposed mechanisms below must be qualified at the named gate before support is advertised.

## Confirmed direction

Optimize routing using scoped project evidence and developer answers. Propose a versioned policy diff, apply only selected changes and support rollback. Do not silently increase budgets, grants or pruning scope.

## Implementation proposal

Aggregate task classes, success/failure, retries, interventions, all supporting costs, latency and retrieval contribution over an explicit retained window. Label sample and price uncertainty. Persist questions and selected policy fields with baseline revision; reject stale apply and validate trusted ceilings.

Detailed contracts and failure ordering are in the [supporting design](../architecture/routing-extensions-design.md). The [task ledger](../plan/20-traceability.md) preserves exact implementation dependencies; referencing a later integration test does not add a new task dependency.

## Alternatives and unresolved choices

Start with local deterministic analysis and a short adaptive interview. Optional model analysis uses a separately admitted optimization task. Controlled trials require their own authorized cap; production correlations alone do not prove causal improvement.

## Qualification evidence

U07/E19 cover empty/biased/pruned history, unresolved charges, malicious recommendations, declined changes, concurrent policy edits, interrupted apply and repeated rollback. Later observations report regression against the recorded baseline.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

Small samples limit specificity; keep current policy where evidence is insufficient. Optimization versions project policy, not model weights. New policy fields require migration and visibility in effective-setting inspection.

Update this record with the selected mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when its decision gate runs. Reopen an engineering choice when those assumptions fail; changes to confirmed product scope need an explicit owner decision.
