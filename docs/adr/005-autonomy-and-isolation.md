# ADR-005 — Autonomy, authority and OS isolation

Status: confirmed product direction recorded; engineering design proposed and qualification pending.
Decision gate: P2-03, P8-01. No implementation, runtime result or owner sign-off is recorded here.

## Context and authority

This record expands the [architecture contract](../architecture/vcp-what.md#10-permissions-and-isolation) and its [ADR register](../architecture/vcp-what.md#221-adr-register). The architecture remains the product authority. Proposed mechanisms below must be qualified at the named gate before support is advertised.

## Confirmed direction

Autonomy, spending, interactivity and OS isolation are independent controls. Trusted denials constrain grants; project text, model output, memory and extensions cannot grant authority.

## Implementation proposal

Evaluate one normalized prepared operation against actor/workspace scope, trusted ceilings, valid grants and any bound user decision. Record rule origins and effective scope. Re-evaluate at dispatch after policy, arguments, schema, paths or steering change. Headless operation returns durable input-required state when necessary.

Detailed contracts and failure ordering are in the [supporting design](../architecture/engine-execution-design.md). The [task ledger](../plan/20-traceability.md) preserves exact implementation dependencies; referencing a later integration test does not add a new task dependency.

## Alternatives and unresolved choices

Plan/ask/workspace/autonomous are candidate preset names, not accepted defaults. P2-03 must specify each effect class, default and escalation behavior; P8-01 qualifies which filesystem/network restrictions the chosen Windows mechanism enforces.

## Qualification evidence

E06/R03 use grant reuse, deny precedence, shell redirection, opaque commands, expiry and stale responses. E07/R04 must observe blocked outside-root access and process/network behavior on real supported hosts, independently of the matcher decision.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

Policy authorization is not an OS sandbox guarantee. Unsupported enforcement is reported and must not silently run under a weaker claimed mode. Reconsider presets with owner feedback and evidence without weakening trusted denials.

Update this record with the selected mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when its decision gate runs. Reopen an engineering choice when those assumptions fail; changes to confirmed product scope need an explicit owner decision.
