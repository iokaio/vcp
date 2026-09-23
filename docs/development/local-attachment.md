# Local attachment construction

Owning item: P9-02. The protocol foundation is delivered in PR #134. This increment
adds canonical controller-lease state, a live connection adapter, controlled
Windows launch and named-pipe attachment to the same live writer. P9-02 remains
in progress: attachment is qualified; live subscriptions and the remaining
execution adapters are not accepted yet.

## Controller identity

A lease is a typed canonical `Access` document for one workspace/session. Its key
is a deterministic digest of that scope and document type, separate from policy
and grant keys. The document stores its revision, monotonically increasing
acquisition generation, and optional holder. The holder binds authenticated actor,
connection identity, current engine process owner and owner epoch. Connections
receive their identity from the trusted transport host, never from method params.

Acquisition conflicts with an occupied lease. Deliberate transfer consists of
release followed by a new acquisition; acquisition does not resume work. Release
keeps the last generation, and the next acquisition increments it. A stale process
holder requires explicit recovery before a replacement can acquire the lease.
There is no timeout that silently promotes an observer to controller.

Lease operations require current write access before receipt lookup. A repeated
operation returns its original durable receipt without changing the lease. That
receipt is evidence of an operation, not a present authorization grant. Obtaining
and checking an opaque controller token separately checks the current holder,
generation, connection, actor, scope and engine owner. Old tokens fail after loss,
transfer or engine restart. Observers cannot obtain a token by replaying a
controller command ID.

The canonical store validates lease shape, scope references and successive
revisions/generations, including replay from disk. A generic `Access` document
cannot replace a typed lease at its reserved key. Lease events and their command
receipts commit together. Only `controller-` followed by 64 lowercase hex digits
is reserved in the `Access` collection. An existing conflicting generic record
fails closed; the store does not reinterpret or rewrite that history. Other
generic keys retain their existing behavior.

## Live host integration

RPC framing, initialization and method negotiation remain separate from host
execution. `RpcHost` supplies current authorization and typed call dispatch. The
existing direct-engine adapter remains useful for protocol tests; the production
adapter lives in `vcp-lifecycle`, which already depends on `vcp-engine`.

The live adapter must validate controller generation in the same serialized worker
operation as decision/task revision checks. Bare engine mutation is insufficient
for steering, pause, cancel and resume: existing lifecycle methods fence retained
execution, own draining, and revalidate continuation. Public command identity must
survive those operations, including the pause-adjusted revision during steering.
An interrupted authority-change intent remains reconciliation work.

Connection loss must hold and pause owned work while retaining the host for
observers and recovery. `CanonicalOwner::close` is host teardown, so it cannot be
used for a reconnectable connection loss. Canonical non-running task state is a
necessary lease transition precondition; it does not prove that retained effects
have drained. The host must establish that independently. A token never replaces
runtime execution admission, budget checks or effect reconciliation.

`PublicConnection` selects public ownership only on a quiescent private host.
The retained `CanonicalOwner` continues to own the writer; authenticated observers
can read without a controller lease. Mutations require both the durable lease and
the connected host owner, checked inside the serialized worker. Refreshed read
access cannot upgrade the connection's original scope or role, or refresh an old
controller token. Public mode also gates startup and effect admission.

Live steering validates the original request before holding work. It records an
authority-change intent containing the original command identity and digest,
pauses canonical tasks, drains retained work and scheduler completion, then
revalidates authority before committing. An opaque engine-local pause proof binds
the adjusted task revision to that original request. It cannot be supplied over
the wire. Dropping the caller's await leaves completion owned by the host; losing
the controller or fencing the worker prevents the final steering commit. Receipt
replay does not perform another hold. An intent alone is not a successful receipt.

Connection loss invalidates admission immediately and starts interruption before
waiting on the writer queue. It releases the durable lease only after owned
draining; observers retain access to paused state. Reacquisition never resumes
execution. If disconnection interrupts construction before the first retained root
attaches, the host must be reopened before another root construction. This
conservative startup restriction rejects late attachment from the interrupted
constructor; it is separate from ordinary attached-root pause/resume.

## Prerequisite verification

On native Windows with Rust 1.95.0, the domain, protocol, engine and store package
suites passed 150 tests, including five controller scenarios on both backends,
seven store lease contracts and two RPC host-interface tests. Two existing
environment-specific store tests remained ignored: real OneDrive and independently
provisioned Go age/Node envelope qualification. Neither is lease acceptance.
The 18-case repository fast suite and affected Rust formatting checks passed.

The lease tests establish writer exclusion, competing connections, scoped access
before receipt replay, generation changes, stale tokens after authority changes
and restart, explicit recovery, and rejection of Running-task ownership changes.
Store tests reject unreachable snapshot states, reserved-key occupation, forged
replay and type replacement without changing canonical state. These are native
library tests; they do not establish local peer authentication or live owner-loss
handling.

