# P10-01 governed lifecycle hook evidence

Native Windows qualification on 2026-09-24 covers the version 1
[hook contract](../development/hooks.md) and [ADR-065](../adr/065-governed-lifecycle-hooks.md).
Executable hooks remain explicit trusted-owner configuration, separate from
skills and importers. All effects use the existing process broker and journal.

## Behavioral evidence

The pure tests cover all eight schemas, deterministic ordering, duplicate and
cyclic definitions, causation depth/fan-out/input limits, strict output parsing,
artifact scope and rewrite binding. Adapter tests cover blocked or duplicate
proposal publication, conflicting rewrites and unsupported tool-gate bypasses.

Seventeen native hook tests pass. File and SQLite cases exercise actual native
executables and independently observed external markers, including:

- Each lifecycle event, deterministic order and duplicate delivery.
- Timeout/tree termination, output ceilings, malformed output and filtered secrets.
- Fresh authorization for rewritten paths/arguments; old proposals cannot dispatch.
- Forced owner termination after durable intent, after an external effect and
  after validation before hook-result commit; repeated reopen never repeats the effect.
- Late results, changed policy, executable and pinned input rejection; a later
  hook cannot invalidate an earlier rewrite and still apply that rewrite.
- Real retained model requests with untrusted receipt-linked hook context,
  transport retries without hook reruns, and conditional compaction hooks.
- Missing/stale model-preparation tokens and explicit approval/pause/resume with
  a new turn ID but the same input, producing exactly one hook execution.

The ignored `native_hook_kill_owner` entry is the independently launched victim
used by the passing supervisor test; it is not omitted crash coverage.

## Reproduction and delivery gates

```powershell
./scripts/test-hooks.ps1
./scripts/test.ps1 -Suite fast
```

The hook runner records the source revision, selected source/runner/lockfile
hashes, commands, passing counts and log hashes, and rejects changed inputs.
The final native run passed all 157 tests on Windows `10.0.26200` with Rust
`1.98.0 (88d9e12ae 2026-08-18)`, target `x86_64-pc-windows-msvc`:
`artifacts/hooks/7df28a7a-3e42-45ac-af9e-10b0d722196d/manifest.json`.
Manifest SHA-256:
`438566181047736d607903f7c630aa88a7ec5476a679e46a8141f9176c47c4d9`.
It binds the tested working-tree inputs at base commit
`0294545002df46c118926e3f77ef1c65279a26cc`; it does not claim a post-merge rerun.

| Stage | Passed |
|---|---:|
| Pure hooks | 7 |
| Lifecycle adapters | 2 |
| Tool wrappers | 3 |
| Native hooks and recovery | 17 |
| CLI contracts | 95 |
| Executable acceptance | 29 |
| Native terminal | 1 |
| Local lifecycle | 3 |

The required delegation adapter example built successfully. Existing package-only
release cases remain ignored (eight executable and two CLI library entries);
they require their separate extracted-package fixtures. No P10 assertion was skipped.
The slow-hook executable test proves status responsiveness, native process stop
on pause and no gated model dispatch for session-start and task-completion hooks.
Local regressions cover CLI/API parity, approval across connection loss and
owner-loss recovery without re-execution.

The earlier `a3edf97a-df5e-43b2-8029-8d881a93d9d5` run is retained as failed:
its new pause assertion incorrectly expected budget-exhaustion code 5 instead of
documented pause code 8, and the runner omitted the existing delegation example
build prerequisite. Both were corrected; behavioral pause assertions were retained.

The final fast-suite run passed all 18 cases:
`artifacts/p10-hooks-fast-delivery/bb1cde5d-1755-465a-94d0-cd0bd25b913a/manifest.json`.
The retained Codex seam independently reconstructs from its immutable upstream
pin plus patches through 0039: 7,940 files, inventory digest
`6d776d865a61abbd041d39f6ae301c6e24dbbd80d060a2dfb65e649dcd7bc125`.
Source verification passes. [G04 upstream evidence](p10-01-gemini-hooks.md)
separately records 59 passing pinned Gemini assertions; these are not counted as
VCP authority or recovery tests.

## Qualification limits

This qualifies native Windows and forced owner-process termination, not hardware
power loss or another execution host. Broker profiles retain their existing
opaque-effect/isolation boundaries; no additional OS sandbox or external-service
charge metering is claimed. Fixtures use synthetic loopback model responses.

Version 1 authorization rewrites support native file/process requests. Configured
authorization hooks explicitly reject unsupported MCP/verification wrapper paths.
Live approval continuation requires a terminal/local owner; one-shot batch exits
for required input, and reopening never reconstructs a pending hook capability.
Completed or uncertain effects cannot be automatically replayed.

P10-02, P10-03 and P10-04, the subsequent skills roadmap and the separate
prompt-audit proposals are outside this increment.
