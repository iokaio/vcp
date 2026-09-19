# Coordinated authority changes

The canonical host now coordinates policy, workspace trust, grants, binding and
steering changes with the retained lifecycle. P2-03 remains in progress; these
are trusted host/library interfaces, and retained model-requested tools remain
disabled pending their governed loop adapter.

`CanonicalHost::command` still accepts authority configuration while native and
retained work are quiescent. It checks owner attachment, start permits, unresolved
work and the broker conflict lease while holding the lifecycle admission lock
through the canonical commit. The lock order is store worker, then lifecycle;
callers never wait for the store worker while holding the lifecycle lock.

During active work, use `change_authority(command, task, expected)` and await the
returned `AuthorityChange::wait()`. Supported commands are `SetPolicy`,
`SetWorkspaceTrust`, `SetGrant`, `Rebind` and `Steer`. The operation:

1. Checks the current owner, command class and expected record revision.
2. Fences canonical admission and atomically holds the current retained root
   and descendants through the existing lifecycle.
3. Captures the requested command as an unapplied intent and durably pauses
   nonterminal canonical tasks. Already-paused children remain paused.
4. Drains native process observers and interrupts retained work outside the
   store worker. Output capture, unknown-charge accounting and late receipts
   remain available.
5. Applies the revision-checked command and returns its canonical receipt.
   Steering accounts only for the task revision advanced by this pause.

The operation owns completion even when its caller drops the waiter. Unsupported
or stale commands do not silently become a new request. A failure after fencing
leaves held/paused or fenced state; it never grants implicit continuation or
automatically retries the command. Other domain validation still runs at the
canonical commit, and an invalid command can leave work paused without applying
the authority change. Owner close remains available and prevents a later change
from reviving that owner.

The owning CLI/adapter can continue inspecting snapshots, projections, artifacts
and accounting. New work, canonical resume and competing authority commands are
rejected while stopping. Native process-count, deadline and output controls are
unchanged. Acknowledged process stopping observes zero Job Object members before
the authority command commits; effect observations still describe residual or
unknown external changes independently of process exit.

Continuation is deliberate: reconcile outstanding effects/accounting as required,
revalidate the current binding/policy/context, release the retained hold, and use
canonical resume with the current task revision and fingerprint. Updating trust,
approving a grant or accepting steering never resumes root or child work.

## Qualification

Run `scripts/test-integration.ps1 -Jobs 2` for the native broker and retained HTTP
stream tests on both canonical stores, and `scripts/test-p1.ps1 -Jobs 2` for the
shared foundation suite. Retain the native `RecoveryTests` and `LifecycleTests`
commands documented in [prepared file tools](p2-tools.md#reproduction), then run
the fast suite. The [qualification report](../evaluations/p2-authority-coordination.md)
records exact commands, outcomes and remaining acceptance work.

The implementation is in `vcp-lifecycle/src/foundation/authority.rs` and its
canonical worker module. It reuses retained controller interruption and owned
process observation. It adds no provider client, parallel scheduler, external
dependency or vendor patch. General CLI control delivery, automatic context
refresh, host-rule configuration and full recovery acceptance remain subsequent
work; the coordination stop is currently workspace-wide even for scoped grants.