The live-host increment passed 68 engine/protocol tests and 30 lifecycle tests on
native Windows with Rust 1.95.0. The latter include seven public-connection/RPC
tests, one existing public metadata test, and 22 existing controller, integration
and port tests. Public steering fixtures cover both stores with an active
synthetic provider stream, a deliberately retained admission permit, dropped RPC
receivers, connection loss and a real output-capture capacity failure. They check
single receipt acceptance, unchanged steering on failure, retained partial
response/unknown provider liability, and the original authority intent. A no-root
fixture proves startup drains before acceptance; a separate fixture rejects late
root attachment after disconnect. These tests require no paid provider or external
test machine. Process-level endpoint authentication remains separate acceptance.

## Connected task controls

The live host advertises `task/read`, `task/cancel`, `turn/pause` and `turn/cancel`.
Reads expose scoped canonical task state, observed pending approvals and effect
uncertainty. The current turn is selected from retained canonical turn-creation
history; missing chronology yields a null turn rather than an invented ordering.
Turn mutation requires the latest provable turn in that task and current steering.
A stale, foreign or unprovable turn is rejected before retained execution is held.

Turn pause pauses the selected task and holds its retained descendants. Turn cancel
and task cancel cancel that task and its canonical descendants. Both use the same
retained fence as CLI stop. Their receipt acknowledges durable intent, while owned
interruption and effect reconciliation may still be draining. The authenticated
connection and its controller lease remain live; reads and receipt reconciliation
continue. Repeating a durable command returns its receipt without another hold.
Acceptance and terminal task state do not settle unknown provider/tool liabilities.
Resume is a separate explicit, revalidated operation and is not advertised yet.

Native Windows/Rust 1.95.0 qualification passed 76 engine/protocol tests, 13
public-host selections, and seven compiled process tests (five stdio attachment,
two task-control cases exercising both stores). Active synthetic root/child streams
prove hold, drain, replay and preserved uncertainty; no-root fixtures prove startup
is sealed and late attachment rejected. Compiled offline fixtures prove scoped
inspection, cancellation/replay, observer denial and a still-readable connection.
They do not claim compiled live execution/resume qualification. All 18 fast delivery
checks, affected Rust formatting and the existing both-store CLI stop/resume
regression pass.

## Durable canonical resume prerequisite

`PublicConnection::resume_canonical` accepts a typed resume intent at the trusted
native host boundary. It preserves the caller's durable command identity and
checks current controller authority before replay. Replay precedes retained binding
lookup, effect reconciliation, hold release and MCP proof collection. Host-local
serialization prevents concurrent duplicates from repeating those observations.
The same shared question-freshness predicates now serve CLI and host resume; old
or expired approvals do not become a new resume blocker.

A new acceptance requires a suspended task, current fingerprint and environment,
remaining budget, reconciled effects, actionable questions answered, and a retained
owner whose interruption and outstanding work permit continuation. The final
lifecycle guard stays held across canonical acceptance. An uncertain commit seals
admission for receipt reconciliation. The existing MCP exception still requires
proof of the exact owned idle process and granted undelivered call, with slot and
lifecycle guards retained through the decision.

This primitive never creates a retained thread, submits a model turn or dispatches
an approved tool call. `session/resume` remains unadvertised on the wire until the
server owns submission and its crash/retry reconciliation. Its native qualification
passes 80 engine/protocol tests and 19 public-host tests on Windows/Rust 1.95.0.
Both stores cover concurrent same-key resume, stale authority, unfinished retained
work, unknown effect liability and real MCP startup/call approvals. Independent
markers prove canonical acceptance and replay do not submit a tool effect.

The real MCP test exposed an idle daemon retaining its scheduler lease during
public owner cleanup. Release, disconnect and steering now close owned MCP
connections before waiting for scheduler quiescence, preserving host credentials
for later use. Both explicit release and disconnect drain the actual fixture
process. Existing CLI control and exact MCP approval regressions pass, as do all
18 fast delivery checks. This prerequisite does not close P9-02.

## Remaining acceptance

Native stdio and named-pipe attachment now have the bounded evidence below.
Remaining real-process acceptance covers controller loss during active execution,
connected task pause, explicit resume and crash/retry of execution effects.
Offline attachment fixtures do not establish those execution cases.

Capture snapshot plus cursor in one canonical worker operation and bridge to
bounded live delivery without missing events. Slow consumers must receive a gap
or disconnect without blocking writes. Retention and access changes remain
explicit errors. P9-03 follows with SDK tests against the compiled server; P4-01
then attaches the prepared extension UI through that SDK.

## Controlled stdio launch

[ADR-044](../adr/044-controlled-local-process-bootstrap.md) selects a native bridge
so that a TypeScript client does not have to reproduce Windows handle security.
Launch the trusted absolute `vcp.exe` with the sole argument `local-bridge` and
private stdin/stdout/stderr pipes. The first stdin JSON line is:

```json
{"schema":"vcp-local-bootstrap/1","workspace":"D:\\work\\project","data":"D:\\private\\vcp-data","role":"controller"}
```

Use `observer` for read-only access. `data` may be null to resolve the bridge's
local application-data default. The workspace must already have a valid selected
descriptor and binding. The bridge returns a `vcp-local-ready/1` frame with the
authenticated server process pin and authorized scope. Then send the public
`initialize` request. No workspace request runs before that handshake. Startup
failures emit stderr diagnostics and close stdout without a CLI JSONL result.

