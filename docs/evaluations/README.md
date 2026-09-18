# Reviewed evaluation summaries

These reports describe executed checks with explicit scope and limitations.
Raw evidence stays in ignored local artifact directories or CI artifacts.

- [P0-01 experiment harness](p0-01-experiment-harness.md): deterministic helpers,
  public synthetic workloads and requirement coverage; no product/model score.
- [P0-07 native candidates](p0-07-native-candidates.md): unmodified Codex Windows
  build and selected Codex, Gemini and Munarium tests; import gate remains open.
- [P0-07 Codex source import](p0-07-codex-import.md): copied source, independent
  reconstruction, provenance checks and native build evidence.
- [P0-07 Munarium source import](p0-07-munarium-import.md): shared Cargo graph, 70-file reconstruction, dependency provenance and 200 native tests.
- [P0-07 Gemini boundaries](p0-07-gemini-baseline.md): explicit metadata/compiler preparation and 402 native policy, scheduler, tool, skill and MCP tests.
- [P0-07 local embeddings](p0-07-local-embeddings.md): pinned CPU runtime/assets, independent numerical reference and explicit remaining memory/index gates.
- [P0-07 Munarium datastore](p0-07-munarium-datastore.md): 200 native tests with
  Tantivy/DiskANN enabled and the selected library dependency boundary.
- [P0-07 boundary inventory](p0-07-boundary-inventory.md): static coverage of 154 Codex packages and 25 source anchors; runtime enforcement remains unqualified.
- [P0-07 native CLI trace](p0-07-cli-trace.md): five request/tool/completion cases through the Windows binary, including retry and rejection.
- [P0-07 helper traces](p0-07-helper-traces.md): nine native cases, including review usage discrepancies and compaction success/rejection with independent request observation.
- [P0-07 selection gate](p0-07-selection-gate.md): consolidated acceptance evidence, schema 2 effect classifications and explicit subsequent-task boundaries.
- [P0-07 compiler compatibility](p0-07-common-rust.md): explicit compiler experiments, a recorded recursion-limit patch and reconstruction evidence.
- [P0-07 hosted Windows](p0-07-hosted-windows.md): clean native build, 102 tests, five CLI traces and exact reconstruction on the second Windows environment.
