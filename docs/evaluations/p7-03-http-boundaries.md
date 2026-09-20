# P7-03 HTTP boundary prerequisites

This increment implements bounded JSON/SSE framing, scoped credential leases and
a private owned HTTP/1 transport with socket admission below TLS. It follows
[ADR-027](../adr/027-owned-http-send-boundary.md) and the
[development contract](../development/p7-mcp-http-boundaries.md).

P7-03 remains in progress. These prerequisites do not yet expose remote MCP
configuration, session negotiation, streaming callbacks or canonical remote tool
dispatch. The existing client continues to reject HTTP registrations.

## Qualification scope

Parser tests cover exact status/media types, empty 202 acknowledgements, JSON
and SSE limits, all chunk splits across BOM/CRLF/UTF-8, inert metadata, absolute
deadlines, partial EOF and preservation of an earlier frame before later bad
input. Framing alone never supplies a tool receipt or triggers another request.

Credential tests exercise exact workspace/server/reference/profile and owner
scope, expiry, revision-checked rotation and revocation, owner shutdown, and the
lock shared with physical sends. Capture tests cover ordinary JSON escapes,
secret-bearing keys, numeric/boolean echoes and replacement-marker collisions.
Malformed or unsanitizable content is rejected; no arbitrary encoding detection
is claimed. No actual credentials are used in these fixtures.

Native socket fixtures exercise zero and partial delivery, TLS validation,
buffered TLS output, owner loss, deadlines and bounded HTTP responses. A local
peer observes the bytes independently of the adapter's counters. Holds prevent
buffered private plaintext from being emitted by a later TLS flush, read or
shutdown. These tests establish local write fencing, not remote exactly-once
execution or proof of absent effects after bytes have left the socket.

## Delivery evidence

The source-frozen qualification passed all 141 tests in
`artifacts/p7-mcp-http-boundaries-qualification/362fdf5d-312f-47e4-9ddc-b25454a94675/manifest.json`.
The ten stages include 30 protocol/framing tests, 40 lifecycle/credential/socket
tests, four fixture tests, 12 canonical MCP tests, one coding-loop test, 43 CLI
tests, six duplex tests, four canonical duplex tests and one process-broker test.
One existing CLI test remained explicitly ignored. Canonical MCP and coding
regressions exercised both Files and SQLite.

Relevant source remained unchanged throughout:
`ab4a3bb88060a059db88319512391a09d6a9f2d5ddb5d3e3b333359021c75597`.
The MCP fixture executable digest was
`8f9531f527728abd1435399014058ecc3505f452766519e7563f368e9f11286d`.
The run used Rust 1.98.1 and MSVC 14.44.35207 on native Windows. The runner is
`scripts/evals/mcp-http-boundaries-qualification.ps1`.

All-target Clippy for extensions, lifecycle and CLI passed in
`artifacts/p7-http-boundaries-clippy.log`. The production CLI build check passed
in `artifacts/p7-http-boundaries-production-check.log`. Existing warnings remain,
along with expected unused-code warnings for the private HTTP prerequisite that
the next canonical integration will consume. Formatting and diff checks passed.

All nine repository delivery checks passed in
`artifacts/p7-http-boundaries-fast/934679e8-e27f-4530-8cd7-3457c7ca45f3/manifest.json`.
The static boundary inventory covers 103 seams, 175 packages and 38 groups.

The dependency change adds local edges to existing locked packages. Patch
`0032-p7-http-boundaries-workspace.patch` reconstructs the changed workspace and
lockfile from their prior committed bytes; no external version changed.

## Remaining integration

Canonical HTTP operations must still connect trusted endpoint resolution, scoped
credential setup, durable intent, source deletion fences, session/protocol
headers, incremental response processing and callback acknowledgements. The
complete-response transport prerequisite does not yet service callbacks while
an SSE response remains open. Remote crash/reconciliation fixtures and attributed
resource/prompt access remain required. No external MCP service or paid model
request was used for this increment.
