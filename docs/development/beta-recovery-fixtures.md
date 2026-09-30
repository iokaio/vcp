# Synthetic inputs for production recovery qualification

The historical `p306-u04-20260920/recovery-inputs` directory is not a distributed
fixture. A public ciphertext fixture has a different contract. Do not fabricate
its metadata or remove the recovery runner's root/child/accounting assertions.

The existing ignored `vcp-lifecycle` integration test
`portable_operator_fixture::export_operator_handoff_fixture_with_child_claim_and_real_accounting`
generates the required inputs. It uses synthetic typed accounting and a local
mock provider fixture, with no external provider request or model acquisition.
Both Files and SQLite exports contain a signed encrypted snapshot, independent
private recovery keys, source bytes, root/child tasks, memory claims, two attempts,
50 settled micros and 67 unresolved micros. This is developer fixture generation;
it is not production artifact qualification or historical evidence reproduction.

Build `canonical_host` with the repository's native development prerequisites
and the selected locked toolchain; retain the emitted test executable's exact path
and SHA-256. For example, from the repository root in the configured native shell:

```powershell
cargo +1.95.0 test --manifest-path src/third_party/codex/codex-rs/Cargo.toml `
  --locked --offline --target-dir artifacts/codex-target -j2 `
  -p vcp-lifecycle --features qualification --test canonical_host --no-run
if ($LASTEXITCODE -ne 0) { throw 'Fixture test compilation failed.' }
```

Select the exact `canonical_host-<hash>.exe` reported by that build. Do not infer
selection from the newest filename when multiple builds exist. Prepare a **new**
private root outside all repository and sync trees:

```powershell
$test = Read-Host 'Absolute canonical_host test executable from this build'
$testHash = (Get-FileHash -LiteralPath $test -Algorithm SHA256).Hash.ToLowerInvariant()
$private = Join-Path ([IO.Path]::GetTempPath()) ('vcp-beta-recovery-' + [guid]::NewGuid())
$git = (Get-Command git -CommandType Application).Source
& ./scripts/evals/prepare-recovery-fixture.ps1 -TestExecutable $test `
  -ExpectedSha256 $testHash -GitExecutable $git -OutputRoot $private
```

Supply `-SyncRoots` for any additional synchronization roots. The wrapper refuses
existing, redirected, repository or declared-sync output roots, clears inherited
provider credentials, bounds process execution/output, checks the exact passing
test and validates both generated inventories. `recovery-inputs/files` and
`recovery-inputs/sqlite` each contain `fixture.json`, `snapshot.age` and
`recovery/*.recovery`; the enrollment checkpoint is in `fixture.json`. The existing
generator also writes a second key copy under `transfer/Recovery` inside the same
private root. Keep **all** of this root, including temporary histories and process
logs, private. Only its reviewed `fixture-preparation.json` receipt may be copied
to a release evidence packet; never upload the root or raw keys.

Run preparation as the intended native Windows test user. A restricted agent
account may receive access-denied errors from source verification or private-key
protection. Retain that failed receipt and use a fresh private root for an
authorized native-account retry; do not weaken those checks or reuse partial
exports. The fixture receipt binds the selected test executable's bytes, not a
claim that it is the production executable or a newly rebuilt historical fixture.

Use `(Join-Path $private 'recovery-inputs')` as `-FixtureRoot` for
`scripts/evals/production-recovery-qualification.ps1`, together with the exact
strict native package result, selected production executable/hash and its pinned
independent tools. Use a separate fresh private `-OutputRoot` for that run. The
[qualification handoff](beta-qualification-handoff.md) records the full runner
arguments and evidence limits. Preparation alone leaves production recovery
`not_run`. A successful same-host run still does not establish independent-machine
recovery, physical full-volume behavior or compatibility with a distinct previous
release. Preserve the fixture receipt and actual run failures without relabeling
them as historical or installed-product evidence.