The server implements seven engine methods, four optional controller methods
and three live task/turn stop methods. Request the required method capabilities during initialization.
`controller/read` gives scoped revision/generation and ownership classification;
it never discloses another actor's token. `controller/acquire` is explicit,
`controller/release` drains and pauses while leaving the connection readable, and
`controller/recover` explicitly resolves a stale process lease. Authentication,
receipt replay and reconnect never acquire or resume automatically. Read/retry
uses the original durable command identity. Replaying recovery after a later
acquisition returns the original receipt without disturbing that acquisition.

With the default `stdio` transport, the bridge and its server share a lifetime. EOF closes the controlling
connection and allows canonical cleanup before releasing the writer. Another
launch against the same root fails while the first writer remains alive. A slow
or invalid stream cannot grow an unbounded queue: frames have a 1 MiB payload
ceiling, helper queues hold at most eight frames, and blocked output has a five
second deadline. Only one RPC is dispatched at a time. Event subscriptions are not
advertised. The optional pipe transport below retains the server independently
after an authenticated bootstrap handoff.

## Named-pipe attachment and reconnect

Add `"transport":"windows_pipe"` to the launch frame to select the pipe service.
The trusted bridge first launches and authenticates the server using the same
inherited-handle bootstrap. The server creates a random local pipe endpoint with
a current-user ACL and rejects remote clients. Before handing off child lifetime,
the bridge authenticates that pipe against the held server process pin, connects
successfully, and exchanges a challenge-bound handoff acknowledgement over the
private bootstrap channel. Failed handoff retains owned child cleanup.

The ready frame adds `attachment`, containing `endpoint`, `server` and a random
`ticket`. A controller-ready frame also includes a distinct `observer_attachment`.
Retain these objects in trusted client memory. Give an observer only the observer
object; its ticket cannot request controller capability. The server does not write
endpoint metadata or credentials into workspace files, discovery files, arguments
or environment variables. Pipe names and PIDs alone are not authentication.

To attach again, launch the same trusted absolute `vcp.exe local-bridge` and send
a `vcp-local-attach/1` frame with `attachment` set to the retained object and `role`
set to `controller` or `observer`. The bridge verifies the kernel-reported pipe
server against the trusted process pin before sending its ticket. The server
reads a bounded authentication frame, then synchronously impersonates the pipe
client, checks the actual client process and token principal, and reverts before
another await. The ticket's own role ceiling governs the new connection. After
the new ready frame, perform public `initialize` again. Fresh authentication gives
a new connection identity; it never acquires a controller lease or resumes work.

Authenticated clients share one canonical host and writer. Disconnecting a
controller still invalidates admission immediately, holds and pauses work, drains
owned effects and releases its lease. Remaining observers continue reading. Once
all authenticated connections and their cleanup have finished, the server retains
its idle writer for 30 seconds to permit reconnect, then tears down. This retention
does not grant execution time after controller loss. If that server has exited,
its old attachment cannot authenticate a replacement; use a fresh trusted launch.
Pipe pumps preserve prefetched authentication bytes and share the stdio frame,
queue and synchronous loss-invalidation limits. Authentication and admitted clients
are independently bounded to 16 concurrent connections each.

Native Windows/Rust 1.95.0 qualification passed 20 local unit tests and eight
compiled production-process tests (five stdio, three named-pipe). The pipe tests
cover both-store competing controllers, observer isolation, explicit reacquisition,
rejected pins/tickets/role escalation, observer-only launch, real 30-second idle
writer release and dead attachment rejection. Native kernel checks include wrong
expected SID/session fixtures; a separate foreign-user process was not exercised.
No credentials, network provider or paid effects are needed. The 18-case fast
delivery suite and affected Rust formatting checks pass. This qualifies the
attachment increment, not the remaining P9-02 execution/event acceptance.

The preceding stdio increment's native qualification uses the compiled production
`vcp` binary in five process
tests: both-store controller receipt replay and explicit release, both-store
observer denial, writer exclusion/EOF cleanup, malformed or unproved bootstrap,
and oversized protocol input after acquisition. They seed actual descriptors and
bindings, remove provider credentials, and verify canonical lease/task state after
process exit. No provider requests or paid effects occur. Separate native tests
check process pin tampering, executable replacement denial, inherited kernel
object identity (including an excluded decoy), bootstrap handle sealing and owned
child cleanup. The ignored child-fixture entry is explicitly launched by the
inheritance test, rather than counted as a deferred qualification.

Transport loss synchronously invalidates the connection and seals owned admission
before notifying asynchronous dispatch. A gated active-stream host test exercises
that interval before `disconnect()` runs; observer and stale loss signals cannot
hold a newer owner. All 11 public-host tests and 71 engine/protocol tests pass on
native Windows/Rust 1.95.0. Generated schemas/types, their nine generator contracts,
and the 18-case fast delivery suite pass. These counts describe the preceding
stdio increment; they do not establish the new named-pipe/reconnect qualification.
