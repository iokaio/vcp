# P1 accounting and history qualification

This report records the second increment. The subsequent
[P1 completion qualification](p1-completion.md) covers retained integration and
the current task status without changing the historical results below.

The second P1 increment implements checked accounting and deterministic history
over the canonical foundation. P1-01 through P1-06 remain `in_progress` because
the retained controller's complete request and output paths still need integration.
The [implementation guide](../development/p1-accounting-history.md) documents
formats, commands, bounds and the fixed-offset local-root daily scope.

The native qualification runs 39 contracts: the 24 foundation regressions,
10 accounting contracts and five history contracts. Synthetic prices are test
inputs, not production pricing. No paid provider or hosted Windows run is used.

On September 18, 2026, `pwsh -NoProfile -File scripts/test-accounting-history.ps1`
passed all 39 contracts (exit 0) on Windows 10.0.26200 / NTFS, Rust 1.98.0
(`88d9e12ae`, August 18, 2026) and MSVC 14.50.35717. Source inputs stayed
unchanged during qualification. Evidence run:
`artifacts/accounting-history/5d2dffe0-a943-43ad-9c3b-ffa7d7bd5435/`.
Manifest SHA-256:
`c6287f43bb62f1e34590b59e55776ddc4b264d7627bef99bc4749ea77d99248d`.
Contract log SHA-256:
`307508f78b5594457836f7b0f3e79006a07ceda541beafd6a130129020d8418c`.

`pwsh -NoProfile -File scripts/test.ps1 -Suite fast` passed all eight cases
(exit 0), run `547ffa3c-cf55-4cec-8e37-59c1a6735cdb`.

| Boundary | Observation |
|---|---|
| Arithmetic | Checked integer rounding and overflow; all six charge categories explicit; cache partitions and reasoning subsets avoid double billing |
| Concurrent admission | Two requests race against settled, active, unknown and protected funds; only one commits and the loser cannot reuse a stale allow |
| Accounting authority | Child allocation, daily liability, verification draw and cap reduction preserve one root; direct malformed transactions fail canonical constraints |
| Reconciliation | Cumulative versions, incremental ranges, corrections, retries, explicit uncertainty and late overrun preserve immutable observations and adjustments |
| Combined transaction | Eight process kills across both stores preserve all-or-none request artifact, revised task, reservation, attempt, ledger, event and receipt; retry is idempotent |
| Backend conversion | Both directions preserve accounting state, receipt identity and full captured request bytes after crash recovery |
| Projection rebuild | A separate process rebuilds after removing only projection records; at the same watermark its result equals the live fold without changing an external effect canary |
| Projection activation | Four process kills leave the old or new version with its matching input watermark; reopen converges to the deterministic fold |
| History | Paused children, fork provenance, unknown effects and late charges persist; filtered snapshot reads enforce current authority, expiry, masks and artifact access |

The fixtures qualify native forced-process failure on Windows/NTFS. They do not
establish hardware power-loss durability or production provider behavior. The
unknown process outcome in the history fixture is a synthetic canonical fact;
retained process/model observations remain separate acceptance work.

Independent reconstruction from the immutable Codex source plus patches 0001–0012
reproduces all 7,938 files, aggregate SHA-256
`e33573af8d77cd0028bda0bf64264cbd7d8132dbf4fecfd82cd5a451b5b5bffd`.
Patch 0012 only registers two original workspace packages and their local lock
entries; all external dependency identities and checksums remain unchanged.
The explicit boundary inventory covers 167 packages, 30 groups and 61 seams.

The first combined runner attempt returned failure because a child process's
diagnostic digest interrupted Cargo's named result line: Cargo passed all 39
tests, while the evidence parser counted 38. The fixture now isolates that
stdout; the expected count and qualification gate remain unchanged.
