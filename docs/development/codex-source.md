# Committed Codex source and native build

The P0-07 selection copies 7,937 files from Codex revision
`3d3ae4965ab370217e871b3a7f0d15589557ee4b` into
`src/third_party/codex/`. This is an upstream qualification baseline. The binary
still identifies itself as Codex; VCP's OpenRouter, budget, persistence, memory,
pause/resume and authority adapters are not implemented. Do not use an upstream
session as evidence that those VCP boundaries work.

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

Bytes are unchanged except that `codex-rs/vendor/bubblewrap/LICENSE`, originally
a symlink to `COPYING`, becomes a regular copy of that selected license. The
selection records this file-kind transformation and both hashes. There are no
code patches yet; the ordered `patches` array is empty. Future changes require
hashed patches under `src/third_party/patches/codex/` and updated result records.
Root Git attributes preserve upstream bytes; original nested fixture attributes
are retained. Executable Git modes must be staged explicitly on Windows and
checked against the result inventory before commit.

Full retained notices are described in [third-party attribution](../../THIRD_PARTY_NOTICES.md).
The Cargo lockfile records external registry checksums and immutable Git source
revisions; the complete Rust workspace supplies internal path dependencies.
Cargo still provisions external dependencies. Native voice source records are
retained without downloading or distributing their runtime DLLs. This source
closure is not a release dependency/license inventory or a qualification of every
workspace member, platform, optional feature or native voice configuration.

## Setup and ordinary build

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

`build.ps1` verifies selected source and executes `cargo +1.95.0 build --locked
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

The importer checks immutable inputs before creating output and refuses existing
output/record files. A failed patch leaves its new partial output for inspection;
it is not a successful reconstruction. Patches use a separate owned scratch Git
index without author identity, commits or metadata inside reconstructed source.
Ordered `git apply --check --index` and `git apply --index` preserve executable
modes, followed by a full comparison with independently read index blobs. Unsafe
paths, case collisions, undeclared links and unsupported file kinds are rejected.

Retained upstream Markdown has its original site/tree-relative links. VCP's
link checker excludes that imported subtree and verifies VCP docs normally;
provenance checks verify every imported file instead. Upstream whitespace is
preserved rather than rewritten to satisfy VCP's original-source whitespace gate.
