# 18 — Deferred VS Code client

Status: P4-01 through P4-03 accepted after owner-directed P8 closure and completed P9; P4-04 inspectors are next. Owns P4-01 through P4-05; file/phase numbering does not place it before CLI memory or routing. Architecture section 18 governs the editor design.

## Code organization

Under `src/packages/vscode/`, separate extension-host modules `engine_connection`, `workspace_map`, `trust`, `document_context`, `edit_bridge`, `commands` and `diagnostics` from webview presentation `session`, `agents`, `diff`, `inspector` and `questions`. Bind transport to `sdk-ts`; never expose provider or recovery keys to the webview.

Reuse qualified G07 editor-context/diff utilities and inspect Cline/Continue candidates where useful. Engine scheduling, policy, canonical memory and cost remain in Rust. Editor packages must not embed a second model gateway.

Use [the deferred client design](../architecture/deferred-clients-design.md#workspace-and-document-projections)
and [client/distribution ADR-012](../adr/012-clients-and-distribution.md) to qualify
editor APIs and packaging. Pin the selected extension API/SDK/toolchain only when
these deferred tasks start, and verify actual concurrency/trust behavior against
that version. Existing source candidates are investigation inputs, not granted
license, installation support or proof of safe document application.

## P4-01 — Connection and workspace mapping

The [initial observer connection](../development/editor-connection.md) connects the
prepared workspace UI through the SDK. [ADR-056](../adr/056-editor-observer-connection.md)
records its read-only boundary and the native API prerequisites for full mapping,
trust/rebind and observer reload. These prerequisites are now implemented and
qualified under [ADR-057](../adr/057-editor-trust-and-observer-recovery.md).

The subsequent root/binding projection adds negotiated `workspace/binding/1`
fields to `workspace/open` and uses them in the editor map and view. Engine-backed
trust/rebind and authenticated observer reload complete P4-01. The
[acceptance evidence](../development/editor-connection.md#p4-01-acceptance)
covers actual editor reload with pending input and another client's controller,
moved-root identity, restricted trust and queued-work revocation.

Implement launch/attach/version negotiation through the SDK, map VS Code roots to durable VCP identities and enforce workspace trust. Handle reload/disconnect and controlling versus observing clients according to P9's explicit owner contract.

Test multiple roots with identical names, a moved root, untrusted workspace, missing/incompatible engine and extension reload during a pending question. Start with the qualified local Windows host; remote workspace support needs P10-04 environment adapters.

**Construction sequence.** Implement a workspace map keyed by folder URI plus
execution host, with engine workspace/root IDs and mapping revision. Resolve nested
or identically named roots unambiguously; display names cannot authorize access.
Changes to roots, links or host placement invalidate cached file mappings and
prepared operations. Use engine rebind commands for moved workspaces rather than
creating a new identity silently or assigning the old identity by pathname alone.

Launch only a configured trusted engine executable, negotiate protocol and actual
capabilities, then acquire the permitted controller/observer role. A workspace
configuration file cannot select an arbitrary executable in restricted trust mode.
Expose engine location/version, owner role and execution host. Trust changes become
effective policy inputs; revocation blocks new project execution and forces prepared
actions to revalidate. Local Windows support does not imply SSH/devcontainer support.

Persist only the connection/session/cursor references needed for recovery, keeping
credentials out of workspace and webview storage. Reload reconnects and queries
durable pending commands/decisions; it must not resubmit the last message with a new
identity or revive an expired approval. Explicit pause leaves the editor/CLI status
views available while the task tree stops; only a deliberate resume restarts work.
Follow [ownership boundaries](../architecture/deferred-clients-design.md#ownership-and-adapter-boundaries)
and [architecture section 18.2](../architecture/vcp-what.md#182-workspace-trust-and-host-placement).

Test a trusted workspace becoming untrusted while a tool is queued, an engine path
suggested by project data, root rename during a question and observing-client reload
while a different controller owns the task. Assert actual dispatch and workspace
scope, not only extension activation or a connected icon.

## P4-02 — Task and child views

Accepted under [ADR-058](../adr/058-editor-task-presentation.md).
The [access-checked presentation API and bounded editor view](../development/editor-tasks.md#qualification)
passed portable and actual-editor qualification, including CLI parity, dropped
subscriptions, stale/duplicate actions and durable reload. Evidence limits are
recorded with the acceptance results.

Display objective, current status, model/group/cost, steering, questions, tool/evidence links and attributed child commentary from the same events as the CLI. Bound webview messages and sanitize external content/links. Question responses carry durable IDs and revision checks.

Test CLI/editor observation of the same trace, dropped cursors, noisy child streams, duplicate clicks, stale question response and malicious output markup. A disconnected UI must not report completion without the engine result.

**Construction sequence.** Build a pure view reducer from engine snapshot plus
ordered events, shared conceptually with the CLI's task/child projections. Preserve
root/child attribution, explicit unknown/partial states, pending input revisions
and ledger-derived known/reserved/uncertain cost. Keep the current owner and pause
state visible; a local spinner or closed stream is not a terminal engine outcome.
Bound rendered rows and fetch older content through authorized pagination.

Use a closed extension-host/webview message schema with opaque action IDs. The
extension host looks up the current decision and attaches its operation/revision
identity; arbitrary webview payloads cannot choose an executable command or grant.
Sanitize external Markdown/links, constrain resource loading/scripts and treat
tool/model markup as data. Provider and recovery secrets never enter page state.

Disable a clicked action while its durable command is pending, then reconcile by
that identity after reload or timeout. Duplicate clicks must not duplicate tool
approval or a user turn. Cursor gaps trigger explicit resynchronization, and a noisy
child cannot hide another child's waiting/failed state. Test a recorded synthetic
trace through both clients and compare final semantic projections, then use actual
extension-host interaction for malicious messages, stale input and dropped
subscriptions. Follow [presentation design](../architecture/deferred-clients-design.md#presentation-and-package-compatibility)
and [visible delegation](../architecture/vcp-what.md#166-visible-sub-agent-work).

## P4-03 — Versioned document edits

Status: accepted after full native engine/editor qualification on Files and SQLite.
See [the implemented contract and evidence](../development/editor-edits.md)
and [ADR-059](../adr/059-versioned-editor-edits.md).

Capture selected/dirty document content with URI, version, scope and artifact reference. Prepare edits against expected document versions and disk state. Revalidate at apply and record actual application receipts; concurrent typing causes a conflict/replan rather than silent overwrite.

Implement multi-file partial-result handling and undo-aware state reconciliation. Saving a dirty document changes the disk fingerprint and invalidates stale verification. Never treat editor APIs as a cross-file atomic transaction unless their tested contract provides it.

Test unsaved buffer divergence, typing between preview/apply, rename/delete, multi-root files, encoding/CRLF, partial application, save/undo and checks run against stale disk. E05/R02 now includes dirty-editor-buffer cases.

**Construction sequence.** Implement versioned document observations with URI,
workspace/host, language, selection ranges, dirty state, content hash, document
version, disk fingerprint and optional bounded content. Diagnostics have their own
producer/time/document revision. Record whether a prompt used disk or editor
content. Unsaved drafts remain ephemeral unless the user requests capture; do not
turn temporary buffer contents into retained memory facts by default. Invalidate
observations on save, rename, close, delete, rebind and relevant version changes.

Use [prepared editor receipts](../architecture/deferred-clients-design.md#prepared-editor-edits-and-receipts)
to bind operation/change-set identity, expected documents/disk, current authority
and proposed edits. Preview cannot authorize a later changed document. Immediately
before apply, check live versions and workspace trust in the extension and current
operation authority in the engine. Qualify the chosen editor API's actual atomicity;
if it cannot safely enforce a version-bound edit, require refreshed review or a
save/retry boundary instead of relying on an unchecked gap between read and write.

Persist engine intent before sending an apply request. Return per-file receipts
with before/after versions/hashes, saved/unsaved state and observed disposition.
An interrupted receipt exchange leaves an operation to reconcile; do not reapply an
insertion based solely on timeout. Multi-file partial success remains partial.
Compensating changes are freshly prepared against current buffers, never unconditional
undo over subsequent typing. Keep buffer and disk receipts distinct.

Save/undo/redo can change which result is current after a successful apply. Invalidate
checks tied to an obsolete fingerprint, and report whether verification covered
disk, buffers or both. Run real editor race fixtures, including user typing during
review, crash after one file applies, undo before receipt reconciliation and dirty
content differing only in line endings/encoding. Compare independent document and
disk observations with engine receipts. Follow
[architecture sections 18.3–18.4](../architecture/vcp-what.md#183-buffer-context)
and [current completion evidence](../architecture/vcp-what.md#44-completion-contract).

## P4-04 — Inspectors

Expose full history/memory/evidence, cost/policy/routing, optimization proposals and pruning previews through engine query services. Respect current retention/access for historic content; do not cache purged text indefinitely in webview state. Cloud export still uses the encrypted engine publisher.

Test inspector parity with CLI, paged large output, purged artifacts, restricted scope, policy rollback and webview reload. Secret references remain opaque and secret values never enter messages.

**Construction sequence.** Map each inspector to an existing access-checked engine
query, with bounded page/range inputs and explicit current versus historical views.
Preserve artifact identity, observed/truncated status, evidence provenance, policy
revision and cost certainty in presentation. A deleted, pruned or restricted object
returns its actual unavailable state; it is not replaced by a cached transcript.
Do not infer current authority from a historical grant displayed in the inspector.

Keep large content out of persistent webview state. Invalidate authorized caches on
retention/access changes and recheck permission for each page/artifact read. Search
snippets are subject to the same restrictions as complete artifacts. Optimization
apply/rollback and pruning application use engine preview/decision commands with
their own revision preconditions; an inspector button cannot directly edit config
or delete memory. Route cloud-directed exports to the existing encrypted publisher.

Use the same fixture queries as CLI inspectors and compare semantic results, then
revoke access or prune between page requests and reload the webview. Assert no
restricted text resurfaces from client persistence. Include hostile artifact links,
unknown charge fields and an optimizer preview stale after a concurrent CLI change.
The owning contracts are [history governance](../architecture/vcp-what.md#118-history-exploration-aging-and-pruning)
and [presentation/access design](../architecture/deferred-clients-design.md#presentation-and-package-compatibility).

## P4-05 — Packaging and compatibility

Package the extension with a documented compatible engine/SDK range and update path. Validate installation without a development checkout, extension restart, engine replacement, incompatible schema and failed upgrade. Existing local state and independently held recovery keys must survive upgrades.

Run actual extension-host integration tests plus real editor interaction for typing/undo/reload cases. Record supported VS Code/Windows versions and not-run environments. Done when the editor preserves established CLI behavior and user buffers under tested races, rather than merely presenting a working chat panel.

**Construction sequence.** Decide and document engine discovery/distribution,
extension/SDK/protocol ranges and upgrade sequencing in ADR-012. Include the exact
qualified extension API and toolchain versions in reproducible build inputs. Test
the packaged extension from a clean user profile without a repository checkout,
development PATH or precreated runtime directories. Inspect package contents,
licenses/notices and generated bindings; do not bundle raw fixtures, credentials or
runtime history.

If managed engine installation is selected, verify artifact provenance and
compatibility before activation; do not download/execute a workspace-specified URL.
Test interrupted install and a failed engine upgrade using distinctly named test
resources. Data migration compatibility governs whether a prior binary can reopen
state; retain recovery data instead of claiming every binary downgrade is safe.
Extension update/uninstall must preserve user data roots and independently stored
recovery material.

Record actual install, launch/attach, reload, incompatible-version, update and
uninstall results plus the real editor race suite. A mock engine verifies rendering
only; package acceptance includes the compiled engine/SDK and native Windows host.
List unsupported remote/environment combinations until P10-04 qualifies them.
Publish the compatible-version matrix and recovery instructions with the release
artifact, following [package design](../architecture/deferred-clients-design.md#presentation-and-package-compatibility).
