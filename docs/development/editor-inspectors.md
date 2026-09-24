# Editor inspectors

Work item: [P4-04](../plan/18-deferred-vscode.md#p4-04--inspectors).
Status: history/memory and policy/routing query prerequisites accepted.
P4-04 acceptance remains open.
[ADR-060](../adr/060-governed-inspector-queries.md) records the query boundary.

## History and memory query increment

`history/query` negotiates `history/query/1`. Requests carry the authenticated
workspace/session scope, optional task and governed selector, optional text or
artifact backlink filter, bounded page limit and continuation. Responses contain
event identity/time/type, scope, sanitized metadata, artifact availability, retained
gaps and derived claim links. Internal event facts and artifact contents are not
part of these rows. A null task selects the current session; it grants no other
session's history.

`memory/history` negotiates `memory/history/1`. It takes a current task context,
claim identity, limit from 1 through 32 and optional cursor. Each version carries
a bounded Unicode-safe summary, explicit truncation, current inspection/evidence
state and retention presentation flags. The page distinguishes a stable memory
sequence window from the current canonical watermark. Full evidence uses existing
authorized range reads.
The version's `current` flag identifies the selected accepted head within that
stable `at` window. Restart the query to include later versions; the observed
canonical watermark alone does not make that historical head the latest version.

Every page revalidates access. Cursor mismatch or changed retention requires
restart; a retained cursor cannot recover purged text. Query failures and missing,
purged or restricted evidence are explicit states, never fabricated empty success.
Read-only queries start no provider work, capture no report and make no policy
change. The host retains no persistent inspector transcript.

## Qualification

Targeted native checks passed on Files and SQLite: session-scoped history,
taskless events, foreign artifact and redacted task exclusion, bounded large
artifact lists, actor/query/access cursor changes and unchanged canonical state.
Memory checks passed with 130 large Unicode versions, stable paging across
appends, exact continuation after byte limits, CLI parity, scoped origins,
retention changes and reopen. Masked versions contain no summary or evidence
links. Unknown claims return an explicit unavailable error.

The native schema exporter regenerated the wire types. All nine generator
contracts, 33 SDK tests and 108 extension regression tests passed. Native
regressions passed: protocol 25, engine 73, audit history 11 and memory 13.
The 18-case fast delivery run passed (`395f501e-d078-4b81-8486-540f9c7f21bd`).
Compiled-host SDK qualification passed on Files and SQLite (17.10 seconds):
37 history rows, 33 memory versions over five pages, exact counters beyond
JavaScript's safe integer range, method/profile negotiation, foreign scope and
cursor rejection, and semantic comparison against the CLI's governed services.
The entire canonical state remains unchanged after observer disposal and reopen.
The fixture is `vcp-cli/tests/local_inspector_queries.rs`; its driver is
`sdk-ts/tests/native-inspector-queries.mjs`.

The initial debug-host fixture with 130 correction versions exceeded the SDK's
10-second bootstrap deadline before its first request. The same transport test
passes with 33 versions; the 130-version paging and retention cases remain
qualified at the lifecycle boundary. The production deadline is unchanged.
Large-history startup through the packaged engine remains a P4-05 qualification
requirement; these query checks do not establish that startup performance.

History pages are at most 64 KiB, with at most 128 artifact links per event and
bounded metadata. A multirow request exceeding the byte cap returns an explicit
resource-limit error; restart with a smaller limit. Truncation never implies that
omitted links or metadata are absent from the canonical history.

## Policy and routing query increment

`policy/read` negotiates `policy/inspection/1`. It separates stored policy,
observed task-effective constraints and exact task or authorized inherited grant
provenance. Shared grant details remain restricted. Expiry, revocation and
revision matches are facts, never an operation admission decision. Missing live
binding evidence is explicit unavailability; inspection cannot reconstruct it.

`routing/status` negotiates `routing/status/1`. Stored policy is distinct from
the effective policy under this host's configured ceilings. Catalog observations
require current source scope and retention. A `retained_metadata_only` source
reference attests metadata, not the existence or integrity of retained bytes;
`artifact/read` performs its existing verification before returning content.

Both methods use at most 32 rows and 64 KiB per page. Continuations bind current
scope, authority and relevant policy/configuration evidence. Neither route calls
owner controls, captures a report, invokes a provider or exposes raw invocations,
catalog source bytes, interview answers or credentials. See
[ADR-061](../adr/061-policy-routing-inspection.md).

Qualification passed: protocol 26 plus the routing wire integration test,
engine 73, schema generation 9, SDK 34, extension 108 and fast delivery 18 passed.
Policy and routing tests passed on both stores: exact inherited revision pins,
foreign/shared grant exclusion, expiry/revocation, 40-candidate catalog paging,
host ceiling changes and source retention between pages. Scoped rows preserve
provenance without invocation payloads or raw catalog bytes.

Compiled-host SDK qualification passed on Files and SQLite in 23.80 seconds:
each backend returned two denials, two exact task grants and two catalog
candidates over bounded pages, alongside 43 history rows and 33 memory versions.
The shared workspace grant was excluded, unavailable effective host policy was
explicit, and exact counters, scope/cursor rejection and unchanged complete
canonical state were verified. The native fixture remains
`vcp-cli/tests/local_inspector_queries.rs`. Fast delivery evidence is
`8e6d93c5-5054-45fa-bf88-bed16eb10dc2`; required SDK/extension regressions were
rerun after adding explicit unsigned 16-bit validation.

## Remaining P4-04 acceptance

Expose policy/routing/optimization through existing governed services, retaining
revision-bound preview/apply/rollback and durable command reconciliation. Present
history, memory, evidence, cost certainty and pruning previews with bounded reads,
opaque actions and explicit current versus historical state. Recheck permission
for every artifact/page and clear transient views on access/retention changes.

Qualify CLI parity, large pages, purged artifacts, restricted scope, policy changes
between preview and apply, hostile links and actual webview reload. Provider and
recovery secrets must remain outside webview messages and persistence. A cloud
export must use the existing encrypted publisher; `session/export` is local only.
