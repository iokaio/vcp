# P0-07 native candidate results

Date: September 17, 2026. Task state: in progress. Prerequisite P0-01 has
[infrastructure evidence](p0-01-experiment-harness.md). These results qualify
specific unmodified upstream experiments, not an imported VCP foundation.

The exact candidate commits, trees and original-byte inventory digests are in
[upstreams.toml](../../src/third_party/upstreams.toml). All three inventories
completed: Codex 8,250 files, Gemini CLI 3,005, Munarium 1,037. The
[candidate guide](../development/upstream-candidates.md) specifies reproduction,
limits, license observations and the initial effect-discovery paths.

## Host and tools

Native Windows x64, OS build `10.0.26200`, 12 logical processors and
68,622,794,752 bytes physical RAM. Codex used Rust 1.95.0 (`59807616e`), Cargo
1.95.0, MSVC 14.50.35717, CMake 4.2.3-msvc3 and Ninja 1.12.1. Munarium used its
installed pinned Rust 1.98.0 (`88d9e12ae`). Gemini used Node 24.10.0, npm 11.6.1
and locked Vitest 3.2.4. PowerShell 7.6.6 orchestrated the checks.

This is one observed host, not a minimum support specification. The two Rust
toolchain pins still need a common integrated-workspace qualification in P0-08.

## Commands and outcomes

| Candidate | Actual command within its workspace | Outcome |
|---|---|---|
| Codex | `cargo +1.95.0 build --locked -p codex-cli --bin codex --target x86_64-pc-windows-msvc -j 4` | Exit 0; first observed build 16m 44s; executable version smoke returned `codex-cli 0.0.0` |
| Codex | `cargo +1.95.0 test --locked -p codex-apply-patch -p codex-execpolicy --target x86_64-pc-windows-msvc -j 4` | Exit 0; 102 tests passed, none ignored or filtered |
| Munarium | `cargo +1.98.0 test --locked --manifest-path server/Cargo.toml -p munarium-core -p munarium-store-mem --target x86_64-pc-windows-msvc -j 4` | Exit 0; 68 kernel and 11 in-memory-store tests passed |
| Gemini | `npm ci --ignore-scripts --no-audit --no-fund` | Exit 0; exact locked test dependencies installed, lifecycle scripts not executed |
| Gemini | `node scripts/generate-git-commit-info.js` | Exit 0; documented build metadata generated after initial test prerequisite failure |
| Gemini, from `packages/core/` | `node ../../node_modules/vitest/vitest.mjs run src/policy/policy-engine.test.ts src/scheduler/scheduler.test.ts src/tools/tools.test.ts --coverage.enabled=false --maxWorkers=2` | Exit 0 on retry; 155 policy, 43 scheduler and 15 tool tests passed |

The reusable `scripts/upstream/build-baseline.ps1` command also passed both
Codex modes with the same pin. Its guard rejected a mismatched commit; the
deterministic regression suite rejects output inside the pristine source root.
The inventory regressions cover unsafe/colliding paths, gitlinks, invalid UTF-8,
mutable revisions, incorrect object identities and truncated/extra bytes.

## Retained failures and evidence identities

The initial Gemini checkout could not materialize seven long snapshot paths.
Command-scoped `git -c core.longpaths=true` restored only those absent files;
the resulting checkout was clean. No global Git setting was changed. The first
Gemini test attempt passed 155 policy tests but failed to load the other two
suites because `generated/git-commit.ts` was absent. The documented generator
resolved that prerequisite; the original failure log remains separate. No
assertion or expected result was altered. Coverage collection was disabled for
this selected test run; it is not a coverage claim.

Raw evidence remains under ignored `artifacts/upstream/`:

| Evidence | Identity |
|---|---|
| Codex recorded build | Run `ffe3111c-0120-4828-80b2-c2f1fba31529`; log SHA-256 `01f19566c544059b2adf17f77e15cd6fc43525bf29026d36bbe5e5fba12d2dc5` |
| Codex boundary tests | Run `c7426b91-270d-4b62-b476-a1a7e2073758`; log SHA-256 `3172e8434f8166bd756712d8a1b8ce07220ab20a9130ded5c4318e3d2eb9e206` |
| Codex executable | SHA-256 `d9ceaa0f48631991ab9a787302776cb26d63fdef10bdf2f4a4e638abf1aca157` |
| Munarium native tests | `munarium-native-baseline/test.log`; SHA-256 `7f6ead014bd53fd2ab93960b08a6dcca13dc013b93d0b3beb7f4c39350b27cc5` |
| Gemini successful retry | `gemini-boundary-tests-retry.log`; SHA-256 `c7b78a6a5f5391fe33bedf060ee7f6176c707b695e2006f6ebf47f066226d4b0` |

## Remaining qualification

P0-07 is not complete. Selected paths, dependency/license closure, link handling,
source import, patch manifests, independent reconstruction and a clean VCP clone
build remain outstanding. No upstream source or binaries are shipped by this
change. Codex's version-only smoke does not run a coding session. Gemini tests
use upstream mocks and do not establish native execution enforcement. Munarium's
in-memory tests do not establish local embedding, DiskANN/Tantivy integration,
durable storage or encrypted portability. No model requests or paid evaluations
were intentionally performed; these were build and selected unit/contract runs.

P0-03/P0-08 must still demonstrate VCP ownership of every helper request and
effect, including explicit pause/resume while the CLI remains open. The presence
of upstream telemetry, login and provider modules is a replacement inventory,
not authorization to retain their implicit behavior in VCP.
