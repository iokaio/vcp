# Build and test scripts

`test-context.ps1` runs 27 native repository/context and foundational type contracts
with bounded Git observations and real artifact stores. See [P2 context](../docs/development/p2-context.md).
`test-provider.ps1` runs 21 provider/context contracts and the retained absolute
deadline regression; [the provider guide](../docs/development/p2-provider.md)
distinguishes scripted qualification from live compatibility.
`test-openrouter-live.ps1` is the explicit paid P2-02 smoke gate for Luna and
Claude Sonnet. It uses only a public synthetic marker, performs zero retries,
requires `OPENROUTER_API_KEY`, and enforces an aggregate maximum cap of `$10`.
It is never called by routine CI or deterministic test commands. Jev retains its
separate later decision-adapter qualification in P6 and is not a regular runner.
`test-policy.ps1` runs 47 authority/foundation contracts with native crash
regressions enabled; see [authority and approvals](../docs/development/p2-policy.md).
`test-tools.ps1` runs 31 prepared-tool/repository/verification/authority contracts and retained
patch unit regressions; [native file tools](../docs/development/p2-tools.md) also
require canonical host integration and recovery/lifecycle qualification.

`test-integration.ps1` runs native host/port contracts, retained process regressions and the private coding CLI on Rust 1.98.0 (1.95.0 is an explicit comparison option). See [the integration guide](../docs/development/p0-integration.md). `upstream/compare-gemini-ports.cjs` and `upstream/rehearse-codex-fix.cjs` run explicit pinned-source maintenance experiments; neither is an ordinary build dependency.

`test-storage.ps1` runs the local P0 SQLite/files, encrypted handoff and native
crash qualification. Supply the pinned independent age binary and optionally the
public handoff fixture; see [the guide](../docs/development/portable-storage-spike.md).

This directory owns repository automation. `test.ps1` and `test-runner.cjs` implement delivery checks; `build.ps1` builds the imported Codex baseline and selected Munarium libraries. See [test setup](../docs/development/delivery-harness.md) and [native build/reconstruction](../docs/development/codex-source.md). Full P8 distribution packaging remains planned; built-in skill asset packaging is implemented for qualification.

| Entry point (planned unless noted) | Responsibility | Work owner |
|---|---|---|
| `build.ps1` (implemented baseline) | Verify and build the selected native Windows Codex workspace, or select Munarium libraries with `-Component Munarium`; `-Mode BoundaryTests` runs component tests; `-Mode LifecycleTests` runs retained admission and host tests; `-Mode RecoveryTests` runs [durable owner and native execution qualification](../docs/development/lifecycle-recovery.md) | P0-03/P0-05/P0-07/P0-08 |
| `test.ps1` (implemented) | Run deterministic repository, harness, experiment and upstream-inventory checks; preserve exit status and evidence | P0-01, extended by feature owners |
| `test-embeddings.ps1` (implemented qualification) | Verify explicit local assets, test/build the CPU helper, check its dependency graph and compare real model results | P0-07; integration continues in P0-02 |
| `test-local-memory.ps1` (implemented prototype) | Build the corpus qualification executable, verify its dependency closure and observe real local indexes across fresh processes; add `-Scale` for the declared resource gate | P0-02 |
| `package.ps1` | Assemble qualified artifacts, licenses, notices, and checksums | P8-04 |
| `package-skills.ps1` (implemented qualification) | Stage an explicit executable with hash-bound built-in skills and notices, then verify every archive entry; see [built-in assets](../docs/development/p7-builtin-skills.md) | P7-02 |
| `evals/builtin-skill-qualification.ps1` (implemented qualification) | Exercise the frozen project catalog and lazy activation contracts without model or ecosystem toolchain calls | P7-02 |
| `evals/duplex-qualification.ps1` (implemented qualification) | Exercise bounded native duplex IO and canonical process authority/recovery with stable source and log identities; see [duplex prerequisite](../docs/development/p7-mcp-duplex.md) | P7-03 |
| `evals/mcp-http-qualification.ps1` (implemented qualification) | Exercise remote MCP sessions, native trust, transport fences and canonical calls alongside stdio, CLI and process regressions; see [HTTP adapter](../docs/development/p7-mcp-http.md) | P7-03 |
| `evals/mcp-content-qualification.ps1` (implemented qualification) | Exercise resources, prompts and scoped cache on both transports alongside existing MCP and process regressions; see [content contract](../docs/development/p7-mcp-content.md) | P7-03 |
| `evals/persisted-json-qualification.ps1` (implemented qualification) | Qualify literal-preserving record/event JSON decoding across native storage replay and history consumers; see [decode contract](../docs/development/p1-persisted-json.md) | P1-04 |
| `evals/` | Orchestrate explicitly configured evaluations and collect results | P5-08/P8-05 |
| `upstream/` (implemented baseline tooling) | `inventory.cjs` records immutable Git bytes; `reconstruct.cjs` reconstructs/verifies selected source; `build-baseline.ps1` runs native builds/tests | P0-07/P0-08, rehearsed in P8-06 |

