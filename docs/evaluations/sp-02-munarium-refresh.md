# SP-02 — Munarium 1.3.0 library refresh

Date: 2026-09-29. Scope: refresh the existing three local libraries; preserve
VCP's authorization, gate defaults, canonical storage and accounting behavior.

## Selected source

The source advances from `8da666067000ca1ee9c131bc67e70b978862faa3` (1.2.1) to
[`2c40480fdc2378e66dc23acdfaf82b529b9d22ee`](https://github.com/iokaio/munarium/tree/2c40480fdc2378e66dc23acdfaf82b529b9d22ee)
(1.3.0). The same include rules now select 80 files, up from 70. Only the three
library manifests are patched for shared-workspace membership and explicit
upstream package/dependency settings. Rust implementation and test bytes match
the selected revision.

The root Apache-2.0 license is unchanged; the current NOTICE is retained.
Every selected Rust source/test file retains its Apache-2.0 header. The
PostgreSQL/Snowball fixtures and their additional notices remain unchanged.
The three local package versions advance to 1.3.0 in the shared Cargo lockfile;
all external package versions and checksums remain unchanged.

## Improvements in the retained boundary

- Checked archive/vector readers reject impossible declared lengths before
  slicing or allocating. This directly improves VCP's DiskANN reader.
- Hydration and adjacency caches recover settled values after poisoned locks;
  the reference source store returns storage errors for poisoned source locks.
- Core/store/datastore enforce upstream's production panic policy.
- Reference-store clocks and IDs can be injected for deterministic fixtures.
- Optional retrieval diagnostics report candidate/work counts without changing
  the existing vector API or ranking. New sparse-generation tests compare
  retrieval against an independent oracle.
- Datastore supports no-default and arbitrary-precision JSON configurations and
  documents its supported embedded API. Its minimum compiler is 1.92 for the
  upstream qualified graph; VCP keeps its existing compiler pins.

`Claim`, `Candidate` and `MeshSnapshot` retain their prior source definitions.
`run_gates` keeps the legacy default through an additive policy wrapper. VCP
does not adopt governance-profile persistence or a second budget ledger in
this refresh. The new core/store-mem APIs remain internal upstream interfaces
consumed at the recorded revision.

## Validation

All checks below passed on native Windows. Cargo used the committed shared
lockfile with `--locked --offline`; imported-library checks used Rust 1.98.0
and VCP consumer checks used the ordinary Rust 1.95.0 compiler.

| Check | Result |
|---|---|
| Three imported libraries, default features plus datastore `vector-diskann` | 239 passed; two upstream manual tests ignored |
| Datastore `--no-default-features` | 70 passed, none ignored |
| Datastore default plus `json-arbitrary-precision` | 117 passed; one upstream exchange test ignored |
| `vcp-memory --lib` | 13 passed, including VCP gate conformance |
| `vcp-memory --test embedding` | Six passed, including corrupt graph headers, scoped reopen and Windows junction rejection; one model-assets test ignored |
| `cargo check --all-targets` for memory/storage spikes | Both consumers passed without adapter changes |
| `cargo fmt --check` for the three imported libraries | Passed |
| `cargo clippy --all-targets`, DiskANN enabled, `-D warnings` | Passed |
| Import, path and dependency contract tests | 16 passed |
| Source reconstruction and working-tree verification | Exact 80-file Munarium selection; 7,940-file Codex inventory passed |
| Staged Git-object verification | Both selected source inventories passed |
| Fresh dependency graphs and lock fingerprints | 147 Munarium, 214 corpus, 141 embedding packages matched their references |
| Fresh Munarium dependency license record | Exact match with the committed reference |
| Native protocol schema regeneration and generated-output contracts | Nine passed; only the shared-lockfile provenance hash changed, with public API types unchanged |
| Static boundary inventory | 175 workspace packages; no boundary errors |
| Repository documentation checks and diff whitespace checks | Passed |

The upstream ignored cases are cross-configuration artifact exchange and a
manual crossover measurement. The VCP ignored case requires separately
provisioned MiniLM assets; no embedding model or runtime was changed here.
The earlier P0-07 200-test reports remain historical results for the 1.2.1 import.

Linux-only filesystem permission tests require Linux and are not represented
as executed by native Windows tests. No model/provider calls, deployment or
release publication are part of this refresh.
