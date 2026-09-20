# P7-03 final fault qualification

Status: qualified; P7-03 complete for its documented supported subset. This
increment closes the two precise gaps
identified by the [numeric acceptance audit](p7-03-numeric.md); it adds no new
transport, authentication flow or schema feature.

## Receipt interruption

The qualification build exposes a one-use barrier on the exact prepared proposal.
Both stdio and HTTPS arrive only after a successful protocol reply is validated;
HTTP sanitization also finishes before arrival. The barrier precedes the worker
closure that captures the durable result artifact and advances the canonical
effect. It retains the existing absolute deadline, call guard and owned resources.
Production builds do not contain this hook.

The passing matrix covers Files and SQLite, both transports, interruption and
normal release. At arrival an independent server marker must exist exactly once,
the effect must still be in progress and its result receipt must be absent.
Cancelling the waiting future uses ordinary guard recovery. Two actual host
reopens must retain OutcomeUnknown without another tool call. Releasing the same
barrier in the control case must instead produce a succeeded effect linked to one
result artifact, with the same single marker after both reopens.

This is controlled interruption before durable receipt capture, not an OS
hard-kill or power-loss qualification. It does not claim atomicity between the
separate artifact capture and terminal effect writes, or test the narrower
artifact-captured/effect-uncommitted interval. A reply observed only in memory
cannot silently become a durable success after interruption.

## Native authentication rejection

The TLS peer expects a different synthetic bearer from the valid scoped credential
installed in the host. The approved initialize request must physically reach the
peer once and receive HTTP 401. An independent peer counter advances only after
the complete 401 write and TLS flush succeed; the test requires exactly one.
The test requires no catalog, initialized
notification, tool call, effect marker or automatic replay. Actual retained
artifact bytes and visible errors must exclude both credential canaries.

The conservative unknown result is intentional: a sent request followed by a
transport/protocol rejection is not treated as proof of remote exactly-once
behavior. Reopen retains that state and performs no network probe. This case is
distinct from already-qualified local credential revocation before send.

## Evidence and scope

The final focused run passed all three native tests in 18.82 seconds after
recompiling the strengthened 401 evidence assertion. Log:
`artifacts/p7-mcp-final-fault-focused.log`. Independent review found no remaining
blockers.
The runner selects lifecycle/transport tests, the new fault matrix and existing
numeric, content, stdio, HTTP, coding, CLI and process regressions. Unchanged pure
numeric/model/persistence evidence remains in the preceding 303-test campaign.

The frozen campaign passed **163 tests across thirteen stages**, with one existing
cloud-directory CLI test ignored because its explicit native cloud path was not
configured. It ran on Windows 10.0.26200, Rust 1.98.1 and MSVC 14.44.35207.
Relevant source content and commit identity remained unchanged throughout.

| Stage | Passed |
| --- | ---: |
| Lifecycle, credentials, transport and trust | 51 |
| Stdio fixture unit and wire tests | 4 |
| Final receipt interruption and native 401 matrix | 3 |
| Numeric host and coding loop | 7 |
| Resource/prompt/cache host and coding loop | 12 |
| Stdio host | 12 |
| HTTP host, faults and coding loop | 15 |
| Stdio coding loop | 1 |
| CLI | 47 |
| Duplex and host regressions | 10 |
| Native process broker | 1 |

Manifest:
`artifacts/p7-mcp-final-fault-qualification/9ffecb52-ac00-4c14-bb57-6045a66d4dff/manifest.json`.

- Source content: `45c3be84b954dd14ff33b2de057d8dfee6ace200bb7caf409fb2580efb7921f5`.
- Canonical HTTP fixture binary: `480f772175c881a9a2e2bdb5abc4226af9ff4bff94080a989f9fcf6d51b16f75`.
- Stdio fixture binary: `3680afdb5cc558bb40f985453803b041c173bf377b700eaeef2391c37238082f`.
- Runner: `e126987fcce0e933b4c3078b560cf9c9a39e6c0568e59844302ab62bd4f852f0`.

Production CLI checking and all-target Clippy for lifecycle/CLI passed in
`artifacts/p7-mcp-final-fault-production-check.log` and
`artifacts/p7-mcp-final-fault-clippy.log`; existing warnings remain visible.
Affected formatting and diff whitespace checks passed.
All nine fast repository delivery gates passed:
`artifacts/p7-mcp-final-fault-fast/fcc6829a-dc92-4d78-ad3b-dd6c6c218e35/manifest.json`.

Together with the [stdio](p7-03-stdio.md), [HTTP](p7-03-http.md),
[content](p7-03-content.md) and [exact schema](p7-03-numeric.md) reports, this
qualifies the documented MCP 2025-11-25 subset. It does not expand the subset to
arbitrary servers, binary content, full JSON Schema or disabled server callbacks.
Live skill quality remains P7-02 work; packaged-executable and fresh-machine
qualification remain P8 work. No paid requests are made by this campaign.
