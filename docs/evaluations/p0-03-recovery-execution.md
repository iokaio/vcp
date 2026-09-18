# P0-03/P0-05 — Durable lifecycle and native Windows execution

Status: bounded feasibility acceptance passed locally on September 18, 2026.
This completes P0-03 and its dependent P0-05 experiment. It does not qualify
the shipped CLI, production canonical store, accounting or general tool sandbox.
The [implementation guide](../development/lifecycle-recovery.md) maps commands,
source, fault ordering and reproduction steps.

## Inputs and commands

The Codex selection remains `3d3ae4965ab370217e871b3a7f0d15589557ee4b`.
Patch `0008-lifecycle-recovery.patch` has SHA-256
`36c36d26ffc78ccf50c1708aaf29a6f332058c38c0cd942236753450ae0a91a9`.
Independent reconstruction matched all 7,938 selected files with result digest
`1d71d71256d50ac9b1649bcd8b8687dcc57c7da8d7fa95759b4f247d50247223`.
Original VCP source and qualification scripts have individual hashes in the
native manifests; the wrappers reject changes during compilation or execution.

Commands executed from the repository root:

```powershell
pwsh -NoProfile -File scripts/build.ps1 -Mode LifecycleTests -TargetRoot artifacts/upstream/codex-target
pwsh -NoProfile -File scripts/build.ps1 -Mode RecoveryTests -TargetRoot artifacts/upstream/codex-target
node --test src/tests/contracts/lifecycle-results.test.cjs
```

All exited zero. Native execution used Windows 10.0.26200, the
`x86_64-pc-windows-msvc` target, Rust/Cargo 1.95.0 and MSVC 14.50.35717.
Retained controller fixtures require a 16 MiB Rust test-thread stack; runners
set this explicitly. Providers are local scripted HTTP fixtures with independently
checked request counts. No paid requests or private workspaces were used.

| Evidence root under ignored `artifacts/` | Observed result |
|---|---|
| `reconstruct-p0-recovery-final.json` | Exact independent reconstruction |
| `build/01305357-b68c-4e8f-8f9e-420690b3ed10/manifest.json` | 13 retained controller cases and 16 host cases passed |
| `build/0a7d2e20-9ca4-4486-8952-4448a000c21e/manifest.json` | Recovery qualification passed; binary, source and runner identities recorded |
| Same run, `recovery/manifest.json` | Three journal tests, 16 host tests, four fresh owner processes and three execution-boundary stages passed |

## Acceptance observations

- The checkpoint admits one writer, binds workspace and thread lineage, and
  syncs framed records before acknowledgment. Every truncated final-frame offset
  recovers only complete records; complete corruption is rejected. Reopening
  restores the same root/child identities paused, without replaying requests.
- Stable private command IDs preserve acknowledgments across restart. A cached
  resume acknowledgment does not execute resume again. Stale revisions, foreign
  scopes, invalid startup grants and unresolved work prevent new dispatch.
- Actual retained model/tool boundaries persist intent before dispatch and
  receipt after observation. Pause during streaming and immediately before tool
  dispatch prevents further work. Dropped requests remain unresolved. Closing
  the owner fences late receipts before a subsequent owner takes the writer lock.
- The synthetic private CLI pauses, reports status and deliberately resumes
  without closing. Independent child holds survive parent pause/resume; inherited
  holds release with the root. Four fresh-process traces cover close/reopen,
  deliberate process crash, command retries and explicit reconciliation.
- Native process launch reuses suspended creation and Job Object assignment.
  Tests observe two members including a grandchild holding an exclusive file
  lock, then zero members after termination. The fixture does not cooperate with
  graceful shutdown. The observer checks lock release separately because kernel
  member count and file-object teardown are distinct observations. Abrupt owner
  exit kills the tree while leaving the durable intent unresolved for inspection.
- Explicit argv/environment, Unicode, spaces, quotes, CRLF, paths beyond 260
  characters and simultaneous 2 MiB stdout/stderr streams pass. Captures retain
  only the declared 1 KiB prefix while recording total bytes and draining pipes.
- A zero-capability AppContainer permits the scoped inside write and blocks
  outside read/write, a replaced junction and a live TCP canary. Unrestricted
  controls before and after succeed; an independent listener sees only the two
  control nonces. Actual token/profile identity and cleanup are recorded.

## Failures found and corrected

The initial retained dispatch denial returned a fatal engine error, producing a
background panic despite a passing test summary. It now returns the retained
recoverable denial response. Both native validators reject background panics;
the final runs above contain none. Earlier passing summaries with a panic are
superseded and are not acceptance evidence.

The copied executable originally inherited low integrity, unintentionally making
the unrestricted control a low-integrity process. The fixture now labels its
owned executable medium and disposable canaries low, records actual token
integrity and requires both live controls. No machine-wide ACL is modified.
Other revisions fixed exclusive-lock observation, independent child holds,
exact scripted request expectations and journal closure with surviving registry
references. These failure records remain in local artifacts.

## Qualified scope and handoff

Select the retained controller plus the injected lifecycle gate and Windows Job
Object as integration candidates. The checkpoint is a private feasibility format,
not an advertised backend. Job membership proves process control, not filesystem
or network isolation. The AppContainer result is a separate single-process
candidate; arbitrary compilers, composed tool trees and hostile path races are
not qualified by it. The CLI is a synthetic experiment, not an installable VCP.

P0-08 must wire the host into global initialization and all helper registries,
disable unowned services and demonstrate model/budget/store/memory adapters in
one retained workflow. P1 owns canonical state/accounting; P2 owns prepared edits,
production authority and effect reconciliation; P3 owns the user CLI; P8 owns
the supported-host sandbox/console matrix. No remaining production task is
marked complete by this report. GitHub CI status is recorded on the associated
PR separately from these local results.
