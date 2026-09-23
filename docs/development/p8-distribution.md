# P8-04 Windows distribution candidate

`scripts/package.ps1` assembles an unsigned Windows ZIP from an explicit native executable, validated built-in skills, notices, and optional runtime files. Its manifest hashes every payload and records source, target, compatibility and model provisioning metadata. A supplied `vcp-local-build/1` receipt must bind the exact executable digest; without it, provenance is explicitly unverified. The archive digest is recorded beside the ZIP in `result.json`.

Build the production candidate with `scripts/build-production.ps1`. It requires native Windows AMD64, PowerShell 7, Node, Git, Rust 1.95.0, Visual C++ tools, and already cached locked dependencies. The recipe builds only `vcp-cli`'s `vcp` executable with `--release --no-default-features --locked --offline`, explicit static-CRT/stack flags, and the committed Cargo configuration. It rejects unqualified build overrides, checks VCP dependency artifacts for the `qualification` feature, and records source inventories before/after compilation, compiler/native tool identities, feature lists, executable hash and any PDB hash. A failed or unstable build does not produce an accepted build receipt. These are local build provenance checks; they do not establish reproducible builds across machines or release acceptance.

```powershell
pwsh -File scripts/build-production.ps1 -OutputRoot artifacts/p8-production-build -Jobs 2
```

Use the copied executable and matching `build-receipt.json` from the same generated build directory when packaging. Keep its source inputs stable through compilation. Packaging and subsequent qualification refer to exact hashes; changing executable bytes requires a new candidate.

```powershell
pwsh -File scripts/package.ps1 -Executable artifacts/vcp.exe `
  -BuildReceipt artifacts/build-receipt.json -OutputRoot artifacts/p8-distribution
```

The package includes standalone PowerShell 7 installer and model-provisioning scripts. Installation uses PowerShell/.NET and does not require Node or a source checkout. Extract `tools/package-install.ps1` from the candidate and invoke it with explicit, disjoint installation and data roots:

```powershell
pwsh -File package-install.ps1 -Action Install -PackageZip vcp-windows-unsigned.zip `
  -InstallRoot 'C:\VCP' -DataRoot 'C:\Users\me\AppData\Local\VCP'
```

`active.json` identifies the selected executable under `releases/<archive-sha256>`. Start that `vcp.exe` with the same explicit `--data-dir` and intended `--workspace`. `doctor --vault <path> --staging <path>` checks separated local, staging and vault paths; a path check is not encrypted recovery verification.

`Upgrade` validates the complete candidate, retains releases side by side and atomically changes the active pointer. `Rollback` selects only the recorded previous compatible release. `Uninstall` validates the installation ownership marker, all retained payloads and ordinary paths before removing installation files. Workspace, history, key and vault locations remain outside that root. Unexpected files, redirects, locked payloads and mismatched data-root ownership are explicit errors. The deterministic `VCP_PACKAGE_INSTALL_FAULT=before-activation` test hook leaves the previous release active and permits retry with the candidate.

Compatibility declares `vcp-store/1+replay-base/2`, `vcp-cli-profile/1`, and `derived-index-rebuild-required`. Upgrade and rollback refuse mismatched declared state formats. This candidate does not implement cross-format migrations; those require a validated snapshot and staged restore before activation. Portable history and encrypted vault formats retain their separate contracts.

Model assets are not bundled or downloaded on startup. The package includes the pinned MiniLM specification and its license declaration. Explicit acquisition verifies each asset's byte count and SHA-256 before publishing the file:

```powershell
pwsh -File tools/package-models.ps1 -Action Acquire -Destination 'C:\VCP-models\MiniLM'
pwsh -File tools/package-models.ps1 -Action Verify -Destination 'C:\VCP-models\MiniLM'
```

Use a new destination outside the installation. Failed acquisition retains its partial evidence; it does not silently reuse unverified bytes or spend provider budget. The package has no signing certificate or signature. Any later signing step changes the artifact identity and requires checksums and qualification of those final bytes.

`scripts/package-install.test.ps1` exercises native paths, locks, redirects, upgrade interruption, rollback and sentinel preservation. The exact-artifact smoke command is:

```powershell
pwsh -File scripts/evals/distribution-qualification.ps1 -PackageResult <result.json>
```

It installs the recorded ZIP using its packaged installer, runs the actual CLI with an empty process environment and fresh profile directories, checks first-run diagnosis, and uninstalls while preserving sentinels. It records startup time, peak working set and CPU time. This same-host smoke test does not establish a clean OS installation, second-machine encrypted recovery, live task quality or owner acceptance. Those remain separate P8 qualification rows.

The following production runners add bounded observations against the frozen artifact without rerunning the historical full matrix. A runner's availability is not a passing qualification result; retain its result, command logs, input hashes and stated limitations.

