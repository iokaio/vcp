# ADR-007 — Cost profiles, selection and escalation

Status: confirmed product direction recorded; engineering design proposed and qualification pending.
Decision gate: P6-02/03/04. No implementation, runtime result or owner sign-off is recorded here.

## Context and authority

This record expands the [architecture contract](../architecture/vcp-what.md#75-routing-algorithm-and-explanation) and its [ADR register](../architecture/vcp-what.md#221-adr-register). The architecture remains the product authority. Proposed mechanisms below must be qualified at the named gate before support is advertised.

## Confirmed direction

Choose models using capabilities, quality evidence, total expected task cost and profile preferences, under the same hard root budget and authority. Low favors spend/speed; high favors successful capability with visible cost.

## Implementation proposal

Filter provider/scope/capability restrictions first, then quality floor and request fit. Rank eligible candidates deterministically using recorded inputs and stable tie breaking. Include retries, handoff, support roles and protected verification in estimates. Reassemble and reserve after the final model envelope is known.

Detailed contracts and failure ordering are in the [supporting design](../architecture/routing-extensions-design.md). The [task ledger](../plan/20-traceability.md) preserves exact implementation dependencies; referencing a later integration test does not add a new task dependency.

## Alternatives and unresolved choices

Start with explicit rules and measured per-task-class outcomes. Compare routed behavior with fixed economical and fixed stronger baselines. Do not select unexplained learned routing or import research ranks as production defaults.

## Qualification evidence

E19/U07 and P6-04 use held-out matched tasks after P5-08 memory qualification. Report failures, intervention, all supporting costs and uncertain charges. Bound escalation count/deadline and test strict pins, smaller contexts and late prior responses.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

A cheap request can increase total task cost. Sparse evidence limits recommendations. Thresholds, exact pools, dollar defaults and escalation limits remain to be measured and selected before release.

Update this record with the selected mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when its decision gate runs. Reopen an engineering choice when those assumptions fail; changes to confirmed product scope need an explicit owner decision.
