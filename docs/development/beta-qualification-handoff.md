# BETA-09 qualification handoff

This is a command/input map, not a completed qualification scorecard. The runner
changes are covered by offline metadata and Windows ZIP-boundary tests. No final
production build, provider call, model download or final installed matrix was run
for this increment. Retain `not run` until an exact candidate produces evidence.

The shared `production-package` reader accepts versioned ZIP names, validates the
strict release/build/source identity, archived manifest and every payload/notice
hash, and rejects traversal/collisions before extraction. Startup and recovery can
use the actual installed native `vcp.exe`; its complete directory must match that
same package. This alone is not proof of registered installation ownership.

## Offline commands once inputs exist

Set `$native` and `$setup` to the selected final result JSON paths, `$launcher` to
the registered program root's stable `vcp.exe`, and `$private` to a private,
nonsynchronized directory outside repositories. Use a fresh output path per run.
The registered smoke refuses an existing VCP registration: use a separate test
account or run it before the installation used by later commands.

```powershell
./scripts/release/candidate-smoke.ps1 -NativeResult $native -SetupResult $setup `
  -OutputRoot (Join-Path $private ('install-' + [guid]::NewGuid()))

$engine = (& $launcher --resolve-installation | ConvertFrom-Json).executable
if ($LASTEXITCODE -ne 0) { throw 'Installed selection failed.' }
$nativeReceipt = Get-Content -LiteralPath $native -Raw | ConvertFrom-Json
$engineHash = ($nativeReceipt.manifest.files | Where-Object path -CEQ 'vcp.exe').sha256

./scripts/evals/production-startup-qualification.ps1 -PackageResult $native `
  -InstalledExecutable $engine -PythonExecutable $python `
  -FixtureManifest $startupFixtures -OutputRoot (Join-Path $private 'startup')

./scripts/evals/production-recovery-qualification.ps1 -PackageResult $native `
  -Executable $engine -ExpectedSha256 $engineHash -FixtureRoot $recoveryFixtures `
  -OutputRoot (Join-Path $private ('recovery-' + [guid]::NewGuid())) `
  -GitExecutable $git -NodeExecutable $node -AgeExecutable $age `
  -EnvelopeVerifier ./scripts/evals/verify-production-envelope.cjs

./scripts/evals/production-distribution-qualification.ps1 -PackageResult $native `
  -PreviousPackageResult $previousNative -OutputRoot (Join-Path $private 'upgrade')
```

Required inputs beyond the final artifacts: `$python` is an explicit Python 3
executable with `sqlite3` (avoid WindowsApps command aliases); `$startupFixtures`
binds retained `p803-mcp-history` receipts/logs and their still-existing private
SQLite/files roots. `$recoveryFixtures` contains both backends' `fixture.json`,
ciphertext and independent recovery-key copies. `$age` must match Go age 1.3.2
SHA-256 `2821a4ed191da07372acd302e5f6feae7a7985e285e1417765ebe74025af45f0`.
`$git` and `$node` are explicit tool paths. `$previousNative` must name a distinct
strict production version and executable, not the older debug qualification
binary. The recovery runner optionally accepts that version's package plus
`-RollbackExecutable` and `-RollbackSha256` for task/ledger read comparison.
The local inventory found the pinned age executable at
`artifacts/upstream/age-v1.3.2/age/age.exe`, with the required hash. Documented P8
startup receipts and private recovery fixture paths were absent. Available CS3
packages have caller-supplied/unverified build receipts and cannot serve as the
strict prior version. Never synthesize passing receipts or relabel old debug
evidence to satisfy these inputs. Newly generated synthetic encrypted fixtures
have a separate [preparation procedure](beta-recovery-fixtures.md); they are not
the missing historical fixtures.

## Synthetic 130-version startup on the final engine

The separate ignored qualification test reuses the historical scenario's exact
governed fixture generator on Files and SQLite. It creates new synthetic history;
it does not reconstruct or claim the missing historical P8 receipts. The selected
native executable's complete payload must match the strict final package. The
test verifies EOF and an unexpected frame during actual pending replay stop the
newly owned server, then measures normal readiness/initialization and SDK task
inspection. Canonical state and offline paused accounting must remain unchanged.

