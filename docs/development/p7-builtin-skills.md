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
