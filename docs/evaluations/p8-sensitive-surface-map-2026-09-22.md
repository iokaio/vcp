# P8-03 sensitive-surface coverage map — 2026-09-22

This map preserves the executed evidence in the [local qualification report](p8-local-qualification-2026-09-22.md) and [release scorecard](p8-release-scorecard-2026-09-22.md). The [history/security follow-up](p8-history-security-followup-2026-09-22.md) records new execution receipts and exact identities. Private snapshot staging, packaged sensitive surfaces, independent crypto and packaged MCP history now pass on both stores. Source inspection and a registered test are not a pass. Machine handoff remains owner-skipped. Production-artifact qualification belongs to P8-01/P8-04.

| Surface | Existing evidence and boundary | Missing-case increment |
|---|---|---|
| Provider input and credential header | `ProviderCredential` is non-serializable and has omitted `Debug`; `CaptureSession::request` serializes only `RequestBody`. Native canonical-host tests compare captured request bytes with wire bodies while the authorized transport holds its header. | `packaged_typed_secrets_stay_out_of_capture_environment_and_presentations`: actual request bodies exclude provider and generated recovery-key material; every authorized synthetic provider request carries its expected header. |
| Native tool environment | `vcp-tools` accepts an explicit public environment allowlist; native process creation calls `env_clear()`. Existing profile tests reject provider-key names. | The packaged child checks that both the provider variable and an ambient variable deliberately carrying disposable recovery material are absent, then emits a positive ordinary-text marker. |
| Full capture and canonical records | Executed packaged oversized-output test reads complete bytes across compaction/reopen and verifies omission categories. | Scan every retained artifact payload and serialized canonical records/events/receipts from both stores, with positive evidence that ordinary token-looking text remains. |
| JSONL, diagnostics and inspectors | Common executable JSONL consumer already checks the provider marker. Executed packaged MCP preflight rejects missing/invalid credentials before task acceptance and networking. | Scan task stdout/stderr, key create/verify replies, `doctor`, ten inspector views and full artifact inspection for exact disposable key material and provider markers. |
| Terminal | Prior interactive rows prove rendering and lifecycle behavior, without combined recovery-key marker assertions. | Actual ConPTY renders an artifact inspection; scan its bytes and require the ordinary marker. This is a terminal inspection claim, not another pause/resume qualification. |
| Private snapshot staging | `Jobs::prepare` writes the scoped neutral archive before encryption; typed key handles and trusted key registry are outside that inventory. | Pass: `private_snapshot_staging_excludes_key_material_but_retains_scoped_ordinary_text` decodes every JSON byte-array payload in the actual `.archive`, checks disposable age identity/writer seed exclusion, preserves ordinary scoped source bytes, verifies its accepted digest and observes cancellation cleanup on both stores. Receipt `sensitive_stage-8b03c29d-5662-43e0-97a2-f07e206c3fbc` and exact hashes are in the follow-up report. |
| Exported encrypted snapshot | Executed `packaged_crypto` generates fresh CLI snapshots; pinned independent Go age decrypts them and Node verifies writer signature, inventory, each payload digest and provider/recovery-marker exclusion. Wrong-key/tamper/rotation/local restore evidence remains retained. | Pass against the `0087829f` candidate: `crypto-056f6858-e4dd-4a43-a40a-6f2bb847f9ba` verifies 203 payloads per backend, exact opposite-backend local activation and retry, negative controls and no replay. Exact receipt/report hashes are in the follow-up report. Marker absence supplements cryptographic verification and is not itself encryption proof. |
| MCP typed credentials and sessions | `CredentialMaterial(Zeroizing<String>)` in `foundation/mcp/remote_authority.rs` has omitted `Debug` and no serialization implementation. Native HTTP 401 qualification checks an actual TLS response, no replay, reply/artifact marker exclusion and reopened identity. Native HTTP echo tests inspect both raw and JSON-decoded artifact bodies for credential/session echoes, including escaped strings and callback IDs. Coding HTTP tests check provider inputs and all retained artifacts for MCP token/session exclusion. | Preserve existing native receipts and source attribution. The new packaged MCP compaction/reopen case uses stdio without a credential; it does not upgrade native HTTP evidence to a packaged authenticated HTTP claim. |
| MCP ordinary content, identity and provenance | The complete history of external observations must survive compaction without reviving a connection or historical authority. | Pass on the exact `0087829f` candidate: SQLite `e9bd65b6` and Files `931fe51c` compare full discovery/resource/prompt bytes and event/provenance links across compaction and fresh inspectors, reject stale cache/connection identity and a revoked resource, then assert filtered purge gaps and no replay. |
| Diagnostic bundle / bug-report export | The current CLI has `doctor`, safe stderr and inspectors, but no diagnostic-bundle or bug-report export command. | Unsupported product surface; no synthetic bundle implementation or passing bundle claim. |

