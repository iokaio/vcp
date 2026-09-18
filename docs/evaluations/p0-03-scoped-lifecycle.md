# P0-03 scoped lifecycle host qualification

Scope: the [in-memory lifecycle host](../development/scoped-lifecycle.md),
thread-aware retained admission, owned interruption and explicit readmission.
P0-03 remains `in_progress`; this is not durable pause, CLI integration or release
acceptance. Inputs are public synthetic fixtures and local scripted providers.

## Source identity

- Codex origin: `3d3ae4965ab370217e871b3a7f0d15589557ee4b` with ordered patches
  0001 through 0007 and the recorded Windows license transformation.
- New patch SHA-256:
  `8bd7c175b0f2a0ca7cdf8a55f541925dc5994088956fbefd9b0a6be78394c1c3`.
- Independently reconstructed 7,937-file result digest:
  `2853a063526962d82a4c12ca02cdbb58ecb42f1b92c29d7393947059480c849a`.
- Original host source: `src/crates/vcp-lifecycle/`; one local package added to
  Cargo.lock, with all 1,566 prior package records unchanged. The static catalog
  covers 160 packages, 24 groups and 51 seams.

## Recorded checks

The first native compile rejected a test helper's missing `ExtensionRegistry`
configuration type. Specifying the retained core `Config` fixed that source
error. The failed compiler log is retained under
`artifacts/lifecycle-proposal/host-build-first-failure.jsonl`.

| Check | Result and evidence |
|---|---|
| Independent upstream reconstruction | Pass; `artifacts/upstream/codex-scoped-lifecycle-reconstructed.json`, exact result digest above |
| Working-tree source verification | Pass; `node scripts/upstream/reconstruct.cjs verify --component codex` |
| Static package/effect coverage | Pass; `node scripts/upstream/check-boundaries.cjs`, 160 packages/24 groups/51 seams |
| Observer false-success regressions | Pass; three cases including exact artifact selection, duplicate/missing/skipped result rejection and Cargo local-package identity |
| Native retained-core and host suite | Pass; 20 cases, zero failures/ignored, documented wrapper evidence at `artifacts/build/4b501031-1202-454c-a03f-206e68e33a3b/manifest.json` |
| Local fast suite | Pass; eight cases, `artifacts/tests/98d4ff1c-942c-4003-ad15-c6c63f027bba/manifest.json`; observer cases rerun after the controller-lifetime fix also passed |
| Full retained turn-input group | Pass; 22 cases, zero failures/ignored, 13.14 seconds; `artifacts/lifecycle-proposal/core-input-regression-413db155-f3de-467f-b8d9-9c8e46852804/attempts/1403c02b-ae16-4919-a549-348bc32d309e/manifest.json` |

The final documented command was
`pwsh -NoProfile -File scripts/build.ps1 -Mode LifecycleTests -TargetRoot artifacts/upstream/codex-target`.
It passed on native Windows x64 with Rust 1.95.0
(`59807616e`, 2026-04-14), Cargo 1.95.0 and MSVC 14.50.35717, from 12:11:53
through 12:12:33 UTC on 2026-09-18. The nested lifecycle observer recorded all
20 cases from 12:12:18 through 12:12:33 UTC. These timings describe this cached
local build, not a clean-host capacity guarantee.

Binary SHA-256 identities:

- Retained core: `28c8074c73b069d9b9e841ad895321efe8f9ebc360566676b99cf437ccd4b0ae`.
- Lifecycle host: `c7eaa4900b3a8e435881d23f5a2e7f7dfc8cc8fcac8178f98399707effa08cd2`.

Combined review identified a strong-reference cycle between host and controller
registry after an initial 19-case pass. The final implementation uses weak
controller references; the added twentieth case verifies actual controller
release, explicit interruption failure and refused readmission for that missing
controller. The earlier run remains under
`artifacts/lifecycle-proposal/scoped-native/4e73329a-6ec8-4f4e-8263-254c11cc6688/`.

## Acceptance boundary

The native observer selects thirteen retained-core cases and seven host cases.
The observed results are independent/inherited holds, zero provider calls
for rejected starts, interruption of both root and child streams without closing
their controllers, stale/foreign revision denial, retained permit draining,
caller-drop survival, failure-preserving drain timeout, owner-loss fencing and
released-controller lifetime/error handling. No test used paid provider access.

The host registers controllers after upstream startup, and isolated helpers can
use a different registry. Active steering, per-effect authority/budget checks,
durable pending-input/receipt capture, process-tree stop confirmation, persisted
owner epochs, fresh-process reconciliation and CLI `/pause` remain outstanding.
An interruption receipt observes retained task handling, not all external effects.
Runtime termination can prevent asynchronous owner-loss cleanup; state is volatile.
Do not infer product pause or release qualification from these tests.

Full hosted native CI remains manual on standard Windows. Routine Ubuntu CI
validates repository/harness portability only; a skipped Windows job is not
native evidence. The unrelated local memory/corpus gates need not rerun for this
lifecycle-only change.
