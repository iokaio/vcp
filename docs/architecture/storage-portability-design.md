# Canonical storage, retention and encrypted portability design

Status: proposed implementation design for P1-04, P5-07, P5-09/P5-10 and P3-06.
No backend, encryption format or recovery implementation is qualified by this
document. [Architecture section 12](vcp-what.md#12-canonical-storage-files-or-sqlite)
defines required behavior. [ADR-003](../adr/003-canonical-storage.md),
[ADR-015](../adr/015-portability-and-storage-choice.md) and
[ADR-019](../adr/019-cloud-encryption-and-keys.md) separate confirmed boundaries
from engineering choices that P0-04/P0-06 must qualify. Names and records below
are proposed internal contracts, not promises of a public API or file format.

## Canonical record boundary

The store owns one ordered transaction history for engine state, retained events,
artifact references, claims, budget facts, indexing intents and retention changes.
See the [engine design](engine-execution-design.md) for command idempotency,
operation receipts and ledger state, and the
[memory design](memory-retrieval-design.md#logical-records-and-identity) for claims
and generations. Separate logical projections do not imply separate commit owners.

`transact` takes a transaction ID, expected entity/policy revisions, appends,
projection updates, outbox/indexing intents and a persisted response identity.
The receipt identifies the committed canonical sequence, affected revisions and
qualified durability status. Validate references and uniqueness in the commit
path. A repeated transaction identity with the same payload returns its original
receipt; a changed payload is a conflict. Model/network work occurs outside this
boundary.

Large immutable artifacts follow a stage/finalize/reference protocol: create in
the owning local artifact root, write and validate full bytes, perform the tested
flush/close/publication procedure, then commit their references. A pending capture
record must explicitly remain incomplete until finalized. Store failure may
leave an unreferenced object for later cleanup; it must not acknowledge a complete
artifact whose bytes never reached the promised durability boundary. Readers
check identity/integrity and distinguish absent, purged, redacted and corrupt.

Proposed shared metadata:

| Record | Meaning and required cross-check |
|---|---|
| `DataRootFormat` | Format/version, engine compatibility, backend choice and migration identity; unsupported newer formats cannot open for writes |
| `ArtifactDescriptor` | Stable ID/version, length/hash, media/capture kind, access and retention metadata, finalized state; payload location is local binding |
| `SnapshotPin` | Canonical watermark and complete reference set, generation IDs, purpose and owning operation; durable pins survive process death |
| `AuthorityRevision` | Monotonic revision in its declared workspace/policy scope; changes invalidate cached decisions, not immutable historical records |
| `DeletionEpoch` | Monotonic retention revision within a lineage and scope, with the exact tombstone records; unrelated offline lineages are not comparable by integer alone |
| `CleanupJob` | Exact owned objects, protected references, retry/error state and verified cleanup receipt; never an arbitrary recursive-delete path |
| `MigrationReceipt` | Source/target format and watermark, record/integrity comparison, activation operation and old-root recovery reference |

Local active records, artifacts and search indexes remain plaintext under local
OS controls. An active root must not be placed in the configured portable vault;
direct synchronization of live database/journal files is unsupported.

## Backend implementation and recovery

SQLite maps logical entities to versioned tables and enforces uniqueness,
references and optimistic revisions in transactions. P0/P1 must record the actual
connection, journal and synchronization settings, validate them on supported
Windows filesystems and measure writer contention. A consistent database backup
still needs pinned external artifacts and search generations. Do not prescribe an
unmeasured PRAGMA set or treat a copied live database file as a complete snapshot.

The files backend uses versioned, length-delimited transaction frames with checks
and an unambiguous commit boundary. Each replayed commit applies the same logical
constraints as SQLite. Recovery verifies a sealed checkpoint and replays complete
commits after its watermark. A torn uncommitted tail has a bounded repair policy;
corruption in acknowledged history enters explicit integrity/read-only recovery.
Never scan past corrupt committed records and pretend the remainder is valid.

Both backends use one-writer ownership per active root, validated on native
Windows. Store recorded process identity for diagnosis, but qualify live ownership
using the selected OS locking mechanism; PID text alone is insufficient. Reader
and snapshot consistency, stale lock recovery, lock timeout and file-sharing
behavior are part of conformance. Index workers commit through the same writer
boundary rather than opening independent unsynchronized mutation paths.

Backend conversion exports a neutral record view and its complete referenced
artifact set, imports into a separate root, checks IDs/edges/sequence mappings,
ledger totals, tombstones and pending intents, then activates through the restore
protocol below. Retain the old valid root under explicit recovery/retention policy.
Do not reset identities, rebuild monetary totals from approximate text or present
a config toggle as a completed migration. A read-only preview reports required
disk, compatibility, unavailable data and protected running work.

## Retention and prune transaction

The three actions have different semantics: exclusion changes recall eligibility,
compaction changes presentation, purge removes retained content and eligible
derivatives. [Architecture 11.8](vcp-what.md#118-history-exploration-aging-and-pruning)
and [ADR-016](../adr/016-history-and-pause.md) own those meanings. Notification
beyond 30 days does not authorize deletion.

1. Parse one typed selector and normalize dates into explicit UTC boundaries,
   retaining the entered timezone and inclusion rules. Reject ambiguous local
   times unless resolved by the command's documented policy. Evaluate the selector
   on a coherent view and materialize exact IDs/version IDs.
2. Traverse dependency edges from selected source content to claims, chunks,
   vectors, cached projections, prompts, exports and retained snapshots. Find
   active operation/recovery and unsettled accounting references. Build separate
   sets for selected content, derivative invalidation, protected facts and pending
   physical cleanup. Shared artifacts require reference-aware treatment; deleting
   one link must neither erase unrelated retained content nor preserve a forbidden
   passage in another derived view.
3. Persist a preview identity, authorized scope, action, selector digest, exact
   selection, relevant revisions, protected exclusions, policy revision and
   estimates. Applying a preview uses its stored selection; never rerun the
   selector against a later database and silently include more records.
4. Recheck authority, affected revisions and protected dependencies at apply.
   Choose exact-original-selection semantics only when the same objects and
   protection conditions remain valid; otherwise require a new preview. Saved
   automatic policy performs the same computation/commit under its recorded
   authority rather than synthesizing a user confirmation.
5. Serialize logical deletion against dispatch admission. Commit tombstones,
   deletion epoch, eligibility changes, context invalidations, indexing intents,
   cleanup jobs and an idempotent receipt atomically. This is the point at which
   newly admitted retrieval/model dispatch must stop using the selected content.
   Already-sent requests remain recorded as disclosed; deletion cannot retract
   their bytes from a provider.
6. Clean physical payloads, cache entries and eligible generations incrementally.
   Resume jobs after interruption. Account for immutable containers, SQLite
   journals/checkpoints and shared deduplicated artifacts using the qualified
   backend rewrite/compaction procedure. A lock or live pin leaves cleanup pending
   and content suppressed; it does not undo the tombstone.
7. Apply the separately configured vault retention policy to snapshots still
   containing that content. Report inaccessible/offline/unmanaged backup copies,
   provider version retention and old local recovery roots separately. Do not
   promise secure block erasure or revocation of copies already downloaded.

Removal metadata must be minimal and non-content-bearing: IDs, action, sequence,
reason category and cleanup/accounting facts as permitted. Do not retain a
deleted prompt inside a prune receipt or diagnostic payload. A claim supported
by several evidence sources may survive only with a permitted value and adequate
remaining evidence; purge-sensitive content must not survive merely because it
was copied into an immutable claim version. Inspectors identify evidence loss
and reduced applicability.

## Snapshot manifest and pinning

Create a durable snapshot operation before lengthy archive construction. Pin a
single canonical watermark and its dependency closure; changes after that point
belong to another snapshot. The snapshot records pending index intents and actual
searchable watermark instead of claiming indexes include later accepted claims.
Pin acquisition and garbage collection are serialized so no referenced component
can disappear between selection and reading.

The encrypted inner manifest must carry:

| Group | Required fields/checks |
|---|---|
| Format | Snapshot schema and compatibility, backend/export version, engine/upstream versions and completeness |
| Identity | Snapshot ID, parent(s) allowed by selected lineage protocol, local writer/device reference, creation time and workspace identities |
| State | Canonical/event/memory watermarks, task/child graph, unresolved effects/charges, policy versions and deletion ancestry |
| Inventory | Every required internal object ID/path, size/hash and media/schema role; bounded object count and total expanded bytes |
| Retrieval | Generation manifests, component specifications, source/chunk/vector rebuild inputs and explicit omitted/incompatible cache status |
| Workspace | Root-relative bindings, repository/base identity, relevant source references, staged/unstaged edits, required untracked files and explicit exclusions |
| Setup | Required local model digests, tools, skill/MCP definitions and secret names without secret values or portable execution grants |
| Trust | Reference to the P0-qualified authorized-writer evidence and lineage/deletion verification contract |

Keep public-recipient references and recovery verification state in trusted local
configuration; never restore an archive-provided recipient as new backup policy.
Portable setup definitions are data needing local validation, especially executable
paths and MCP configuration. Live handles, OS grants, credential-store secrets and
recovery identities are excluded. A source checkpoint is limited to its declared
selection and is not a claim to back up every repository file or installed tool.

## Key lifecycle and encrypted publication

P0-04/P0-06 must qualify the existing age/Rust candidate or an approved replacement,
including version/features, archive encoding, key export/import, authenticated
stream completion and native Windows interoperability. Writer authentication is
a separate gate: anyone knowing a public recipient may be able to encrypt for it,
so decryption success alone cannot authorize a writer. This design does not select
a signing construction, create a custom cipher or define a custom nonce scheme.

Enrollment creates/imports a developer-controlled recovery identity through the
qualified library and saves it independently outside declared sync roots.
Secret entry/export bypasses ordinary capture and argv. Recovery verification
reads the independently saved copy for a small round trip; a process's existing
cached secret alone is insufficient evidence of recoverability. Record only the
public/key reference, verification outcome and library format. Writer enrollment
and trust anchors must follow the separately qualified contract and cannot be
granted by archive content itself.

Publication progresses through durable operation states such as `pinning`,
`staging`, `encrypting`, `ciphertext_finalized`, `locally_published` and `failed`.
These are internal proposal names; the user-visible distinction remains local
durability, local publication, observable transfer and verified restore.

1. Resolve configured active/staging/vault paths using tested Windows path and
   reparse-point handling. Confirm local plaintext staging is outside known sync
   roots; fail on unresolved or changed bindings instead of trusting string-prefix
   checks. Document that unrelated unknown synchronization programs cannot all be
   detected.
2. Materialize the consistent checkpoint and retained objects in private local
   staging; validate hashes/reference closure and build the inner inventory.
   Apply approved writer authentication and encrypt the entire archive, including
   internal names and inventory, using qualified library operations.
3. Finalize and close the ciphertext stream locally. Return a typed
   `FinalizedEncryptedObject` containing an owned local ciphertext handle,
   opaque destination identifier and finalization receipt. Only the encryptor can
   construct this production type; a generic plaintext path cannot reach the
   publisher. Compile-time restrictions supplement integration tests. At final
   publication admission, recheck current deletion/publication policy under the
   controller's retention fence. If a purge invalidates content pinned earlier,
   rebuild the snapshot or leave the job pending; do not publish it as current
   state. Copies admitted before the purge and already published older objects
   remain explicit pending/retained backup obligations, including cancellation
   and cleanup status. Do not hold a canonical transaction open during the copy.
4. Copy ciphertext to an immutable opaque vault object. An interrupted copy remains
   ciphertext. Do not publish plaintext hashes, manifests, project names or
   recovery material in filenames/sidecars. Never overwrite the last good object.
   Source publication ordering cannot prove destination arrival ordering.
5. Record local publication with selected key/writer references and sequence range.
   Retry uncertain copies by verifying the operation's owned destination identity,
   or use a new opaque object; do not overwrite a different object based on name
   coincidence. Reopen reconciles copy state before reporting success.
6. Release pins and clean only this operation's staging resources when safe.
   Retain retry material only with an explicit local cleanup state and policy.
   Missing vault, key/trust failure or disk exhaustion preserves local work and
   unsynced range. A control timeout during exit leaves pending backup work for
   the next deliberate run, never a plaintext fallback or hidden continuation.
   Explicit pause while the CLI stays open permits configured local snapshot work
   and history/cost/status inspection, without restarting task-scoped models,
   extraction or children.

For multi-object experiments, every object and catalog is encrypted; restore
checks complete membership without assuming a manifest arrived last. Compare
transfer churn using ordinary randomized encryption, without adding convergent
encryption. Full versus incremental packaging remains the P0 portability choice.

Rotation verifies a replacement before changing future publication policy and
tracks old keys required by retained objects. Re-encryption creates new verified
objects before old retention changes. Loss of every applicable recovery key makes
existing snapshots unrecoverable; remaining local plaintext may start a new
lineage. Rotation cannot retract old keys/ciphertexts already held elsewhere.

## Restore validation and atomic activation

1. Select ciphertext into an isolated local restore operation and ensure complete
   hydration. Apply input, expanded-size, entry-count and nesting limits before
   allocations grow without bounds. Use the independently supplied recovery
   identity and qualified stream verifier; tentative plaintext remains confined
   to private staging until final authentication succeeds.
2. Verify writer authorization against locally enrolled or independently supplied
   trust, then verify the protected manifest, complete inventory and payload
   hashes. Successful archive parsing or decryption alone is insufficient. Missing
   objects, duplicate entries, unknown mandatory schema fields and incompatible
   versions produce bounded explicit failures.
3. Extract with root containment checks at each materialized path. Test absolute,
   drive-relative, UNC, parent traversal, alternate stream, reserved-name and
   case/normalization collisions on Windows. Reject or safely implement the
   explicitly qualified link policy; an archive-created junction must not redirect
   later writes outside staging. Do not execute archive-provided commands.
4. Compare lineage and deletion state with trusted current knowledge. Enforce all
   applicable newer tombstones before merging/activating into that lineage. A
   numeric epoch from an unrelated branch is not an ordered substitute for
   ancestry. On a fresh machine, the archive cannot prove that no newer snapshot
   exists; display verification limits and use the P0-qualified independently
   trusted checkpoint/head procedure. Never claim global replay detection from
   local metadata alone.
5. Reconstruct/import canonical state and independently validate references,
   logical counts, claim history, budget totals/liabilities, task/child edges and
   pending intents. Rebind workspace paths and required local tools/secrets.
   Preserve current user files through the prepared revision-checked change path;
   a validated state root does not authorize overwriting a dirty workspace.
6. Reopen compatible indexes or rebuild locally from retained source/vectors.
   Record search readiness separately from canonical restore validity. Tasks stay
   paused, old grants remain invalid on the new host, and unknown effects/charges
   remain subject to normal reconciliation.
7. Acquire the destination activation lock, verify its expected current root and
   record a durable activation intent referencing both roots and validation
   receipt. Publish the selected root through the qualified Windows pointer/switch
   procedure. Reopen and verify it before releasing the last valid root.
8. Recovery resolves an interrupted activation from the durable intent and
   validated root identities, not newest file time. Until the new root is opened
   and validated, no normal writes resume. After new writes occur, returning to
   the old root requires reconciliation/export; it is not an automatic rollback.

Sequential A-to-B-to-A transfer preserves stable IDs and ancestry. If both hosts
advance from the same parent, preserve both descendants and require explicit
selection or a separately supported reconciliation procedure. Do not compare
modification time, merge database files or claim a cloud-wide writer lock. An
intentional isolated historical restore is labeled with its older retention
state and cannot silently join a newer lineage.

## Failure matrix and validation evidence

| Fault boundary | Required independent evidence |
|---|---|
| Artifact finalized, transaction fails | No acknowledged dangling reference; orphan accounted for |
| Commit succeeds, response lost | Reopen and same request yield original receipt; ledger/claims applied once |
| Prune races model dispatch | Ordered deletion/admission receipts prove which won; actual submission certainty stays separate; no stale new admission after deletion |
| Cleanup stopped by open reader/file lock | Tombstone active; pending object inventory explicit; idempotent retry |
| Backup racing prune | Publication admission rechecks deletion state; invalidated pinned content is rebuilt/held; prior published objects remain explicit retention obligations |
| Encryption finalization fails | No plaintext reaches vault; previous valid object remains |
| Copy/hydration interrupted or reordered | Ciphertext only at destination; restore refuses incomplete inventory/stream |
| Correct recipient, untrusted writer | Writer check rejects activation even when decryption succeeds |
| Stale valid snapshot on fresh host | Trusted-head knowledge limits disclosed; no invented global freshness proof |
| Restore expansion/path attack | Resource bounds hold and no file outside operation staging changes |
| Kill before/after activation | One validated selected root, recoverable prior root and no mixed state |
| Rotation with retained old backup | Old key dependency visible; new backup independently recoverable |

Use both backend variants, real Windows process-kill barriers and two independently
provisioned environments in [U04/U05](../plan/16-test-fixtures-and-acceptance.md).
Compare canonical data with an independent neutral exporter, not only the same
restore routine that wrote it. Observe vault creation/error paths for synthetic
plaintext markers and verify the qualified cryptographic format independently;
marker absence alone is not proof. Record actual commands, format/library/model
versions, hardware/filesystem, tested failure envelope and not-run prerequisites.
The proposed `store`, `retention`, `portability` and `recovery` suites become
commands only when their owning implementation work adds them.
