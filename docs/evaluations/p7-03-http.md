# P7-03 canonical HTTP qualification

The [canonical remote MCP adapter](../development/p7-mcp-http.md) connects the
previously qualified HTTP and credential boundaries to tool discovery and calls.
The selected protocol remains `2025-11-25`, using bounded HTTPS POST with JSON or
SSE responses. GET event streams, automatic replay, OAuth discovery, resources and
prompts are outside this increment. P7-03 remains in progress.

## Qualification contract

`scripts/evals/mcp-http-qualification.ps1` freezes source identities before and
after native Windows tests, records exact commands and log hashes, rejects empty
test selections and background panics, and retains the stdio fixture executable
hash. The controlled TLS peer uses explicit synthetic roots and independently
records requests and file effects. It does not modify Windows certificate stores
or contact a hosted MCP service.

The campaign combines protocol session state, HTTP framing, credential and raw
socket gates, Windows trust snapshots, canonical remote calls, stdio and coding
regressions, CLI configuration and process lifecycle regressions. Both canonical
storage engines participate in the remote host fixtures. Required boundary cases
include denied or revoked sends, callback POST acknowledgements, secret exclusion,
known-reply preservation and lost-reply recovery without another effect.

## Evidence status

The final frozen campaign passed **179 tests**, with one existing CLI test ignored,
on native Windows using Rust 1.98.1 and MSVC 14.44.35207. Source identity remained
unchanged throughout all eleven stages:

| Stage | Passed |
| --- | ---: |
| Protocol, schema, identity and HTTP sessions | 43 |
| Lifecycle, credential, transport and trust boundaries | 48 |
| Stdio fixture unit and wire tests | 4 |
| Governed stdio host tests | 12 |
| Governed HTTP host, fault and model-wrapper tests | 15 |
| Stdio model-wrapper regression | 1 |
| CLI | 45 |
| Native duplex and host regressions | 10 |
| Native process broker regression | 1 |

Manifest:
`artifacts/p7-mcp-http-qualification/2a72783a-20d3-475f-b11b-f3ad60805a66/manifest.json`.

- Source content: `59afd30042ac400c7fb61f5d0b3fe938fce07122eef7f6adc1581136d7228566`.
- Canonical HTTP fixture binary: `eece9d46ec69fa7391b5ab10e4742224c50af4b4b5c516ec27a07dd669782fe1`.
- Stdio fixture binary: `94d6acbc2e61ec3204ed23a06ef854a9abe53bbe2ab71efdb6021c91a96c1189`.

The HTTP matrix exercises both stores, real TLS, explicit approval, independent
synced marker effects, source deletion before dispatch and during TLS setup,
retention after the last source check, credential revocation, cancellation and
owner shutdown followed by reopening, and lost replies without replay. It also
checks wrong trust, redirects, changed sessions, cumulative output limits and
preservation of validated replies before malformed or invalid trailing events.
Persisted artifact bodies exclude plain and JSON-escaped bearer/session echoes,
including callback IDs. A bearer-bearing endpoint is rejected before creating any
artifact, effect or approval. Local flush observations bind to exact wire intents;
they are explicitly distinct from remote-effect receipts.

The scripted coding test executes the actual model-facing MCP wrapper on both
stores. Each run observes two owner approvals, six settled model attempts, four
TLS requests matching durable intents and one marker effect. This proves adapter
integration and accounting, not live model quality.

Production CLI checking passed in `artifacts/p7-mcp-http-production-check.log`.
All-target Clippy for extensions, lifecycle and CLI passed in
`artifacts/p7-mcp-http-clippy-final.log`; existing warnings remain visible.
Affected Rust files pass formatting checks. The fast delivery harness passed all
nine gates, including selected-source reconstruction, documentation, dependency
inventories and 105 classified effect boundaries across 175 packages:
`artifacts/p7-mcp-http-fast/3263f7de-bfa0-491e-9cf2-d5e06351238e/manifest.json`.

`0033-p7-mcp-http-workspace.patch` reconstructs the locked Windows API dependency
edge byte-for-byte from the prior selected source; no external version changes.
Roundtrip evidence is under
`artifacts/p7-mcp-http-workspace-roundtrip-d49b33ef-c7dc-47c3-ba35-e529786bb2bf`.

## Limits

This campaign establishes controlled local TLS and canonical lifecycle behavior.
It does not establish compatibility with arbitrary hosted servers, Windows chain
engine equivalence or remote exactly-once execution. A local write observation
cannot prove a remote effect. Missing receipts remain unknown until independently
reconciled. Resource and prompt qualification remains subsequent P7-03 work;
packaged executable and fresh-machine qualification remains P8 work.
