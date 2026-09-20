# 03 — Canonical storage and atomic budget accounting

Status: P1-04/05 complete with [native acceptance evidence](../evaluations/p1-completion.md) for the [durable foundation](../development/p1-foundation.md), [accounting/history](../development/p1-accounting-history.md) and actual retained transport admission. Owns P1-04 and P1-05. Requires P1-01/P1-03 and P0 storage qualification; uses P1-02 command envelopes. Read architecture sections 8 and 12 and [state/capture](02-engine-state-and-capture.md). Production provider normalization and encrypted transfer retain their later acceptance gates.

## Implementation references

[Canonical storage](../architecture/vcp-what.md#12-canonical-storage-files-or-sqlite)
and [root budget enforcement](../architecture/vcp-what.md#8-cost-accounting-and-budget-enforcement)
define the required behavior. Use the proposed
[storage and portability design](../architecture/storage-portability-design.md)
for physical layouts/activation, and
[engine transaction and accounting design](../architecture/engine-execution-design.md#atomic-budget-admission-and-settlement)
for cross-module ordering. [ADR-003](../adr/003-canonical-storage.md) owns the
backend decision. The implementation guides record the selected settings,
integer microcurrency and physical formats qualified on native Windows/NTFS.
Hardware power-loss and final package qualification remain later release gates.

## Code organization and contracts

Implement logical modules under `vcp-store`: `contract`, `transaction`, `snapshot`, `schema`, `artifacts`, `sqlite`, `files`, `migration`, `integrity` and `writer_lock`. Put cost arithmetic, reservation policy and attribution in `vcp-budget`, using store transactions through an interface. Backend-specific errors translate into canonical conflict, unavailable, corruption, incompatible-format and durability errors.

`CanonicalStore.transact` accepts transaction ID, expected revisions, appends, projection updates, indexing intents and idempotent result. It returns an acknowledged sequence/durability receipt only after the selected backend's tested commit boundary. No model/network call is held inside a transaction. Snapshot readers pin a coherent view and artifact references.

## P1-04 — Storage increments

The P7-03 numeric audit reproduced an existing literal-JSON replay failure under
the current serde feature graph. The [decode maintenance increment](../development/p1-persisted-json.md)
and [qualification](../evaluations/p1-persisted-json.md) own its correction before
broader MCP numeric integration; stored bytes and receipt hashes must remain
unchanged.

1. Define logical collections for workspaces, sessions/tasks, events, operation/idempotency receipts, artifacts/evidence, grants, reservations/settlements, claims, indexing intents, generations and tombstones. Specify unique keys, foreign-reference rules, scoped sequences and migration version.
2. Implement SQLite transactions and constraints first as the default candidate. Measure and record connection/journal/synchronization settings on supported Windows filesystems. Stage external artifacts before committing references; failed transactions may leave reclaimable orphans, never dangling acknowledged references.
3. Implement the files preference against the same contract: versioned transaction frames, lengths/checksums, durable commit marker, replayable journal, sealed checkpoints and tested pointer publication. Distinguish a recoverable torn tail from corruption inside committed history; do not skip the latter silently.
4. Add one-writer ownership per active data root. Readers/snapshots receive declared consistency guarantees. Reopen handles stale owner metadata without treating a recorded PID alone as proof of a live owner.
5. Implement integrity scans, export/import backend conversion and format migration. Validate a replacement root before activation; retain a compatible recovery snapshot. Reject unsupported explicit preferences instead of changing backend silently.

No production choice gets advertised until its shared tests pass. Reuse a single artifact layout and generation contract for both backends. All active data is plaintext; cloud encryption belongs at the snapshot publication boundary, not in SQL or individual journal records.

Implement a backend-neutral transaction fixture before backend-specific schema
work. The fixture should append an event, revise a task, admit a reservation,
attach a finalized artifact and return an idempotent command result in one
operation. Kill the process before/after the commit boundary and independently
inspect all records after reopen. A storage API returning `Ok` without a tested
durability receipt is insufficient.

| Proposed record constraint | Implementation consequence |
|---|---|
| `(scope, command_id)` unique with canonical payload digest | Lost-reply retry resolves once; different payload conflicts |
| Event ID and `(session_id, session_seq)` unique | Re-delivery cannot duplicate events or reorder a session |
| Attempt → reservation and root task are valid | A submitted request cannot exist without admitted accounting |
| Finalized artifact descriptor resolves to verified content | External file staging precedes canonical reference publication |
| Entity revision matches expected revision | Concurrent writers cannot silently overwrite accepted state |
| Index intent bound to source transaction | Later memory indexing can retry without losing canonical acceptance |

For SQLite, map these constraints to a versioned schema and explicit transaction
boundaries. Translate busy/locked, I/O, corruption and unsupported-format errors
into typed store failures. Bound contention retries outside a transaction using
the controller deadline; never treat repeated busy results as successful commit.
The selected journal/synchronization/connection settings and backup procedure
must be recorded with actual Windows filesystem evidence. Do not copy a live
database and unrelated artifact directories and call it a coherent snapshot.

For files/journal, first define a versioned binary or textual frame specification:
header, transaction identity, scoped sequence, length, integrity metadata and
commit marker. Replay only complete verified committed transactions. Validate a
checkpoint manifest and its journal boundary before following the active
pointer. Treat that pointer as publication metadata with tested replacement
semantics; if it is lost or torn, recovery must either identify a verified active
state or report explicit ambiguity. A torn uncommitted tail may be quarantined;
corruption inside committed history must stop ordinary writes.

Writer ownership spans canonical records, ledger and activation, not individual
collections. Store process-start identity/nonce with the lock so stale metadata
cannot authorize terminating an unrelated reused PID. Test abrupt death while
read snapshots are pinned and ensure cleanup retains referenced artifacts and
journals. Keep shared scan/order/retention behavior out of backend-specific CLI
code.

Migration builds a separate destination from a pinned logical snapshot, validates
IDs, sequence boundaries, reference closure and ledger totals, then records a
controlled activation. A backend switch is not changing a TOML field while the
writer remains active. Preserve source state for the defined recovery period;
do not merge post-migration writes back into it implicitly. P5-09/P5-10 later add
encrypted transport and cross-machine lineage to this same logical snapshot.

## P1-05 — Ledger increments

Use fixed-precision decimal or integer microcurrency with explicit currency and checked arithmetic. A root ledger contains cap, settled charges, active reservations, unresolved charge reserves and protected verification reserve. A child allocation is a subdivision, not extra available money or a second charge.

1. Implement a pure admission calculator with per-attempt maximum output and known charge categories. Missing required price/capability information blocks admission or requires an explicitly qualified policy; unknown cannot become zero.
2. Commit reservation, task/attempt revision and event atomically. Concurrent children contend at this boundary; allocating a child budget alone does not admit a request.
3. Implement the architecture's `created`/`submitted`/`settled`/`released`/`reconciliation_pending`/`explicitly_resolved` lifecycle and idempotent usage reconciliation. Before-send cancellation can release an undispatched reservation; ambiguous post-send outcomes preserve liability. Each retry creates a new attempt linked to its predecessor.
4. Attribute main, helper, compaction, reviewer, child, optimizer and model-assisted memory costs to their root/maintenance owner. Record local embedding CPU/RAM/disk separately from remote charges.
5. Preserve liabilities through pause, restore, policy edits and backend migration. Define optional daily policy scope/timezone; disconnected machines cannot claim a synchronized global lock.

If actual usage exceeds an estimate, record the truth, mark the overrun and block further unaffordable work. Never claim the local ledger can guarantee a provider invoice when price/usage is incomplete.

Separate the pure quote/admission arithmetic from persisted ledger mutation.
Proposed `CostQuote` fields are currency, amount precision, input/output bounds,
known per-request/cache/tool categories, price snapshot, estimation method and
unknown categories. Missing required price information is a typed inability to
quote; it is never represented by a zero-valued category. Preserve raw provider
usage beside normalized disjoint charges for later audit.

Use this implementation order:

1. Write checked arithmetic and serialization fixtures, including rounding up
   reservation estimates, overflow and different currencies/price units.
2. Add root and child limit evaluation over one snapshot, returning a reasoned
   allow/deny result and required aggregate revisions.
3. Commit reservation + attempt + root/child aggregates + event with those
   expected revisions. On a race, reread and recompute; do not retry the old
   allowed result against changed totals.
4. Fence submission with durable attempt/send intent before transport access.
   Release only with positive no-send evidence. Preserve potentially billed
   reserves after cancellation, stream loss, CLI pause and process crash.
5. Normalize cumulative versus incremental usage, settle by unique observation
   identity and retain late corrections as explicit adjustments. Repeated terminal
   usage does not create another charge.

For a synthetic arithmetic fixture, a USD 2.00 cap minus USD 0.40 settled,
USD 0.30 active, USD 0.20 unresolved and USD 0.10 protected leaves USD 1.00.
Two racing USD 0.65 requests cannot both be admitted. A child allocation does not
increase that USD 1.00 or count as spend. Drawing the protected reserve for an
eligible verification attempt must atomically reduce its protected amount so the
same funds are not counted twice. These values are fixtures, not product defaults.

When a provider exposes overlapping totals, retain an explicit normalization
version and source fields; do not add reasoning/cache token subtotals to a total
that already includes them. Late usage after a task is paused or completed still
updates accounting and may reveal an overrun. Completion reports need a cost
certainty marker so a formerly estimated total is not mistaken for an invoice.
An explicit resolution of unavailable usage records actor/policy/reason and the
remaining uncertainty; it does not rewrite the failed attempt as free.

Extend the race fixture through root/helper/compaction/child entry points in the
retained engine. Instrument the actual transport boundary so an accidental
upstream helper request without a reservation fails the test. The ledger alone
cannot demonstrate that every caller obeys it.

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

Run the implemented `scripts/test-p1.ps1` and native recovery/integration commands in the [host guide](../development/p1-retained-host.md#source-and-qualification); the proposed shared `store`/`recovery` suite names are not implemented. Coverage includes the P1 portions of E10/E12/E14, M01/M02/M07/M08 and I-03/I-04/I-05/I-13/I-17. Post-reopen record/receipt/ledger comparisons and the exact filesystem/process-failure envelope are recorded in [P1 qualification](../evaluations/p1-completion.md). Encrypted transfer and hardware-loss limitations remain explicit.

P1-04 completes when both advertised backends behave equivalently on canonical contracts. P1-05 completes when every integration path can require a reservation before a billable request and unsettled charges survive restarts and transfer. Later encrypted restore tests recheck the same ledger invariants.
