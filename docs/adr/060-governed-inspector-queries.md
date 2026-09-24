# ADR-060: Governed inspector query pages

Date: 2026-09-23
Status: accepted query prerequisite under P4-04; full inspector acceptance remains open.

## Decision

Inspector history and memory navigation use the existing governed query services.
The public boundary projects bounded, typed pages and rechecks current authority,
scope and retention for every continuation. A cursor freezes an ordering boundary,
not a permission grant. A changed access or retention boundary requires an explicit
restart instead of continuing from cached content.

`history/query` exposes sanitized event metadata, artifact availability and derived
claim links. It never serializes internal event facts, raw operation arguments or
captured artifact bytes into history rows. An authenticated session is the ceiling;
an optional task or selector narrows it. Full content remains available through
existing authorized artifact-range reads. Search coverage and truncation are
explicit, and unsupported selectors cannot silently broaden a query.

`memory/history` pages immutable versions of a claim through the same source access
and retention checks used by CLI inspection. Version summaries are bounded, and
current applicability, resolution, evidence availability and retained/purged state
remain distinct. This adds continuation beyond the existing `memory/inspect`
single-response limit without changing that method's contract.

The independent `history/query/1` and `memory/history/1` profiles require actual
hosted methods and explicit negotiation. Both methods are observational: they do
not acquire control, resume tasks, invoke providers or persist new history. Exact
counters remain decimal strings on the wire. Public cursors bind the requesting
actor, scope, semantic query and underlying ordering/access/retention evidence.

History event boundaries remain stable while newly appended events are reported
separately. Claim links and evidence availability are observed under current
governance. Memory pages identify both their stable memory-sequence bound and
current canonical watermark. Neither page implies that a historical grant remains
effective now.

## Qualification and remaining scope

Compare public projections against the existing CLI/governed query semantics on
both stores. Exercise more than one page, append while paging, revoke access,
purge between requests, foreign scope, hostile metadata, exact large counters and
bounded output. Inspection must leave the canonical watermark unchanged.

These APIs are P4-04 prerequisites. Policy/optimization inspectors, revision-bound
optimizer actions, pruning presentation and actual webview invalidation/reload
remain separate acceptance conditions. Local export is not encrypted cloud
publication; a later export action must use the existing encrypted publisher.

See [the inspector contract](../development/editor-inspectors.md) and
[P4-04](../plan/18-deferred-vscode.md#p4-04--inspectors).
