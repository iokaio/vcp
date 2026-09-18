# Scoped retained-controller lifecycle

P0-03's host prototype combines trusted thread registration, tree holds, active
interruption and explicit readmission. The owning [plan](../plan/01-upstream-feasibility.md#p0-03--codex-lifecycle-seam)
and [engine design](../architecture/engine-execution-design.md) still require
durable recovery, effect authority and private CLI integration. Those are not
implemented by this in-memory milestone.

## Source and authority

[`vcp-lifecycle`](../../src/crates/vcp-lifecycle/src/lib.rs) is original source
in the shared Cargo workspace. It wraps retained `CodexThread` handles rather
than running another coding loop. Its `Lifecycle` instance owns one registered
tree. `OwnerLease` represents the owning host connection; clones of the API do
not extend that lease. The host keeps weak controller references to avoid a
cycle through the controller's extension registry. An interruption temporarily
owns live handles; missing controllers produce an explicit failure while other
live members still receive interruption. The actual thread ID comes from the retained controller,
not from a model's parent-turn string.

The host installs the lifecycle object as `TurnStartAdmission` before creating
its controllers. It attaches one root, then explicitly registers each child
under a known parent and current revision. Unregistered threads using the gate
are denied. Revisions carry an unforgeable in-process instance identity as well
as a counter, so a revision from another host or an older transition is rejected.
This is a trusted host API, not a public protocol or a portable authority token.

The seventh [Codex patch](../../src/third_party/patches/codex/README.md) forwards
controller-owned thread IDs at ordinary, delegated, review and mailbox admission
sites. Default trait methods preserve older hosts' global drain behavior. The
VCP host denies unscoped calls and uses the same fence for both admission kinds.
The patch also exposes `CodexThread::interrupt_for_host`, which runs the existing
interruption logic in a retained-runtime task and waits for it without consuming
the event stream or closing the controller.

## Holds and readmission

1. `hold` validates the expected instance/revision, sets the selected node's local
   hold and snapshots registered descendants under the same mutex used by child
   registration and admission. Descendants inherit the hold; siblings outside
   the subtree remain admissible.
2. Accepted starts retain permits until the core publishes their starts. The
   host denies subsequent admission and waits for the issued permits to drain.
   Registering a child or resuming during the transition is rejected.
3. The host starts interruption for all selected controllers before waiting for
   individual completion. The operation is owned independently of `HoldWaiter`;
   dropping the caller's waiter does not undo the seal or abandon interruption.
4. Completion records only that retained task interruption returned. Inspection
   remains available. A drain timeout or interruption failure leaves holds set,
   records the error and prevents readmission until a subsequent interruption
   succeeds. The timeout does not turn an incomplete stop into success.
5. `resume` requires a fresh revision, attached owner, completed subtree
   interruption and no held ancestor. It releases only the selected local hold.
   Independently held children remain held when their parent resumes. It does
   not itself start a turn or wake queued mail; the host deliberately submits
   subsequent work.

Dropping `OwnerLease` synchronously denies the entire registered tree, including
continuations, and attempts owned interruption. This instance cannot regain
authority by calling `resume` or registering another root. Its inspection view
preserves owner loss and any interruption error. Recovery needs a future
canonical checkpoint/reconciliation path; constructing a new object is not
permission to replay prior work.

## Local validation

```powershell
pwsh -NoProfile -File scripts/build.ps1 -Mode LifecycleTests
pwsh -NoProfile -File scripts/test.ps1 -Suite fast
```

`LifecycleTests` compiles the retained core `all` target and the host `controller`
target using locked dependencies, then independently checks named test results:
five legacy drain cases, five continuation cases, three thread-identity cases
and seven host cases. The host cases cover root/child active streams, independent
holds, stale and foreign revisions, unknown scopes, permit draining, dropped
waiters, timeout refusal, owner loss and released-controller lifetime/failure.
Provider observations are synthetic
loopback request counts, not paid model evaluations.

The observer requires both exact Cargo artifacts, hashes both binaries and
rejects missing, duplicate, ignored or failed cases. Native libtest workers use
the existing 8 MiB stack requirement. Full hosted qualification remains manual
on standard Windows; ordinary Ubuntu CI checks the repository and observer.
See [executed evidence](../evaluations/p0-03-scoped-lifecycle.md) for actual results.

## Remaining product boundaries

- Thread registration occurs after upstream creation. It does not fence startup
  side effects, and isolated upstream helper sessions may use an empty extension
  registry. Mandatory authority must also reach those paths before product use.
- Active-turn steering and model/tool effects need current authority at their
  own dispatch boundaries. A start permit is not effect or budget authorization.
- Retained interruption can clear accepted pending input/approval state and does
  not confirm native descendant quiescence or complete tool-output receipts.
  These limitations must be reconciled by the recovery/execution milestone.
- State and completion observations are memory-only. Runtime/process death can
  interrupt cleanup. There is no persisted owner epoch, checkpoint, fresh-process
  reopen, `/pause` command or inspector UI in this change.

P0-03 stays `in_progress`. The [delivery milestones](../plan/README.md#pr-milestones-and-local-validation)
require the next connected recovery/execution work rather than treating this
prototype as completed product pause.