The [delivery contract](../docs/plan/00-delivery-contract.md) specifies the test command interface and future product suites. The [layout](../docs/plan/code-layout.md) separates automation from reusable test code and graders under `src/`.

Under [ADR-013](../docs/adr/013-upstream-reuse-and-vendoring.md), normal builds consume committed Codex source. They do not fetch Codex, advance its pin or apply patches. Reconstruction runs separately in a disposable directory as explicit maintenance/verification work.

[Candidate commands](../docs/development/upstream-candidates.md) describe the implemented acquisition/inventory and native baseline workflow. Those experiments do not import source or establish a VCP application build.

`build.ps1 -Component Munarium -Mode BoundaryTests` tests the committed libraries
in the shared workspace. See [the source guide](../docs/development/munarium-source.md).

`node scripts/upstream/test-gemini.cjs --source <pinned checkout> --output-root <evidence directory> --prepare`
qualifies 402 selected Gemini boundary tests on native Windows, with explicit
metadata/compiler preparation and isolated child profiles. See [the procedure](../docs/development/gemini-baseline.md).

`node scripts/upstream/model-assets.cjs acquire --root <new external model directory>`
explicitly acquires the pinned model and verifies every file. `verify` checks an
existing selection. `test-embeddings.ps1 -AssetsRoot <model directory>` never
downloads models. Add `-Offline` for [observed Windows network isolation and asset fault injection](../docs/development/offline-embeddings.md). See [local embedding setup and qualification](../docs/development/local-embeddings.md).

`build-baseline.ps1 -Candidate Munarium` selects the pinned kernel/store/datastore
experiment with DiskANN enabled; its dependency checker rejects the identified
server/provider/PostgreSQL packages. See [the native library procedure](../docs/development/munarium-baseline.md).

`node scripts/upstream/trace-cli.cjs --binary <built codex.exe>` from the repository root runs nine native CLI
cases against a synthetic loopback provider, with independent request and file
observations. See [the trace procedure](../docs/development/native-cli-trace.md)
for the explicit unsandboxed patch fixture and qualification limits.

`test-local-memory.ps1 -AssetsRoot <model directory>` runs the
[local corpus prototype](../docs/development/local-memory-spike.md), with real
Tantivy/DiskANN artifacts, CPU vectors, canonical fixture filtering and repeated
fresh-process reopen. It does not download models or qualify OS network isolation.

Resolve paths from the script's location, document real tool prerequisites, reject unknown inputs, and return failures to callers. Missing tools, model assets, or environments must be reported as not run. Add working entry points with their owning implementation; do not add no-op success scripts to satisfy the tree.

`test-foundation.ps1` qualifies the [P1 durable foundation](../docs/development/p1-foundation.md)
with native typed-state, command, capture, backend, process-crash and controlled
activation contracts. It records stable source inputs and actual native test
results. `test-accounting-history.ps1` runs the six-package, 39-contract
[accounting/history qualification](../docs/development/p1-accounting-history.md),
including the foundation regressions and real accounting/projection process kills.
`test-p1.ps1` runs all 70 foundation, accounting/history and retained host contracts;
see [the canonical host guide](../docs/development/p1-retained-host.md).
