# ADR-048: Capability-gated result fields

Date: 2026-09-23
Status: accepted for the pending-input increment of P9-02.

## Decision

An optional presentation extension may retain protocol version 1.0 only when its
new fields are absent from responses to clients that did not negotiate it. Strict
older decoders must continue to accept their original result shape. This does not
permit unknown request fields, authority semantics or enum variants.

The first extension is `approval/source-revisions/1`. A host may advertise it only
when both `task/read` and `approval/respond` are implemented. Negotiated task views,
including tasks within `session/snapshot`, include the pending approval's observed
`effect_revision` and `policy_revision` as decimal-string counters. These fields
are optional in the generated schema and TypeScript. RPC serialization omits both
for clients without the capability; a client that needs them should require it
during initialization. Existing method names, command digests and versions remain
unchanged. Baseline server configurations remain valid.

The counters describe the approval's recorded sources, not a grant or assertion
that it remains actionable. Answering still checks current authorization, the
connection's controller lease, the engine owner, operation digest and all source
and steering revisions. Reconnecting does not acquire ownership, answer an input
or resume work. An approval from an earlier engine process does not become valid
merely because a new client can inspect it.

## Rationale and verification

The original pending-input projection omitted two counters required by
`approval/respond`. Guessing counters or opening the private store cannot be a
public client workflow. Adding them unconditionally would break strict v1.0
decoders; silently tolerating all future fields would weaken the existing contract.

Tests exercise actual initialized RPC serialization for task and snapshot results,
strict legacy decoding, counters beyond JavaScript's exact integer range and
unavailable required capabilities. Compiled Windows pipe qualification creates a
real pending patch approval, disconnects its controller, and reconnects to the
same server. It checks observer and missing-lease denial, stale source rejection,
explicit reacquisition and same-command receipt replay without executing the patch
or resuming the paused task. Both canonical stores use the same scenario.

This closes the missing projection contract, not the remaining P9-02 acceptance
or future SDK qualification.
