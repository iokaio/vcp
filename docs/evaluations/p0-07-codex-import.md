# P0-07 Codex source import evidence

Run date: 2026-09-17. Scope: committed-source preparation, reconstruction,
native build and deterministic provenance tooling. P0-07 remains in progress;
this does not qualify a VCP model session or mark integration tasks complete.

## Inputs and independent checks

The [selection and commands](../development/codex-source.md) bind Codex commit
`3d3ae4965ab370217e871b3a7f0d15589557ee4b`. Original native results are in the
[candidate report](p0-07-native-candidates.md). Selected source contains 7,937
files, including 34 executable modes. Root/bundled notices and the complete
Rust workspace/lockfile are retained. The only source transformation materializes
the declared bubblewrap license symlink; no code patches are applied.

Two separate reconstructions produced identical records with result-file digest
`da7e62d1ff0e0f27a1927bde43b0081e11d1804c108df55767ab56ea089cb56e`.
The second output is `artifacts/upstream/codex-reconstruction-01`, with its adjacent
JSON record. `verify-index --component codex` passed after explicit staging of
Git executable modes. No imported path was ignored and no force-add was used.

The deterministic suite verifies repository links/ownership, runner failures,
synthetic experiment helpers, original-byte inventory, reconstruction, patch
escape rejection, executable-mode preservation and source file-set/hash drift.
The first full local attempt under the restricted tool account failed Git
ownership checks and an existing Node-hardlink test. Running as the checkout
owner passed; no Git trust setting or regression assertion was weakened.

Staging the import exposed the recorder's 64 MiB diff-buffer limit. Source
identity now hashes Git's binary diff as a stream, with external diff/textconv
disabled, and rejects partial command failure. A 65 MiB synthetic regression
passes. The final pre-commit deterministic run
`02ad3705-4602-4fb7-96ed-c1873c39782c` passed all five cases and 28 regression tests;
its manifest/logs are under `artifacts/tests/`.

Final review added rejection of tampered patch digests before allocation and
patches deleting declared license/closure inputs. Run
`458c3b78-e8f8-44b2-9bb7-0ed6d5714ede` passed all five cases and 28 tests after
those changes. CI run `35266499206` passed on the initial import commit, with the
actual job label `ubuntu-8core`; the PR records checks for the final head.

## Native build

`pwsh -NoProfile -File scripts/build.ps1 -TargetRoot artifacts/upstream/codex-target`
passed with exit 0. Cargo built the copied source in 9m50s. Evidence:
`artifacts/build/81cfc2ae-ba01-47a1-8650-2c7dc024bf85/manifest.json` and `command.log`.
Log SHA-256: `0e7ee6883b53d6c55e2cb061432df1c78085736aa9341f7d22f8710aa4cbf7cf`.
This preparation run had uncommitted source; committed/clean-clone verification
is recorded separately below.

Host: native Windows 10.0.26200, 12 logical processors, 68,622,794,752 RAM bytes;
Rust/Cargo 1.95.0, MSVC 14.50.35717, CMake 4.2.3-msvc3, Ninja 1.12.1.
The task-owned target directory reused previously compiled external dependencies;
this is not a cold-cache measurement or proof of an offline dependency install.

`pwsh -NoProfile -File scripts/build.ps1 -Mode BoundaryTests -TargetRoot artifacts/upstream/codex-target`
also passed: 102 retained patch/policy tests, no failures or ignored tests.
Evidence: `artifacts/build/616039f3-e002-4c9b-ab8e-c7d588139e73/manifest.json`;
log SHA-256 `74ac6247a12821a2ffa0612bce55da1cc2081cbaed2fc7bc943200b7eb4d436e`.

## Clean committed clone

A fresh clone at `artifacts/qualification-clone-02` checked out VCP commit
`5615f063382a68bfd081f47f14f12e3658565b44` using `git clone -c core.longpaths=true
--no-hardlinks --single-branch --branch feat/01-codex-source-import`. The pinned
development dependency was installed with `npm ci --prefix
artifacts/qualification-clone-02/src/tests --ignore-scripts --no-audit --no-fund
--registry=https://registry.npmjs.org`. File and Git-index verification passed.

`pwsh -NoProfile -File artifacts/qualification-clone-02/scripts/build.ps1 -OutputRoot artifacts/clean-clone-build -TargetRoot artifacts/upstream/codex-target`
passed with exit 0 in 8m59s of Cargo time. The manifest records `vcp_dirty: false`,
and Git status remained clean afterward. Source/selection hashes match the
independently reconstructed tree. Only the existing external dependency cache
was reused; the build consumed committed source from the fresh clone, with no
upstream acquisition or patch application.

Evidence: `artifacts/clean-clone-build/b79847e1-5ac5-4f7f-ae3f-7678abbe43c1/manifest.json`;
log SHA-256 `68230237d2c3afdf16810d16fe4415b209b5e82663d719b6b4081ceebe558d5a`.
Subsequent changes in this PR tighten reconstruction validation and document
Windows setup; they do not change the imported Cargo source or its build inputs.

## Limits

The first nested clean-clone attempt failed checkout of five long upstream
snapshot paths under Windows Git's default limit; Git did not populate its index.
That failed clone was preserved. A fresh clone with repository-local
`core.longpaths=true` is the documented setup; no global configuration or source
snapshot names were changed.

Unchanged upstream fixtures and documents contain intentional/pre-existing
whitespace. `git diff --check` excludes only `src/third_party/codex`; the source
inventory checks that tree's exact bytes instead. Original VCP docs retain link
checks; upstream Markdown retains upstream-relative links without rewriting.

The binary retains upstream branding/behavior and has not been used for a live
session. Model/provider authority, telemetry removal, canonical durability,
budget accounting and explicit VCP pause/resume remain P0-03/P0-08 and subsequent
tasks. No paid model request was made. The full workspace, other operating
systems, optional features, native voice packaging and another Windows host
have not been qualified by these results. Complete effect classification and
other component selections still gate P0-07 completion.
