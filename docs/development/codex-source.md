# Committed Codex source and native build

The current selection contains 7,938 files, including the original private
work-admission module added by VCP. It copies upstream source from Codex revision
`3d3ae4965ab370217e871b3a7f0d15589557ee4b` into
`src/third_party/codex/`. This is an upstream qualification baseline. The binary
still identifies itself as Codex. The separate [lifecycle qualification host](lifecycle-recovery.md)
now exercises private checkpoints, startup/dispatch authority and pause/reopen;
production OpenRouter, budget, storage, memory and CLI integration remain with
their owning tasks. Do not treat an ordinary upstream session as VCP acceptance.

## Selection and retained structure

[upstreams.toml](../../src/third_party/upstreams.toml) binds origin, commit, Git
tree and original-file inventory digest. [The selection](../../src/third_party/components/codex-selection.json)
defines exact files and recursive prefixes, destination, required closure/license
inputs, explicit link materialization and ordered patches. [The result inventory](../../src/third_party/components/codex-files.json)
records every original/result byte count, SHA-256, Git mode and transformation.
Digests of record arrays use UTF-8 `JSON.stringify` in stored order; selection
digests use parsed JSON, so formatting does not change their identity. Git blob
SHA-1 is verified before original content is accepted. SHA-256 binds content.

| Retained paths under the Codex root | Purpose |
|---|---|
| `codex-rs/` | Cohesive Cargo workspace, internal path dependencies, tests, generated schemas, Rust toolchain and lockfile |
| `third_party/` | WezTerm supporting sources and native voice provenance/build inputs with their separate terms |
| `docs/`, `scripts/`, `patches/`, `bazel/` and selected root build files | Upstream documentation and maintenance/build closure; retaining a script does not make it a VCP command |
| `LICENSE`, `NOTICE`, `README.md`, `.gitattributes` | Original rights, orientation and file attributes |

Upstream `.git`, `.github`, editor/agent root settings, JavaScript distribution
clients and SDKs are excluded. Retained Cargo members include platform and public
API machinery needed by the cohesive workspace; their presence does not enable
deferred VCP features. No upstream telemetry or credential behavior has yet been
adapted. The [effect map](upstream-candidates.md) identifies the next review seams;
P0-03/P0-08 must trace and replace all competing effect authority before integration.

Most bytes are unchanged. `codex-rs/vendor/bubblewrap/LICENSE`, originally
a symlink to `COPYING`, becomes a regular copy of that selected license. The
selection records this file-kind transformation and both hashes. The ordered
[patch series](../../src/third_party/patches/codex/README.md) raises the retained
`codex-chatgpt` crate's recursion limit for the Rust 1.98.0 compiler experiment.
A second patch registers the three [Munarium libraries](munarium-source.md) in
this workspace and extends its lockfile without replacing existing Codex package
versions. Modified files carry VCP modification notices. Further changes require
hashed patches under `src/third_party/patches/codex/` and updated result records.
Root Git attributes preserve upstream bytes; original nested fixture attributes
are retained. Executable Git modes must be staged explicitly on Windows and
checked against the result inventory before commit.

The sixth patch adds [host continuation admission](continuation-admission.md) to
four retained implementation files and five integration cases. It preserves
default drain semantics and does not change dependencies or provide a complete
pause operation. Run `scripts/build.ps1 -Mode LifecycleTests` for the native gate.

The seventh patch adds thread-scoped admission and owned interruption, plus the
original [lifecycle host](scoped-lifecycle.md) in the same workspace. Its local
lock entry reuses existing dependency identities. `LifecycleTests` now compiles
both retained-core and host integration targets and observes all four named
groups; it does not implement durable pause or CLI controls.

Full retained notices are described in [third-party attribution](../../THIRD_PARTY_NOTICES.md).
The Cargo lockfile records external registry checksums and immutable Git source
revisions; the complete Rust workspace supplies internal path dependencies.
Cargo still provisions external dependencies. Native voice source records are
retained without downloading or distributing their runtime DLLs. This source
closure is not a release dependency/license inventory or a qualification of every
workspace member, platform, optional feature or native voice configuration.

## Setup and ordinary build

