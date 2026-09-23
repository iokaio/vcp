# Local attachment construction

Owning item: P9-02. The protocol foundation is delivered in PR #134. This increment
adds canonical controller-lease state, an injectable RPC host boundary and a live
connection adapter. It does not expose a local server or qualify endpoint
authentication.

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

## Remaining acceptance

Before advertising attachment, qualify controlled stdio launch and native Windows
pipe peer authentication, endpoint permissions, inherited handles and writer-lock
ownership. Then prove real-process competing controllers, observer disconnect,
controller loss, connected pause, explicit resume and crash/retry behavior.

Capture snapshot plus cursor in one canonical worker operation and bridge to
bounded live delivery without missing events. Slow consumers must receive a gap
or disconnect without blocking writes. Retention and access changes remain
explicit errors. P9-03 follows with SDK tests against the compiled server; P4-01
then attaches the prepared extension UI through that SDK.
