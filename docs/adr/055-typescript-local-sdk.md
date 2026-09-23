# ADR-055: TypeScript local SDK

Date: 2026-09-23
Status: accepted for P9-03.

## Decision

`src/packages/sdk-ts` is the private ESM package `@vcp/sdk` 0.1.0. It imports
canonical generated types and JSON Schema from `@vcp/protocol`, and emits runtime
JavaScript and declarations. Node 24, TypeScript 5.9.3 and Node type definitions
24.10.1 are the selected development baseline. The lockfile pins development
dependencies; runtime behavior uses the Node standard library and bundled schema.

The SDK delegates local authentication to the existing native `local-bridge`.
The caller supplies a trusted absolute executable and configured workspace. The
SDK sends the versioned bootstrap through private stdin, validates the returned
ready shape and then initializes the public protocol. Opaque attachment handles
retain tickets in a private WeakMap. No ticket is exposed in argv, files, error
messages or serializable discovery metadata. Disposal can terminate only the
bridge it spawned; it never kills a reattached server by PID.

Generated DTOs remain separate from hand-written framing, runtime validation,
method/result associations and connection/subscription ergonomics. The runtime
validator supports the actual bundled draft-07 assertion subset and rejects
unknown assertions. It performs no remote schema resolution. Explicit byte,
depth and work limits bound decoding and validation. Exact counters remain
canonical decimal strings; unknown authority semantics fail closed. Typed calls
preserve result envelopes, including snapshot/event gaps and durable outcomes.

One wire request remains outstanding at a time. Admission is limited to 32 queued
calls and 2 MiB of encoded requests. Encoding freezes the caller's intent before
queueing. Caller deadlines include queued time. Cancelling an unsent await removes
it; cancelling an in-flight await leaves its slot until the genuine response or
transport deadline. No cancellation implicitly sends a task mutation. A transport
deadline closes the connection and preserves the original command identity for
reconciliation, including the native owner-loss pause consequences.

Streams pull one page at a time and preserve explicit gaps. Snapshot pagination
binds a stable canonical cut; incomplete or abandoned reads retain late replies
long enough to clean up their subscription. Reconnect, receipt reads and heartbeat
never acquire a controller, answer input or resume work. Durable IDs and revision
guards remain caller-owned; there are no automatic mutation retries.

## Verification boundary

Package tests exercise malformed frames, exact counters, schema drift, typed
consumers, negotiated capabilities, queue bounds, cancellation and cleanup.
Native Rust integration drives the built SDK and executable examples through
the compiled server on both stores with synthetic local providers.
Final qualification passes 29 SDK tests, including a clean isolated offline
install/build and a consumer compiled through the package exports. Four compiled
Windows tests pass on both stores in 64.34 seconds, executing all seven example
modules, retained child inspection, authenticated stdio/pipe attachment, bounded
admission, event recovery, artifact bytes, pending-input reconnect, connected
pause and explicit resume. Independent canonical checks prove one successful
patch effect and stable original receipts after retry.

A test-only transport gate may delay delivery of a complete genuine native
response to prove cancellation after durable admission. That proves the client
delivery/reconciliation boundary, not a crash before the server buffers or sends
the reply. Native full-pipe pressure remains separately qualified by the P9-02
compiled tests; SDK admission flooding proves its own bounds and cleanup.

This decision does not publish an npm package, qualify a remote server or claim
editor integration. P4-01 consumes the SDK after its compiled qualification.
