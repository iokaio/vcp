# ADR-053: Scoped local session export

Date: 2026-09-23
Status: accepted for the export increment of P9-02.

## Decision

`session/export` requires `session/export-local/1` and the current controller.
It creates a bounded local derived artifact and a visibility manifest under the
authenticated reader's existing history/artifact disclosure ceiling. The request
chooses metadata history or metadata history with retained artifact octets. It
cannot supply a destination, upload permission or disclosure grant. An explicit
host denial of `session/export` applies before creation and replay.

The history profile is `event-metadata/1`: event identity, ordering, attribution,
causation and source references. Arbitrary event data and record payloads are
omitted. This is not a full conversational transcript. The visibility manifest
lists omissions, original capture limitations and `secret_sanitization: false`;
it never represents arbitrary artifact bytes as sanitized. This profile therefore
returns `complete: false`. Aggregate exports and forecast artifacts are excluded
as sources to avoid copying content with a different visibility boundary.

The source boundary pins the workspace/session/task set, source records and event
prefix, authority, policy and retention state. The renderer uses existing scoped
history access. Both completed descriptors and the original public command receipt
commit in one transaction. A precommit failure can leave unreferenced spool objects,
but cannot publish a canonical half-export. Replaying a command checks current
controller/disclosure and source validity before returning the same output IDs.

An export anchored to one task may describe a whole session. Every payload and
manifest read therefore checks the complete source boundary, not just the anchor
task. The ordinary audit and public artifact readers share this check. Later
source changes, retention masks, policy or authority changes invalidate old
exports; metadata, a spool schema or a prior receipt alone cannot bypass it.
Schema downgrades and missing/malformed canonical export provenance fail closed.

The collector permits at most 4,096 source events/tasks and 128 source artifacts,
with a 4 MiB source/output budget and 512 KiB raw artifact budget. Capped borrowing
serialization checks the budget before canonical JSON cloning. Oversized captures
fail rather than silently truncate. Sources that were already removed are recorded
as omitted; subsequent removal makes the original export unavailable.

Task-specific requests bind task revision and steering. Session-wide requests bind
session revision and use steering zero; the current host root is their anchor.
This adds no task execution, external publication or background-work permission.

## Verification

Native tests cover both stores, both capture modes, exact original bytes, atomic
paired outputs, receipt replay/restart, stale policy/authority/deletion, task scope,
malformed provenance, schema downgrade and oversized retained events. The compiled
fixture exercises the authenticated bridge, role/profile checks, explicit host
denial, read/replay and source invalidation without a provider.
