# Editor inspectors

Work item: [P4-04](../plan/18-deferred-vscode.md#p4-04--inspectors).
Status: P4-04 accepted on Windows with VS Code 1.138.0 and both Files and SQLite.
Packaging and large-history startup qualification remain P4-05 work.
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

## Optimizer command increment

The optimizer API increment is qualified under
[ADR-062](../adr/062-editor-optimizer-commands.md). `routing/optimizer/1`
adds `routing/reportCapture`, `routing/reportRead`, `routing/preview`,
`routing/apply` and `routing/rollback`. Capture returns a durable acceptance;
its command ID is the public report ID. Internal forecast identities remain
unchanged. Session coverage is explicit; observers cannot read workspace reports.

Preview returns exact prior, proposed and host-effective policies, selected edits
and clamped fields. It expires after 60 seconds and the connection keeps at most
two previews. Reload discards these previews. Accepted commands survive reload
and reconcile by their original command ID before consulting that cache.
Mutation preconditions use workspace and binding revisions; preview separately
pins policy revision. Capture acceptance has revision zero; apply and rollback
acceptance carry the new policy revision. Neither replaces the workspace revision
needed for a subsequent mutation. Report pages and exact preview payloads are
limited to 64 KiB. Oversized policies are rejected rather than silently omitting
a field from review.

Native foundation checks passed on Files and SQLite: atomic report/receipt
publication, original report identities, foreign source session with caller-session
receipt, conflicting command identity, apply/rollback replay after reopen,
cancellation and interruption after spool finalization without accepted capture.
Configured-host qualification passed on both stores: exact preview, policy apply
and rollback, a stale preview after an actual CLI rollback, connection replacement
with receipt reconciliation, observer denial, wrong binding and a fresh controller
denied after trust revocation. These metadata tests create no provider attempts.

Protocol checks passed (26 unit and three optimizer wire tests), along with
schema generation 9, engine 73, SDK 34 and extension 108. The fast delivery suite
passed all 18 cases (`2ca20a99-a005-4a34-93f6-78dc8b54a731`).
Three lifecycle projection checks also passed: all 17 edits retain exact values,
review preserves complete allowed sets, and 40 hostile cohort rows paginate at the
byte limit without skipped rows. A changed canonical deletion epoch invalidates
both continuation and fresh reads; this does not claim physical purge cleanup.
The existing routing/forecast regression suite passed 42 tests, with its supervised
crash child excluded from direct invocation. The final three foundation checks
also passed, including actual process termination before and after public policy
commit on both stores: policy, binding and receipt survive or remain absent
together; exact retry publishes at most once.

Compiled CLI/SDK qualification passed on both stores in 2.41 seconds: four source
pages and two report receipts per store, exact capture replay after reconnect,
conflicting payload rejection, observer workspace-report denial, stable read
watermarks and authority revocation between pages. Provider request count remained
zero and persisted policy remained revision zero. This inspection-only fixture
explicitly rejects preview without host ceilings; positive apply/rollback evidence
comes from the configured lifecycle host. Fixtures are
`vcp-cli/tests/local_optimizer_commands.rs` and
`sdk-ts/tests/native-optimizer-commands.mjs`.

Existing provider send fences were inspected; this increment does not claim a
newly exercised provider send race.

## P4-04 acceptance

The inspector UI is implemented under
[ADR-064](../adr/064-editor-inspector-views.md). Its nine tabs use the qualified
engine APIs, bounded pages and artifact ranges, opaque actions, current access
checks and transient content. Exact policy previews fail closed if the complete
review exceeds the display bound. Mutation journals retain nonsecret original
command identities before submission and reconcile receipts without replay.
Host prompts also open existing reports, jobs and backups by bounded ID. Pruning
previews remain connection-local and expire after 60 seconds: an observer may
create and page its own authorized preview, while another connection's preview
is unavailable. Applying pruning still requires current controller authority.
The initial policy-edit control supports input tokens, output tokens and quality
floor. It is not a complete policy authoring interface: the engine still supplies
the complete prior, persisted and effective policy for exact review and rollback.

Pruning command IDs and job IDs are distinct. A received pruning result supplies
the job reference retained for later inspection. If that result is lost,
`command/read` can recover acceptance but does not supply a job reference; the
editor must report that limitation instead of inventing a job ID.

The first actual installed-editor read qualification passed on Files and SQLite
(95.01 seconds) using VS Code 1.138.0 and an external controller. Real renderer
clicks covered paged history with native query parity, 33 retained memory
versions, multi-range hostile artifact text, policy/routing, explicit unavailable
cost, hidden-view recreation and workbench reload. The external client retained
control, no mutation was replayed, and the editor's persisted state contained no
artifact sentinel. This read fixture does not qualify positive cost or mutation
controls; separate native fixtures qualify them below.

The configured execution-host renderer fixture passed on Files and SQLite
(76.08 seconds). It displayed an actual settled 100-micro-unit charge, captured
an optimization report, reviewed and applied a policy edit, rejected an old
preview exactly once after a competing native publication, and reviewed and
applied rollback. Reopening canonical storage confirmed policy revision 3. The
competing publication used a real RPC on the same authenticated controller;
delivery of actual event replies was delayed to keep the stale review visible.
This exercises the native CLI host rather than starting a second canonical
writer. Only the fixture's synthetic loopback provider was called. Its scoped
pruning request returned the actual policy denial, displayed no stale review,
and submitted no forget command.

The encrypted publisher renderer fixture passed on Files and SQLite (104.99
seconds). A deliberate native profile dialog and actual publication click
created one encrypted object, published job and original command receipt per
backend. Independent known-head verification checked decryption and writer
signature. The job released source pins; cloud transfer remained unknown and
restore verification remained not observed. Actual workbench reload changed
the extension-host PID, recovered observation and read the original receipt and
job without reacquiring control, republishing or loading keys. Private profile
references and recovery-key text were absent from editor state. No provider
requests or attempt records were created by this fixture.

The final extended observer renderer fixture passed on both stores (161.29 seconds).
It retained another client's controller ownership through actual editor reload,
created its own read-only pruning preview, rejected another connection's preview,
and rejected a preview after actual engine expiry (the 60-second wait runs once
on Files). An external controller purged an eligible artifact: displayed bytes
cleared, the old DOM action issued no artifact read, fresh preview reads rejected
stale state, and hide/reveal/reload did not recover purged text. Trust revocation
cleared presentation before reauthorization; any subsequently displayed historical
observation came from a fresh authorized engine read. Revocation does not make
otherwise permitted historical observation permanently unavailable.

Portable extension tests passed (157 tests), including mismatched pruning-job
reply rejection, and the repository fast gate passed
all 18 cases. Native tests use private installed extension profiles and the
compiled engine, with actual renderer clicks and engine replies. They do not
inject inspector results or use paid providers. These results accept P4-04 within
the stated Windows/editor envelope; other operating systems and remote editors
remain unqualified. Package installation, upgrades and the larger startup corpus
remain P4-05 work. Cloud export uses the encrypted publisher; `session/export`
remains local only.

## Encrypted publisher prerequisite

[ADR-063](../adr/063-editor-encrypted-publisher.md) defines the public boundary
for the existing native encrypted publisher. [Native profile selection and API
usage](encrypted-publisher.md) keep recovery material outside editor messages.
The API prerequisite is accepted. Native protocol tests passed (26 existing plus
3 publisher cases), all 73 engine tests passed, and schema generation contracts
passed. The SDK passed 35 tests and the extension passed 108 regression tests.

Both-store lifecycle tests passed: atomic caller/session intent and receipt,
lost-reply reopen and exact replay, conflicts, opaque capability replacement,
source/stage/copy authority loss, and deterministic public disconnect or cancel
before the background capture first runs. Real adapter publication, completed
retry without a second vault copy, and published-plus-cancelled-intent status
passed. The existing native dirty/untracked/generation backup regression passed.

Native loader tests passed for explicit controller selection, bounded external
profiles, redirected/hard-linked file rejection, and key/Git file guards. A
separate repository test proved the executable guard survives its initial owner
through a background Arc and rejects another file identity or alias. The selected
single-link Git for this qualification was `C:/Program Files/Git/bin/git.exe`.

The actual CLI/SDK fixture passed on Files and SQLite (65.06 seconds). It created
one encrypted local object, verified decryption and writer signature against the
independently trusted published-head digest, preserved another connection's
controller ownership during observer reconnection, and reopened without loaded
keys to recover the original receipt and job metadata. Default SDK initialization
negotiated the publisher and workspace-binding profiles. No provider requests or
Attempt records were created. Cloud transfer remained unknown and the public
restore-verification field remained not observed; no restore activation occurred.

The final fast suite passed all 18 cases. Independent review identified and fixed
file-guard lifetime gaps and checked authorization, cancellation ordering and
receipt ownership. The subsequent inspector presentation, renderer mutation and
actual webview reload evidence is recorded in the P4-04 acceptance section above.
