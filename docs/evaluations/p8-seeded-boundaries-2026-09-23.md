# P8/M9 seeded boundary qualification — 2026-09-23

Status: implementation, targeted native tests and all three selected source-bound
campaign rows passed. This is a bounded synthetic
extension to the [P8 continuation](p8-continuation-2026-09-23.md), not P8-05 owner
acceptance or actual production delegation. No paid model call is authorized by
this campaign.

## Frozen scope and oracles

The generator selects operation sequences; a separately written state or
visibility model judges their effects. Fixed seeds control synthetic operations,
never encryption keys, authentication material or filesystem identity. Failure
output must identify the seed, backend and replayable operation prefix. Neither
passing production assertions nor the generator's choice establishes legality.

Controller traces use seeds `1`, `0x5eed`, `0x8a01`, `0xc0ffee`, with 48 generated
operations plus an 18-operation coverage prefix and 12-operation suffix per seed
on Files and SQLite: 624 total operations, including 384 generated choices.
They drive real CanonicalHost/Engine boundaries. The oracle checks legal state
changes, expected revisions, stale/foreign controller admission, repeated pause,
durable receipt replay and cooperative owner close/reopen. These traces create
no provider attempts, reservations, ledger entries or external effects.

Retention traces use seeds `0x5eed`, `0xc0ffee`, `0xbadc0de`, `1`, with 28 operations
per seed/backend: a 12-operation coverage prefix and 16 generated choices. Two
governed claims in distinct task scopes supply the truth set. The independent
Visible/Hidden/Erased model checks exact scoped source visibility, stale refusal
without canonical mutation, repeated prune and reopen. Each trace ends with an
authenticated encrypted restore into the opposite store and another visibility
check. This is logical exclusion/deletion evidence, not physical media erasure.
The default campaign totals 224 operations and eight restores.

Accounting traces use seeds `1`, `0x5eed`, `0x8a02`, `0xc0ffee`, with six generated
attempt episodes per seed on each store. The first episode exercises release,
the last a late overrun, and four intermediate episodes use generated choices
and amounts. A separate arithmetic model tracks held, unknown and settled money
from the inputs, rather than reading expected values from the ledger under test.
Real budget APIs exercise admission, submit, uncertain holds, partial/late final
usage, duplicate and stale observations, and reopen. Rejected operations must not
mutate state; an acknowledged stale usage observation may append its audit record
but must report `applied=false` and leave accounting unchanged. No provider
transport or synthetic charge against a real account occurs.
The default campaign totals 48 episodes.

## Execution contract

The versioned P8 manifest adds `p8-01-seeded-controller-traces`,
`p8-01-seeded-retention-restore` and `p8-01-seeded-liability-traces`.
Run only those rows for this increment and retain
the source-bound receipt, stdout/stderr and toolchain. Unselected rows stay
not-run in that receipt; existing hand-authored evidence keeps its original
artifact and scope. Test discovery must observe the exact named passing tests;
a zero-test command is not a pass.

Replay-only environment overrides must be cleared for the default qualification.
A reduced seed or failing-prefix reproduction is diagnostic evidence, not a
replacement for the complete declared seed set and bounds.

## Targeted native results

Rust 1.95.0 on the current native Windows workstation passed all three targeted
tests and changed-file formatting. No production implementation changed.

| Boundary | Default executed coverage | Result |
| --- | --- | --- |
| Controller | 624 operations: 138 accepted, 372 denied, 54 exact receipt replays and 60 cooperative reopens; each reopen also probes closed/stale owners | Passed on Files and SQLite |
| Retention/restore | 224 operations: 86 accepted, 32 post-purge refusals, 40 stale previews, 26 wrong-scope operations (two denials each), 40 reopens; eight authenticated cross-backend restores | Passed on Files and SQLite |
| Accounting | 48 episodes, 534 budget API operations, 96 reopens, 206 denials, 96 exact retries, 34 audited stale records and 34 late final settlements | Passed on Files and SQLite |

Logs include `artifacts/p8-seeded-retention-exploratory-3.log` and
`artifacts/p8-m9-accounting.log`; the controller's exact named test passed in the
native `canonical_host` binary. The formal campaign retains all three complete
outputs under one source identity.

Exploratory failures remain retained. Retention's test-only access helper needed
explicit field copying because `Access` does not implement `Clone`. Its first
executable run then found an incorrect oracle expectation: repeat `Purge` is
explicitly permitted by `retention.rs`, while non-purge mutations of a purged
target remain forbidden. The corrected oracle preserves those refusals, exact
scoped visibility and no-resurrection checks; product behavior was unchanged.
The initial formal runner attempt failed all selected commands before discovery
because the process environment contained an empty Cargo target override. Receipt:
`artifacts/p8-seeded-campaign/66c89cf2-83f2-44ca-a722-49319c06da71/manifest.json`.
The override was removed and the same frozen source and seed sets were rerun.

The formal rerun passed all three selected rows with no failures:
`artifacts/p8-seeded-campaign/a943a6ce-a030-4fbe-99ff-6526aa17903f/manifest.json`,
SHA-256 `a5b0d31cab06e4c413dd7fa5b7c201523aabf500a2390861e987d98f5055d00f`.
Before/after source digest matched
`e70a5007a7ec752329c971a72c9a689aa89d1467a0c871c2b4daa851aeadace8`.
The whole manifest correctly remains **incomplete**, with 47 unselected rows
not-run; this targeted execution is not a new full P8 matrix pass.
All 17 fast delivery groups also passed:
`artifacts/p8-seeded-fast/c21cae30-0abd-499c-bdb8-6490343fac0f/manifest.json`.

## Remaining M9 coverage

This increment does not claim generated scripted-provider bursts, forked
delegation, late or unknown non-billing external effects, process-kill acknowledgement
durability, minimum hardware or packaged actual delegation. Existing adversarial
fixtures supplement these traces; they are not relabeled as generated coverage.
M9 remains partial until its uncovered enabled boundaries and production
integration have appropriate evidence.
The subsequent [provider and child campaign](p8-seeded-provider-children-2026-09-23.md)
records those additional bounded synthetic scenarios separately from this run.
