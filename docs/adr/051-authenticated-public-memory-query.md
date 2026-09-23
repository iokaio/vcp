# ADR-051: Authenticated public memory query

Date: 2026-09-23
Status: accepted for the memory query increment of P9-02.

## Decision

`memory/query` requires the method capability and `memory/query-sources/1`.
Its result tag is `memory_query`, with a typed source distinguishing captured
artifacts from governed claim/version identities. The original schema-only
`MemoryFinding` shape required a claim/version for every result and cannot
faithfully represent repository and history passages. No artifact receives an
invented claim identity. Method-only legacy clients fail before host dispatch;
existing request serialization and command identities are unchanged.

The public adapter performs capture, shared search and fresh finish checks using
the original authenticated actor, authority, connection and exact task scope.
The serialized canonical worker owns capture and finish. Search runs outside it
against a pinned existing lexical generation using the same resource admission
and cancellation machinery as CLI inspection. The shared runner has no wire-error
dependency. It loads no embedding model, publishes no generation and writes no
canonical state. Missing generation is an explicit degraded result, not a hidden
indexing operation.

Existing publication recovery APIs still reject task-restricted access because
they expose a whole generation view. The scoped search entry point validates its
captured principal, authority and snapshot binding, opens and pins the same fully
verified components internally, and returns only an opaque retrieval selection.
It does not construct broader access or expose the full inventory. Eligibility
and final materialization continue to use the original restricted access.

The page preserves retained record identity, source digest/span, root, governance
and evidence status, referenced evidence, trimmed text and final retrieval order.
It reports canonical and generation watermarks, indexed memory sequence, rebuild
requirements and fixed degradation codes. These are source observations, not
instructions, permissions or a relevance guarantee.

Shared retrieval explicitly records truncation at inventory, candidate, recent
overlay, result, byte or token limits. The short recent-overlay time budget can
produce a truncated result; expiry of the overall request deadline is an error.
`complete` is false when known source, generation or materialization deficits
remain. Filling an exact result limit without omitting a candidate is not itself
truncation. This reporting also benefits existing CLI inspection.

This host profile accepts at most 4,096 query bytes and 64 results, within the
older schema's broader absolute ceilings. Larger otherwise valid requests return
`RESOURCE_LIMIT`; they are never silently shortened. Materialized results have
existing retrieval byte/token bounds and a 256 KiB encoded-page ceiling. Loss of
the client, cancellation, a fenced worker or the thirty-second deadline interrupts
work and prevents stale results from being released.

## Verification

Tests exercise tagged source decoding, explicit capability gating, exact counters,
authorization, result/byte/overlay truncation and unchanged CLI inspection.
Compiled observer fixtures use real lexical-only publication and canonical source
evidence on both stores; no provider or downloaded model is required. Search
results do not replace governed claim inspection or mutation acceptance.
