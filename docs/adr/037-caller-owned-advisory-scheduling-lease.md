# ADR-037 — Caller-owned escalation-advisory scheduling lease

Status: selected for the second P6-03/M2 increment. This bounds scheduling and
recovery around canonical advisory records. It performs no transport, creates no
provider attempt and consumes no advice.

## Evidence and problem

ADR-036 provides immutable request/result identity, but persistence alone does not
prevent two callers from sending the same prepared request. A background scheduler
would also compete with the canonical owner during pause or steering. M2 requires
caller-owned work, a second current-state check before send and reopen behavior
that never guesses whether an interrupted remote request was submitted.

## Decision

Add one mutable `vcp_escalation_advisory_schedule_v1` projection per canonical
request. Its closed states are `pending`, `claimed`, `cancelled` and `completed`.
The schedule binds the request/task/digests/deadline and advances with revision-CAS
updates.

Scheduling requires the exact current binding, a running task and an unexpired
request. A pending lease can be claimed once. Callers may dispatch only after a
new claim and `revalidate_dispatch` returns that same active claim. The recheck
atomically cancels for pause, another non-running state, changed input or deadline.
Only the exact claimant can mark caller cancellation/interruption.

Reopening a claimed lease returns `Existing`; it never grants another send, even
to the same claimant. Recovery must settle or interrupt the claim and create a new
canonical request if policy permits another attempt. Completion requires the
matching immutable result with `accepted_current` disposition. Historical results
cannot complete a schedule.

## Consequences

The canonical owner can drive async work without a competing background loop, and
the lease supplies deterministic deduplication and recovery state. Pause and stale
input close the send path before transport. An interrupted claim is visible and
cannot silently replay.

The subsequent [accounting binding](038-advisory-helper-accounting-binding.md)
connects a claim to an ordinary helper reservation and its live charge state.
Caller-owned transport and raw response capture remain open. The lease itself
never implies a model call or charge. Verification is
recorded in the [increment evidence](../evaluations/p6-advisory-scheduling.md).
