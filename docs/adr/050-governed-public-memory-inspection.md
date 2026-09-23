# ADR-050: Governed public memory inspection

Date: 2026-09-23
Status: accepted for the memory inspection increment of P9-02.

## Decision

`memory/inspect` uses governed claim history, not retrieval or an adapter-owned
copy of memory. It requires negotiation of `memory/inspection-state/1` as well as
the method. The old schema-only method had no live implementation; a client that
does not negotiate the state profile receives a capability error before dispatch.
The optional `MemoryFinding.state` field follows the policy in ADR-048 and does
not change old response shapes for other methods.

The read derives its actor and authority from current authenticated connection
access. It validates the requested workspace/session/task and supplies a read-only
memory access object containing only authorized session task identities. It never
uses the host owner's unrestricted memory access. The selected task's canonical
fingerprint supplies applicability context. Governed history remains responsible
for claim visibility, source authorization, logical retention and physical purge.

Every selected version reports its memory sequence, canonical watermark,
retained/pruned/purged visibility, applicability, current-head status, retained
resolution and evidence availability. Pruned or purged payloads carry no statement,
resolution or evidence content; retained lineage is not represented as missing.
A dispute remains a dispute. Evidence references preserve their governed range
and digest. These observations convey neither authority nor proof of truth.

An explicit version filter is applied only after governed claim authorization.
Unknown versions fail rather than returning an apparently complete empty history.
Reads are bounded to 128 findings, 64 KiB text per finding and a 256 KiB encoded
page, with cooperative interruption on connection loss, a fenced worker or a
two-second deadline. Overflow fails rather than silently truncating history.
`complete` describes the selected history projection, not evidence availability.

## Verification

Compatibility tests require the profile before host dispatch and reject stale
access. Both-store governed fixtures cover accepted/disputed versions, exact
evidence ranges, scope and authority denial, interruption, logical pruning and
physical purge/reopen. Compiled observer fixtures check the same wire states and
verify that reads leave canonical history unchanged. No model, index build,
provider request or downloaded asset is needed.

Search, proposal, resolution and forgetting remain separate public adapters.