The first five increment rows pass against the post-correction `0087829f` candidate in packaged receipt `sensitive-e8863ef3-5abf-45f4-9f92-d93be17ca64c`: 78 artifact payloads per backend, ten inspector views including both chain pages, actual ConPTY bytes, preserved ordinary text, authenticated provider headers and filtered child environment. The follow-up report records its exact digest, source inventory and times, preserving the earlier `ecf570da` pass against the original package separately. These are finite typed-secret checks, not arbitrary source-text secret detection.

The finite new sensitive-surface schedule consists of the two named tests, each under an outer ten-minute process-tree deadline. The packaged test requires the exact extracted package and checks its executable identity through the existing fixture. Each CLI step has a 90-second wait bound; native tool execution has a ten-second limit and ConPTY inspection has a 30-second bound plus five seconds to drain output. These inner bounds supplement the outer process-tree supervisor.

The separate packaged MCP history schedule exposed a startup-classifier defect:
deliberately stopped, canonically acknowledged partial process output fenced a
new owner. The scoped correction, positive and negative both-store regressions,
and preserved capture-failure/MCP owner-kill guards are recorded in the follow-up
report. A rebuilt candidate with CLI SHA-256
`0087829f97e4e13b3e36198449de5af47b1f346e6f54d419aa33f0bb83f552b3`
has a verified inventory and passing final sensitive-surface and independent
crypto receipts, plus complete packaged MCP history passes. Earlier passing
sensitive evidence retains the `4343607c` candidate identity.
The first MCP history reruns used separate Files/SQLite wrapper tests with
predeclared 1,200-second outer deadlines to accommodate measured fresh-process
inspection costs. The first split attempts failed their inner 90-second CLI
bound after passing the startup boundary; neither exhausted its outer deadline.
The second split attempts declared 180 seconds per MCP CLI before execution and removed
only temporary diagnostic Store/spool opens, retaining all semantic assertions.
This is a fixture scheduling change, not a production latency or P8-05 acceptance
threshold change. The changed harness build `a1c9c0e2` passes, and inventory
comparison shows only the MCP test file changed; the CLI remains the exact
`0087829f` candidate. SQLite `a1323d9e` and Files `8cc78b59` subsequently failed
their inner 180-second CLI bound at the second fresh owner; the first fresh
owner passed in 112/113 seconds. Neither exhausted its outer deadline. Retained
canonical events show continuing progress, with substantial startup/history
processing cost; those failed attempts do not qualify the complete MCP schedule.
The final correctness runs predeclared 600 seconds per MCP CLI and 2,400 seconds
per store-specific test, retaining every semantic assertion. These are fixture
execution bounds; debug startup/history latency remains a limitation and no
P8-05 latency threshold is accepted or changed.
The subsequent `cddea95e` test-only build passes with unchanged `0087829f` CLI
bytes; the follow-up report records its source and harness identities. SQLite
`e9bd65b6` and Files `931fe51c` both pass the complete selected schedule under
those bounds. Full result hashes, stable source identity and failed attempts
remain in the follow-up report. These correctness passes do not qualify the
observed debug startup/history latency for owner acceptance.
Earlier sensitive-surface passes retain their original
candidate identity and ten-minute deadlines.

Typed credentials are excluded before capture serialization; authentication headers are deliberately present only at the authorized transport. Capture metadata uses `authentication_headers` and `recovery_material`, with no invented retained offsets for those exclusions. Ordinary source/model/tool text remains subject to explicit capture scope and export policy: a token-looking string intentionally supplied as ordinary source is retained. This review does not promise heuristic discovery or deletion of arbitrary embedded secrets, remote copies, or operator-created logs.
