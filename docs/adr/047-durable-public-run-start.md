# ADR-047: Durable public run-start acceptance

Date: 2026-09-23
Status: accepted for the run-start increment of P9-02; the work item remains in progress.

## Decision

`turn/start` records the caller's new task and turn identities before retained
construction or submission. One canonical transaction creates a pending root,
queued turn, completed input-capture reference and empty budget ledger together
with their events and the original public command receipt. The receipt means
accepted work; it is not proof of a model request, completed effect or successful
constructor. Its turn identity comes from the retained acceptance event, even
after another turn exists.

The server remains bound to one workspace, session and root for its lifetime.
An optional `root_task` in the private controller launch bootstrap selects that
root before the canonical host opens. It never rewrites the workspace descriptor.
Under the acquired writer, an existing selected task must be an unredacted root
in that exact workspace/session. An absent selection creates nothing until an
explicit controller acquisition and accepted `turn/start`. This supplies new-run
selection without changing live bindings or granting an observer execution rights.

The public request must select the pinned root, use unused task/turn identities
and zero creation/steering revisions, and match the effective durable budget cap,
currency and validated profile request/deadline limits. The profile remains
byte-pinned and bound to the current trusted workspace, policy and model. The
canonical worker observes the native repository immediately before acceptance;
callers cannot supply their own fingerprint. Editing/check requirements come from
the validated execution profile. No existing grants or liabilities are copied.
The accepted request ceiling counts root requests across reopen, including
children, helpers and retries. Its deadline is absolute from acceptance and does
not restart on relaunch. Shared coding setup and provider admission retain these
limits even for a later CLI resume. Missing or suppressed creation evidence cannot
convert a public run into an unconstrained legacy run. Existing CLI roots retain
their established configured-window behavior.

Only a fresh accepted result creates an opaque process-local start ticket. That
ticket binds the original connection, controller generation, authority, command
digest, receipt and pristine accepted task/turn/ledger/input evidence. A persisted
receipt cannot recreate it. Reuse, loss, stale policy, changed records or replay
cannot authorize another constructor. The constructor uses the existing one-use
startup nonce and attaches held. A separate guarded activation rechecks the ticket
before releasing execution. The coding loop binds the accepted queued turn rather
than allocating a second canonical turn.

The local supervisor owns acceptance, construction and submission under the same
bounded operation and event-consumer rules as [ADR-045](045-owned-local-execution.md).
A dropped response waiter does not abandon accepted work. Failure after acceptance
holds execution and requests a canonical task/turn pause; a failed pause commit
retains the hold or worker fence and requires reconciliation. The receipt remains
durable. Reopen
uses existing canonical recovery to pause pending work and retain unknown outcomes.
Retry returns the original receipt before profile installation or construction.
Only a new explicit resume can continue a paused root; reconnect does not do so.

## Consequences and evidence boundary

The initial start adapter accepts a new root in the selected session. Starting a
different root requires a new trusted host selection after the current writer
closes. Selecting an existing root supports reconciliation and explicit resume,
not overwriting it with another start request. Child scheduling remains governed
by the existing shared engine rather than by a second public model loop.

Native engine, lifecycle and compiled-process tests must prove exact caller IDs,
atomic acceptance, controller loss, current authority, replay without duplicate
effects and cleanup. Results and limitations belong in the
[local attachment guide](../development/local-attachment.md). This decision does
not close remaining P9-02 methods or establish P9-03 SDK compatibility.
