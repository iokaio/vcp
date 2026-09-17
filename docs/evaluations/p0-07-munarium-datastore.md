# P0-07 Munarium datastore baseline

Run date: 2026-09-17. Candidate: `iokaio/munarium` commit
`8da666067000ca1ee9c131bc67e70b978862faa3`. No runtime source is imported by this
step. [Reproduction and source boundaries](../development/munarium-baseline.md)
describe the native command; [component provenance](../../src/third_party/components/munarium.md)
records required source/contract inputs and remaining workspace adaptation.

## Native results

`scripts/upstream/build-baseline.ps1 -Candidate Munarium -Mode BoundaryTests`
ran the selected packages with the pinned server toolchain, native Windows
target, locked dependencies and the real DiskANN feature enabled. Total:
**200 passed, 0 failed, 1 intentionally ignored performance benchmark**.

| Target | Passed | Scope |
|---|---:|---|
| `munarium-core` | 68 | Kernel domain/governance and evidence contracts |
| `munarium-datastore` unit tests | 92 | Artifact and retrieval components |
| `contract_vectors` | 8 | Independent canonical identity vectors |
| `diskann_contract` | 4 | Real 0.56.0 API/provider surface and search smoke |
| `lexical_parity` | 4 | Recorded analyzer fixture, determinism and accepted-difference policy |
| `round_trip` | 13 | Build/seal/open/query, required components, corrupt inputs and feature support |
| `munarium-store-mem` | 11 | In-memory reference backend |
| `vector_crossover` | 0 | One ignored benchmark; no scale/performance result claimed |

No doctests failed. Lexical parity deliberately measures some accepted upstream
differences rather than claiming exact PostgreSQL equivalence for all inputs.
The server and PostgreSQL were not started. These are upstream fixtures; they
do not establish VCP workspace isolation, durable pause/resume or production recall.

Host: native Windows 10.0.26200, 12 logical processors, 68,622,794,752 RAM bytes;
Rust/Cargo 1.98.0, MSVC 14.50.35717, CMake 4.2.3-msvc3, Ninja 1.12.1, Node 24.10.0.
The existing task-owned Munarium Cargo cache was reused. No cold-cache timing,
model inference, model request or network-sandbox qualification is claimed.

Initial successful native run:
`artifacts/upstream/munarium-datastore-qualified/76f1ff1d-5149-412f-ae9b-6ee76bbb5384/manifest.json`.
Log SHA-256: `043eb56360b8ad0352e46e5f0f519f817391a90981470ae44160393aa96517cc`.
An earlier runner attempt looked for the toolchain at the repository root and
failed before compilation; the corrected command reads `server/rust-toolchain.toml`.
Upstream tests and source were not altered to make the run pass.

## Dependency boundary

The full command with the dependency check passed in run
`21905b9f-49d9-4499-b912-299a2783bfb6`, under the same evidence root. It captured
the native normal/build graph offline after tests, including all three libraries,
Tantivy 0.22.1 and DiskANN/diskann-vector 0.56.0. The normalized record contains
148 unique package/version pairs, excluding the prohibited SQLx/Axum/Tonic/
Reqwest/Hyper and selected Munarium server/provider/PostgreSQL packages.

- Test log SHA-256: `163db55179c4c907ef05e41875ecee503cbeb27edff01ed19d9d91bbacf3ea39`.
- Normalized closure SHA-256: `aa5ff73f5752097b4b15af71289d6eab116bb654888e3cbb379909a0efdd4fc5`.
- Raw tree log SHA-256: `b5700d0c0f00bb2dafef147e60e9e62e8747eb3a6b1d9e2d7c55025393a830e0`.

The checker rejects disabled search engines, forbidden dependencies and unknown
tree syntax. Regression cases also verify duplicate handling and removal of host
paths from normalized records. This is a dependency boundary, not proof that
arbitrary library code cannot open a socket or write outside a supplied root.
Raw host paths remain only in ignored evidence.

The final native rerun `ba6dd451-abb3-4ba8-a0f6-c1f8d53c6051` also passed 200 tests
and the same 148-package closure. Its manifest additionally records SHA-256
identities for the runner, dependency-check CLI and reusable policy module.
The full deterministic VCP run `459631ad-54b0-429b-9137-15b5e6f296d4` passed all
five cases and 30 regression tests; evidence is under `artifacts/tests/`.

The generalized runner also passed Codex's 102 patch/policy tests through
`scripts/build.ps1 -Mode BoundaryTests`. Evidence:
`artifacts/build-munarium-step-codex-regression/f04bc066-beed-434f-b6dc-db728b7ebb13/manifest.json`;
log SHA-256 `1654f51cf7af4010fa2b69b290db6cb6ec76d2676a97abe512747fa565c4cc96`.

## Remaining gates

P0-07 remains in progress for full source/effect selection. The three-library
candidate still needs workspace/manifest adaptation and a common compiler
qualification with the Codex baseline. No separate Rust engine is selected.
P0-02 must add real pinned local embeddings, network observation, missing/corrupt
asset checks, declared workload measurements and VCP scope/reopen tests. A
serialized in-memory DiskANN provider and upstream synthetic vectors alone do
not satisfy those gates. The ignored crossover benchmark is not acceptance evidence.
