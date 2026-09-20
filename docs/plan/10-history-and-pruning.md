# 10 — History exploration, aging notices and precise pruning

Status: P5-07 and P3-05 complete with [retention qualification](../development/p5-retention.md#qualification) and [CLI evidence](../development/p3-history.md#qualification). Encrypted snapshot/handoff integration remains P5-09/P5-10. Requires P5-05/P5-06, capture/store and common inspectors. Read architecture sections 11.8, 12 and 14.

## Design references and integration prerequisites

The owning requirements are [history and pruning](../architecture/vcp-what.md#118-history-exploration-aging-and-pruning),
[capture truthfulness](../architecture/vcp-what.md#143-capture-policy-and-truthfulness)
and [ADR-016](../adr/016-history-and-pause.md). Use the proposed
[prune transaction](../architecture/storage-portability-design.md#retention-and-prune-transaction)
and [query/send boundary](../architecture/memory-retrieval-design.md#query-authorization-and-context-handoff)
for implementation ordering. P5-07 needs P1-03/P1-04 and P5-05/P5-06 evidence;
P3-05 additionally uses P3-03's inspector contract. No retention feature bypasses
canonical storage because its user interface looks like a file deletion command.

## Code organization and contracts

Use `vcp-audit/history_query` and `history_filter` for browsing; `vcp-memory/retention`, `prune_plan`, `tombstone`, `dependency_scan` and `cleanup` for semantics; CLI `history`, `memory`, `retention` and `prune` commands for interaction. Shared selectors must have one typed parser used by preview and apply.

`PrunePreview` records workspace/scope, normalized filter, cutoff/timezone, source revisions, exact selected IDs, dependent derived records, byte estimates, protected/excluded refs, expected recall impact and preview identity. `PruneReceipt` records applied selection, deletion epoch, cleanup state and backup-retention limits. A stale preview never expands silently to newly matching records.

## P5-07 — Retention engine

**Planned supporting refactor for [M1](21-markov-integration.md#m1--retained-evidence-and-analysis-foundation):**
register source dependencies for transition counts, fitted models, forecasts and
consumed statistical receipts. Prune/revoke must invalidate derived serving state
and inaccessible reports; rebuilding uses only currently retained authorized
history. Test caches, saved reports and replay tombstones on both stores before
any persistent statistical artifact is enabled. No surviving private aggregate
is implicitly authorized by this extension.

1. Implement explicit actions: exclude from recall, compact presentation and purge retained content. Exclusion is reversible; compaction preserves raw history; purge coordinates source/claim/chunk/vector/cache removal according to policy.
2. Implement filters for date, workspace/root/path, task/agent/model/provider, event/claim type, status and supersession. Use inclusive/exclusive cutoff semantics documented in CLI help and tests; all date comparisons use explicit timestamps/timezone.
3. Build a dependency graph before purge. Protect active-task recovery and unsettled accounting; report exclusions or require affected tasks to pause/reconcile. Redactable content and minimal unresolved liability facts have separate lifetimes.
4. Apply the revision-bound selector, commit tombstones/deletion epoch before removing derived payloads, invalidate affected context manifests and block stale prepared dispatch. Cleanup runs idempotently and resumes after interruption.
5. Track old generation/snapshot references. Report active deletion, pending physical cleanup and retained cloud copies separately. Apply configured snapshot retention through the vault interface without promising erasure of provider version history or SSD blocks.
6. Default to a nonblocking notice when retained history exceeds 30 days, with no automatic deletion. Save user pruning policy separately; repeat-notice cadence remains an explicit configuration/ADR detail.

If an immutable live artifact cannot be physically removed yet, retain the tombstone and prevent retrieval while reporting the protected reference. Never claim purge finished while a pending cleanup or retained snapshot still contains the selected payload.

### Selector and dependency planning

Define the selector as a versioned typed tree shared by CLI, saved policy, preview
and execution. Normalize date-only inputs and explicit timestamp inputs once;
persist UTC boundaries and display the chosen timezone/inclusivity. Document
behavior for daylight-saving gaps/overlaps and reject unresolved ambiguity rather
than depending on the machine's later timezone. Keep workspace identity/root
bindings separate from display paths and stable event order separate from time.

Evaluate against a coherent read snapshot, materialize IDs/version IDs and store
the exact selection with relevant revisions. Make byte counts explicitly exact
or estimated, particularly for shared artifacts/compressed containers. Traverse
source-to-claim, source-to-chunk/vector, context-manifest, generation, export and
snapshot references. Separate selected objects, derivative invalidations,
protected recovery/accounting facts and retained backup copies. The preview must
explain why objects are protected without reproducing the sensitive payload it
proposes to remove.

Keep semantic actions independent. Exclusion changes current recall eligibility
and can be reversed; compaction creates a presentation view while preserving raw
artifacts; purge tombstones content and schedules actual removal. A claim with
several evidence sources needs reassessment when one disappears. It may retain a
permitted value with reduced evidence, but copying purged text into an immutable
claim or summary is not a valid escape from deletion policy. Track derivatives
by lineage, not by trying to find duplicate strings during cleanup.

### Atomic apply and restartable cleanup

Apply by preview identity with current authority and expected selection/protection
revisions. Use exact-original-selection semantics only while those selected
objects and protection conditions remain valid; otherwise return a stale-preview
result with a new-preview path. A fresh matching event must not join the deletion
set automatically. Saved automatic policies invoke the same planner/apply path
under recorded policy authority and produce inspectable receipts.

Before commit, acquire the controller's deletion/dispatch ordering fence. Commit
tombstones, new deletion epoch, eligibility changes, context invalidations,
index intents, cleanup jobs and receipt together. A paused task is not necessarily
safe to erase: pending effects and unsettled charges must first be reconciled or
their minimum facts protected. For dispatch admitted before the deletion fence,
retain actual submission certainty and attempt cancellation where supported;
admission alone does not prove bytes were sent. Subsequent admissions must reject
stale manifests, and already-sent bytes cannot be recalled.

Run physical cleanup as resumable per-object work. Recheck pins/references before
deleting owned payloads; coordinate generation retirement, journal/checkpoint
compaction and any backend-supported rewrite with P1-04. Locked files and disk
errors leave tombstones effective and cleanup pending. Reopen resumes from job
receipts rather than rerunning the user's broad filter. Serialize pin acquisition
against cleanup so a snapshot/reader cannot acquire a file already selected for
removal. Record failures with IDs and reason categories, not deleted content.

Expose separate states for logically unavailable, local payload removal pending,
local cleanup completed and backup retention remaining. Consult P5-09/P5-10 for
snapshots that contain the selected data and preserve tombstones across handoff.
If a backup was pinned before prune, reevaluate its publication against current
retention policy; a new snapshot cannot silently reintroduce purged content or
report a stale deletion epoch as current. Existing retained backups remain an
explicit retention obligation.

### Aging notices and explicit policy

Calculate age from any retained history, including recall-excluded records, using
the injected clock; the
notice threshold is strictly beyond 30 days, not at exactly 30 days. Persist
workspace notice state independently of events so one notice does not produce a
notice loop. Show oldest retained date, retained size and controls. Weekly repeat
is a proposal in the architecture; record the final cadence in ADR-016/configuration
before treating it as a default.

Policies need a version, scope, selector/action, cadence and authority provenance.
Notification-only is the default. A policy change affecting automatic purge is an
explicit authorized configuration mutation, not an optimizer recommendation
applied silently. Test disabling a saved policy while jobs are queued: already
committed tombstones remain facts, but new applications require current authority.

## P3-05 — Browsing and CLI controls

Implement `history list/search`, `memory inspect/prune`, `history prune --preview`, `prune apply`, and `retention show/set`. Explain current filter scope and show counts/examples, protected records and consequences before destructive application. Reuse user decisions/saved policy; `/optimize` suggestions cannot silently execute purge.

History queries include full tool/child artifacts with paged access, source links and lost/redacted-content labels. Date trimming can be combined with workspace/path/claim filters. Notices provide direct navigation to preview and policy without interrupting every task.

### Command handlers and read projections

Treat the command spellings above as proposed UX until P3-01/P3-03 bind the actual
parser. Handlers send typed queries/mutations to the controller; they do not open
store/index files directly. History pagination uses a stable snapshot/cursor
containing scope and ordering identity, with deterministic tie-breaking. Report
newer records separately or refresh deliberately; appending history cannot cause
page duplicates, skipped entries or a surprise expansion of a prune preview.

Implement event-to-artifact and event-to-derived-claim navigation in both
directions. Display retained full output beyond the CLI tail through bounded
pages/streamed local inspection, with explicit unavailable/capture-gap labels.
Search must distinguish raw history from accepted/inferred/disputed knowledge.
Keep query bounds and export redaction separate from source retention so viewing
a shortened response does not remove its full artifact.

Explicit pause leaves the CLI open for history, memory, cost and pending-effect
inspection. Read-only navigation must not resume the task, start a model-assisted
extraction or restart children. Prune remains a separately authorized operation
while paused and must still protect unresolved dependencies. Command results
show applied preview ID, exact counts, deletion epoch and cleanup/backup status;
headless mode emits the same facts as structured records without secret text.

## Tests and expected observations

| Fixture/action | Expected result |
|---|---|
| Frozen clock at exactly 30 days and beyond | Correct threshold; notice occurs beyond 30 days; default retains all history |
| Explicit timezone/date cutoff | Boundary records selected deterministically; displayed and applied IDs agree |
| New matching records after preview | Revision conflict or exact original selection; no silently broadened deletion |
| Active tool and unknown charge in selected range | Protected refs reported, required facts preserved, no false complete purge |
| Query/model dispatch races with prune | Current tombstone check blocks purged passage; stale manifest/operation is invalidated |
| Kill between tombstone and physical cleanup | Purged content stays unavailable; cleanup resumes exactly once |
| Exclude versus compact versus purge | Distinct retained bytes, recall eligibility and reversibility match the action |
| Restore old encrypted snapshot | Newer lineage's deletion policy prevents resurrection; isolated historical restore is labelled |
| Both backend preferences | Identical selected IDs, protected sets and logical deletion epochs |

Store fixtures in `src/tests/fixtures/retention/`; use a frozen clock and synthetic sensitive text rather than real private history. Run `retention`, `memory`, `store`, `recovery` and U05/M05/M07. Segment 11 adds retained-cloud-copy and restoration tests.

Register shared selector/dependency contracts for both backends and executable
kill/race fixtures when the harness exists. Use barriers at preview materialization,
tombstone commit, context dispatch admission, each cleanup category and snapshot
publication. Assert forbidden marker absence in returned context, cached context,
inspectors and snapshots admitted after purge, while independently checking
protected ledger/effect facts. Copies admitted before purge remain explicit backup
cleanup/retention obligations. Compare physical artifact inventories before/after separately
from logical query results; a hidden passage is not proof of removed bytes. Test
explicit open-CLI pause with inspection and pruning without any new provider
request or child scheduling. Suite names here remain planned interfaces.

## Exit

Done when users can inspect all retained activity, choose exact pruning scope, see 30-day notices without default deletion, and recover from interrupted cleanup without stale recall or lost required accounting. Evidence must distinguish logical exclusion, local physical cleanup and backup-copy retention.
