# ADR-026 — Governed sequential MCP stdio

Status: accepted implementation direction for the bounded P7-03 stdio increment.
Qualification and remaining P7-03 acceptance are recorded separately.

## Decision

Implement an original sequential codec and state machine for MCP `2025-11-25`
in `vcp-extensions::mcp`, using existing serde, domain identity and protocol digest
facilities. The core validates data and returns bytes; the canonical host owns
Windows duplex processes, deadlines, current authority, source validation, durable
intent and outcome capture. No new SDK/runtime dependency is introduced.

The initial profile supports explicit configured stdio servers, initialize,
bounded paginated tool discovery and one outstanding tool call per connection.
Ping receives a control response. Other server requests receive method-not-found;
sampling, roots, elicitation and task callbacks are not advertised or executed.
Unknown or malformed protocol input closes the connection. The codec does not
retry, reconnect, renew authority, fetch resource URLs or reset deadlines.

A registration resolves a trusted process-profile reference and its current
digest. A connection receives a freshly allocated UUID generation, including
after owner reopen. Tool identity binds registration, connection generation,
remote name, schema digest and catalog revision. List changes and rediscovery
invalidate earlier prepared identities even when the schema bytes are unchanged.
Remote descriptions and annotations remain untrusted data. Calls retain the
startup process's conservative Read/Write/Execute/Network/Install/Publish/Opaque
effect set and write-capable resources rather than trusting read-only hints.

Schema admission is an explicit restricted profile, not full JSON Schema support.
It validates every supported predicate and rejects unsupported keywords at
discovery. Numeric input is limited to exact `i64`/`u64` JSON representations;
decimal/exponent forms are rejected before they can be rounded into dispatch
bytes. Duplicate keys and depth/node/byte excess fail during bounded parsing.
The [development contract](../development/p7-mcp-stdio.md) lists the supported
schema and message subset.

## Authority and preservation

Process startup permission does not authorize later protocol calls. The live
opaque server retains the process conflict claim until shutdown. Each call has
its own canonical operation, immutable arguments, current source/authority checks,
durable dispatch intent and receipt. Owner-entered arguments and model-produced
arguments have distinct provenance; model calls retain their accounted verified
context and source dependencies. Source deletion or authority changes must fence
dispatch, including the final transport boundary.

Timeout, cancellation, malformed replies and process loss do not establish that
a remote effect did not happen. An interrupted write closes the connection and
an unobserved dispatched call remains uncertain. No status probe repeats a write.
Server success, tool error and JSON-RPC error are separate observations. Server
content is captured as external evidence and cannot alter VCP authority.

An exact granted call may leave its task waiting while the already-authorized
server is idle. Deliberate resume may use a private ephemeral proof to exempt
only that owned running process lifetime from reconciliation. The proof retains
the slot/lease, checks the exact approval and effect revision, live job, current
sources and policy, and holds the final lifecycle guard through the transition.
It applies only to `WaitingForInput`, never a paused/held owner; unrelated or
unknown effects still block. The call still needs normal dispatch admission.

## Alternatives and evidence

The retained workspace pins `rmcp` 3.2.0. Its model definitions and the retained
`rmcp-client` are useful implementation evidence, but the service runtime is not
the dispatch boundary selected here. Inspection found unbounded default stdio
line accumulation, compatibility parsing, automatic discovery fallback, enabled
response caching with stale-on-error, and high-level tool-call follow-up rounds.
Its legacy initialize path also needs an explicit negotiated-version check.
Wrapping these behaviors would add state that competes with the canonical
single-use dispatch and owned sequential duplex interface.

A small original state machine keeps each physical send visible to the host.
Before writing, the host checks the outbound token's connection ownership and
unsent state, then applies current canonical fences. It confirms delivery only
after the complete local write succeeds. This does not prove remote execution;
only an observed response produces a protocol receipt.

## Consequences and reconsideration

The supported subset is intentionally narrower than general MCP interoperability.
Streamable HTTP, scoped credential injection/sanitization, resource and prompt
access, and broader schema/numeric support remain separate required P7-03 work.
Existing authentication-reference fields do not imply an implemented auth flow.
The stdio increment cannot mark P7-03 or a usable release complete.

Reconsider SDK reuse when a bounded transport and service profile can demonstrate
no hidden sends, replay, cache authority or callback execution while preserving
the same current-source and durable-intent boundary. Broader numeric support must
preserve exact values rather than silently canonicalizing rounded floats. Remote
transport needs its own endpoint identity, auth and negative upload fixtures.
