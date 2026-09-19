# ADR-005 — Autonomy, authority and OS isolation

Status: P2 authority rules and native broker acceptance recorded; general OS isolation remains unqualified.
Decision gates: P2-03 complete for the implemented authority boundary; P8-01 remains open for broader Windows sandbox enforcement.

## Context and authority

This record expands the [architecture contract](../architecture/vcp-what.md#10-permissions-and-isolation) and its [ADR register](../architecture/vcp-what.md#221-adr-register). The architecture remains the product authority. Proposed mechanisms below must be qualified at the named gate before support is advertised.

## Confirmed direction

Autonomy, spending, interactivity and OS isolation are independent controls. Trusted denials constrain grants; project text, model output, memory and extensions cannot grant authority.

## Implemented authority behavior

Evaluate one normalized prepared operation against actor/workspace scope, trusted ceilings, valid grants and any bound user decision. Record rule origins and effective scope. Re-evaluate at dispatch after policy, arguments, schema, paths or steering change. Headless operation returns durable input-required state when necessary.

Detailed contracts and failure ordering are in the [supporting design](../architecture/engine-execution-design.md). The [task ledger](../plan/20-traceability.md) preserves exact implementation dependencies; referencing a later integration test does not add a new task dependency.

## Alternatives and unresolved choices

The P2 authority implementation uses plan/ask/workspace/autonomous, with workspace
as the library default. The [source guide](../development/p2-policy.md) specifies
effect classes, denial precedence and grant matching. The
[acceptance assessment](../evaluations/p2-policy-completion.md) records the preset
matrix and current native broker evidence. P3 owns installed CLI usability;
P8-01 qualifies general filesystem/network enforcement.

## Qualification evidence

The [authority core increment](../evaluations/p2-policy-increment.md) records pure
operation/preset/grant decisions and canonical question/answer persistence on
both stores. Pending questions bind the owner and authority epochs; answering
does not resume a waiting/paused task. Later increments below qualify native
dispatch; general OS isolation remains a separate gate. No model risk classifier
is used. Configured commands
match exact invocation identity rather than interpreting shell prefixes as grants.

The [native process increment](../evaluations/p2-process-increment.md) exercises
actual dispatch denial, exact-grant reuse and explicitly selected reduced
isolation. Required but unavailable filesystem/network controls still deny
dispatch. Autonomous fixture policy explicitly covers each executable root and
opaque effect; this is not permission inferred from project text or a new
automatic-execution default.

Later retained coding, verification and authority-coordination increments route
model-requested operations through those prepared brokers, recheck current
sources/authority, and stop owned work before active authority changes. The
acceptance assessment separates pure identity-mutation contracts, shared JSONL
waiting-state receipts and independently observed native effects. Current-source
qualification for that assessment passed without changing the authority rules.

P0-05 [native evidence](../evaluations/p0-03-recovery-execution.md) selects Job Objects for process ownership and a zero-capability AppContainer as a single-process isolation candidate. Live unrestricted controls and independent file/network observers pass. General toolchains, authority presets and composed containment remain P2/P8.

E06/R03 use grant reuse, deny precedence, shell redirection, opaque commands, expiry and stale responses. E07/R04 must observe blocked outside-root access and process/network behavior on real supported hosts, independently of the matcher decision.

Attach exact source/package, fixture, configuration and environment identities, actual commands and pass/fail/not-run outcomes. No linked plan or ADR is itself passing evidence.

## Consequences and reconsideration

Policy authorization is not an OS sandbox guarantee. Unsupported enforcement is reported and must not silently run under a weaker claimed mode. Reconsider presets with owner feedback and evidence without weakening trusted denials.

Update the OS-isolation selection with its mechanism, rejected alternatives, measured operational burden, compatibility/migration implications and evidence when P8-01 runs. Reopen an engineering choice when its assumptions fail; changes to confirmed product scope need an explicit owner decision.
