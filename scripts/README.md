# Build and test scripts

This directory owns repository automation. `test.ps1` and `test-runner.cjs` implement delivery checks; `build.ps1` builds the imported Codex baseline. See [test setup](../docs/development/delivery-harness.md) and [native build/reconstruction](../docs/development/codex-source.md). Packaging remains planned.

| Entry point (planned unless noted) | Responsibility | Work owner |
|---|---|---|
| `build.ps1` (implemented baseline) | Verify and build the selected native Windows Codex workspace, or run its patch/policy tests | P0-07/P0-08 |
| `test.ps1` (implemented) | Run deterministic repository, harness, experiment and upstream-inventory checks; preserve exit status and evidence | P0-01, extended by feature owners |
| `package.ps1` | Assemble qualified artifacts, licenses, notices, and checksums | P8-04 |
| `evals/` | Orchestrate explicitly configured evaluations and collect results | P5-08/P8-05 |
| `upstream/` (implemented baseline tooling) | `inventory.cjs` records immutable Git bytes; `reconstruct.cjs` reconstructs/verifies selected source; `build-baseline.ps1` runs native builds/tests | P0-07/P0-08, rehearsed in P8-06 |

The [delivery contract](../docs/plan/00-delivery-contract.md) specifies the test command interface and future product suites. The [layout](../docs/plan/code-layout.md) separates automation from reusable test code and graders under `src/`.

Under [ADR-013](../docs/adr/013-upstream-reuse-and-vendoring.md), normal builds consume committed Codex source. They do not fetch Codex, advance its pin or apply patches. Reconstruction runs separately in a disposable directory as explicit maintenance/verification work.

[Candidate commands](../docs/development/upstream-candidates.md) describe the implemented acquisition/inventory and native baseline workflow. Those experiments do not import source or establish a VCP application build.

Resolve paths from the script's location, document real tool prerequisites, reject unknown inputs, and return failures to callers. Missing tools, model assets, or environments must be reported as not run. Add working entry points with their owning implementation; do not add no-op success scripts to satisfy the tree.
