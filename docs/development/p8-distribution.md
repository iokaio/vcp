# P8-04 Windows distribution candidate

## Explicit local-memory commands

After provisioning the pinned local embedding assets and creating a durable
workspace session, close any active owner before standalone memory maintenance:

```powershell
vcp --workspace C:/work/project --data-dir C:/private/vcp memory build --assets C:/private/models/minilm
vcp --workspace C:/work/project --data-dir C:/private/vcp memory query "retained design decision" --assets C:/private/models/minilm --task TASK_ID
```

Build publishes lexical and vector indexes from authorized retained evidence.
Query embeds the supplied text locally; its scope flags match `memory search`.
Neither command downloads assets, needs a provider key, or resumes paused work.
Local CPU/resource observations are retained. Missing/corrupt assets fail with a
nonzero exit; build permits degraded publication only with the explicit
`--allow-lexical-only` flag. Query text is bounded by the qualified embedding
chunk limit (192 UTF-8 bytes). Use local-drive assets without reparse redirects.

`memory search` remains read-only and does not load an embedding model. A local
build/query cannot attach to a competing live owner; close it before retrying.
Ctrl+C requests cancellation and waits for bounded native work to drain before
closing the canonical owner. See the
[current qualification record](../evaluations/p8-continuation-2026-09-23.md)
for actual tested scope and outstanding acceptance gates.

`scripts/package.ps1` assembles an unsigned Windows ZIP from an explicit native executable, validated built-in skills, notices, and optional runtime files. Its manifest hashes every payload and records source, target, compatibility and model provisioning metadata. A supplied `vcp-local-build/1` receipt must bind the exact executable digest; without it, provenance is explicitly unverified. The archive digest is recorded beside the ZIP in `result.json`.

Build the production candidate with `scripts/build-production.ps1`. It requires native Windows AMD64, PowerShell 7, Node, Git, Rust 1.95.0, Visual C++ tools, and already cached locked dependencies. The recipe builds only `vcp-cli`'s `vcp` executable with `--release --no-default-features --locked --offline`, explicit static-CRT/stack flags, and the committed Cargo configuration. It rejects unqualified build overrides, checks VCP dependency artifacts for the `qualification` feature, and records source inventories before/after compilation, compiler/native tool identities, feature lists, executable hash and any PDB hash. A failed or unstable build does not produce an accepted build receipt. These are local build provenance checks; they do not establish reproducible builds across machines or release acceptance.

```powershell
pwsh -File scripts/build-production.ps1 -OutputRoot artifacts/p8-production-build -Jobs 2
```

Use the copied executable and matching `build-receipt.json` from the same generated build directory when packaging. Keep its source inputs stable through compilation. Packaging and subsequent qualification refer to exact hashes; changing executable bytes requires a new candidate.

### BETA-04 strict internal beta provenance

The [internal channel](../../release/internal-beta.json) selects native
`0.2.0-beta.1` and SDK/VSIX `0.2.1`; canonical/configuration/wire formats remain
unchanged. The default commands remain exploratory candidates. For the release
path, supply `-Release -ReviewedCommit <exact-40-character-commit>` to **both**
build and package. The reviewed commit must be the clean checkout's HEAD. This
selection is a gate input; it is not evidence of human approval by itself.

```powershell
pwsh -File scripts/build-production.ps1 -Release -ReviewedCommit <reviewed-commit> `
  -OutputRoot artifacts/beta-build -Jobs 2
pwsh -File scripts/package.ps1 -Release -ReviewedCommit <same-reviewed-commit> `
  -Executable <same-build-directory>/vcp.exe `
  -BuildReceipt <same-build-directory>/build-receipt.json -OutputRoot artifacts/beta-package
