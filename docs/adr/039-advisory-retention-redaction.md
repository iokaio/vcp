# ADR-039 — Escalation-advisory retention and typed redaction

Status: selected for the P6-03/M2 runtime integration prerequisite.

## Context

[Advisory records](036-canonical-escalation-advisory-records.md) contain bounded
observation text. Retention originally enumerated task, artifact, accounting and
memory records but did not traverse advisory projections. Adding artifact
dependency references therefore did not by itself remove derived advisory copies
when their source evidence was selected for purge.

## Decision

Recognize the four existing advisory request, result, schedule and accounting
document types in retention selection and dependency closure. Support their
explicit typed redaction through the canonical store's existing purge/rewrite
boundary. Preserve identity, scope, original document type, revision, deletion
epoch and original-content digest in a `vcp_escalation_redacted_advisory_v1`
tombstone. Preserve dependency references, erase the payload, and reject ordinary
writes that would restore a redacted record.

This is not generic deletion of arbitrary projections. Operational routing
policies, budget state and unknown document types retain their existing contracts.
Active-task and unsettled-liability protection remains authoritative before purge.

The runtime must link each advisory request projection to its source artifacts and
each copied request, response and receipt artifact back to that projection. The
reverse dependency closure then includes all derived copies. Advisory readers
reject the typed tombstone rather than presenting purged advice as current data.

## Consequences

The lower-level store owns durable redaction and replay validation; lifecycle
readers do not rewrite persisted records during a read. Reopen preserves the
tombstones and cannot resurrect content through an ordinary `Put`. Retained charge
identity remains subject to the existing accounting/redaction contract.

## Verification

The focused `vcp-store` tests `redacted_advisory` (two cases across Files and
SQLite) and existing `redacted_receipts` (one case) pass. They cover all four
document types, reference/digest preservation, protected active recovery, ordinary
write rejection, malformed/future-epoch tombstones, rewrite validation and reopen.
