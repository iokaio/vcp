# ADR-045: Owned local execution and explicit resume startup

Date: 2026-09-23
Status: accepted for the owned execution increment of P9-02; the work item remains in progress.

## Decision

The authenticated local server may own retained execution for its configured root
task. It reuses the CLI's retained session, canonical provider/tool admission and
execution helpers. It does not introduce another model loop. This extends
[ADR-044](044-controlled-local-process-bootstrap.md); launch and attachment without
execution configuration retain their observation/control behavior.

An optional private bootstrap `execution` object carries an absolute trusted
`profile` path, `provider_credential`, and a bounded `credentials` map keyed by
configured credential names. Credential values remain in memory and private pipes,
never arguments, inherited environment, workspace files, attachment tickets or
diagnostics. The server has no environment fallback for provider, MCP or decision
credentials. Profile identity is pinned for the server lifetime and checked against
the durable workspace, policy and model configuration. Synthetic qualification
retains its existing feature-gated literal-loopback endpoint and exact synthetic
credential requirement; this decision authorizes no paid provider tests.

Authentication and controller acquisition do not start execution. Explicit
`session/resume` first checks current authority and the original durable command
identity. A replay returns its receipt before profile installation, construction,
hold release or submission. A new admission produces an opaque controller-bound
ticket. Commit revalidates the original connection, lease, task and steering
revisions, environment, budget, pending questions and effects. Only a newly
`Accepted` result permits the server supervisor to schedule a submission; `Replay`
does not. A receipt records durable acceptance, not successful execution or a
promise that submission occurred. Clients inspect task, turn and command state.

The first retained root uses a one-use constructor-specific startup grant. Generic
startup stays held, including after a no-root owner loss. Grant creation requires
the configured root, current controller and fully drained startup/work/scheduler
state. Its dedicated admission adapter and final attachment both recheck authority
and the grant identity. A lost or abandoned grant cannot authorize a later
constructor; an old generic constructor cannot attach after rearming. The exact
returned thread attaches held, then explicit canonical resume releases that hold.
Acquisition, reconnect and receipt replay never perform this rearming.

One server supervisor serializes construction, acceptance and submission and owns
the retained event consumer across connections. Dropping a response waiter does
not cancel its owned operation. Completion and interruption events must match the
identified submitted turn; queued events from a previous turn cannot complete or
pause a new one. Failed submission or completion pauses execution. Reconnect does
not resume, and an accepted command is never automatically resubmitted after a
server restart. Canonical recovery retains effects and uncertain liabilities.

Interrupted provider response captures carry their exact attempt identity. Recovery
can expose reads and receipt replay only when the physical aborted descriptor
matches its canonical record and the linked attempt, reservation and ledger prove
retained accounting. This acknowledgement does not authorize execution: unresolved
response liability keeps constructor and runtime admission blocked. Missing or
mismatched evidence, capture failures and older unlinked response captures retain
the hard recovery fence. Independent accounting reconciliation is required before
the acknowledged response can stop blocking execution.

The retained constructor is not assumed cancellation-safe. Each new resume admission
has a 30-second watchdog covering preparation, construction and submission; receipt
replay bypasses it. Expiry
invalidates the controlling connection and seals admission while the owned future
continues cleanup. If it
has not finished after five further seconds, the dedicated server process exits.
Shutdown keeps the retained session alive through canonical owner close, then joins
retained thread shutdown under the same process deadline. Forced exit is crash recovery, not proof
of graceful draining or effect completion. Explicit resume within a server does
not renew its configured execution deadline; a new window requires relaunch.

## Consequences and evidence boundary

The bare engine and lifecycle resume primitive remain distinct from the production
server's submission capability. Construction and native process tests must prove
still-connected pause/resume, owner loss, replay, first startup and restart without
repeated effects. Test results belong in the construction/evidence record; this ADR
does not assert pending checks passed or close P9-02.

Remaining method adapters and bounded event recovery retain their own acceptance
requirements. P9-03 must test the TypeScript SDK against the compiled server before
P4-01 connects the extension UI. [ADR-042](042-owner-directed-p8-closure.md) remains
the authority for P8 closure; its provider liability, missing environment checks
and unreviewed acceptance evidence remain historical gaps, not passing evidence.