```powershell
$env:VCP_BETA_NATIVE_RESULT = $native
$env:VCP_BETA_INSTALLED_EXECUTABLE = $engine
$env:VCP_BETA_STARTUP_OUTPUT = Join-Path $private ('startup130-' + [guid]::NewGuid())
$env:VCP_TEST_NODE = $node
$env:VCP_TEST_PWSH = Join-Path $PSHOME 'pwsh.exe'
cargo +1.98.0 test --manifest-path src/third_party/codex/codex-rs/Cargo.toml `
  -p vcp-cli --features qualification --test local_inspector_queries -j2 `
  final_production_startup_130_versions_both_stores -- --ignored --exact `
  --nocapture --test-threads=1
```

Run from an admitted Windows native build environment. The qualification test
binary prepares the fixture; only the explicitly selected production engine is
launched for observations. No provider/model calls occur. The aggregate private
`result.json` is passing only after both stores' observations, canonical-state
comparisons and kernel-confirmed process-tree cleanup pass. Nested observation
receipts alone are insufficient. A no-breakaway Windows Job Object contains the
PowerShell/Node/native descendants. Driver, cleanup and wrapper deadlines are
300, 10 and 480 seconds per backend, respectively; readiness and initialization
retain their original 60/10-second bounds and cancellation must precede five
seconds. The standalone PowerShell helper is intended to run inside this wrapper.
If optimized startup reaches readiness before the existing 250ms observation and
owned-child query can intercept replay, the cancellation boundary is inconclusive
and its matrix row remains not run. Preserve the receipt and successful startup
observations; do not weaken the assertion or classify fast startup as a product
failure.

Evidence binds package/build/source identity, executable, qualification binary,
SDK/helper/input hashes, actual commands and deadlines, hardware, elapsed times,
and sampled native bridge/server CPU and working sets. Sampling can miss short
processes and does not measure a complete process tree. These are single samples
per operation on the current host with uncontrolled OS cache, not clean-host,
real-task, p95 or minimum-hardware evidence. Synthetic fixtures are retained at
the private roots recorded in the aggregate result, including after failures;
input and before/after canonical-state receipts remain in the private output.

## Final matrix coverage and outstanding work

| Matrix area | Can run offline here after exact inputs exist | Still requires separate evidence |
| --- | --- | --- |
| Production identity | Strict package reader; candidate workflow build/inventory/notices gates | Successful reviewed production build and final pair, not source tests |
| Clean installation | Registered smoke with custom spaces/Unicode roots on this host | Clean standard-user Windows, absent prerequisite, insufficient space and retry |
| First useful CLI task | Missing-profile/tool/trust/expiry guards and onboarding unit tests | Supported first task and renewal with separately admitted live spend |
| First editor task | `editor-smoke.ps1` with exact VSIX/native, VS Code 1.138.0 and retained paused workspace/data | Human task start/progress/pause/resume/reviewed edit; no observer-only substitution |
| Cross-client configuration | Native parity and stale-import tests on both stores | Actual installed CLI/editor task and reconnect observations |
| Editor lifecycle/buffers | SDK/extension and native integration regressions | Actual restart, partial edits, typing, undo, dirty buffers and failed update on both stores |
| Upgrade/removal | Distribution runner; recovery task/ledger read comparison when a strict prior exists | Registered native plus VSIX distinct-version upgrade with real retained task/accounting state |
| Skills/memory/helpers | Shipped catalog/hash checks; helper smoke if explicitly installed; verify existing model bytes | Real installed helpers and production network-denied build/query; model acquisition remains explicit |
| Trust/privacy/recovery | Recovery runner's same-host encrypted restore/publication/tamper/wrong-key checks | Controlled physical full-volume, crash and independent-machine rows where required; no inference from synthetic errors |
| Performance/usability | Retained-history runner when its inputs exist; final-engine synthetic 130-version startup/early-cancellation test above | Actual passing final-byte receipts; long-check/pause behavior and human interventions |
| Owner acceptance | Assemble current evidence/dispositions without launching paid tasks | Owner quality review and distribution decision |

The developer workstation and hosted build image are not clean-machine evidence.
Serially run setup/uninstall and direct engine lifecycle campaigns: their per-user
locks intentionally refuse competing operations. Keep fixture history, recovery
keys, plaintext outputs and unreviewed logs private; publish only sanitized hashes,
commands and selected receipts.
