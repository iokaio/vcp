# 03 — Canonical storage and atomic budget accounting

Status: planned. Owns P1-04 and P1-05. Requires P1-01/P1-03 and P0 storage qualification; use P1-02 when wiring command envelopes. Read architecture sections 8 and 12 and [state/capture](02-engine-state-and-capture.md).

## Code organization and contracts

Implement logical modules under `vcp-store`: `contract`, `transaction`, `snapshot`, `schema`, `artifacts`, `sqlite`, `files`, `migration`, `integrity` and `writer_lock`. Put cost arithmetic, reservation policy and attribution in `vcp-budget`, using store transactions through an interface. Backend-specific errors translate into canonical conflict, unavailable, corruption, incompatible-format and durability errors.

`CanonicalStore.transact` accepts transaction ID, expected revisions, appends, projection updates, indexing intents and idempotent result. It returns an acknowledged sequence/durability receipt only after the selected backend's tested commit boundary. No model/network call is held inside a transaction. Snapshot readers pin a coherent view and artifact references.

## P1-04 — Storage increments

1. Define logical collections for workspaces, sessions/tasks, events, operation/idempotency receipts, artifacts/evidence, grants, reservations/settlements, claims, indexing intents, generations and tombstones. Specify unique keys, foreign-reference rules, scoped sequences and migration version.
2. Implement SQLite transactions and constraints first as the default candidate. Measure and record connection/journal/synchronization settings on supported Windows filesystems. Stage external artifacts before committing references; failed transactions may leave reclaimable orphans, never dangling acknowledged references.
3. Implement the files preference against the same contract: versioned transaction frames, lengths/checksums, durable commit marker, replayable journal, sealed checkpoints and tested pointer publication. Distinguish a recoverable torn tail from corruption inside committed history; do not skip the latter silently.
4. Add one-writer ownership per active data root. Readers/snapshots receive declared consistency guarantees. Reopen handles stale owner metadata without treating a recorded PID alone as proof of a live owner.
5. Implement integrity scans, export/import backend conversion and format migration. Validate a replacement root before activation; retain a compatible recovery snapshot. Reject unsupported explicit preferences instead of changing backend silently.

No production choice gets advertised until its shared tests pass. Reuse a single artifact layout and generation contract for both backends. All active data is plaintext; cloud encryption belongs at the snapshot publication boundary, not in SQL or individual journal records.

## P1-05 — Ledger increments

Use fixed-precision decimal or integer microcurrency with explicit currency and checked arithmetic. A root ledger contains cap, settled charges, active reservations, unresolved charge reserves and protected verification reserve. A child allocation is a subdivision, not extra available money or a second charge.

1. Implement a pure admission calculator with per-attempt maximum output and known charge categories. Missing required price/capability information blocks admission or requires an explicitly qualified policy; unknown cannot become zero.
2. Commit reservation, task/attempt revision and event atomically. Concurrent children contend at this boundary; allocating a child budget alone does not admit a request.
3. Implement reserved/dispatched/settled/released/unresolved lifecycle and idempotent usage reconciliation. Before-send cancellation can release an undispatched reservation; ambiguous post-send outcomes preserve liability. Each retry creates a new attempt linked to its predecessor.
4. Attribute main, helper, compaction, reviewer, child, optimizer and model-assisted memory costs to their root/maintenance owner. Record local embedding CPU/RAM/disk separately from remote charges.
5. Preserve liabilities through pause, restore, policy edits and backend migration. Define optional daily policy scope/timezone; disconnected machines cannot claim a synchronized global lock.

If actual usage exceeds an estimate, record the truth, mark the overrun and block further unaffordable work. Never claim the local ledger can guarantee a provider invoice when price/usage is incomplete.

## Shared conformance matrix

Run every applicable case with `-Backend sqlite`, `files` and `both`:

| Fixture | Injection/action | Expected result |
|---|---|---|
| Multi-record transaction | Kill before/after commit and before reply | All-or-none canonical state; acknowledged commits survive tested crash; retry returns same receipt |
| Artifact reference | Fail payload finalization or transaction | No missing finalized payload referenced; orphan staging is reclaimable |
| Writer collision | Open two writers; kill one; retry | Exactly one writer; bounded error/recovery with no interleaved journals |
| File journal | Truncate uncommitted tail; corrupt committed middle | Defined tail recovery; explicit integrity failure for committed corruption |
| SQLite contention | Hold lock, fail disk write, reopen | Bounded busy/failure handling; no false acknowledgement |
| Migration | Kill before validation/activation and after switch | One recognized active format; old valid root recoverable; no partial mixed schema |
| Budget race | Simultaneous requests near root cap | Only affordable reservations admitted; attributed totals balance |
| Usage retry | Duplicate/late usage and ambiguous disconnect | Settlement applied once; unknown reserve retained until resolved |
| Restored task | Transfer with open reservations and child allocations | Spend does not reset, allocations are not double-counted |

Add generated arithmetic/boundary cases for overflow, rounding, currency mismatch and monotonic reservations. Failure injection uses real child-process termination in addition to returned mock errors.

## Exit and evidence

Run `store` and `recovery`; include E10/E12/E14, M01/M02/M07/M08 and I-03/I-04/I-05/I-13/I-17. Capture post-reopen record/receipt/ledger comparisons from an independent oracle. Publish the exact tested durability envelope, including filesystem and forced-process versus hardware-loss limitations.

P1-04 completes when both advertised backends behave equivalently on canonical contracts. P1-05 completes when every integration path can require a reservation before a billable request and unsettled charges survive restarts and transfer. Later encrypted restore tests recheck the same ledger invariants.
