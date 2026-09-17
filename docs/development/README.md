# Development design and implementation guides

These guides describe how to implement and qualify the planned system. The [delivery harness](delivery-harness.md) and [committed Codex baseline build](codex-source.md) are executable. VCP product-runtime qualification remains outstanding.

- [Implementation workflow](implementation-workflow.md): turn a ledger task into a concrete source change, contract checks and reviewable evidence.
- [Experiment fixtures](experiment-fixtures.md): deterministic P0 helpers, synthetic workloads, independent graders and proposed measurement sizes.
- [Upstream qualification](upstream-qualification.md): immutable source selection, native baseline experiments, effect inventory and reconstruction.
- [Current upstream candidates](upstream-candidates.md): pinned investigation inputs, original-byte inventory and native baseline commands.
- [Committed Codex source](codex-source.md): source selection, notices, reconstruction, native setup and ordinary build.
- [Codex package and effect boundaries](codex-boundaries.md): complete workspace package ownership and concrete source anchors for future adapters.
- [Native CLI trace](native-cli-trace.md): scripted loopback requests, synthetic patch observation, retry and rejection through the built Codex CLI.
- [Munarium native baseline](munarium-baseline.md): kernel/store/datastore qualification with real Tantivy and DiskANN and a checked dependency boundary.
- [Qualification and release design](../architecture/qualification-release-design.md): test runner/result contracts, independent fault oracles and packaged acceptance.
- [Code layout](../plan/code-layout.md): responsibility boundaries and proposed paths.
- [ADR index](../adr/README.md): confirmed directions and unresolved engineering gates.

The Codex guide records the first concrete source map and native setup. Other P0
components and product interfaces remain qualification work.
