# 17 — Deferred public API, local server and TypeScript SDK

Status: P9-01 and P9-02 complete following [owner-directed closure](../adr/042-owner-directed-p8-closure.md) of P8-05. P9-03 is the next ready item. Owns P9-01 through P9-03, to be implemented in order. It is not a Windows CLI release prerequisite. Architecture section 5 is the external contract, and internal commands/events from P1 remain the engine boundary.

## Code organization

Extend `vcp-protocol` with `jsonrpc`, `handshake`, `methods`, `errors`, `schema` and `compatibility`. Add `vcp-engine/server` adapters for stdio and local Windows pipes. Generate `src/packages/protocol-ts/`; implement hand-written connection/subscription ergonomics in `src/packages/sdk-ts/`. Keep generated types distinct from edited SDK source, and preserve the CLI's direct internal path.

Public protocol adapters translate to existing typed commands; they do not introduce new task state, budgeting, policy or memory ownership. Reuse selected C01 schema/lifecycle machinery with explicit VCP wire adaptations and fixture evidence.

Use [the deferred client design](../architecture/deferred-clients-design.md#ownership-and-adapter-boundaries),
[protocol ADR-002](../adr/002-internal-and-public-protocol.md) and
[client/distribution ADR-012](../adr/012-clients-and-distribution.md). Choose concrete
schema and local-authentication implementations during these tasks; this plan does
not declare a supported protocol version or transport merely by reserving its path.

## P9-01 — Public protocol

1. Finalize version/capability negotiation and JSON-RPC envelopes including `jsonrpc: "2.0"`. Define method schemas for workspace/session/task operations, decisions, inspection and event subscription, using architecture section 5 names or an explicitly versioned revision.
2. Separate request ID from durable command/idempotency identity. Preserve task/turn/root budget IDs and serialize large counters safely. Define structured errors for invalid/stale state, missing authority/input, unsupported version and unknown effects.
3. Generate JSON schemas and TypeScript types from the chosen canonical definitions; add examples and compatibility/version policy. Enforce unknown-required-field and enum evolution rules rather than silently accepting unsupported semantics.

Tests: codec/schema round trips, malformed frames, unsupported versions, duplicate commands with same/different payload, stale approvals, large counters and CLI/API parity. R01/E01/E09 extend to the public boundary now; earlier CLI-only tests do not establish this compatibility.

**Construction sequence.** Inventory each method in
[architecture section 5.3](../architecture/vcp-what.md#53-minimum-methods) against an
existing internal command/query. Produce a method table with scope, controller or
observer requirement, revision preconditions, mutation identity, size limits and
durable acceptance/result semantics. Expose pending inputs, unknown effects and
partial outcomes as typed data; do not flatten them into generic success/failure.
Identify any missing internal behavior separately rather than implementing it only
inside the public adapter.

Implement framing, envelope validation, initialization, authorization and method
decoding as distinct stages. Incrementally bound UTF-8 line buffering before parsing;
handle split reads, escaped newlines, invalid encoding and oversized frames. Keep
diagnostics off stdout. Define the compatibility profile for standard JSON-RPC
notifications/batches/error behavior and verify against the standard before claiming
conformance; selected upstream framing is not evidence of VCP compatibility.

Use [schema and identity design](../architecture/deferred-clients-design.md#schema-and-request-identity)
for canonical command digests and persisted deduplication. Distinguish request ID
from durable mutation identity across reconnects. Return the existing result for
the same scoped command/payload only after current access checks; reject reuse with
a changed payload. Serialize counters and money without JavaScript precision loss.
Classify unknown fields/enums by safety: presentation additions can be ignored,
unknown authority/governance semantics cannot.

Choose one canonical schema definition/generator and produce JSON Schema,
TypeScript bindings, examples and error documentation from it. Record the real
regeneration command after tooling exists. Golden traces must cover an older and
newer peer, capability absent despite matching version, unsupported major version,
concurrent duplicate mutation and stale approval. Compare final canonical state and
independent effects with the same scenario driven by internal CLI commands.

## P9-02 — Local server and attachment

Accepted on September 23, 2026. The [native construction and acceptance record](../development/local-attachment.md#p9-02-acceptance-and-next-dependency)
covers authenticated processes, leases, execution/inspection/governance adapters,
owner loss, pending input, snapshot/event recovery, durable retry and bounded
readers. The final scoped-retention increment qualifies physical source/derived
copy cleanup and abandoned-waiter/MCP recovery. SDK consumer evidence belongs to
P9-03; unadvertised editor observations/reconciliation belong to P4.

P9-01 acceptance covers the complete method schema inventory, generated artifacts,
compatibility profile and qualified initial six-method engine adapter. P9-02 owns
the remaining live host adapters and their execution evidence; schema presence
does not establish a runtime capability. Begin with canonical controller leases
and a host dispatch interface, then bind the existing lifecycle host to it before
exposing authenticated transports. Live steering must fence and drain through
the lifecycle host rather than mutate the engine directly. The
[local attachment construction record](../development/local-attachment.md)
describes this boundary and the remaining native-process acceptance.

Implement authenticated stdio/Windows-pipe attachment with explicit controller versus observer roles and bounded subscribers. One engine remains writer per data root; attach cannot create a competing controller. Bind grants/decision responses to authorized controller identity and session revision.

Define owner-loss/pause and reattachment semantics explicitly. Do not silently change the confirmed close-to-pause default because the engine is now a separate process. An observation client disconnect is different from loss of the controlling task owner; any grace period must be documented and tested.

An explicit pause also keeps the client connection open: status, inspectors and
event subscriptions continue while root/child dispatch is paused. Resume requires
an explicit revalidated command, not reconnect, a status read or a lease heartbeat.

Tests: unauthorized local client, two controller requests, dropped reader, cursor gap, reconnect during pending input, duplicate cancel/resume and process restart. A session can reconstruct events without repeating effects. Network-hosted VCP remains outside scope; local server support does not authorize remote exposure.

**Construction sequence.** Qualify endpoint permissions, peer authentication,
launch bootstrap and inherited handles on native Windows following
[local authentication and leases](../architecture/deferred-clients-design.md#local-authentication-and-controller-leases).
Do not use a pipe name/PID alone as authentication or put bootstrap credentials in
argv, workspace files or logs. Acquire the canonical data-root writer lock before
accepting mutations. A failed attachment cannot start a second competing writer.
Recovery of stale endpoint metadata verifies liveness and ownership explicitly.

Persist controller lease identity/generation and transfer/expiry events. Decision
responses atomically verify controller generation, pending question/approval and
operation/policy revision. An observer cannot gain mutation rights by replaying a
controller's command ID. Bind workspace scope to every request and artifact read,
including reads of prior command results. Separate owner-loss pause from an
observer's disconnect; a separately running server process is not a background-work
authorization. Resolve the reserved daemon wording in ADR-002 before adding any
future alternative ownership mode.

Implement snapshot-plus-event replay at a canonical cursor boundary so no event
falls between snapshot capture and subscription. Bound subscribers, queue bytes,
pending requests and artifact ranges. A slow reader gets a gap/resync/disconnect
diagnostic without blocking engine writes. Retention gaps and access revocation
cannot be disguised as a complete stream. Use
[event/reconnect semantics](../architecture/deferred-clients-design.md#events-reconnect-and-cancellation).

Test real local client/server processes with unauthorized identities where feasible,
competing leases, owner termination, still-connected pause, blocked subscribers and
crashes after durable acceptance but before response. Independent effect markers
prove a retried command did not create a second task or tool effect. Assert writer
ownership, retained unknown outcomes and exactly which cursor boundary rebuilt the
client view; mocked transport handlers cannot prove local endpoint security.

## P9-03 — SDK

Implement typed request/result calls, cancellable streams, resumable subscriptions, lifecycle errors and explicit resource cleanup. Avoid hidden retry of non-idempotent methods; retries use the caller's durable command identity and contract. Provide examples for run, inspect, wait/input, pause/resume and child progress using synthetic fixtures.

Run the SDK against a compiled real local server, not only mocked TypeScript types. Assert event ordering/gaps, cancellation propagation, no duplicate task on retry, wrong-version diagnostics and stable SDK/package compatibility. Generated source drift fails checks.

**Construction sequence.** Keep generated method/event types in `protocol-ts` and
hand-written connection lifecycle, operation handles, bounded decoders and streams
in `sdk-ts`. Each mutation API accepts or exposes the durable command identity so
the caller can reconcile timeout/disconnection. Pending connection request IDs may
change on retry; durable command identity must not. Classify errors into unsupported
version/capability, denied/stale authority, required input, interrupted transport
and durable engine outcomes without losing structured details.

Make cancellation semantics explicit: cancelling a local await/subscription stops
that consumer; cancelling a task/turn is a separate engine mutation whose receipt
and eventual reconciliation state remain inspectable. Document connection disposal,
controller ownership and resource cleanup. Reconnect restores a snapshot/cursor and
resolves pending operations before resubmission; it does not answer pending input
or resume paused work automatically.

Provide small synthetic examples for run/inspect, pending input, same-key retry,
pause while connected/resume, child progress, gap recovery and artifact reads.
Run them against the compiled fixture server under bounded subprocess cleanup.
Include an abandoned consumer and full output queue to prove resources release;
a type-check-only example cannot establish this. Validate package exports and
generated drift from a clean source checkout, recording the selected toolchain,
generator and engine/SDK compatibility range in the future protocol guide.

## Exit

Run public protocol/SDK contract and end-to-end suites alongside existing CLI behavior. Produce protocol reference, supported version matrix, sample traces and a reproducible generated schema diff. Only then can segment 18 depend on external attachment and types.
