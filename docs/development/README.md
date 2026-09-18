# Development design and implementation guides

- [Portable storage qualification](portable-storage-spike.md): native backend parity, encrypted snapshots, independent cryptographic interoperability and cross-machine handoff.

These guides describe how to implement and qualify the planned system. The [delivery harness](delivery-harness.md) and [committed Codex baseline build](codex-source.md) are executable. VCP product-runtime qualification remains outstanding.

- [Implementation workflow](implementation-workflow.md): turn a ledger task into a concrete source change, contract checks and reviewable evidence.
- [Experiment fixtures](experiment-fixtures.md): deterministic P0 helpers, synthetic workloads, independent graders and proposed measurement sizes.
- [Upstream qualification](upstream-qualification.md): immutable source selection, native baseline experiments, effect inventory and reconstruction.
- [Current upstream candidates](upstream-candidates.md): pinned investigation inputs, original-byte inventory and native baseline commands.
- [Committed Codex source](codex-source.md): source selection, notices, reconstruction, native setup and ordinary build.
- [Codex package and effect boundaries](codex-boundaries.md): complete workspace package ownership and concrete source anchors for future adapters.
- [Classified upstream effects](upstream-effect-classes.md): checked module ceilings, named effects, replacement owners and release-update/maintenance paths.
- [Native CLI trace](native-cli-trace.md): nine scripted coding, tool, review and compaction traces through the built Codex CLI.
- [Helper effect map](helper-effect-traces.md): review/compaction request ownership, usage discrepancies and pause/history adapter responsibilities.
- [Continuation admission](continuation-admission.md): host gating of child/review/mailbox starts, native tests and remaining pause obligations.
- [Scoped lifecycle host](scoped-lifecycle.md): registered thread authority, tree holds, owned retained interruption, owner loss and explicit readmission.
- [Lifecycle recovery](lifecycle-recovery.md): durable private checkpoints, startup/dispatch authority, same-owner controls and native Windows qualification.
- [Committed Munarium libraries](munarium-source.md): three libraries in the shared Cargo workspace, source reconstruction and native dependency evidence.
- [Munarium native baseline](munarium-baseline.md): kernel/store/datastore qualification with real Tantivy and DiskANN and a checked dependency boundary.
- [Gemini native baseline](gemini-baseline.md): reproducible 402-test comparison, explicit preparation and policy/skill/MCP adaptation boundaries.
- [Local CPU embeddings](local-embeddings.md): verified model assets, bounded file-only inference, independent vectors and native qualification.
- [Offline embedding qualification](offline-embeddings.md): Windows AppContainer token checks, independently observed local canaries and real asset fault injection.
- [Local corpus prototype](local-memory-spike.md): real Tantivy/DiskANN construction, scoped current-version results and fresh-process reopen with CPU vectors.
- [Local resource qualification](local-memory-resources.md): three declared corpus sizes, native resident/committed/mapped measurements, sampled disk growth and repeated query checks.
- [Local governance adapter](local-governance-spike.md): scoped Munarium gates, disputed proposal evidence and pin-aware supersession in the corpus experiment.
- [Qualification and release design](../architecture/qualification-release-design.md): test runner/result contracts, independent fault oracles and packaged acceptance.
- [Code layout](../plan/code-layout.md): responsibility boundaries and proposed paths.
- [ADR index](../adr/README.md): confirmed directions and unresolved engineering gates.

The Codex guide records the first concrete source map and native setup. Other P0
components and product interfaces remain qualification work.
