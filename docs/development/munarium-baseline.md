# Munarium native local-library baseline

P0-07 qualifies three libraries from
`iokaio/munarium@8da666067000ca1ee9c131bc67e70b978862faa3` without starting its
server, PostgreSQL or a model provider. This extends the earlier kernel/store
experiment to real Tantivy and DiskANN. It is not P0-02 embedding qualification
or VCP memory integration. See [candidate provenance](../../src/third_party/components/munarium.md),
[the owning plan](../plan/01-upstream-feasibility.md) and [ADR-008](../adr/008-local-governed-memory.md).

This procedure preserves the unmodified standalone baseline. For normal VCP
checkout builds, use the [committed shared-workspace selection](munarium-source.md).
Its resolved dependencies differ and have separate executed evidence.

## Reproduction

Use a new ignored checkout and [inventory its original bytes](upstream-candidates.md).
Native prerequisites are the [Windows compiler setup](codex-source.md#setup-and-ordinary-build),
plus the upstream server's Rust 1.98.0 toolchain. Node 24 is required for the
dependency check; that check uses only the Node standard library.

```powershell
git init artifacts/upstream/munarium-candidate
git -C artifacts/upstream/munarium-candidate fetch --depth 1 https://github.com/iokaio/munarium.git 8da666067000ca1ee9c131bc67e70b978862faa3
git -C artifacts/upstream/munarium-candidate checkout --detach 8da666067000ca1ee9c131bc67e70b978862faa3
rustup toolchain install 1.98.0 --profile minimal
pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -Candidate Munarium -SourceRoot artifacts/upstream/munarium-candidate -Commit 8da666067000ca1ee9c131bc67e70b978862faa3 -OutputRoot artifacts/upstream/munarium-baseline -Mode BoundaryTests
```

`-Mode Build` compiles the same selected libraries. Both modes use `--locked`,
the explicit `x86_64-pc-windows-msvc` target and default four jobs. Tests select
`munarium-core`, `munarium-store-mem` and `munarium-datastore`, with
`munarium-datastore/vector-diskann` enabled in addition to datastore defaults
(`lexical-tantivy`, `vector-flat`, `artifact-file`). Rust's toolchain file lives
under `server/`; the experiment reads that immutable channel.

After successful compilation/tests, the runner invokes `cargo tree --locked
--offline` for the same packages, feature and target with `--edges normal,build
--prefix none --format '{p}'`. It records raw stdout/stderr and runs
`scripts/upstream/dependency-closure.cjs` to produce normalized package/version
JSON, removing host paths and duplicate tree rows. A nonzero tree/check exit
fails the overall run. The manifest records both command identities and hashes.

The dependency policy requires both search engines and all three selected
libraries. It rejects SQLx, Axum, Tonic, Reqwest, Hyper and the identified Munarium
server/provider/PostgreSQL/coordinator packages, including relevant crate-family
variants. Unknown tree syntax fails closed. This checks the linked normal/build
graph for this target; it is not a network sandbox, a complete static effect audit,
a dev-dependency inventory or a guarantee about other features/platforms.

Source must match the supplied immutable commit and be clean. Evidence/cache
paths must be outside that checkout. Each attempt gets a fresh result directory;
missing prerequisites return `not_run`/3, while compiler, test or closure failures
return nonzero. `-TargetRoot` reuses an explicit task-owned cache. No source is
imported or modified by this command. Standard Cargo dependency downloads may
occur during compilation; the post-build closure check is offline.

## Source and effect boundaries

| Candidate input | Relevant behavior and VCP adaptation |
|---|---|
| `server/src/munarium-core/` | Claims/evidence/governance and storage/retrieval traits; inject VCP authority and canonical storage, preserving provenance and disputes |
| `server/src/munarium-store-mem/` | In-memory reference implementation; `budget.rs` reads the clock and creates UUIDs, and `lib.rs` generates IDs; isolate test clock/identity and use the one VCP ledger |
| `server/src/munarium-datastore/` | Real Tantivy 0.22.1 lexical index and DiskANN 0.56.0 adapter; local artifact files, hydration, rename/removal and cache lifetime require VCP workspace/snapshot/deletion policy |
| `server/contract/matrix/VERSION` | Compile-time input to `munarium-core/src/evidence.rs`; a directory-only extraction would miss it |
| `server/contract/datastore/` | Independent identity vectors, schema/examples and reference implementation used by datastore contract tests |
| Datastore lexical fixtures | Recorded PostgreSQL analyzer oracle and documented provenance; test consumption does not run PostgreSQL |

DiskANN's upstream adapter uses its real algorithm with a custom in-memory
provider, serializes vectors/adjacency/start point into an immutable artifact and
reconstructs that provider on open. That is API and artifact feasibility, not
proof of large-corpus RAM use, recall for real text embeddings or a production
query-latency envelope. One explicit performance benchmark stays ignored in the
normal suite; P0-02 must run its own declared workloads and exact-vector oracle.

The selected graph does not contain local embedding inference. Munarium's
`server/src/munarium-providers/src/ollama.rs` is an HTTP transport outside this
selection, and its URL validation accepts nonlocal hosts. Do not describe it as
an embedded inference runtime or silently accept remote embeddings. P0-02 still
needs pinned model assets, actual CPU inference, observed network restrictions,
missing/corrupt asset behavior and VCP workspace-scope tests under the
[memory/retrieval design](../architecture/memory-retrieval-design.md).

The [executed report](../evaluations/p0-07-munarium-datastore.md) records the tested
envelope and limitations. It does not complete P0-07's remaining Codex/Gemini
effect/selection work or P0-02's integrated local path.
