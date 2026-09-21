# ADR-036 — Canonical escalation-advisory request and result records

Status: selected for the first P6-03/M2 implementation increment. This records
qualified bounded advisory inputs and decoded outputs. It does not schedule a
remote request, enable advice, or qualify a provider.

## Evidence and problem

The escalation advisory codec already binds closed questions to task, steering,
authority, deletion, policy, catalog, input and evidence revisions. The existing
remote routing comparison persists transport artifacts inside its routing-only
worker path. P6-03 needs a purpose-specific canonical seam before adding async
scheduling or a separate local statistical producer. Without it, retries can
duplicate a helper request and a response arriving after steering can look current.

## Decision

Add `routing_state::advisory` with immutable
`vcp_escalation_advisory_request_v1` and
`vcp_escalation_advisory_result_v1` projections.

`record_request` accepts only a prepared `Purpose::Escalation` advisory. It
revalidates the supplied current binding against the canonical workspace and task,
including scope, root, task revision, steering, authority and deletion epoch. The
record retains the bounded request, exact qualified evaluator, canonical request
digest and prepared transport commitment. Its deterministic ID includes workspace
and transport commitment. Exact retries return the existing record without a new
transaction; conflicting identity reuse fails.

`record_result` associates one decoded outcome with that request. An advice outcome
must reproduce the original binding, purpose, question revision, evaluator, mode
and transport commitment. The result records a canonical outcome digest and one of
two closed dispositions:

- `accepted_current` means the caller's freshly validated binding still equals the
  request binding and the request deadline has not passed;
- `historical_stale` retains an otherwise valid late outcome after the input changed
  or its deadline passed.

Historical results are evidence only. This module exposes no consumer that can
apply them. Request and result records reference their task; results also reference
their request projection. Reads enforce task scope and validate record digests.

## Consequences

Caller-owned scheduling can now deduplicate before transport and classify a late
decoded response without discarding audit evidence. Files and SQLite reopen the
same immutable records. A task-scoped reader cannot inspect another task's advice.

The subsequent [caller-owned scheduling lease](037-caller-owned-advisory-scheduling-lease.md)
adds a single claim, pause/stale/deadline cancellation and interruption-safe reopen.
Ordinary reservation/unknown-charge settlement still precedes exact-cycle evidence
and the local shadow producer. Verification is recorded in the
[increment evidence](../evaluations/p6-advisory-records.md).
