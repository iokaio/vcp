# Editor task and child views

Work item: [P4-02](../plan/18-deferred-vscode.md#p4-02--task-and-child-views).
Status: P4-02 accepted on September 23, 2026, with qualification boundaries below.
[ADR-058](../adr/058-editor-task-presentation.md) records the presentation and
durable action boundary. The accepted [workspace connection](editor-connection.md)
supplies the authenticated client and durable workspace binding.

## Presentation

The Tasks view reads a canonical paged snapshot, retaining its subscription and
event cursor. Ordered events invalidate presentation and trigger a fresh snapshot.
Gaps remove actionable content and resynchronize; unavailable streams retain only
the last known task statuses with an explicit disconnected state. Neither event
outcomes nor command acceptance manufacture task completion.

The snapshot reducer bounds tasks and pending inputs at 8192, pages at 256 and
per-task event summaries at eight. The UI displays 20 task rows per page in stable
root/child identity order and global waiting/blocked/failed/paused counts. Event
volume cannot evict a sibling's canonical state. Host-to-webview messages have a
512 KiB serialized bound in addition to bounded text and row rendering.

`task/presentation` is an additive authorized read, using scope, task, limit and
optional cursor. It returns a current task view, truncated objective summary,
complete bounded objective criteria, assignment role/model policy, observed model
selection when retained, approval validity and bounded effect/evidence/commentary
rows. Cursors are read preconditions, never grants. Changed authority, deletion,
query or canonical watermark requires a fresh page sequence. Original strict
`task/read` and event response shapes remain unchanged.

Actual model/group comes from a validated retained routing selection, not a
configured policy string. Commentary comes from verified retained child
transcripts with matching scope and capture schema. Unavailable or purged content
stays unavailable. Large transcripts retain authorized evidence links instead of
triggering unbounded automatic reads. Cost uses the shared root ledger from `usage/read` (never summed per child), with known, reserved
and unresolved USD micros kept distinct and exact.

Artifact links are host-registered opaque handles. Each selected byte range goes
through `artifact/read` with current access, identity, offset and digest checks.
Base64 wire data becomes a bounded UTF-8 text preview, with explicit byte ranges
and authorized continuation handles. Content is cleared on invalidation, access
failure, disconnection and selection changes. No model/tool HTML, arbitrary link,
command URI or executable payload is interpreted by the webview.

## Actions and recovery

Explicit controller ownership and current editor/engine trust are prerequisites
for task mutations. Opaque actions capture task/input revisions; the host reads
current task state before dispatch and the engine enforces final authority and
revision checks. Expired/non-actionable questions are disabled. Approval uses the
input, steering, effect and policy revisions plus operation digest. It never
resumes a paused task. Guidance is collected in the extension host and preserves
the complete prior constraints and acceptance criteria; unavailable criteria
disable steering rather than silently dropping them.

Connection commands currently launch inspection hosts, without configuring an
execution profile or starting a new task. Controls are shown only for advertised
engine methods; an inspection host does not advertise provider execution/resume.
An observer attached to an executing engine preserves that engine's controller
ownership. This work adds presentation and guarded actions, not provider setup or
controller credential transfer between applications.

Before sending a mutation, the extension persists only command ID, scope, task,
operation and bounded outcome metadata. Journals are partitioned by the configured
engine/data profile. Each profile retains one journal writer across navigation,
so a late reply cannot overwrite newer command identities after switching away
and back. They contain no guidance, question text, approval payload,
provider credentials or controller tickets. Duplicate clicks share the pending
operation. Independent pause/cancel actions do not wait on an unrelated action's
local coordinator lock; transport and native authority rules still apply.

Reload and lost replies reconcile `command/read` using the original ID. Missing or
pruned receipts remain unknown; there is no automatic replay or replacement ID.
A reconciled command means its acceptance was verified, not that its task/effect
completed. The UI displays the original identity and current canonical state.

## Qualification

Portable tests cover canonical snapshot/event semantics, child isolation, gaps,
duplicate and stale actions, expiry, lost replies, journal validation, criteria
preservation, out-of-order replies, hostile markup/messages and resource bounds.
Native interaction uses the staged extension and official VS Code 1.138.0 Windows
archive. The restricted fixture compares editor task/child semantics with the CLI
projection against both canonical stores, checks malicious content/messages and
observer mutation rejection, and drops real task subscriptions to verify automatic
resnapshot with the selected child and pending question preserved. Canonical data
is unchanged by those observations. The trusted fixture checks stale-owner
approval rejection, duplicate cancellation with one durable receipt, and real
reload restoring observation, command identities and a still-pending question.

These fixtures contain no ledger, so native cost parity proves explicit
unavailability rather than a positive cost total. Positive approval uses the
existing native SDK live-owner qualification plus extension guard/action tests;
the actual editor fixture exercises stale approval and successful cancellation.
Mid-RPC lost-response reload is covered by portable reconciliation tests; native
reload occurs after the rejected approval and reconciled cancellation. These are
the boundaries of the recorded evidence, not claims of an executing editor host.

Passing checks for this increment:

- Extension build and 69 portable tests, including the late-reply profile-switch
  regression; staged package build.
- SDK build/examples/consumer and 33 tests; protocol 32 tests; generated
  bindings/provenance 9 tests.
- Engine/lifecycle presentation 5 tests, including both storage backends,
  access/retention invalidation, model chronology and bounded retained commentary.
- Fast delivery suite: 18 passed, manifest
  `94d740a6-d6ad-4743-807c-9a51bfaec5af`.
- Native editor suite: two scenarios, recorded in
  `artifacts/p4-task-editor-final.log`; detailed results in
  `artifacts/p4-extension-host/result.json` and
  `artifacts/p4-task-extension-host/result.json`.
- Affected Rust formatting, JavaScript/PowerShell syntax and repository/link
  contracts.

Run the native editor fixture from a normal Windows user-token process. The
launcher rejects restricted tokens before starting the GUI, checks required editor
runtime assets, and isolates user data, extensions and shared storage. Its owned
child process inherits error-dialog suppression while exit codes and logs remain
available. Trusted fixtures seed only their disposable folder/workspace paths in
the pinned editor's trust preferences; trust enforcement remains enabled. These
settings do not change the user's installed editor or profile.

Versioned document edits belong to P4-03; inspectors to P4-04; installation/update
and release packaging to P4-05. This increment publishes no extension release.
