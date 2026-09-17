# 17 — Deferred public API, local server and TypeScript SDK

Status: deferred. Owns P9-01 through P9-03. Starts after P8-05; it is not a Windows CLI release prerequisite. Architecture section 5 is the reserved external contract, and internal commands/events from P1 remain the engine boundary.

## Code organization

Extend `vcp-protocol` with `jsonrpc`, `handshake`, `methods`, `errors`, `schema` and `compatibility`. Add `vcp-engine/server` adapters for stdio and local Windows pipes. Generate `packages/protocol-ts/`; implement hand-written connection/subscription ergonomics in `packages/sdk-ts/`. Keep generated types distinct from edited SDK source, and preserve the CLI's direct internal path.

Public protocol adapters translate to existing typed commands; they do not introduce new task state, budgeting, policy or memory ownership. Reuse selected C01 schema/lifecycle machinery with explicit VCP wire adaptations and fixture evidence.

## P9-01 — Public protocol

1. Finalize version/capability negotiation and JSON-RPC envelopes including `jsonrpc: "2.0"`. Define method schemas for workspace/session/task operations, decisions, inspection and event subscription, using architecture section 5 names or an explicitly versioned revision.
2. Separate request ID from durable command/idempotency identity. Preserve task/turn/root budget IDs and serialize large counters safely. Define structured errors for invalid/stale state, missing authority/input, unsupported version and unknown effects.
3. Generate JSON schemas and TypeScript types from the chosen canonical definitions; add examples and compatibility/version policy. Enforce unknown-required-field and enum evolution rules rather than silently accepting unsupported semantics.

Tests: codec/schema round trips, malformed frames, unsupported versions, duplicate commands with same/different payload, stale approvals, large counters and CLI/API parity. R01/E01/E09 extend to the public boundary now; earlier CLI-only tests do not establish this compatibility.

## P9-02 — Local server and attachment

Implement authenticated stdio/Windows-pipe attachment with explicit controller versus observer roles and bounded subscribers. One engine remains writer per data root; attach cannot create a competing controller. Bind grants/decision responses to authorized controller identity and session revision.

Define owner-loss/pause and reattachment semantics explicitly. Do not silently change the confirmed close-to-pause default because the engine is now a separate process. An observation client disconnect is different from loss of the controlling task owner; any grace period must be documented and tested.

Tests: unauthorized local client, two controller requests, dropped reader, cursor gap, reconnect during pending input, duplicate cancel/resume and process restart. A session can reconstruct events without repeating effects. Network-hosted VCP remains outside scope; local server support does not authorize remote exposure.

## P9-03 — SDK

Implement typed request/result calls, cancellable streams, resumable subscriptions, lifecycle errors and explicit resource cleanup. Avoid hidden retry of non-idempotent methods; retries use the caller's durable command identity and contract. Provide examples for run, inspect, wait/input, pause/resume and child progress using synthetic fixtures.

Run the SDK against a compiled real local server, not only mocked TypeScript types. Assert event ordering/gaps, cancellation propagation, no duplicate task on retry, wrong-version diagnostics and stable SDK/package compatibility. Generated source drift fails checks.

## Exit

Run public protocol/SDK contract and end-to-end suites alongside existing CLI behavior. Produce protocol reference, supported version matrix, sample traces and a reproducible generated schema diff. Only then can segment 18 depend on external attachment and types.
