# P7-03 HTTP boundary prerequisites

The HTTP implementation extends the governed MCP work in
[ADR-026](../adr/026-governed-mcp-stdio.md), following the send-boundary decision in
[ADR-027](../adr/027-owned-http-send-boundary.md). These components establish transport
and credential boundaries; they do not yet enable remote MCP configuration or
complete P7-03. The canonical adapter must still connect negotiation, current
source validation, durable intent and outcome recovery to these components.
The [qualification report](../evaluations/p7-03-http-boundaries.md) records the
native boundary and regression evidence.

## Response framing

`vcp-extensions::mcp::http` handles bounded POST responses for the pinned
`2025-11-25` protocol. Requests accept status 200 with JSON or SSE; notifications
and protocol responses require status 202 with an empty body. Unsupported status,
media type, event type, malformed UTF-8, exceeded limits or deadline expire the
decoder. Each parsing step yields at most one frame before consuming the remaining
input, preserving an observed reply if a later event is invalid.

SSE identifiers and retry fields remain untrusted metadata. They never initiate
reconnect, replay or another request. Empty priming events are supported; a partial
event at EOF is discarded and reported. Body, frame, line, event and metadata
limits supplement the adapter's independent transport and header limits.

Framing does not establish a protocol receipt. The existing protocol client must
validate complete JSON messages. An HTTP response header is not proof that the
complete request was written. Notification acknowledgement must finish before the
next request; callback replies require separate admitted POSTs before further
protocol messages are consumed. The stdio client's HTTP rejection remains in
place until this exchange logic is integrated.

## Scoped credentials

Trusted remote profiles identify a workspace, server, profile revision, exact
HTTPS endpoint and optional credential reference. URL userinfo, queries,
fragments and ambiguous normalization are rejected. Profiles contain no secret
values and cannot themselves authorize network access.

The owner injects credential material into an in-memory scoped resolver. Leases
bind the workspace, server, reference revision, profile digest, owner and current
authority. Rotation, revocation, expiry or resolver closure invalidates old
leases. The transport checks a lease while holding its lock through a physical
socket write; revocation cannot race between that check and the write.

Credential material has no serialized representation, and diagnostic formatting
omits its contents. It is zeroized when the last in-memory owner releases it.
Late responses retain the original lease for sanitization after revocation.
Untrusted content stays in bounded private memory until complete values are
sanitized for capture. Auth headers and raw credential-bearing traffic must never
be persisted. This is an explicit bearer-token subset, not OAuth discovery,
automatic refresh or an environment-variable credential lookup.

## Physical transport boundary

The adapter uses the existing locked Hyper HTTP/1 and Rustls libraries. Each
POST owns a fresh connection and one request attempt; no connection pool,
redirect, proxy discovery, automatic authentication or retry is introduced.
The caller supplies the authorized recipient and explicit TLS trust configuration.
Production requests require TLS. Plaintext loopback is available only to test and
qualification builds. The current exchange returns a bounded complete response;
incremental SSE delivery and callback POSTs still need canonical integration.

The write gate belongs below TLS, around the raw socket. TLS can accept plaintext
while retaining encrypted output, and subsequent flushes or reads can write that
output. Checking only the request body or TLS plaintext API would miss those
writes. Owner attachment, holds, admission generation, deadlines and credential
revocation must therefore remain current during each underlying socket write.
Bytes already accepted by the operating system may still arrive remotely;
interruption after any delivery remains uncertain.
Connecting sockets are registered before the operation yields. A hold or owner
loss closes the selected sockets even when the caller has stopped polling.

These primitives grant no tool authority. Canonical HTTP dispatch must register
owned work before its final source fence, persist intent before sending and retain
unknown outcomes without replay. Source deletion, session negotiation, scoped
credential setup in the CLI, resource/prompt access and end-to-end remote recovery
remain separate integration work.