Clone with `git clone -c core.longpaths=true https://github.com/iokaio/vcp.git`.
The setting is local to that clone. Some retained upstream snapshots exceed
Windows Git's default path limit when nested under VCP. Existing Windows clones
can set `git config --local core.longpaths true` before pulling the source import.
Keep the checkout root short; tool-specific path limits still need qualification.

Use native Windows x64, Git, PowerShell 7, Node 24, Visual Studio C++ x64 build
tools and Windows SDK, with CMake and Ninja. The script discovers Visual Studio
using `vswhere` and initializes its developer environment. Install the immutable
Rust toolchain and development-only TOML parser:

```powershell
rustup toolchain install 1.95.0 --profile minimal --component clippy,rustfmt,rust-src
npm ci --prefix src/tests --ignore-scripts --no-audit --no-fund
pwsh -NoProfile -File scripts/build.ps1
pwsh -NoProfile -File scripts/build.ps1 -Mode BoundaryTests
```

`build.ps1` verifies both Codex and Munarium source inventories and executes `cargo +1.95.0 build --locked
-p codex-cli --bin codex --target x86_64-pc-windows-msvc -j 4` inside the committed
`codex-rs` directory. `BoundaryTests` selects `codex-apply-patch` and
`codex-execpolicy`. It neither acquires Codex nor applies patches. These commands
build/test the baseline only and never start a model session. Ordinary Cargo
dependency provisioning may use the network; an offline build requires those
dependencies to be provisioned already.

`-Jobs` accepts 1–16. `-OutputRoot` and `-TargetRoot` default to ignored
`artifacts/build` and `artifacts/codex-target`; overrides resolve from the caller.
Each run writes a separate manifest/log with tool versions, commands, source
identity, source-record hashes and outcome. Missing prerequisites return
`not_run`/3. Compiler/test failures propagate. Linux delivery CI checks provenance
and deterministic tooling; it does not qualify a Windows binary.

### Native Windows CI

