# P7-03 duplex broker prerequisite

This increment adds owned, bounded native stdin/stdout transport and canonical
process lifetime integration. It does not complete MCP protocol or remote-server
support. Startup authority does not grant individual protocol calls; production
writes remain internal and the test write entry point requires `qualification`.

## Native evidence

The source-stable qualification run is
`artifacts/p7-duplex-qualification/3ff09356-ebc4-4201-8653-c4904c931e74/manifest.json`.
Five low-level tests, three canonical host tests and the existing process broker
regression passed. Relevant source identity was unchanged throughout:
`89d399a6a5288de32ba025de2947bb0bac669b975332e459fdc2f421f15becd7`.
The native fixture executable digest was
`a28518ec417167bded23a480d72051c780c49b0c8d688b2af95eb922e7b6aeb4`.
The run used Rust 1.98.1 and MSVC 14.44.35207 on native Windows.

The native fixtures verify bounded framed exchange, cleared ambient environment,
input budget with actual delivered-byte markers, malformed/oversized output,
queue overflow, stderr/output and lifetime caps, descendant process limits and
file-lock release, pause/owner close, caller drop and cancelled partial writes.

Each canonical case runs on Files and SQLite. They verify trusted denied startup
with no native marker or execution ID, stale input rejection, exclusive lifetime
claim retention, durable input/outcome artifacts, revocation blocking both queued
output and future input, and outcome-unknown preservation after reopening. No
test infers MCP call success from a successful server-process exit.

## Other checks

All nine delivery checks passed in
`artifacts/p7-03-duplex-fast/1e30b8ff-e6b8-4502-8678-d071c6331ade/manifest.json`.
Rust formatting and diff checks passed. Final all-target lifecycle Clippy and the
production-feature build check passed, with existing warnings. Logs are
`artifacts/p7-03-duplex-clippy-final.log` and
`artifacts/p7-03-duplex-production-check.log`.

## Remaining P7-03 gates

Protocol/version negotiation, registration, bounded discovery/schema validation,
scoped credentials, remote transport, resources/prompts, per-call source fences,
durable MCP receipts and remote-effect crash/reconciliation fixtures remain open.
No external MCP service, model call or paid evaluation was used. This prerequisite
retains the existing 120-second process cap and conservative process isolation.
