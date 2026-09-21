# Built-in skill catalog

P7-02 adds 21 original packages under `src/skills/builtin`. The catalog covers
architecture, review/debugging, testing, Git hygiene, JavaScript/TypeScript, Python,
Rust, .NET/PowerShell, JVM, Go, C/C++, Ruby, PHP, Swift, Dart, shell, SQL, data,
infrastructure, project optimization and memory hygiene. Source/license records
and exact content hashes accompany every package.

The native catalog verifier embeds metadata only. At runtime the CLI registers
`<executable-directory>/skills/builtin` under reserved identity `vcp-builtin`, even
without a user `skills` configuration. The existing registered workspace and
canonical read policy still apply. Configured workspace and user sources retain
precedence. Only an available default source consumes a source slot; a bare
binary can retain the existing 32-source configuration limit.

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

## Qualification boundaries

### P7-02 developer workflow revision

Catalog 1.1.0 revises `architecture`, `review-debug` and `testing` to 1.1.0.
Their descriptor/body identities and coverage versions change together. Guidance
now uses bounded search followed by relevant source ranges, records review scope
and causal uncertainty, and preserves owned-instrumentation evidence across
interruption and concurrent human edits. Testing guidance distinguishes trusted
long-check configuration, raw outcomes and decoded previews. These instructions
do not themselves qualify live usefulness or grant additional authority.

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
integrity, discovery and activation reads. No model call occurs.

All packages require only the ordinary read/list adapters for analysis. Execution
requires actual configured tools and current authority; a descriptor match cannot
claim a successful compile, test, migration or remote action. Shell, SQL, data and
infrastructure currently require explicit selection. Root marker detection also
does not cover every Python, .NET or JVM project layout. The skill procedures read
actual manifests and repository guidance before suggesting commands.

Shipped coverage declares guidance present but unqualified. Live U01–U03/U08
usefulness and unsupported toolchain execution remain separate P7-02 gates. Static
lint, fake model outputs and successful packaging do not complete those gates.

The [native toolchain report](../evaluations/p7-02-native-toolchains.md) records
actual isolated-copy checks, including the seeded failing regression and all
not-run cases. The [paired live runner](../../scripts/evals/builtin-live-runner.md)
prepares selected U01/U02/U08 guidance observations with explicit activation,
canonical context/cost evidence and a one-shot spend cap. Its preparation and
contract tests do not qualify live usefulness; U03 generation remains separate.
