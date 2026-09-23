# ADR-052: Explicit public memory review

Date: 2026-09-23
Status: accepted for the manual governance increment of P9-02.

## Decision

Public memory proposal and resolution use explicit typed candidates under the
negotiated `memory/governance/1` profile. The original prose-only parameter shape
remains decodable with identical command serialization, but this host rejects it:
prose and a list of artifacts do not determine a typed governed claim. The caller
supplies candidate meaning and expected revisions. The authenticated host supplies
actor, scope, registry, extractor and current authority. Existing automatic
ingestion continues to evaluate proposals directly.

A manual submission is a distinct immutable record awaiting review. It creates
no claim version, recall entry or indexing intent. `memory/review` exposes the
retained candidate and any subsequent decision to currently authorized readers.
The write result separates durable command acceptance from review disposition;
pending candidates receive no invented version identity.

Exactly one immutable decision is permitted per scoped submission. Reject records
the decision without creating a version. Accept evaluates the existing governance
gates against current evidence and claim heads; the result can remain disputed or
rejected. An existing immutable proposal result is never rewritten to change its
outcome. The decision, ordinary governed proposal/result, any version/head/index
intent and original public command receipt commit in one canonical transaction.

Fresh writes check task revision, steering, policy, deletion, authority and expected
claim head. Missing canonical projections fail closed instead of silently changing
conflict evaluation. Retries check current access and retained source availability,
then reconcile the original command digest before fresh-write preconditions. A
different payload cannot reuse the same command. A read or reconnect cannot acquire
controller authority or answer the pending review.

Review records participate in source-driven retention closure. Purge replaces
candidate text, reasons and resolution findings with exact content-free identity
and lineage records, through the canonical rewrite path only. Ordinary record
writes cannot forge those redactions. Masked or purged review content is unavailable
through reads and command replay.

This implements the explicitly configured review path described by the memory
design; it does not add routine approval to automatic ingestion. Session export,
forgetting and editor operations retain their own adapters and acceptance evidence.

## Verification

Qualification covers immutable linkage and atomic receipt validation, both storage
backends, reopen/retry, changed payloads and stale guards, accepted/disputed/rejected
governance outcomes, source masking and physical purge. Public tests exercise
capability negotiation, controller versus observer access and typed result states.
