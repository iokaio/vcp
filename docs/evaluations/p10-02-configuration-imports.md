# P10-02 configuration import evidence

Native Windows qualification on 2026-09-24 exercises the explicit
[import subset](../development/configuration-imports.md) selected in
[ADR-066](../adr/066-versioned-configuration-imports.md).

## Native qualification

`./scripts/test-imports.ps1` passed all 145 tests on Windows `10.0.26200`,
Rust `1.98.0`, target `x86_64-pc-windows-msvc`:
`artifacts/imports/c4f3c291-8f33-44e5-8335-4a6a760c297d/manifest.json`.
Manifest SHA-256:
`919d1a48843fbe482b3c383191eca9a89d07b39fc123f74b9993c8a23223aadc`.
The manifest records the tested working-tree inputs at base
`7ad17f036f07ca44d645e430c8963a2f4d0b9f82`, command lines and log hashes.

| Stage | Passed |
|---|---:|
| Pinned import parser and mapping contracts | 7 |
| CLI contracts, including five native journal tests | 100 |
| Import executable acceptance and owner-kill recovery | 5 |
| Existing executable acceptance | 29 |
| Native terminal | 1 |
| Local-service lifecycle | 3 |

The delegation adapter prerequisite built successfully. Existing package-only
release cases remain ignored (eight executable and two CLI library entries);
their separate release fixtures are not waived by this qualification.

The pure fixtures cover both pinned source formats, exact HTTP/stdio identity,
positive integral deadline conversion, tool intersections, malformed and future
versions, duplicate JSON keys, depth bounds, conflicting transports, credential
references, redacted unknown keys/values and forged/stale selections.

Native tests prove selected-field apply, immutable prior revisions and rollback,
unchanged raw source/profile bytes, runtime materialization, no provider requests
and no imported command execution. Stale source/base/rollback previews reject.
An old revision restored after native authority revocation remains clipped to
the current tool and deadline ceiling. Hard links, junctions, traversal, synced
profile/history roots and repository-contained history reject before publication.

The supervisor kills the actual CLI owner after its complete staging write and
after publication before acknowledgement. Two reopens select the same old or new
complete configuration. The original profile remains intact, no publication
replays, and a lost-acknowledgement preview cannot create another revision.

## Final delivery checks

The five native import tests passed again after adding their Windows/qualification
test guard: `artifacts/p10-imports-cli-final.log` records the final fixture hash
and command. No production Rust changed after the full run. The runner's trailing
blank line was removed during final formatting.

A fresh native `vcp-protocol-schema` build and export regenerated lockfile
provenance in the protocol schema; all nine protocol-generation tests pass.
The generated wire types are unchanged. Evidence is in
`artifacts/p10-imports-schema-build.log`,
`artifacts/p10-imports-protocol-schema.json` and
`artifacts/p10-imports-protocol-generation.log`.

The final repository suite passed all 18 cases:
`artifacts/p10-imports-fast-final/d426b73d-d6ae-42ff-992a-582a9cef05b0/manifest.json`.
Rust formatting, diff checks and indexed retained-source verification pass.

The first fast run passed 17/18 cases. Its protocol-generation gate correctly
required rebuilding the native schema exporter after the lockfile changed;
the prior generated provenance was not silently accepted.

The retained source independently reconstructs from Codex's immutable pin plus
patches through 0040: 7,940 files, inventory digest
`ec5b348ce6ce9db6d181810db490c5fdadebdd77ddb7e9a2fe9963464a470275`.
Patch 0040 adds only the already pinned TOML dependency edge for `vcp-extensions`.
Working-tree and indexed source verification pass; no package version changed.

## Scope and limits

This is explicit partial compatibility for existing MCP restrictions, not
drop-in Codex/Gemini configuration loading. Skills, executable hooks, provider
selection, credentials and authority remain separate native setup. No upstream
configuration loader runs. Original files remain recoverable because they are
never modified; the journal retains safe deltas, not raw secret-bearing files.

Qualification covers native Windows and forced process termination, not hardware
power loss or another execution host. Imported preferences apply on the next
profile load; live owners are not reconfigured. The bounded journal retains 256
revisions without automatic pruning. P10-03/P10-04, the skills roadmap and
prompt-audit proposals remain outside this increment.
