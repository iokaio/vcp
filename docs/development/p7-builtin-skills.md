# Built-in skill catalog

P7-02 adds 21 original packages under `src/skills/builtin`. The catalog covers
architecture, review/debugging, testing, Git hygiene, JavaScript/TypeScript, Python,
Rust, .NET/PowerShell, JVM, Go, C/C++, Ruby, PHP, Swift, Dart, shell, SQL, data,
infrastructure, project optimization and memory hygiene. Source/license records
and exact content hashes accompany every package.

Catalog 1.10.0 preserves those 21 families and adds eight workflow packages:
`document-authoring`, `skill-authoring`, `frontend-design`, `mcp-development`,
`llm-integration`, `webapp-testing`, `pdf-workflows`, and
`spreadsheet-workflows`. Six adapt Apache-2.0 Anthropic skills; PDF and spreadsheet
helpers are independently authored using permitted libraries because the
corresponding Anthropic packages have restrictive licenses. Package notices pin
the selected sources and describe adaptations and host dependencies.

Catalog 1.11.0 (SU-04, [ADR-070](../adr/070-skill-resource-roles-and-discovery.md))
removes the never-emitted `explicit:*` cues. Workflow and general packages are
cue-less, so their descriptions appear in the bounded description-only discovery
context. Language families match root markers, including `build.gradle.kts`,
`requirements.txt`, `setup.py`, `Pipfile`, `deno.json(c)`, `go.work`, `meson.build`,
and root `*.sln`/`*.slnx`/`*.csproj`/`*.fsproj`/`*.vcxproj`/`*.vbproj` files. Each description states when to use
the skill. Listing a skill never activates it or grants tools.

Catalog 1.12.0 (SU-05) declares licenses, notices, provenance records, requirement
files and helper source as `use: "file"` resources. They stay installed and
hash-verified, but never become model context, which cuts the eight workflows'
activation context by about 70%. Bodies tell the model to copy a helper with
`vcp_skill` `materialize` and run it only through an authorized `vcp_exec` process
profile. The CLI test `installed_pdf_helper_materializes_and_runs_through_authorized_process`
exercises that path end to end when `VCP_TEST_PYTHON` names a Python with the
pdf-workflows requirements; otherwise it reports not run.

Catalogs 1.13.0 through 1.41.0 (SU-06 to SU-15, the
[skills upgrade plan](../research/skills-upgrade-plan.md)) deepen the eight
workflows and the 21 baseline families. Baseline bodies are version-agnostic,
about 6 KB or less, and end with a one-line authority statement.

Catalog 1.42.0 onward (the SH hardening series,
[ADR-071](../adr/071-on-demand-skill-references.md)) adds the on-demand
`reference` resource role. `vcp_skill` `read` returns a reference's verified
text on request, so llm-integration, mcp-development, frontend-design and
document-authoring send only their bodies on activation. `vcp_skill` is implied
by the `vcp_read` ceiling, `materialize` also needs `vcp_patch`, and child
tasks are not offered the tool. Baseline families name commands that avoid
hidden downloads or installs and state when a check has effects. Root markers
also cover `meson.build`, `*.vcxproj` and `*.vbproj`.

The [replacement skills plan](../research/skillsplan-new.md) and
[ADR-069](../adr/069-practical-skill-ports.md) govern these additions. Development
uses scoped functional checks, package verification, and ordinary repository
checks. Historical comparative campaigns below remain historical evidence, not
a universal prerequisite for shipping a skill. Runtime authority and durable
memory admission rules are unchanged.

The native catalog verifier embeds metadata only. At runtime the CLI registers
`<executable-directory>/skills/builtin` under reserved identity `vcp-builtin`, even
without a user `skills` configuration. The existing registered workspace and
canonical read policy still apply. Configured workspace and user sources retain
precedence. Only an available default source consumes a source slot; a bare
binary can retain the existing 32-source configuration limit.

A configured optional user source whose directory is missing reports
`source_unavailable` without preventing discovery from other sources. Restore the
directory and configure skills again to include it. Missing workspace sources,
configured shipped assets and read-authority failures still stop discovery.
Task-local disables apply before source precedence, so disabling an override
allows an enabled lower-priority copy to appear in discovery.

`vcp skills list` and `/skills list` expose descriptors, setup diagnostics and
integrity read costs. Missing sidecar assets report `builtin_assets_missing` in
offline inspection and `builtin_source_unavailable` through the interactive host;
present corrupt metadata is an integrity failure. Explicit activation and disable
use the normal P7-01 controls. A body change is rejected on activation or subsequent
dependency validation. The catalog grants no process, network or install authority.

