# Local attachment construction

Owning item: P9-02. The protocol foundation is delivered in PR #134. This increment
adds canonical controller-lease state and an injectable RPC host boundary. It does
not expose a local server or qualify endpoint authentication.

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
adapter must live in `vcp-lifecycle`, which already depends on `vcp-engine`.

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
