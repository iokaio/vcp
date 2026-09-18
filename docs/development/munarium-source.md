# Committed Munarium libraries in the shared workspace

P0-07 imports 70 files from `iokaio/munarium` revision
`8da666067000ca1ee9c131bc67e70b978862faa3` under `src/third_party/munarium/`.
The selection contains `munarium-core`, `munarium-store-mem`,
`munarium-datastore`, their retained tests/fixtures, datastore contract inputs,
the matrix contract VERSION, baseline Cargo/toolchain files and original
LICENSE/NOTICE. Server, PostgreSQL, provider and client packages are excluded.

[The selection](../../src/third_party/components/munarium-selection.json),
[file inventory](../../src/third_party/components/munarium-files.json) and
[ordered patch](../../src/third_party/patches/munarium/README.md) bind original
and resulting bytes. All Rust implementation and test bodies remain unchanged.
The patch changes the three library manifests to point at the existing
`src/third_party/codex/codex-rs/Cargo.toml` workspace, with explicit original
Munarium version/edition/license and dependency requirements/features. This
prevents accidental inheritance of Codex's package identity or feature choices.

## One workspace and component qualification commands

The Codex workspace and lockfile are authoritative for both selections. The
retained Munarium `server/Cargo.toml` and `Cargo.lock` are upstream provenance
inputs; do not run them as VCP build entry points. Cargo and the static boundary
checker now discover 158 workspace packages: the original 154, three Munarium
libraries and the separately owned [CPU embedding helper](local-embeddings.md).
The checker accepts outside-workspace paths only beneath explicitly selected
component roots and rejects lexical and link escapes.

Install the [native prerequisites](codex-source.md#setup-and-ordinary-build),
Node 24 and both upstream-pinned compilers, then run from the repository root:

```powershell
rustup toolchain install 1.95.0 --profile minimal
rustup toolchain install 1.98.0 --profile minimal
npm ci --prefix src/tests --ignore-scripts --no-audit --no-fund
pwsh -NoProfile -File scripts/build.ps1
pwsh -NoProfile -File scripts/build.ps1 -Mode BoundaryTests
pwsh -NoProfile -File scripts/build.ps1 -Component Munarium -Mode BoundaryTests -TargetRoot artifacts/munarium-target
pwsh -NoProfile -File scripts/test.ps1 -Suite fast
```

`-Component` accepts `Codex` (default) or `Munarium`; `-Mode` accepts `Build` or
`BoundaryTests`. The selected libraries use Rust 1.98.0 and enable
`munarium-datastore/vector-diskann`; Codex keeps its 1.95.0 ordinary build pin.
Both commands execute in the same Cargo workspace, with locked dependency
resolution and native `x86_64-pc-windows-msvc`. This deliberately preserves the
component qualification compilers; a single supported VCP compiler is not yet
selected. An explicit compiler experiment can still test a shared compiler.

Both source inventories are verified before a selected build. The native result
records the selected component's original pin/toolchain, its source inventory,
the shared-workspace inventory, actual compiler/native tools, command, exit and
log digest. Missing prerequisites return `not_run` / 3. Munarium checks also
capture the actual normal/build dependency graph and reject the identified
server/provider/PostgreSQL packages. Required Tantivy, DiskANN and vector
libraries cannot disappear from that graph without failing the check.
The selected run also compares all package versions and the shared lockfile hash
with the committed dependency record. Graph or lock drift fails qualification.
Both component source trees reject evidence/target output before allocation.

## Dependency and effect boundaries

The Munarium import preserved all 1,491 original Codex package identities and
checksums, adding 34 entries (three selected libraries plus 31 external entries).
The subsequent CPU helper adds 40 net entries and requires `regex-automata`
0.4.13 to 0.4.14; all other preexisting identities remain. Both library graphs
and their lockfile-bound references are requalified for that change.
Compatible shared requirements resolve to existing Codex versions. This changes
some Munarium dependency versions relative to its standalone baseline, so the
original 200 passing tests are rerun against the shared graph. Preserve both
baselines when comparing future failures. The [source import report](../evaluations/p0-07-munarium-import.md)
records the actual outcomes and closure identity.

After provisioning the selected packages and obtaining a native run's
`dependencies.json`, reproduce the license-declaration inventory into a new file:

```powershell
node scripts/upstream/record-munarium-dependencies.cjs artifacts/build/<run-id>/dependencies.json artifacts/munarium-dependencies-reviewed.json
```

Compare the complete result with
`src/third_party/components/munarium-dependencies.json`. The generator reads the
shared lockfile and selected package manifests in the Cargo registry cache;
it never downloads packages. Missing sources, ambiguous identities, absent
license declarations and existing output files fail. New dependencies require
review before replacing the committed reference; this is not a release notice
generator and does not infer asset rights from software declarations.

| Source surface | Observed responsibility and required VCP boundary |
|---|---|
| `munarium-core/src/gates.rs` | Read-only snapshot/candidate evaluation; map gate findings to canonical VCP evidence and revisions |
| Core backend traits, evidence and context modules | Domain contracts and injected storage/retrieval; preserve provenance and enforce workspace scope in VCP adapters |
| `munarium-store-mem/src/lib.rs` | Mutable reference state, clocks and generated IDs; a test/reference backend, not VCP durability or a second accounting ledger |
| Datastore `store.rs`, `hydrate.rs`, `verify.rs` | Local file reads/writes and content verification; supply authorized roots, canonical generations and durable publication through VCP |
| Datastore `lexical.rs` | Real Tantivy construction/open with temporary files and mappings; account for local resources and coherent generations |
| Datastore `vector_diskann.rs` | Real DiskANN graph build/query/serialization; retain exact-vector comparison and qualify memory/recall with actual embeddings |

No local embedding runtime is included in the Munarium selection; the separate
[VCP helper](local-embeddings.md) now provides file-only CPU qualification.
Munarium's Ollama HTTP transport remains
outside this selection; it does not establish CPU-local inference or a network
restriction. P0-02 still owns that experiment, corpus scopes and missing/corrupt
asset behavior under [ADR-008](../adr/008-local-governed-memory.md). A passing
dependency-name check does not prove absence of all network effects or implement
canonical VCP authority.

## Independent reconstruction and CI

After explicit [candidate acquisition](munarium-baseline.md#reproduction), use
new output/record paths:

```powershell
node scripts/upstream/reconstruct.cjs reconstruct --component munarium --source artifacts/upstream/munarium-candidate --output artifacts/reconstructed/munarium --record artifacts/munarium-reconstructed.json
node scripts/upstream/reconstruct.cjs verify --component munarium
node scripts/upstream/reconstruct.cjs verify-index --component munarium
```

Compare the complete reconstructed JSON with the committed file inventory. For
a reconstructed shared graph, reconstruct Codex under the sibling
`artifacts/reconstructed/codex` directory; preserve the same relative layout.
Neither ordinary build fetches upstream source or reapplies either patch series.

The delivery workflow checks both imports on `ubuntu-8core` and `win8core`.
Windows also runs the committed library tests and dependency check, then
independently fetches/reconstructs both pinned selections. Artifacts retain the
result inventories and logs, excluding source trees, binaries and caches.
Imported Markdown keeps upstream-relative links; byte inventories verify it,
while VCP's documentation checker continues to validate VCP-owned links.
