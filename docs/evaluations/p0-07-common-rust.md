# P0-07 compiler compatibility experiment

Date: 2026-09-17. Codex's upstream pin is Rust 1.95.0; the selected Munarium
candidate pins 1.98.0. The explicit `-ExperimentToolchain` option permits a
recorded comparison without silently changing either source pin. Normal
`scripts/build.ps1` builds continue to select 1.95.0.

## Failure and compatibility change

Unmodified Codex revision `3d3ae4965ab370217e871b3a7f0d15589557ee4b` failed on
native Windows with Rust 1.98.0, exit 101, after 13m46s. `codex-chatgpt` exceeded
the compiler query-depth limit while computing the future layout of
`connectors::list_connectors()`. The compiler suggested a crate recursion limit
of 256. The full failed manifest/log are retained at
`artifacts/upstream/codex-rust-198/f7d7d487-e2fe-474f-a6eb-d3cb0bf46d39/`;
log SHA-256 is `63f1fbc0e53ee97d1d34412e9694103dc4dfccce5a033d46047350a8365fc0e8`.

The [recorded patch](../../src/third_party/patches/codex/README.md) adds
`#![recursion_limit = "256"]` and a modification notice to
`codex-rs/chatgpt/src/lib.rs`. It does not change provider/policy source logic,
enable a VCP entry point or alter either upstream toolchain file. Original
copyright and license terms remain intact.

The patched 1.98.0 CLI build passed, exit 0, in 4m29s using the previous failed
build's dependency cache. This is not a cold-build performance comparison.
Manifest: `artifacts/upstream/codex-rust-198-patched/93c142a8-b350-4dbd-b577-28509f1c1647/manifest.json`;
log SHA-256: `cc438beaa1d8bcff0e04bd2b52dedd1eb5040db2a304f59d55209b819dd82e62`.
Cargo emitted a future-incompatibility warning for `proc-macro-error2` 2.0.1;
the warning remains recorded and that dependency was not upgraded.

The ordinary `scripts/build.ps1` path also built the patched CLI successfully
with its unchanged 1.95.0 pin, exit 0. Manifest:
`artifacts/upstream/codex-rust-195-patched/ada3a838-da4a-4d85-a612-9ac0585781eb/manifest.json`;
log SHA-256: `88e67a6e5e0c4b7838777ae9ee6cc72de237a424e0314df10de330f37a379434`.

## Reconstruction and regression evidence

The successful independent reconstruction is
`artifacts/upstream/codex-reconstruction-rust198-02/`, with its adjacent JSON
record. It matches all 7,937 working source files and preserves all 34 executable
Git modes. Its aggregate file-record SHA-256 is
`f2f87e73619411696d7c62f51505b88f1d1f071353806b247a6beb1d3cc28a62`.
Only the modified crate-root file is marked `patch-series`; the untouched license
materialization retains its original transformation label.

The first patched reconstruction failed with `spawnSync git EOF` while the
disposable Git index lacked Windows long-path configuration. The command-scoped `core.longpaths=true` fix
allowed reconstruction to pass; no global/user setting was changed. The failed
directory remains available for inspection. A new long-path fixture and checks
that unchanged files retain their provenance labels cover these boundaries.
The initial synthetic patch test also failed due to insufficient patch context;
its fixture was corrected to use pinned input and sufficient context.

All seven `fast` cases and 39 regression tests passed, exit 0:
`artifacts/tests/1e0f05a4-ed3d-4a9a-b7e2-6d108699fdc4/manifest.json`.
This includes compiler-alias rejection before output allocation, source integrity,
patch reconstruction, ordinary-verification rejection of tampered patch bytes,
ownership coverage and documentation/task consistency.
The default runner was separately observed selecting 1.95.0 with no override;
all 102 patch/policy tests passed in
`artifacts/upstream/codex-default-compiler-regression/bbb4b3c9-121d-461d-b1c1-c3f5cf1a2077/`
before the crate attribute was added.

The patched 1.98.0 binary passed all five [native CLI traces](../development/native-cli-trace.md),
observing eight loopback requests, one synthetic patch, read-only rejection,
transient retry and provider denial. Manifest:
`artifacts/cli-trace/4d73f8b0-7042-44b2-bb3b-77f9a21f73ad/manifest.json`.
The tested 1.98.0 binary's SHA-256 is
`6d0e2b6e283c7db1697b6dcdc8e143fde8b674db763034ec057e60548afc13a4`.

The patched 1.98.0 selection also passed all 102 patch/policy tests, exit 0:
`artifacts/upstream/codex-rust-198-patched-tests/9aa63d28-b5c9-496e-b9ee-5ecee6e99fa8/manifest.json`.
Some native checks ran concurrently; elapsed times are execution records rather
than controlled compiler-performance measurements.

The patched 1.95.0 binary also passed all five CLI traces:
`artifacts/cli-trace/0817fe3e-b52b-456e-9462-2ff6124a2aec/manifest.json`.
Its SHA-256 is `7433d3ec9aeddf33ab2574e6d916d20fd8a8220c84a11fc566999442bf968a0a`.

## Scope and next gate

Use [the documented experiment commands](../development/codex-source.md#explicit-compiler-experiments)
to reproduce. Evidence records actual Rust/Cargo versions, MSVC 14.50.35717,
CMake/Ninja, Windows 10.0.26200, source/patch identities and commands. The host has
12 logical CPUs and 68,622,794,752 bytes of RAM; these are host properties, not
measured peak process usage or minimum system requirements.

Munarium's [200 native datastore tests](p0-07-munarium-datastore.md) already passed
on its pinned 1.98.0 compiler. These independent builds do not yet qualify a
combined Cargo dependency graph, inference assets, VCP adapters or another
Windows environment. P0-07 remains in progress. Normal source builds consume
the committed patched files and never fetch/apply this patch implicitly.