| Runner | Inputs and scope |
|---|---|
| `scripts/evals/production-startup-qualification.ps1` | `-PackageResult` plus `-FixtureManifest` (`vcp-retained-startup-fixtures/1`) or explicit `-FixtureResults`. Copies retained SQLite/files data before each fresh-process cost inspection/history listing, checks semantic results and preserved originals, and records history counts, elapsed time and resources. Requires Python for the read-only counter. Defaults to three repeats and a 120-second process deadline; the small sample and uncontrolled OS cache do not establish p95 latency or a controlled scaling curve. |
| `scripts/evals/production-distribution-qualification.ps1` | `-PackageResult` and `-PreviousPackageResult`. Exercises the packaged installer with a fresh profile, compatible upgrade/rollback, interrupted activation, locked payloads and uninstall preservation. Its real CLI state round trip covers storage preferences and sentinels; canonical task/ledger compatibility belongs to the recovery runner. |
| `scripts/evals/production-recovery-qualification.ps1` | Explicit `-Executable`, `-ExpectedSha256`, retained `-FixtureRoot`, new private `-OutputRoot`, and Git/Node/pinned Go age paths through `-GitExecutable`, `-NodeExecutable`, `-AgeExecutable`. Supply `-EnvelopeVerifier scripts/evals/verify-production-envelope.cjs`. Both backends exercise restore authentication/path refusals, paused retained tasks and source bytes, then fresh encrypted publication from disposable Git workspaces. Independent age decryption and Node signature/payload checks inspect the new snapshot. Optional paired `-RollbackExecutable`/`-RollbackSha256` compare the prior debug executable's task/ledger reads with a production reopen; this is same-format read compatibility, not downgrade writes. `-SyntheticProfile` adds a differential check that production rejects qualification endpoints. |
| `scripts/evals/production-interactive-qualification.cjs` | `prepare <package-result.json> <new-private-dir> <private-source-profile.json>` freezes inputs without inference; `run <plan.json> <exact-plan-sha256>` executes one bounded real-provider ConPTY observation using the existing campaign budget and a $16 reservation. It checks same-process/task pause/resume, a brief paused attempt observation and durable reopen. The current fixture uses the qualified Qwen profile and retained PTY/export helpers. This is paid execution with explicit campaign admission, not an offline smoke test or P8-05 task-quality acceptance. |
| `scripts/evals/p805-owner-prepare.cjs` and `p805-owner-runner.cjs` | Materialize the frozen `src/evals/release/p8-owner-v3` fixtures into a new private directory using their `tools/prepare.cjs`. Bind `<preparation.json> <package-result.json> <private-source-profile.json> <page-launcher-build-receipt.json>` with the owner preparer, then use the runner's `validate` or authorized `run <owner-execution-plan.json> <exact-plan-sha256>`. The one-shot six-slot cohort reserves $8 per task in the existing campaign, with a $48 aggregate ceiling. It retains failed attempts, stops on unknown charges or supervision failure, verifies preservation and current-parent generation checks, and keeps human scoring/acceptance pending. Preparation and validation make no provider calls. |
| `scripts/evals/p805-owner-integration.cjs` | `prepare <owner-plan.json> <owner-result.json> <package-result.json> <new-spec.json>`, then `run <spec.json> <new-private-output-directory> <python-executable>`. Performs zero-provider history/output/retention checks on disposable copies of accepted owner roots, including failed tasks. Independently decodes canonical rows with `p805-owner-state-oracle.py`, freezes supported selection/protection sets before product preview, verifies complete artifact bytes and checks original roots are unchanged. Unsupported retention lineage stays not run. Optimizer/skill inspection does not establish activation, live routing or MCP invocation. |

Active plaintext history, workspaces and private profiles must live outside repositories and synchronization roots. Use a new directory under `[IO.Path]::GetTempPath()` for the distribution/recovery output and interactive preparation; do not put those runs under repository `artifacts/`. The startup runner separates its artifact receipts from disposable canonical copies in system TEMP. Ensure TEMP itself is private and unsynchronized; declare additional recovery exclusions with `-SyncRoots`. Retain private runs for diagnosis and copy only appropriate non-secret receipts into the repository.

The recovery vault scan covers final published files, not concurrent interrupted-write observation. Physical full-volume exhaustion remains an explicit gap, machine handoff remains skipped, and these runners do not supply owner sign-off, the full sensitive-surface matrix, signing or publication. Preserve historical evidence alongside each new artifact-specific receipt.

The [production qualification report](../evaluations/p8-production-qualification-2026-09-22.md) records the original artifact's measured scope. The [qualification follow-up](../evaluations/p8-qualification-followup-2026-09-22.md) tracks its restore/startup corrections and new results. Fresh restore now excludes mandatory staging from trust/canonical storage even without known/declared sync roots; the recovery runner covers both default and explicitly configured exclusions. Restored source remains untrusted until ordinary revision-checked workspace trust is granted before backup capture. Consult the exact execution receipts for paid-run status; no runner supplies owner acceptance.
