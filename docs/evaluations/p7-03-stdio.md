# P7-03 governed stdio tools

This increment implements explicitly configured local MCP tool discovery and
calls through the canonical process and effect lifecycle. It follows
[ADR-026](../adr/026-governed-mcp-stdio.md) and the
[stdio contract](../development/p7-mcp-stdio.md). P7-03 remains in progress:
HTTP transport, scoped credentials and resource/prompt support are not qualified.

## Frozen native evidence

The source-stable run is
`artifacts/p7-mcp-stdio-qualification/ad12a1b3-7ef6-4220-8a5d-f1c0235d9bee/manifest.json`.
Its ten stages passed 111 tests: 15 core tests, four fixture tests, 12 canonical
MCP host tests, one scripted-provider coding test, 43 CLI unit tests, six duplex
regressions, 25 lifecycle unit tests, four canonical duplex tests and one process
broker regression. One existing CLI unit test remained explicitly ignored. The
canonical MCP and coding cases exercise both Files and SQLite.

Relevant source identity was unchanged throughout:
`99dbe3e2f51f7a3065eb99df615d392cdb252378133b8d0bad752c324cef5057`.
The native MCP fixture executable digest was
`852f9e80899a0ed78688fdfc78402937287db4abc2a1b6ce03f5270dd9dcf788`.
The run used Rust 1.98.1 and MSVC 14.44.35207 on native Windows.

The fixtures verify exact protocol/version negotiation, bounded discovery and
schemas, immutable connection identities, callback rejection, malformed and
oversized output, explicit approval reuse, current authority and independent
stderr limits. A deleted captured model source prevents dispatch. A deterministic
barrier between canonical source validation and the physical pipe write proves
that applying retention prevents the tool payload from reaching the peer.

Actual server markers distinguish successful calls from transport observations.
Lost responses and cancellation after a marker retain one write and an unknown
outcome after reopening, without replay. Idle-server approval resume requires the
exact granted call; unrelated running or unknown effects still block it. The
scripted provider discovers a tool, calls its exact identity, disconnects, then
reads the actual marker through a native tool. Its receipts retain the originating
verified model context and source artifacts.

Conflicting server startup fails immediately with a cancelled unsent effect.
Disconnect, owner shutdown, stale policy and invalid IO limits also retire unsent
proposals. Historical pending questions become non-actionable through effect
revision changes; no approval decision or grant is manufactured by cleanup.

## Delivery checks

All nine repository delivery checks passed in
`artifacts/p7-03-mcp-delivery/f5fae4ff-f7ed-41bf-aef6-e4c08619dce1/manifest.json`.
The boundary inventory covers 102 seams, 175 packages and 38 groups. No external
dependency versions or retained upstream source were changed.

The final all-target lifecycle/CLI Clippy check passed with existing warnings in
`artifacts/p7-mcp-cleanup-clippy.log`. The production CLI build check, with
qualification hooks disabled, passed in `artifacts/p7-mcp-production-check.log`.
Rust formatting and diff checks passed.

Before the final unsent-proposal cleanup, the complete canonical-host regression
suite passed 53 tests with six explicit asset/operator qualification cases ignored
in `artifacts/p7-mcp-full-host.log`. The final cleanup was then verified by the
source-frozen MCP, scheduler, duplex and process cases above. Ignored live-asset
and operator gates are not reported as passes.

## Remaining gates

This is the bounded sequential stdio profile, not complete MCP interoperability.
The numeric/schema subset, disabled optional capabilities and conservative
process authority are explicit in the development contract. Scoped credential
resolution and capture sanitization, HTTP recipient/data authority, JSON/SSE
transport, resources/prompts, and their remote crash/reconciliation fixtures
remain open. No paid model request or external MCP service was used.
