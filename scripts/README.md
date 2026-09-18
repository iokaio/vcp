# Build and test scripts

This directory owns repository automation. `test.ps1` and `test-runner.cjs` implement delivery checks; `build.ps1` builds the imported Codex baseline and selected Munarium libraries. See [test setup](../docs/development/delivery-harness.md) and [native build/reconstruction](../docs/development/codex-source.md). Packaging remains planned.

| Entry point (planned unless noted) | Responsibility | Work owner |
|---|---|---|
| `build.ps1` (implemented baseline) | Verify and build the selected native Windows Codex workspace, or select Munarium libraries with `-Component Munarium`; `-Mode BoundaryTests` runs component tests; `-Mode LifecycleTests` runs ten [continuation/drain tests](../docs/development/continuation-admission.md) | P0-03/P0-07/P0-08 |
| `test.ps1` (implemented) | Run deterministic repository, harness, experiment and upstream-inventory checks; preserve exit status and evidence | P0-01, extended by feature owners |
| `test-embeddings.ps1` (implemented qualification) | Verify explicit local assets, test/build the CPU helper, check its dependency graph and compare real model results | P0-07; integration continues in P0-02 |
| `test-local-memory.ps1` (implemented prototype) | Build the corpus qualification executable, verify its dependency closure and observe real local indexes across fresh processes; add `-Scale` for the declared resource gate | P0-02 |
| `package.ps1` | Assemble qualified artifacts, licenses, notices, and checksums | P8-04 |
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
