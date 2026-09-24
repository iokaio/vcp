# ADR-059: Versioned editor observations and per-file receipts

Date: 2026-09-23
Status: accepted under P4-03.

## Decision

Editor buffers are a distinct representation of a workspace resource. A URI and
document version alone do not identify an observation after closing, reopening or
reloading. Observations bind the authenticated editor connection, document-open
identity, canonical workspace root and binding revision, document version and
logical text hash, with independent disk evidence. Language, selections and EOL
describe the observed document. Unknown encoding is explicit; configuration is not
proof of the disk encoding.

Unsaved document content remains bounded connection-local evidence by default.
Persisting an edit intent does not implicitly capture the entire original draft.
Durable metadata distinguishes buffer observations, proposed edits and disk
fingerprints. Explicit capture uses the existing artifact/access/retention rules.

Prepare an immutable change against those observations and current task authority.
Review does not grant later authority over changed content. Each file requires a
fresh engine admission immediately before application, with dispatch intent
committed before returning its edit. Trust revocation, changed steering, root
rebinding, pause or stale resource evidence stops subsequent admissions. Existing
policy and approval semantics remain in force.

A current observation that supersedes or closes a prepared source atomically
rejects its undispatched files and cancels only eligible linked effects with the
exact operation identity. Dispatched and unknown effects are not retired this
way. Advancing the cancelled effect invalidates its old approval question;
the task stays held until explicit resume. Replanning requires fresh observation
and preparation, not reuse of the cancelled proposal.

The extension checks the live mapping, trust, document-open identity, version and
text hash immediately before calling the qualified version-bound editor primitive.
There is no asynchronous gap between its final document check and constructing
the version-bound edit. Application is per file; there is no cross-file atomicity
claim or automatic rollback.

Receipts distinguish applied, rejected and unknown outcomes and preserve actual
before/after document versions and hashes. An unsaved buffer edit never satisfies
a disk-write receipt. Undo, redo and save produce new observations and invalidate
verification of obsolete representations, even when text returns to an earlier
hash. A compensating edit is a new prepared operation against current state.

Lost replies and reload reconcile durable operation identities. A recovered
dispatch response is historical evidence, not permission to apply again. Matching
current bytes to proposed bytes cannot by itself prove which operation wrote them.
An interruption after application but before receipt remains unknown until there
is sufficient evidence; insertion is never automatically replayed.

Disk verification declares disk-only coverage. Any tracked active editor
observation keeps buffer coverage unverified, even after save. Actual closure
may retire only the current authenticated connection's exact observation, after
independent native disk validation. This is an editor close attestation, not a
claim that no other application holds unsaved text. Close cannot clear unresolved
dispatches. A deliberate current controlling-editor observation may replace
older metadata for the same path without certifying another editor's drafts.

The extension retries close retirement at most three times after definite
revision-conflict rejection, using freshly read task revisions and new command
identities. **Refresh Editor Observations** permits explicit further refresh.
Unknown close outcomes retain their original identity and permit only command
reads, not mutation replay. Neither refresh nor question invalidation resumes
execution or authorizes an edit.

## Persisted compatibility

Persisted editor change and buffer records use `vcp_editor_change_v2` and
`vcp_editor_buffers_v2`, with `schema_version: 2`. The pre-editor store accepts
unknown Projection documents only at schema version 1; version 2 therefore makes
that reader reject an editor-bearing store rather than ignore buffer uncertainty
and let its disk-only verifier replace the buffer fingerprint. The current store
recognizes these typed records before its generic fallback and refuses generic
`DropProjection` for either record. Conversion preserves both records and their
version fence. This is a persisted compatibility boundary, independent of public
protocol capability negotiation. Source inspection establishes the old-reader
rejection rule; an actual older binary is not claimed as qualified by that fact.

## Qualification gate

The pinned VS Code 1.138.0 implementation captures `TextEditor.edit`'s document
version before its synchronous callback, then checks the renderer's model version
immediately before synchronous application. The actual-editor primitive fixture
passed: a genuine built-in typing command queued inside the edit callback changed
the renderer version before the prepared edit arrived; that edit returned false
and preserved the human text. An earlier file's successful edit remained intact.
Actual undo/redo, save and independently read BOM/CRLF/Unicode disk bytes also
passed. Results are in `artifacts/p4-editor-edit-primitives/result.json` and
`artifacts/p4-edit-primitives.log`. The separate full engine/editor workflow passed
on both stores, including dirty-buffer preservation across actual reload before
receipt, observer reconnection and no replay. See the
[qualification evidence and envelope](../development/editor-edits.md#qualification).

P4-03 requires actual typing races, dirty buffer/disk divergence, document reopen,
rename/delete, multi-root identity, UTF-16 ranges, BOM/CRLF, partial application,
save/undo, interrupted receipt exchange and stale verification. Record the measured
editor primitive and the engine/extension integration separately. Native disk
qualification remains in [ADR-004](004-edits-and-execution.md); this record does not
change its tested envelope.
