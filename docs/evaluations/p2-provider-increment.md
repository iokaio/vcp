# P2 provider gateway increment

P2-02 remains `in_progress`. The gateway now validates captured context before
retained HTTP admission, parses bounded Responses streams and settles observed
cost. P2-01 retained request integration is exercised on both canonical backends.
This is a scripted qualification increment, not live model compatibility or a
complete coding loop.

## Executed qualification

Qualification uses Windows 10.0.26200, NTFS, Rust 1.98.0 and MSVC 14.50.35717.
The compiler is an explicit experiment; the retained upstream 1.95 pin is
unchanged. Manifest commit IDs identify the base checkout; per-input hashes
identify the tested implementation before its commit. Raw evidence stays ignored.

`pwsh -NoProfile -File scripts/test-provider.ps1` passed, exit 0: sixteen
provider/context contracts and one retained deadline test with three stream
modes. Evidence: `artifacts/provider/d8a6dbb4-51d1-4ecd-a1b0-c790a6a00bf6/manifest.json`,
SHA-256 `1faf3bc6be64cef542f1354c4594470395354425ad9e5985c1b82928c0b5fc56`.
Contract log SHA-256:
`7a064950d7fb2c4c0a3a4ef2d1cdf686e9ec191598f6c27cd31767bc0c58e122`.
Retained deadline log SHA-256:
`0fbce564dcca481c30f640c3b695f68df0883ba3012c1b3561687371b657983e`.

`pwsh -NoProfile -File scripts/test-integration.ps1` passed, exit 0: 33 native
host/port contracts, two retained process-containment regressions and the
independently observed seven-request private CLI trace. Evidence:
`artifacts/integration/8271b237-f48f-45e9-a336-7bd866b6b30e/manifest.json`, SHA-256
`7c9068389938f9ca886713cca36c692673cc3a45d8309b53f9352d93b23675de`.

The guide's explicit `build-baseline.ps1 -Mode RecoveryTests` command passed,
exit 0: four journal and sixteen host cases, independent owner-recovery and
execution-boundary qualification. Evidence:
`artifacts/build/fe8acea2-a223-4eed-9106-99f52fdfcfc0/manifest.json`, SHA-256
`ae125baf2c9cd60cf06e58e69a0e18ee30d2d65f0e03c82c6da451edf07901d6`.
The native observer saw two job members become zero and the exclusive lock
become available in 16 ms; this fixture observation is not a latency guarantee.

The guide's explicit `build-baseline.ps1 -Mode LifecycleTests` command passed,
exit 0: all 29 retained regressions (13 core drain/continuation/scoped-start
cases and 16 host cases). Evidence:
`artifacts/build/5c306e83-0e29-41e6-b56f-3fc4aa2a5623/manifest.json`, SHA-256
`58e20e66ae4376a62d35076f15999cab01117fd3826798e09d97f9f1300082c4`.

`pwsh -NoProfile -File scripts/test.ps1 -Suite fast` passed all eight cases,
exit 0; evidence is `artifacts/tests/13f00112-44e7-413b-9bb3-0f390dec0287/manifest.json`.
Final documentation passed the repository checker and staged whitespace check.
One trailing blank line was removed from the original provider Cargo manifest
after the provider/integration runs; its dependency declarations are unchanged.

Independent reconstruction from Codex `3d3ae4965ab370217e871b3a7f0d15589557ee4b`
and ordered patches matches all 7,938 source files, aggregate SHA-256
`84ff18ca54aab9f7f6db53bc61a28d955472f36c95254b9ecef2e81eabb4981b`.
Patch 0015 SHA-256:
`4c951969c037aa78d047a8eac126707ed273588335df00ef817700bd3ac4fefa`.
External Cargo dependency entries and checksums are unchanged. The static
boundary inventory covers 170 packages, 33 groups and 70 seam anchors.

## Observed boundaries

| Boundary | Evidence |
|---|---|
| Catalog | Dated compatibility, exact metadata hash, required parameters, conservative decimal pricing and explicit request ceiling; missing/stale/ambiguous inputs reject |
| Conversion | Exact sealed bytes, attributed roles and source ranges, local function schema subset, explicit provider restrictions and disabled implicit fallback |
| Stream | Every UTF-8/CRLF split, interleaved tool identities, malformed/truncated/duplicate events and byte/event/argument bounds |
| Tool eligibility | Only complete schema-valid calls agreed by the terminal become proposals; no fragment grants execution |
| Admission | Independent HTTP observer sees captured manifest, submitted attempt and send intent before exact body arrival |
| Freshness | Real file edits and steering changes reject before any billable attempt; user bytes remain unchanged |
| Accounting | Observed cost settles on SQLite and files; missing cost and header timeout retain liability; absent served identity stays unknown |
| Deadline | Silent, periodic-keepalive and continuously ready streams stop at an absolute bound |
| Retained behavior | Canonical host, native process containment, private CLI and recovery/lifecycle regression runners |

The source guide documents [the implementation and reproduction commands](../development/p2-provider.md).
Inputs are original synthetic metadata, prompts and loopback responses. A public
endpoint catalog and primary provider documentation informed the codec but are
not passing live compatibility evidence. No credentials or paid model calls are
used by these deterministic checks.

## Remaining acceptance

Automatic retry orchestration must connect the bounded policy to fresh context,
predecessor-linked attempts and reservations in P2-05. The current transport
performs zero implicit retries. A separately capped live smoke run must qualify
an actual model/provider pair, tokenizer estimate and policy enforcement before
support is advertised. P2-03 through P2-08 still own policy, prepared tools,
coding orchestration, verification, production close/reopen and compaction.

Unknown costs remain reserved. The captured response describes bytes observed
through the retained terminal boundary, not unseen trailing network bytes.
No optional repository-map benefit, general sandbox guarantee, paid-provider
support or installable product is claimed by this increment.
