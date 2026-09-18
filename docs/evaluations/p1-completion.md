# P1 canonical foundation qualification

The P1 implementation consists of the [typed foundation](../development/p1-foundation.md),
[accounting/history](../development/p1-accounting-history.md) and
[retained canonical host](../development/p1-retained-host.md). All six P1 tasks
are complete within their foundation contracts. The native acceptance and
retained regression results below support the [task ledger](../plan/20-traceability.md).

| Task | Implementation and acceptance boundary |
|---|---|
| P1-01 | Typed scope/revision/lifecycle domains, current verification, preserved objectives and stable workspace binding; retained tasks and native effects consume these contracts |
| P1-02 | Private commands, authenticated durable receipts, JSONL parity, operation-bound decisions, scoped sequences and bounded subscriptions |
| P1-03 | Full request capture before send, raw observed response capture before parsing, native stdout/stderr before display limits, secret-header separation and failure fences |
| P1-04 | Shared SQLite/files constraints, combined task/artifact/reservation/event/receipt crash recovery, writer ownership, conversion and atomic activation |
| P1-05 | Checked integer pricing, one root ledger, child/protected/daily accounting, immutable observations, late corrections and unresolved liabilities; actual root/helper/compaction/child HTTP arrival requires prior admission |
| P1-06 | Deterministic folds, version/watermark atomicity, current-authority snapshot filters and retention gaps; fresh-process reconstruction preserves an actual unknown process, paused child and late charge |

## Native commands and results

On September 18, 2026, the following checks used Windows 10.0.26200 / NTFS,
Rust 1.98.0 (`88d9e12ae`, August 18, 2026) and MSVC 14.50.35717. Source inputs
stayed unchanged during qualification. Commands are reproduced in the
[host guide](../development/p1-retained-host.md#source-and-qualification).

- `pwsh -NoProfile -File scripts/test-p1.ps1`: **pass**, exit 0, all 70 named
  contracts (39 foundation/accounting/history, 27 retained P0 and four canonical
  host integration contracts). Evidence:
  `artifacts/p1/f5ec580f-99a0-4a27-9b8d-cecd463b34c4/manifest.json`, SHA-256
  `4008e3e4a9edd7aaedac515e6bb9939463cca9ba98c043bada0c9df1951cdd56`.
  Contract log SHA-256:
  `42d2a7f26fb050cb00af7eee9591f407c522595d4f87744369252ce328c23a2d`.
- `pwsh -NoProfile -File scripts/test-integration.ps1`: **pass**, exit 0,
  31 host contracts, both retained native containment regressions, example build
  and the independently observed seven-request private CLI trace. Evidence:
  `artifacts/integration/77744779-fe25-4259-8d19-9820d3fc075f/manifest.json`, SHA-256
  `6ce56181426a2c7171bf78e34f2562e0ccd08441a80711777480b95f7b91556d`.
- `build-baseline.ps1 -SelectedCodex -Mode RecoveryTests -ExperimentToolchain 1.98.0`
  with the output/target arguments in the guide: **pass**, exit 0. Four journal
  and sixteen host regressions passed, followed by independent owner-recovery and
  execution-boundary qualification. Evidence:
  `artifacts/build/08ef114a-d2e5-47da-af2d-625b2caa17e9/manifest.json`, SHA-256
  `e65f4c447f75353936271e80d73be3ad06a70be8aed1138b4ef5565c6b952f9a`.
  Nested recovery manifest SHA-256:
  `b81f6598fce3b0c766e5d8d9063a3cfc9d703b89c6e5c50ec33daa4470ff4cc8`.
  The native stop observer saw two job members become zero and the exclusive
  lock become available in 15 ms; this is a fixture observation, not a latency SLA.
- `build-baseline.ps1 -SelectedCodex -Mode LifecycleTests -ExperimentToolchain 1.98.0`
  with the output/target arguments in the guide: **pass**, exit 0, all 29 retained
  regressions (13 core drain/continuation/scoped-start cases and 16 host cases).
  Evidence: `artifacts/build/f874e639-95b5-4fa3-ad2f-b07aaebf42d3/manifest.json`,
  SHA-256 `7b3cea67ed8765f6e04eea88ea0bb53d4921bcdbbf5fe5fa047877a2d987361d`.
  Detailed results:
  `lifecycle/9396294f-e4fa-497c-807e-de700ad0ea0c/manifest.json` under that run.
- `pwsh -NoProfile -File scripts/test.ps1 -Suite fast`: **pass**, exit 0, all
  eight repository/harness/source/boundary cases; run
  `artifacts/tests/96d2c216-cad3-4011-900b-857e9ca0562f/manifest.json`.

These commands have overlapping test coverage. The manifest commit identifies
the base checkout; per-source hashes
identify the implementation tested before its commit.

An initial generic recovery invocation selected the preserved upstream Rust 1.95
pin and was interrupted without a qualification result. The successful run uses
the explicit common Rust 1.98 command; upstream toolchain metadata is unchanged.

## Source and limitations

Independent reconstruction from the immutable Codex selection plus patches
0001–0013 reproduces all 7,938 source files, aggregate SHA-256
`bd9f2411a6602d35f6a035197ca5f141533039d4b589a08a0a9ec69121b66e14`.
Patch 0013 SHA-256 is
`1775252d090f164c8e6d5d01d2fdf4be1ddc64400d16405e6e26e93d6679efdc`.
External dependency identities/checksums are unchanged. The explicit inventory
covers 167 packages, 30 review groups and 64 effect seams.

Qualification uses synthetic loopback responses and prices, native contained
process fixtures, plaintext local canonical stores and ignored evidence roots.
No paid provider calls or hosted Windows jobs are dispatched. Forced-process
termination and explicit spool-capacity failures do not establish physical
disk-full behavior, hardware power-loss durability or final package installation.

The first retained fixture exposed two configuration mismatches: the P0 profile
overrode its synthetic bearer token, and child startup did not automatically
inherit the root's tool restriction. The fixture now supplies both explicitly;
the host's rejection of unqualified tools remains enforced. Request/response
observation does not rely on the retained controller reporting its own success.

P2/P3 retain production OpenRouter configuration and capability qualification,
execution policy/tool brokering, context reconstruction and usable CLI work.
Later phases retain routing, local memory/retrieval, retention operations and
encrypted handoff. P1 completion is a qualified canonical foundation, not the
first usable VCP release.
