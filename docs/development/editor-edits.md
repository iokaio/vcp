# Versioned editor changes

Work item: [P4-03](../plan/18-deferred-vscode.md#p4-03--versioned-document-edits).
Status: accepted on 2026-09-23 for the qualified native Windows/editor envelope.
[ADR-059](../adr/059-versioned-editor-edits.md) records the representation,
authorization, receipt and recovery decisions.

## Workflow

The ordinary connection commands still launch inspection hosts. **Start
Execution-backed Task** explicitly selects an existing trusted execution profile,
collects a provider credential without saving it, and submits a fresh task with
durable command identity. The engine enforces the configured cost cap, request
limit, deadline, model and policy. Connection alone starts no provider work.
The initial provider prompt uses disk context; registering a later editor
observation does not retroactively change that prompt.

Select a task in Tasks and a local source document, then choose **Create Editor
Draft**. Edit the separate untitled document. **Review Editor Drafts** submits
bounded observations and prepares replacements for selected drafts belonging to
one task. Native read-only diffs display the exact captured before/proposed text.
The source remains unchanged during drafting and review. Draft documents follow
the editor's own unsaved-document behavior; VCP does not copy their contents into
workspace state or a canonical memory fact.

Resolve any engine policy question in Tasks and resume explicitly when required.
**Apply Reviewed Editor Changes** obtains new engine authorization for each file,
then uses the version-bound editor primitive. It neither saves nor rolls back
files automatically. A conflicting later file leaves earlier results intact.
**Inspect Editor Change Outcomes** reads durable command and per-file results;
unknown outcomes never cause an insertion to be replayed.

Changing or closing a source cancels its eligible undispatched prepared effects
and marks the corresponding files rejected. Old policy questions become
nonactionable; cancellation does not manufacture an approval or resume the task.
Use explicit **Resume** when the held task is ready, then create a fresh draft
against current text. Already dispatched or unknown effects remain unresolved
until their own receipt/reconciliation, even if the source is later closed.

The initial mode supports existing local files in the selected Windows root,
bounded UTF-16 ranges and buffer-only edits. Each document and result is bounded
at 64 KiB; a review contains at most 16 distinct files and 64 KiB combined original
and replacement content. New-file creation, automatic saving and cross-file
atomicity are not advertised. Encoding is unknown in the public editor API;
native disk bytes and BOMs are independently observed.

## Authority and observations

The negotiated `editor/prepared-edits/1` profile covers `editor/context`,
`editor/prepare`, `editor/changeRead`, `editor/dispatch` and `editor/changeResult`.
Read-only inspection remains possible as an observer, while mutations require
current controller ownership. Preparation/dispatch use a real execution-backed
task binding and the existing effect, policy, approval and interruption checks.
An inspection host cannot synthesize a running task to grant an edit.

An observation includes execution host, URI and canonical relative path,
document-open identity, version, logical UTF-8 text hash, dirty state, language,
selections, EOL and encoding evidence. The engine assigns the connection
generation/observation identity and captures the disk fingerprint using its native
root guards. Closing/reopening, save, rename/delete, root rebinding, trust changes
and content changes invalidate applicable observations. Diagnostics retain their
collector, observation time, observed document version, count and bounded digest.
The producer's document revision is explicitly unknown because VS Code does not
expose it; collecting diagnostics does not certify their freshness.

Full unsaved text and replacement bytes remain connection-local unless explicit
capture is requested through the API. Canonical records retain metadata, hashes,
operation identities and receipts. Diagnostics messages are not persisted by
default. Every page/read retains the normal access and retention boundaries.

## Dispatch, receipts and verification

The prepare command ID is also the change ID and is saved before sending. A lost
prepare response therefore has a queryable original identity. Every file dispatch
commits intent and an execution identity before the editor receives permission.
A replayed dispatch response always has `apply: false`. The extension keeps only
bounded command metadata in its per-profile journal, with one writer per profile.

The final live document/trust/mapping check and construction of `TextEditor.edit`
are synchronous. Its renderer version check preserves typing that occurs before
application. The extension reads actual after state and repeats that observation
before submitting the receipt. Undo, save or other intervening changes can make
the result unknown even when the editor initially reported success. Receipts
distinguish buffer application from saved disk bytes and retain per-file partial
outcomes.

Disk checks are explicitly disk-only. An active registered editor observation
keeps buffer coverage unverified, including when its latest dirty flag is false.
An actual document-close event can retire the current connection's exact
observation after a native disk read and fingerprint update. It cannot retire a
newer observation or erase unresolved dispatched effects. A deliberate current
controlling editor observation of the same path supplies the current editor
representation; it does not prove the absence of another application's unsaved
drafts. Historical receipts never establish present buffer contents.

**Refresh Editor Observations** explicitly refreshes tracked document metadata
and retries retirement after a bounded known conflict. A close attestation may
retry at most three times with fresh task revisions and new command IDs, only
after a definite version-conflict rejection. An uncertain close retains its
original command ID; subsequent refresh reads that command and never replays it.
Closing is an authenticated editor attestation plus independent native disk
evidence, not native proof that no editor anywhere holds a buffer.

Reload discards transient proposals and restores observation only. Command IDs
and change IDs survive for authorized reads. An interruption after apply but
before receipt remains dispatched/unknown; current matching bytes alone do not
prove which operation changed them. No automatic controller acquisition, task
resume, changed-document rebaseline or compensating undo occurs.

## Qualification

The native primitive fixture passed on the official VS Code 1.138.0 Windows
archive. Genuine typing queued between version capture and renderer application
caused the stale edit to return false while preserving the human text. A prior
file's successful edit survived the later conflict. Real undo/redo, independent
save/disk observations and exact UTF-8 BOM/CRLF/Unicode bytes also passed.
Evidence: `artifacts/p4-editor-edit-primitives/result.json` and
`artifacts/p4-edit-primitives.log`.

The full staged extension, SDK and compiled engine workflow passed on both Files
and SQLite in 82.69 seconds. It exercises explicit execution-task start, wrong-root
rejection, actual close/reopen and rename/delete events, exact save/close observation
retirement, stale preview rejection and per-file partial application. Explicit
observation refresh retires the superseded second file without inventing execution.
The final edit applies `44`, real undo restores dirty `43`, and a real window reload
preserves that dirty buffer while disk remains `42`. The new host reconnects as an
observer, reads the original dispatched/unknown change and never reapplies it.
Independent Rust reads confirm four canonical changes, partial outcomes, matching
execution identity and version-2 change/buffer records.

Evidence: `artifacts/p4-editor-workflow.log`, and `result.json`,
`canonical-changes.json` and `runtime.json` under
`artifacts/p4-editor-workflow-files/` and `artifacts/p4-editor-workflow-sqlite/`.
The loopback provider is synthetic; no paid provider is used. Only native UI
answers and fault timing are automated; engine replies and editor outcomes are real.

Dirty-buffer reload uses the staged extension and driver copied into the fixture's
private normal extensions directory. The pinned editor skips durable backup-path
registration for development-host windows, so that mode cannot qualify hot-exit
recovery. The launcher retains normal-token, hidden-window, runtime-asset and
private-profile checks. VSIX installation and upgrades remain P4-05 work.

Portable tests additionally cover lost prepare/dispatch replies, profile journals,
late replies after invalidation and bounded close-conflict reconciliation. Engine
tests cover trust/authority rejection, policy questions, stale verification and
both-store replay/conversion. Those cases are compositional evidence; the actual
editor workflow uses an automatically permitted edit policy.

The extension gate passed 108 tests; SDK build/examples/consumer and 33 tests,
protocol 34, generation/provenance 9, engine 72, store 11 and lifecycle editor 4
tests passed. The targeted engine editor tests cover both backends, replay,
conversion, close retirement and refused projection drops. The existing real
OneDrive store test remains ignored. The final fast gate passed 18 checks
(manifest `94490182-86a3-46f4-8bd3-442a2a1e2da3`). Affected formatting, syntax,
generated drift and repository contracts also passed.

Editor-bearing stores contain typed change/buffer Projection records at schema
version 2. This deliberately rejects pre-editor readers whose generic Projection
validator accepts only version 1. Do not remove these records as rebuildable
projections: they preserve unresolved editor authority and buffer uncertainty.
The current store rejects generic projection drops and retains the records across
backend conversion. This guard is based on the checked pre-editor source contract;
it does not represent an actual old-binary compatibility run.
