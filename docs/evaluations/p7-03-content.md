# P7-03 resource and prompt qualification

Status: resource/prompt increment qualified. P7-03 remains in progress pending
broader exact numeric/schema support required by ADR-026.

The [content contract](../development/p7-mcp-content.md) defines the supported
MCP 2025-11-25 subset. The native campaign combines pure codec tests, canonical
content fixtures on stdio and HTTPS with both SQLite and file stores, CLI/model
controls, and existing tool, transport and process regressions.

The passing matrix covers exact-URI selection without local dereference,
resource-only and prompt-only servers, bounded pagination, changed identities,
external prompt roles, explicit binary omissions, malformed and oversized
replies, credential/session sanitization, scoped cache hits and current denial,
pruning/reconnect/revocation, and cancellation after an independent marker.

The cache matrix distinguishes active retention compaction from terminal purge:
compaction invalidates active cache access while retaining evidence bytes;
purge runs only after existing task/effect protections permit deletion. Stdio
policy changes drain the process and invalidate its cache; HTTP additionally
tests a current Read denial against an otherwise live session. A separate stdio
exit barrier proves that a dead job cannot continue serving cached observations.

The scripted model test observed eleven settled synthetic provider responses,
four explicit approvals, six HTTP requests and two independently recorded
content markers per store. Its provenance assertion follows the persisted
coding-pair source edge from the original cache artifact to the untrusted
ToolResult part included in the later prompt context. It does not assume raw
dependency artifacts are themselves directly selected context parts.

Review reproduced a pre-existing integer-profile bypass under serde_json feature
unification: `1.5` reached a custom visitor as a synthetic object and passed an
open-object schema. The scoped parser now rejects the reserved internal keys
and escaped lookalikes before conversion. Both normal and explicit
arbitrary-precision feature graphs are covered; no dependency feature or version
is changed by this increment. Review also added decoded session-value screening
before operation capture and truthful cache availability after invalidation.

The runner records selected test names, log hashes, source identities before and
after the campaign and the actual fixture executable identities. It retries only
bounded, transient Windows sharing/lock errors when writing its manifest.

Two earlier campaigns remain recorded as failed evidence. The first exposed a
receipt compatibility regression: discovery must retain a null receipt identity
and tool calls their direct ToolIdentity. The correction keeps richer prepared
operation evidence separate from that receipt contract; the unchanged HTTP model
regression then passed on both stores. The second passed all 209 tests reached
but stopped on a transient manifest sharing lock before the final two stages.
Neither partial run qualifies the increment.

Production CLI checking and all-target Clippy for extensions, lifecycle and CLI
passed in `artifacts/p7-mcp-content-production-check-final.log` and
`artifacts/p7-mcp-content-clippy-final.log`; existing warnings remain visible.

## Frozen campaign

The final campaign passed **214 tests**, with one existing CLI test ignored, on
Windows 10.0.26200 using Rust 1.98.1 and MSVC 14.44.35207. Source identity remained
unchanged throughout all thirteen stages.
The only subsequent source cleanup removed a trailing blank line from the runner.

| Stage | Passed |
| --- | ---: |
| Protocol, content, schema, identity and HTTP sessions | 57 |
| Explicit arbitrary-precision feature graph schema regression | 5 |
| Lifecycle, credential, transport and trust boundaries | 50 |
| Stdio fixture unit and wire tests | 4 |
| Content host and actual model-wrapper tests | 12 |
| Existing stdio host tests | 12 |
| Existing HTTP host, fault and model-wrapper tests | 15 |
| Existing stdio model-wrapper test | 1 |
| CLI | 47 |
| Duplex and host regressions | 10 |
| Native process broker regression | 1 |

Manifest:
`artifacts/p7-mcp-content-qualification/f4825179-f02f-495e-93b0-b83e79c76c5b/manifest.json`.

- Source content: `63e0fa26c0deeb2c64bd4ac61a814d51b4b39f46d6c81d5cb3b0bf5e5789a61f`.
- Canonical HTTP fixture binary: `aa0eb64b71197a0d0892bb9f0242b1c257af0bfcc72a489036ac5bbb7211d6e5`.
- Stdio fixture binary: `1bc16ec7b996a715c611357bb839a209ab8b007aaf3ce43bbdf14bdbf68de363`.

Affected Rust formatting and `git diff --check` passed. The manifest sharing-lock
probe recovered from a transient lock and rejected a persistent lock within its
bounded attempts.

All nine fast repository delivery gates passed, including documentation,
selected-source reconstruction, dependency inventories and classified effect
boundaries:
`artifacts/p7-mcp-content-fast-final/68fdedd6-ada6-4134-94a6-3a2a06b5cd56/manifest.json`.

## Remaining gates

The current MCP JSON profile still admits only exact i64/u64 integer spellings;
broader exact numeric/schema support remains required P7-03 work. Binary content
is explicitly omitted. These controlled stdio/TLS peers and synthetic model
responses prove canonical integration, not arbitrary hosted-server compatibility
or live model quality. Packaged executable and fresh-machine qualification remain
P8 work.