`vcp run "Review this project" --skill vcp-builtin::architecture::architecture`
activates the selected skill through those same controls before the first model
turn, including noninteractive runs. Repeat `--skill` for up to 32 distinct IDs.
Short IDs use normal source precedence; qualified IDs pin the intended source.
Invalid, unavailable or inapplicable selections stop execution before dispatch.

## Asset packaging

`scripts/package-skills.ps1 -Executable <explicit-vcp.exe>` creates a unique ignored
qualification directory containing a staged executable, notices, exact skill
assets, an external inventory and a verified ZIP. `-OutputRoot` selects the parent
directory. Existing package directories are not overwritten. No build, dependency
installation, signing, upload or release publication occurs.

The helper rejects missing, changed, extra and linked assets. It then verifies
archive paths, duplicates, symlink metadata, lengths and hashes against the staged
inventory. Its result records the final ZIP digest. It does not establish that an
arbitrary supplied executable was built from current source; native smoke tests
must exercise those exact staged bytes. The existing `scripts/build.ps1` remains
an upstream baseline builder and is not a VCP installer.

For source edits, `node scripts/skills/builtin-assets.cjs rehash src/skills/builtin`
validates the complete proposed inventory before updating metadata. Invalid roles,
unexpected files, descriptor aliases and linked directories (including ancestors
of the selected root) are rejected before writing. `--check` applies the same
validation and reports stale hashes without writing.

## Qualification boundaries

### P7-02 developer workflow revision

Catalog 1.1.0 revises `architecture`, `review-debug` and `testing` to 1.1.0.
Their descriptor/body identities and coverage versions change together. Guidance
now uses bounded search followed by relevant source ranges, records review scope
and causal uncertainty, and preserves owned-instrumentation evidence across
interruption and concurrent human edits. Testing guidance distinguishes trusted
long-check configuration, raw outcomes and decoded previews. These instructions
do not themselves qualify live usefulness or grant additional authority.

