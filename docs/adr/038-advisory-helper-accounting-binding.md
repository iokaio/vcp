# ADR-038 — Escalation-advisory helper accounting binding

Status: selected for the third P6-03/M2 increment. This binds a claimed advisory
lease to ordinary canonical helper accounting. It does not send a request or
enable advice.

## Decision

Add immutable `vcp_escalation_advisory_accounting_v1` records. Binding requires the
exact active claimant and an unsubmitted canonical `RequestRole::Helper` attempt
whose scope, root and steering match the advisory request. The attempt must point
to a complete retained request-body artifact with the closed advisory schema and
source identity; its artifact digest must equal the admitted request digest.

The record retains attempt, reservation, artifact, quote, claimant and schedule
revision identities and references every canonical dependency. Exact rebinding is
idempotent. `accounting_attempt` revalidates the immutable binding and returns the
current canonical attempt. Submission, release, observed usage, uncertain liability
and settlement remain exclusively owned by `vcp-budget`; advisory records never
copy or reinterpret charge state.

## Consequences

A future transport adapter can use the ordinary budget send permit and settlement
path without a second ledger. Reopen sees submitted or reconciliation-pending state
from the canonical attempt. Raw response capture and the actual caller-owned send
remain the next integration increment. Verification is recorded in
[the increment evidence](../evaluations/p6-advisory-accounting.md).
