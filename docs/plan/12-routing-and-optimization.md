# 12 — Model groups, cost profiles and project optimization

Status: planned. Owns P6-01 through P6-05. Registry work follows P2-02 and P5-06; final profile qualification also needs P5-08. Architecture sections 7–8 govern routing. [Model research](../architecture/model-groups.md) supplies candidates, not shipping ranks/prices.

## Code organization

Use `vcp-models/catalog` for gateway metadata and `vcp-routing/group_registry`, `eligibility`, `policy`, `selector`, `explanation`, `escalation`, `handoff`, `optimizer` and `evaluation` for VCP decisions. CLI interview/policy-diff presentation belongs in `vcp-cli/optimize`; policy revisions and outcome observations use the canonical store.

`RoutingInput` binds task/role, capability/context needs, project policy, model pin/fallback permission, catalog/evaluation versions and remaining budget. `RoutingDecision` records all candidates, exclusions, selected group/model/provider, estimated cost, evidence and escalation reason. Final reservation uses the actual assembled request size.

## P6-01 — Versioned registry

1. Model Frontier/High/Medium/Low as capability groups, separately from user profiles low/med/high. Group membership carries evidence date/version and role suitability.
2. Import research candidates only after current model/provider availability and parameter support checks. Preserve user exclusions and pins; unknown price/context/tool support cannot silently become eligible.
3. Store compatibility observations, quality data, usage/latency distributions and source provenance. A catalog refresh creates a new revision without changing an active request.

Test removed/renamed candidate, stale price, unknown capability, contradictory research membership and provider restriction. Persist an inspectable rejection reason for every ineligible candidate.

## P6-02 — Deterministic profile policy

Implement an explainable selection pipeline: required capability/scope/provider filtering, measured quality floor, total estimated task cost and profile-dependent latency/capability preference. Low emphasizes fast inexpensive work with bounded escalation; high prioritizes capable successful completion under the same hard spending policy. Profiles need not map to one group.

Include context transfer, retries, support roles and reserved verification in estimates. Select inexpensive eligible support roles even when the main model is stronger. Use deterministic tie-breaking from recorded inputs; keep unexplained learned routing deferred until sufficient evidence exists.

Tests cover a cheap candidate that fails the quality floor, a fast candidate with expensive retry history, insufficient verification reserve, a strict pin and concurrent root allocations. Compare decisions against explicit eligibility/ordering assertions, not a duplicated implementation function.

## P6-03 — Escalation and model handoff

Define bounded triggers from invalid tool output, repeated failed verification, unsupported capability or declared task complexity. Record trigger/evidence and remaining budget. Reassemble for the new capability envelope and preserve objective, current changes, applicable instructions, unknown effects and tool/result pairing.

No escalation widens autonomy or provider-data permissions. An ambiguous request remains accounted for while a later attempt uses its own reservation. Repeated failure reaches a visible blocked/failed/input state rather than cycling indefinitely.

Test smaller-context fallback, tool-schema change, repeated failures, paused escalation and late response from the prior model. E04/E11/E12/U07 apply.

## P6-05 — `/optimize` workflow

1. Read a scoped history window and calculate task classes, languages, failures/abandonment, retries, intervention, token/effort, cost certainty, latency, child overhead and retrieval contribution. Mark missing/pruned/small samples.
2. Build a baseline report with evidence links and uncertainty. Avoid presenting correlations from unlike tasks/providers as causal improvement.
3. Ask a small adaptive interview about spend/speed/quality, expected task size, review preferences and model restrictions. Reuse current project answers; no complete questionnaire on every invocation.
4. Produce a versioned policy diff for groups/roles, effort/output, context/retrieval, escalation and concurrency. Explain intended effect and evidence. Keep pruning suggestions separate from executable deletion.
5. Apply only the developer's selected changes within trusted ceilings, retain the prior revision and implement rollback. No silent budget increase, grant change or extra paid trials.
6. Optional model assistance uses a separate admitted/attributed optimization task; local analysis and questions remain available without it. Observe later tasks and report regression without claiming retraining.

Tests: empty history, only failed tasks, pruned comparison period, unknown charges, an attempted budget/grant escalation in a model suggestion, declined changes, interrupted apply and repeated rollback. Confirm exact old/new effective policy revisions and no deletion or unrequested model call.

## P6-04 — Profile qualification

After P5-08 memory evidence is available, compare fixed economical, fixed stronger and routed strategies on the same versioned analysis/review/generation pool. Record all attempts and supporting costs, held-out task outcomes, wall-time distributions, interventions and quality failures. Pin catalog/provider/configuration versions for each run.

Select thresholds and defaults from declared evidence and product priorities, not the research workbook's ranking or an illustrative dollar amount. Initial subsystem tests can script child events/accounting; P8 must recheck total costs and quality with the actual delegation implementation before release.

Run `routing`, `provider`, E19/U07 and budget/handoff contracts. Done when every decision is explainable/reproducible, optimizer changes are reversible and explicitly chosen, and the proposed profile defaults have a measured quality/cost/latency basis.
