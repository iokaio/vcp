# 10 — History exploration, aging notices and precise pruning

Status: planned. Owns P5-07 and P3-05. Requires P5-05/P5-06, capture/store and common inspectors. Read architecture sections 11.8, 12 and 14.

## Code organization and contracts

Use `vcp-audit/history_query` and `history_filter` for browsing; `vcp-memory/retention`, `prune_plan`, `tombstone`, `dependency_scan` and `cleanup` for semantics; CLI `history`, `memory`, `retention` and `prune` commands for interaction. Shared selectors must have one typed parser used by preview and apply.

`PrunePreview` records workspace/scope, normalized filter, cutoff/timezone, source revisions, exact selected IDs, dependent derived records, byte estimates, protected/excluded refs, expected recall impact and preview identity. `PruneReceipt` records applied selection, deletion epoch, cleanup state and backup-retention limits. A stale preview never expands silently to newly matching records.

## P5-07 — Retention engine

1. Implement explicit actions: exclude from recall, compact presentation and purge retained content. Exclusion is reversible; compaction preserves raw history; purge coordinates source/claim/chunk/vector/cache removal according to policy.
2. Implement filters for date, workspace/root/path, task/agent/model/provider, event/claim type, status and supersession. Use inclusive/exclusive cutoff semantics documented in CLI help and tests; all date comparisons use explicit timestamps/timezone.
3. Build a dependency graph before purge. Protect active-task recovery and unsettled accounting; report exclusions or require affected tasks to pause/reconcile. Redactable content and minimal unresolved liability facts have separate lifetimes.
4. Apply the revision-bound selector, commit tombstones/deletion epoch before removing derived payloads, invalidate affected context manifests and block stale prepared dispatch. Cleanup runs idempotently and resumes after interruption.
5. Track old generation/snapshot references. Report active deletion, pending physical cleanup and retained cloud copies separately. Apply configured snapshot retention through the vault interface without promising erasure of provider version history or SSD blocks.
6. Default to a nonblocking notice when retained history exceeds 30 days, with no automatic deletion. Save user pruning policy separately; repeat-notice cadence remains an explicit configuration/ADR detail.

If an immutable live artifact cannot be physically removed yet, retain the tombstone and prevent retrieval while reporting the protected reference. Never claim purge finished while a pending cleanup or retained snapshot still contains the selected payload.

## P3-05 — Browsing and CLI controls

Implement `history list/search`, `memory inspect/prune`, `history prune --preview`, `prune apply`, and `retention show/set`. Explain current filter scope and show counts/examples, protected records and consequences before destructive application. Reuse user decisions/saved policy; `/optimize` suggestions cannot silently execute purge.

History queries include full tool/child artifacts with paged access, source links and lost/redacted-content labels. Date trimming can be combined with workspace/path/claim filters. Notices provide direct navigation to preview and policy without interrupting every task.

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

Store fixtures in `tests/fixtures/retention/`; use a frozen clock and synthetic sensitive text rather than real private history. Run `retention`, `memory`, `store`, `recovery` and U05/M05/M07. Segment 11 adds retained-cloud-copy and restoration tests.

## Exit

Done when users can inspect all retained activity, choose exact pruning scope, see 30-day notices without default deletion, and recover from interrupted cleanup without stale recall or lost required accounting. Evidence must distinguish logical exclusion, local physical cleanup and backup-copy retention.