Catalog 1.2.0 additionally revises `javascript-typescript` to 1.1.0 after live
generation exposed inexact intermediate arithmetic. It covers numeric bounds,
runtime-compatible exact arithmetic, omitted versus invalid options and honest
test coverage. Existing frozen fixtures and oracle requirements are unchanged;
the revised body passed the final selected generation comparison recorded in
[the completion qualification](../evaluations/p7-02-completion.md#final-generation-acceptance).

`vcp_search` keeps literal matching by default. Optional `mode: "regex"` enables
bounded regex matching; `path_pattern` is a regex over normalized root-relative
paths. Optional `max_files` and `max_scan_bytes` can lower discovery ceilings.
Incomplete scans and hit limits remain explicit, and pause/steering invalidation
cancels preparation through the existing scheduler generation. No separate index
or filename discovery service is introduced.

`vcp_read` accepts optional one-based inclusive `start_line`/`end_line` bounds.
Ranged results retain a full-source version and expose `returned_range`,
`next_line` and `total_lines`; `complete` describes whole-file coverage. The
existing byte ceiling applies to returned text; a line too large for it is an
error. Source capture remains bounded at 64 MiB. Omitted ranges preserve the
existing whole-file contract. Source changes between pages must be treated as
changed evidence. The advertised tool schemas change, invalidating preparations
bound to earlier schema identities.

Trusted process profiles may set `max_timeout_ms` up to 3,600,000 milliseconds;
omission retains 120,000. Explicit verification requirements may set `timeout_ms`
within that profile ceiling and the remaining task deadline. Model requests,
project manifests and skill bodies cannot raise the profile ceiling. Execution
stays in the foreground with existing authority, output and owned-tree controls.

Coding context lists configured process names, modes, terminal availability and
timeout ceilings. It omits executable, environment and pinned input paths, and
listing a profile grants no authority. `vcp_exec` uses an empty directory string
for the workspace root. `vcp_verify` runs configured acceptance checks itself;
unchanged analysis supplies relevant artifact IDs from tool-result `evidence`
fields, rather than file paths or check selectors.

Profiles may explicitly declare `output_encoding` as `utf8` or `utf16_le`.
Without a declaration the existing UTF-8 interpretation remains. Presentation
records the decoding decision, replacement count, omitted bytes and split-prefix
bytes. The preview is an actual bounded tail; raw artifacts, stop reasons, exit
codes and verification pass rules remain authoritative. Qualification evidence
must identify the host/toolchain actually tested before claiming encoding support.

### Existing fixture and live gates

The [frozen projects](../../src/evals/skills/builtin/README.md) provide one normal
and one negative/missing-prerequisite case per family. Run
`scripts/evals/builtin-skill-qualification.ps1` in the provisioned native Rust
environment. It records exact sources, all attempted outcomes and separate catalog
integrity, discovery and activation reads. No model call occurs. The
`builtin_skill_qualification` lifecycle test runs the same contract on fixture
revision v2 (`manifest-v2.json`) in the Windows native job.

All packages require only the ordinary read/list adapters for analysis. Execution
requires actual configured tools and current authority; a descriptor match cannot
claim a successful compile, test, migration or remote action. Shell, SQL, data and
infrastructure have no cues, so they are always listed by description
(ADR-070). Root marker detection sees root files only; nested-only projects and
PowerShell-only repositories are not detected, and a user can still activate a
family explicitly. The skill procedures read
actual manifests and repository guidance before suggesting commands.

Shipped coverage declares guidance present without general toolchain qualification.
Selected live U01–U03/U08 observations and their limits are recorded in the
[completion qualification](../evaluations/p7-02-completion.md). Per the owning task, unavailable
host/toolchains remain explicitly unvalidated in the coverage matrix; they must
not acquire execution-support claims from guidance or another family's checks.
Static lint, fake model outputs and successful packaging do not qualify usefulness.

### TI-01: Common development toolchain setup

Catalog `1.75.0` adds `toolchain-installation` as the 30th bundled package. It is
available by description even in an empty workspace. Select it with
`vcp run --skill vcp-builtin::toolchain-installation::toolchain-installation`
or the ordinary `/skills activate` control before asking VCP to prepare tools.
The installation's sidecar catalog must match its executable; the next reviewed
build carries these source changes. Do not replace a running older installation's
assets independently of its embedded catalog.

The skill covers finding the requested versions, authorized installation via
platform package managers or official distributions, environment setup and actual
project verification across common stacks. It follows project version pins and
existing user scope. Its on-demand references include Windows/macOS/Linux setup
and JVM, JavaScript, Python, .NET, Rust, Go, C/C++, Ruby, PHP, Dart/Flutter,
Swift/mobile, data and infrastructure tools. New tools use the same documented
vendor/package-manager procedure rather than a hard-coded package allowlist.

The PowerShell 7.4+ `install-verified-archive.ps1` resource supplies verified
portable ZIP installation. VCP copies it with `vcp_skill materialize` and invokes
it with an existing authorized PowerShell process profile. The helper requires
an exact SHA256/SHA512 digest and a fresh workspace-relative destination, rejects
unsafe archive paths/links/collisions and excessive sizes, and preserves existing
files. It neither executes the downloaded tool nor changes PATH. Package-manager,
SDK-workload and non-ZIP installs use their own authorized vendor procedures.

Run its offline regressions with
`pwsh -NoProfile -File src/tests/skills/ToolchainInstallation.Tests.ps1` or the
`toolchain-installation` case in `scripts/test.ps1`. They also run in the fast
suite and skill-helper CI. The native CLI test
`installed_toolchain_helper_materializes_and_installs_verified_local_archive`
checks real installed-skill materialization/execution using a synthetic provider
and local ZIP. See [TI-01](../research/skills-upgrade-plan.md#ti-01--cross-stack-toolchain-installation)
for actual results and unrun platform/vendor cases. No blanket all-stack or live
model-quality claim follows from these checks.

The [native toolchain report](../evaluations/p7-02-native-toolchains.md) records
actual isolated-copy checks, including the seeded failing regression and all
not-run cases. The [paired live runner](../../scripts/evals/builtin-live-runner.md)
prepares selected U01/U02/U08 guidance observations with explicit activation,
canonical context/cost evidence and a one-shot spend cap. Its preparation and
contract tests do not qualify live usefulness; U03 generation has separate live evidence.

The [completion qualification](../evaluations/p7-02-completion.md) records the
fresh P7 trial bounds and current evidence. Read and patch tool descriptions now
clarify relative paths, nullable line arguments and literal patch syntax without
changing source-version or authority checks.
It now records selected read-only usefulness observations and reviewed CR-06
scenarios, preserving every strict failure and the offline runner correction.
The final baseline and skill generation arms both completed with canonical
verification and all 44 independent oracle checks, including exact arithmetic.
These selected observations complete P7-02 without claiming general toolchain
support or statistical skill superiority.
