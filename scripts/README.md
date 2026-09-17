# Build and test scripts

This directory is reserved for repository automation. No build, test, or packaging runner is implemented yet.

| Planned entry point | Responsibility | Work owner |
|---|---|---|
| `build.ps1` | Build the selected native Windows workspace and report prerequisites | P0-07/P0-08 |
| `test.ps1` | Dispatch suites, cases, and backends; preserve exit status and run evidence | P0-01, extended by feature owners |
| `package.ps1` | Assemble qualified artifacts, licenses, notices, and checksums | P8-04 |
| `evals/` | Orchestrate explicitly configured evaluations and collect results | P5-08/P8-05 |

The [delivery contract](../docs/plan/00-delivery-contract.md) specifies the proposed test command interface. The [layout](../docs/plan/code-layout.md) separates automation from reusable test code and graders under `src/`.

Resolve paths from the script's location, document real tool prerequisites, reject unknown inputs, and return failures to callers. Missing tools, model assets, or environments must be reported as not run. Add working entry points with their owning implementation; do not add no-op success scripts to satisfy the tree.
