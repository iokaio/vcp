# Lifecycle recovery and Windows qualification

The original `src/crates/vcp-lifecycle/` host owns registered retained Codex
controllers. Its private qualification CLI is an example, not the VCP product
executable. Read this with [engine execution](../architecture/engine-execution-design.md),
[storage and portability](../architecture/storage-portability-design.md),
[ADR-002](../adr/002-internal-and-public-protocol.md), and
[the scoped lifecycle increment](scoped-lifecycle.md).

## Ownership and dispatch

`Lifecycle::open` exclusively locks one checkpoint file and verifies its workspace
identity. The file remains locked while the owner can write. Graceful owner close
waits for registered controllers and process jobs, seals late receipts, and
releases the lock. An abrupt process exit releases the OS lock; the next process
opens with the root paused. Opening never sends input or repeats an effect.

The extension registry carries both turn-start admission and a private work gate.
`Session::new` consumes a separate one-use startup grant bound to a canonical
workspace path and, for recovery, the original thread identity. Pause invalidates
unused grants and drains constructors already admitted. A startup grant does not
authorize a model call: unregistered, paused and unowned threads fail dispatch.

The retained Responses client records an intent before a request and acknowledges
completion only when its consumer observes the provider's completion event. The
tool registry records an intent before hooks/handlers and acknowledges after the
accepted result. Dropping a stream or failing/cancelling a tool leaves an unresolved
intent. Reconciliation can ingest a receipt while paused; it cannot dispatch work.
Resume refuses unresolved work and stale revisions.

Realtime and remote memory summarization are rejected for this gated client;
speculative WebSocket prewarming is suppressed. This is a bounded feasibility
seam, not full provider accounting. P0-08 must wire every selected helper and
service into the integrated profile, including effects before session construction.
P1/P2 still own production budget admission, prepared changes, receipt contents
and capability policy. An arbitrary upstream registry without the host gate is
not a qualified VCP session.

## Checkpoint and private controls

`journal.rs` uses length/complement headers, JSON snapshots and SHA-256 chaining.
Each append calls `sync_all` before acknowledgement. Reopen discards only an
incomplete final frame; malformed complete headers, checksum failures, workspace
mismatches, repeated revisions and invalid lineage fail closed. This local
checksum is not authenticated encryption or cloud portability. The P0 format is
disposable and bounded to a 16 MiB frame; P0-04 compares production candidates.

Snapshots retain lineage, independent holds, operation intents/receipts and stable
control-command IDs. Parent pause is inherited; an independently paused child
remains paused when its parent resumes. Owner loss adds a root hold without
converting inherited child holds into independent holds. Recovered controllers
must bind the same thread IDs and complete interruption before readmission.

`control.rs` delivers pause/resume to that owner. A duplicate command returns its
recorded outcome without repeating the transition; reusing its ID for another
action fails. A command missing a completion receipt remains pending after a
crash and is never automatically replayed. Resume requires a current host
validation callback. Production validation will include workspace content,
permissions, context and budget rather than the fixture's workspace identity
check alone.

`examples/lifecycle-owner.rs` connects these controls to retained root/child
controllers and a scripted loopback provider. It accepts lines such as
`pause-1 /pause`, `status-1 /status`, and `resume-1 /resume`, with optional `child`
scope. `/turn` is a fixed synthetic request, `/child` creates one fixture child,
and `/crash` exits without destructors for recovery qualification. These are
private experiment commands, not a public protocol or general coding client.
The crash scenario also launches a native two-process fixture with `/process`.
The next process sees its unresolved intent; `/reconcile-process` observes the
known fixture's exclusive lock release before recording a receipt. This observer
is specific to the synthetic fixture, not general PID-based recovery authority.

## Native execution boundary

`process.rs` uses the retained `codex-utils-pty::JobObject` implementation: create
the child suspended, assign it through its process handle, and resume only after
successful assignment. Breakaway is disabled. The owner retains job handles so
dropping a caller cannot remove a still-stopping process from observation. Pause
terminates jobs and queries their kernel membership before acknowledging stop.
Numeric PIDs are diagnostic locators, not recovered authority.

Executable, argv, cwd and environment are explicit. Capture drains both output
streams while retaining only the configured prefix and full byte counts. Root
exit terminates remaining descendants. Termination does not roll back filesystem
effects. Windows file-object cleanup can finish shortly after job membership
reaches zero; tests independently wait for release of an exclusive file lock.

A Job Object supplies process containment, not filesystem or network isolation.
The separate AppContainer experiment reuses the existing disposable Windows
fixture, verifies the actual token and capability count, and observes inside/outside
file canaries, a prepared directory replaced by a junction, and network traffic.
Its single-process profile is a candidate for brokered execution; qualification
does not advertise arbitrary build-tool compatibility or a composed product sandbox.
The copied control executable receives a medium-integrity label. Both disposable
canary directories permit low-integrity writes, and only the inside directory
grants the AppContainer SID access. This prevents a missing write grant or a
mandatory integrity label from masquerading as the tested identity boundary.

## Local validation

Use the [native setup](codex-source.md) and committed source selection:

```powershell
pwsh -NoProfile -File scripts/build.ps1 -Mode RecoveryTests -TargetRoot artifacts/upstream/codex-target
pwsh -NoProfile -File scripts/build.ps1 -Mode LifecycleTests -TargetRoot artifacts/upstream/codex-target
pwsh -NoProfile -File scripts/test.ps1 -Suite fast
```

`RecoveryTests` builds the host tests, native helper and owner example from the
same Cargo invocation. It checks journal failure cases, actual retained dispatch,
startup authority, late receipts, native trees/output/paths, four owner processes
including a crash, and separately observed AppContainer controls. The manifest
binds executable and log hashes and rejects missing or failed evidence.
`LifecycleTests` additionally checks the retained drain/continuation/scoped-start
regressions. Both are local native checks; routine GitHub CI remains lightweight.

No provider credentials or paid calls are required. Raw state, rollouts and test
artifacts are disposable local inputs, never public fixtures or commit contents.
