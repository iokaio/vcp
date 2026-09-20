# P7-03 exact numeric/schema qualification

Status: exact numeric/schema increment qualified. P7-03's two final fault proofs
were still pending when this campaign completed.

Follow-up: the [final fault campaign](p7-03-final-faults.md) now closes those
proofs and completes P7-03's documented subset. The evidence below records the
numeric increment before that follow-up.

[ADR-028](../adr/028-exact-mcp-schema-profile.md) and the
[profile contract](../development/p7-mcp-schema.md) define this increment. It must
preserve admitted exact values across schema validation, prepared arguments,
physical stdio/HTTPS requests, secret filtering, retained receipts and subsequent
model context. Both storage engines participate.

Preparation evidence includes 23 isolated numeric/schema prototype tests and 83
tests against an adapted extensions crate with the actual path dependencies;
strict Clippy passed for both. A separate copied model codec passed 47 tests.
These runs are recorded under `artifacts/p7-mcp-numeric-prototype`,
`artifacts/p7-mcp-numeric-promotion` and `artifacts/p7-mcp-numeric-model-draft`.
They do not replace the tracked native campaign.

The audit first reproduced and delivered the
[P1-04 persistence repair](p1-persisted-json.md), preserving literal historical
JSON objects and unchanged canonical hashes. Model decision decoding is another
compatibility seam: exact decimal confidence, score and monetary usage must not
arrive as synthetic serde maps under feature unification.

Capture review reproduced another integration gap: numeric secrets such as
`0.3000` changed to `3e-1` before final capture and escaped literal-only matching.
The corrected filter uses the same exact profile to protect normalized aliases
and compare numeric scalar forms; raw serde exponent normalization is covered
too. The isolated before-fix evidence is retained in
`artifacts/p7-mcp-numeric-secret-probe/result.log`. Native argument, discovery and
reply tests passed on both stores, including rejection before artifact/effect/
approval creation, discovery rejection without mutating identity, and omission
of sensitive observed results without erasing observed completion.

The seven focused native cases passed. Independent peers compare actual request
bytes with fixed exact-value expectations, then join their digests to canonical
wire intents. The matrix includes stdio and HTTPS, both stores, stale schema
identities, invalid arguments before dispatch, exact callback IDs, invalid output
and loss after one independently recorded effect. Resource reads, cache hits and
prompts retain numeric-looking text byte-for-byte while accepting fractional
metadata; cache hits perform no new I/O.

The coding case uses six settled synthetic model responses, two owner approvals,
four actual TLS requests and one tool marker per store. It verifies exact numeric
tool results in later model context and their retained source/identity evidence.
It makes no live model-quality claim.

Pure exact-arithmetic tests include an independent scaled-integer oracle over
24,108 pairs, boundary coefficients/exponents, recursive numeric equality and
bounded graph/predicate work. Unsupported keywords and references fail even in
unused definitions or unselected branches; exhausted work is an error rather
than an assumed nonmatch. Model tests separately preserve existing probability
semantics and upward-rounded exact monetary usage.

Production CLI checking and all-target Clippy for extensions, models, lifecycle
and CLI passed. Logs are `artifacts/p7-mcp-numeric-production-check.log` and
`artifacts/p7-mcp-numeric-clippy.log`; existing warnings remain visible. A new
fixture comparison warning was corrected while retaining its exact numeric-token
assertion, before the frozen campaign. Affected formatting and the staged diff
whitespace check passed.

Earlier focused native failures came from fixture metadata outside the supported
tool-annotation shape and selecting the first resource rather than its exact URI.
Those fixture corrections preserved production validation. Failed logs remain
available; their partial results do not qualify this increment.

## Frozen campaign

The campaign passed **303 tests across sixteen stages**, with one existing
cloud-directory CLI test ignored because its explicit native cloud path was not
configured. It ran on Windows 10.0.26200, Rust 1.98.1 and MSVC 14.44.35207.
Relevant source content and commit identity remained unchanged throughout.

| Stage | Passed |
| --- | ---: |
| MCP codec, exact schema, identity and HTTP sessions | 84 |
| Models and decision codec | 47 |
| Exact-feature protocol and store preservation | 12 |
| Lifecycle, credential, transport and trust boundaries | 51 |
| Stdio fixture unit and wire tests | 4 |
| Numeric host and actual coding loop | 7 |
| Existing content host and coding loop | 12 |
| Existing stdio host | 12 |
| Existing HTTP host, faults and coding loop | 15 |
| Existing stdio coding loop | 1 |
| CLI | 47 |
| Duplex and host regressions | 10 |
| Native process broker | 1 |

Manifest:
`artifacts/p7-mcp-numeric-qualification/bdce7893-861b-4847-bd73-6cb2cd404ba5/manifest.json`.

- Source content: `8c2b1deafb6151ede1efe12cb6bfff782c42db7e0f3546d92a0b47e72334da53`.
- Canonical HTTP fixture binary: `57c9fc4b92319e5430826d76e885df4035871bbc5a8caf80cf59f6cefd23c539`.
- Stdio fixture binary: `6ec34665c33c691f905a459b7358447579379129a32f559ab41943172bd3e0f7`.
- Runner: `63b1877750efb13d7ef9e26dbd771b3dfc8c1c8ab027ee6dbdf2b6eef92fb306`.

The initial fast delivery run passed eight gates and found a stale ADR inventory
count. The inventory and documentation now include ADR-028. All nine final fast
delivery gates passed, including repository contracts, source reconstruction,
dependency inventories and classified effect boundaries:
`artifacts/p7-mcp-numeric-fast-final/dec1a277-412a-4b90-87c2-ac93fd20010b/manifest.json`.

## Remaining gates

An acceptance audit identified two bounded P7-03 proofs still required: controlled
interruption after a valid reply but before local receipt persistence, with
reopen/no replay; and an actual native HTTP 401 response. Existing marker-loss,
owner-close and pure decoder tests do not substitute for those exact cases.

The [profile contract](../development/p7-mcp-schema.md) lists supported semantics
and explicit rejection limits. Binary content remains omitted and unsupported
server-initiated capabilities remain disabled. Controlled peers and synthetic
model responses do not qualify arbitrary hosted servers or live model quality.
P7-02 skill quality and P8 packaged/fresh-machine qualification remain separate.