```

Release builds use a fresh Cargo target directory and inventory all tracked
source bytes before and after compilation, including dependency locks and
packaged assets. Dirty/untracked source, Git assume-unchanged/skip-worktree
entries, unselected source, version/lock mismatches, unsupported targets,
qualification features and modified toolchains are refused. The executable must
be PE32+ x64 and report the selected product version. Packaging rechecks the
compiler record, retained source/build/upstream logs and every staged resource's
source hash; external runtime files, model records and custom skill roots require
a separately reviewed recipe. Every ZIP entry is independently hashed after
compression. Keep the entire build evidence directory with the candidate.

The native result records `verified-release-build`, `release-candidate` and a
`vcp-release-identity/1` shared by its matching VSIX. Once strict VSIX packaging
has produced a matching receipt, freeze final hashes with:

```powershell
node scripts/release/pair.cjs <native-result.json> <vsix-manifest.json> <new-pair.json> [setup-result.json]
```

The pair recorder verifies final ZIP/VSIX/setup bytes, source/version agreement,
native executable/build identity and the exact native result consumed by the
VSIX. Changed bytes require a new pair. Setup is optional for this identity
helper, but remains required for beta acceptance. The unsigned channel has no
signing transformation; a signed-channel input is refused until a separately
qualified transformation receipt is implemented. Pairing records
`qualification-required`; it does not pass installed-product tests, owner
acceptance or publication gates.

### BETA-05 dependency, license and prerequisite inventory

Strict builds verify the locked Windows normal/build dependency graph before and
after compilation. Registry source files must match their checksum-verified
original `.crate` archives; Git sources must match their pinned clean checkout.
Only Cargo's documented cache markers are tolerated as extra extracted metadata.
This validation is offline and does not rebuild dependencies. Its stable
`vcp-release-dependencies/1` receipt binds the source and license selection to the
native build; a modified cache fails before compilation or candidate acceptance.

Strict packaging reruns the verification, joins compiler-observed package
identities to the selected graph and produces `component-inventory.json`,
`licenses/` and `PREREQUISITES.md`. Full license/notice bytes come from pinned
Cargo archives, Git commits and the reviewed
[license overrides](../../release/license-overrides.json). Identical license
texts share one SHA-256 filename; per-package attributions and original paths
remain in the inventory. PostgreSQL/Snowball, Munarium, Codex/WezTerm and embedded
system-skill notices are included alongside the separately hashed VCP skill
payload. Linux bubblewrap, non-Windows voice binaries, model weights and optional
helper runtimes are excluded from this Windows payload.

Original source archives for MPL-only components are supplied unchanged in
`licenses/sources`, under their original terms. Those archives preserve upstream
tests and documentation as source material, not runnable VCP qualification
fixtures. The inventory distinguishes normal dependencies from build tools and
records compiler observations rather than assuming every dependency contributes
surviving linked code. It also records two precise provenance limits: published
Apache packages that omit a standalone text receive the full selected standard
license, and `debugserver-types` publishes an MIT declaration/authors without a
standalone notice, so its declaration, standard terms and original source are
all preserved. No missing text is silently marked complete.

The [packaged prerequisite matrix](../../release/package-prerequisites.md) names
user-provisioned Python/Node/helper/browser/model requirements. Focused inventory
tests and an offline cache audit do not replace final installed helper smokes,
clean-machine qualification or the BETA-09 secret/payload review.

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
| `scripts/evals/production-startup-qualification.ps1` | Strict `-PackageResult`, explicit `-PythonExecutable`, plus `-FixtureManifest` (`vcp-retained-startup-fixtures/1`) or `-FixtureResults`. Optional `-InstalledExecutable` selects the actual native engine while requiring its entire payload to match the ZIP. Copies retained SQLite/files data before each cost/history inspection, checks preserved originals, and records counts/resources. Three repeats by default; uncontrolled OS cache and a small sample do not establish p95 latency or a hardware floor. |
| `scripts/evals/production-distribution-qualification.ps1` | Strict `-PackageResult` and `-PreviousPackageResult` with distinct executable bytes and product versions. Exercises the packaged engine lifecycle with a fresh profile, compatible upgrade/rollback, interrupted activation, locked payloads and uninstall preservation. Real CLI state covers storage preferences and sentinels; this does not replace registered setup, VSIX upgrade or canonical task/ledger recovery observations. |
| `scripts/evals/production-recovery-qualification.ps1` | Strict `-PackageResult`, explicit package-matching `-Executable`/`-ExpectedSha256`, retained `-FixtureRoot`, new private `-OutputRoot`, and `-GitExecutable`, `-NodeExecutable`, `-AgeExecutable` (pinned Go age). Supply `-EnvelopeVerifier scripts/evals/verify-production-envelope.cjs`. Both backends restore retained encrypted state and publish/independently verify a fresh snapshot without inference. Optional `-RollbackExecutable`, `-RollbackSha256`, **and** `-RollbackPackageResult` bind a distinct prior strict production version for task/ledger read comparison. This is same-format read compatibility, not downgrade writes. `-SyntheticProfile` adds the production qualification-endpoint schema guard. |
| `scripts/evals/production-interactive-qualification.cjs` | `prepare <package-result.json> <new-private-dir> <private-source-profile.json>` freezes inputs without inference; `run <plan.json> <exact-plan-sha256>` executes one bounded real-provider ConPTY observation using the existing campaign budget and a $16 reservation. It checks same-process/task pause/resume, a brief paused attempt observation and durable reopen. The current fixture uses the qualified Qwen profile and retained PTY/export helpers. This is paid execution with explicit campaign admission, not an offline smoke test or P8-05 task-quality acceptance. |
| `scripts/evals/p805-owner-prepare.cjs` and `p805-owner-runner.cjs` | Materialize the frozen `src/evals/release/p8-owner-v3` fixtures into a new private directory using their `tools/prepare.cjs`. Bind `<preparation.json> <package-result.json> <private-source-profile.json> <page-launcher-build-receipt.json>` with the owner preparer, then use the runner's `validate` or authorized `run <owner-execution-plan.json> <exact-plan-sha256>`. The one-shot six-slot cohort reserves $8 per task in the existing campaign, with a $48 aggregate ceiling. It retains failed attempts, stops on unknown charges or supervision failure, verifies preservation and current-parent generation checks, and keeps human scoring/acceptance pending. Preparation and validation make no provider calls. |
| `scripts/evals/p805-owner-integration.cjs` | `prepare <owner-plan.json> <owner-result.json> <package-result.json> <new-spec.json>`, then `run <spec.json> <new-private-output-directory> <python-executable>`. Performs zero-provider history/output/retention checks on disposable copies of accepted owner roots, including failed tasks. Independently decodes canonical rows with `p805-owner-state-oracle.py`, freezes supported selection/protection sets before product preview, verifies complete artifact bytes and checks original roots are unchanged. Unsupported retention lineage stays not run. Optimizer/skill inspection does not establish activation, live routing or MCP invocation. |

Active plaintext history, workspaces and private profiles must live outside repositories and synchronization roots. Use a new directory under `[IO.Path]::GetTempPath()` for the distribution/recovery output and interactive preparation; do not put those runs under repository `artifacts/`. The startup runner separates its artifact receipts from disposable canonical copies in system TEMP. Ensure TEMP itself is private and unsynchronized; declare additional recovery exclusions with `-SyncRoots`. Retain private runs for diagnosis and copy only appropriate non-secret receipts into the repository.

For the internal beta, these three runners reject historical `recorded-local-build`
receipts and require `verified-release-build`. They validate archived source/build
identity, executable feature/target evidence, complete payload inventory and notices
before launching the candidate. See the [BETA-09 handoff](beta-qualification-handoff.md)
for commands, input prerequisites and the remaining installed-product matrix.

For an affected retention-only follow-up, the owner integration runner accepts `run-retention <same-spec.json> <new-private-directory> <python-executable> <completed-result.json> [row-id ...]`. It rechecks the exact artifact, original inventories, independent canonical state, successful history/default-policy command receipts and every complete raw-output digest before reusing observations. Failed observations cannot become reused passes. Retention uses fresh disposable copies and newly frozen independent selection/protection sets; the prior receipt and its command/payload hashes remain bound through completion.

The recovery vault scan covers final published files, not concurrent interrupted-write observation. Physical full-volume exhaustion remains an explicit gap, machine handoff remains skipped, and these runners do not supply owner sign-off, the full sensitive-surface matrix, signing or publication. Preserve historical evidence alongside each new artifact-specific receipt.

The [production qualification report](../evaluations/p8-production-qualification-2026-09-22.md) records the original artifact's measured scope. The [qualification follow-up](../evaluations/p8-qualification-followup-2026-09-22.md) tracks its restore/startup corrections and new results. Fresh restore now excludes mandatory staging from trust/canonical storage even without known/declared sync roots; the recovery runner covers both default and explicitly configured exclusions. Restored source remains untrusted until ordinary revision-checked workspace trust is granted before backup capture. Consult the exact execution receipts for paid-run status; no runner supplies owner acceptance.