The [delivery workflow](../../.github/workflows/ci.yml) also targets the
standard `windows-2025` GitHub-hosted Windows x64 runner. GitHub lists 4 CPUs,
16 GB RAM and 14 GB SSD storage for this public-repository runner; see the
[standard runner specifications](https://docs.github.com/en/actions/reference/runners/github-hosted-runners#standard-github-hosted-runners-for-public-repositories).
Each job starts from checkout with process-scoped
Git long-path support, installs Node 24.10.0 and Rust 1.95.0/1.98.0, and provisions Cargo
dependencies without a restored VCP build cache. Visual Studio, SDK, CMake and
Ninja discovery uses the same native build runner as local qualification.

Standard runners do not require an organization runner group. The workflow maps
`ubuntu-8core` to `ubuntu-24.04` and `win8core` to `windows-2025`, removing the
larger-runner compute charges for this public repository. Windows Cargo commands
explicitly use two build jobs to reduce peak memory demand; local command defaults
are unchanged. All qualification cases and acceptance thresholds remain required.
Earlier evaluation records describe the machines on which they actually ran;
they do not establish capacity or qualification on the smaller runner. Check the
new CI result for memory, disk and timing failures before claiming that evidence.

The Windows job runs the fast suite, verifies imported Git bytes/modes, builds
the committed CLI, runs patch/policy and selected Munarium library tests, and executes the five synthetic
[CLI trace cases](native-cli-trace.md). A separate maintenance step fetches the
exact Codex and Munarium commits from their selections, reconstructs both with
their recorded patches, and compares each complete result with its committed inventory. The
ordinary build remains independent of this acquisition and reconstruction.

Native qualification is temporarily manual-only to reduce CI work. In GitHub,
select **Actions > Delivery checks > Run workflow**, choose the intended branch,
and run it. Alternatively, after this workflow is on the default branch:

```powershell
gh workflow run ci.yml --ref main
```

The `windows` job runs only for `workflow_dispatch`, after the fast `delivery`
job passes. PRs and pushes to main run only the fast Ubuntu checks. The manual
job retains source reconstruction, Gemini, embeddings, all corpus scales,
native builds/tests and CLI traces. No scheduled heavy run or paid-model call
is added. Run affected native checks deliberately before claiming native or
release acceptance; a skipped Windows job supplies no such evidence. To restore
automatic native qualification, remove its event condition in the workflow.

Every native command must pass; missing prerequisites fail the job. The job has
a 180-minute timeout to accommodate the smaller runner and uploads test, build
and trace manifests/logs plus the reconstruction inventory on success or failure,
retained for seven days. Download needed evidence before expiry. Acquired
source, reconstructed source, dependency caches and binaries are excluded from
the artifact selection. Record the run/head, actual assigned runner, tool versions
and outcomes in evaluation evidence before claiming second-environment success.

### Explicit compiler experiments

Codex's retained toolchain pin is 1.95.0; the selected Munarium libraries pin
1.98.0. To compare compatibility without editing either source, invoke the
qualification runner with an immutable experiment override and a separate cache:

```powershell
pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex -ExperimentToolchain 1.98.0 -OutputRoot artifacts/upstream/codex-rust-198 -TargetRoot artifacts/upstream/codex-target-198 -Mode Build
pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SelectedCodex -ExperimentToolchain 1.98.0 -OutputRoot artifacts/upstream/codex-rust-198-tests -TargetRoot artifacts/upstream/codex-target-198 -Mode BoundaryTests
```

The selected compiler must already be installed. Floating aliases such as
`stable`, `beta` and `nightly` are rejected before output is allocated. The
manifest records both `upstream_toolchain` and `experiment_toolchain`, the actual
compiler version and exact command. A failed experiment remains failed; it does
not fall back to the upstream compiler or rewrite a toolchain/lockfile.
`scripts/build.ps1` continues to use the upstream pin and exposes no override.
Cross-compiler success alone would not qualify a unified Cargo dependency graph,
optional platforms/features or VCP adapters.
The [compiler experiment report](../evaluations/p0-07-common-rust.md) records the
unmodified failure, compatibility patch and executed follow-up checks.

## Explicit reconstruction

Acquire the immutable candidate using [the candidate procedure](upstream-candidates.md).
Reconstruction reads Git objects at the pinned commit, not modified checkout
files, and never fetches. Choose new output and record paths:

```powershell
node scripts/upstream/reconstruct.cjs reconstruct --component codex --source artifacts/upstream/codex-candidate --output artifacts/codex-reconstructed --record artifacts/codex-reconstructed.json
node scripts/upstream/reconstruct.cjs verify --component codex
node scripts/upstream/reconstruct.cjs verify-index --component codex
```

Compare the reconstructed JSON record byte-for-byte with the committed result
inventory. `verify` rejects extra/missing/changed files, links and unsupported
file kinds, and checks executable modes on hosts exposing them. `verify-index`
checks Git's staged modes and bytes on Windows as well. Neither verification mode
requires the acquisition checkout. CI runs both on the checked-out PR commit.

The disposable patch index enables Git long-path support through command-scoped
configuration. It does not change user/global Git settings. Unchanged files keep
their original transformation labels; only changed bytes/modes are marked as
patched, preserving an earlier materialization label where applicable.
Ordinary source verification also checks each declared patch's actual digest,
without applying it; a modified patch cannot silently escape CI/build checks.

The importer checks immutable inputs before creating output and refuses existing
output/record files. It creates missing output parents after input verification,
then exclusively creates the destination. A failed patch leaves its new partial output for inspection;
it is not a successful reconstruction. Patches use a separate owned scratch Git
index without author identity, commits or metadata inside reconstructed source.
Ordered `git apply --check --index` and `git apply --index` preserve executable
modes, followed by a full comparison with independently read index blobs. Unsafe
paths, case collisions, undeclared links and unsupported file kinds are rejected.
Tampered patch digests fail before output allocation. Patches that remove declared
license or closure inputs fail reconstruction; a partial tree is not accepted.

Retained upstream Markdown has its original site/tree-relative links. VCP's
link checker excludes that imported subtree and verifies VCP docs normally;
provenance checks verify every imported file instead. Upstream whitespace is
preserved rather than rewritten to satisfy VCP's original-source whitespace gate.
