# P6-03 / M2 — Canonical escalation shadow execution

This increment connects the existing decision transport to canonical escalation
requests, schedules, helper accounting and results. It changes no chosen action.

## Runtime contract

The owner retains an admitted escalation plan with its verified context seed.
An installed escalation-purpose evaluator must have advisory qualification and
the exact escalation question revision. The host remains in shadow mode. Disabled,
unqualified or unsupported inputs preserve the deterministic baseline without
admitting a helper.

The initial evidence mapping supports invalid tool output and failed verification
from retained canonical call/result pairs. Each complete observation is bounded to
4096 bytes, with at most 64 observations and the existing total request limit.
Oversized or unavailable evidence abstains; no truncation invents missing context.
Scope, pairing, steering, artifact identity and failed verification records are
checked through authorized history reads. Other trigger mappings remain unavailable.
Review-required, hard-failure and incomplete-check flags are conservative shadow
guardrails, not claims that a review was ordered or checks succeeded.

Only the already selected action and stop enter the closed answer space. Preparing
a request cannot grant permission to change models, retry, skip review or complete
the task. The finite capability still qualifies the exact operation, evaluator,
price, input/output bounds and credential recipient. Native production finite
charge qualification remains unavailable; fixture qualification is not production
authorization.

A single durable run key per admitted main attempt bounds comparison admission
even when a caller regenerates a deadline or changes evaluator purpose. The
request, claim and helper binding are retained before submission. The ordinary
budget service supplies the one-shot send permit and owns all charge state.
Failure after reservation releases an unsent attempt or retains submitted
uncertainty. Cancellation closes the claim. Reopen never resubmits it.

The asynchronous caller uses the existing model-work permit and transport. Source,
credential, qualification, deadline and claim are rechecked before application
writes. Current responses complete the schedule; stale or failed responses remain
historical evidence and close it. Sanitized response capture and observed usage
settlement remain independent of whether advice can be accepted. The CLI drives
the installed decision purpose without awaiting network work on its command pump.
Request, response and receipt artifacts carry explicit dependency references to
the canonical request, whose references include original evidence artifacts.
Retention selection of an observation therefore includes its derived copies even
when the provider returned no known usage.
The [typed advisory-redaction prerequisite](../adr/039-advisory-retention-redaction.md)
makes these projections part of retention closure and prevents purged content from
being restored by an ordinary write.
Advisory reads honor logical purge before physical cleanup, including after reopen.
The automatic input builder also rejects evidence excluded from recall.

## Verification

- Final escalation runtime tests: 4 passed, covering both stores and evaluator
  protocols, pause/interruption, qualification rejection, late responses and purge.
- Existing routing shadow regression tests: 11 passed in regression runs.
- Routing-state tests: 27 passed; the process-kill supervisor's child entry remains
  intentionally ignored when run directly.
- Bounded evidence-builder unit test: 1 passed.
- Lifecycle and CLI qualification builds passed. Focused Rust formatting and
  `git diff --check` passed.
- Fast repository suite: 9 passed, run
  `5f4962b2-8ae3-4bce-814b-303e465e2bf2`.

These are local fixture checks; no paid or live provider qualification was run.

## Remaining

P6-03/M2 still requires exact-cycle detection and the separately identified local
statistical shadow producer. Live outcome qualification and behavioral influence
remain P6-04. Neither this transport fixture nor a remote qualification qualifies
the local producer.
