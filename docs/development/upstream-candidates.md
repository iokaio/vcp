# P0-07 candidate acquisition and byte inventory

Status: investigation in progress. The immutable candidates are recorded in
[upstreams.toml](../../src/third_party/upstreams.toml). The [Codex selection](codex-source.md)
is now imported as a qualification baseline; the other candidates remain external.
These results contribute to
[upstream qualification](upstream-qualification.md), not a replacement for its
native build, closure, reconstruction and integration gates.

## Reproduce a candidate checkout

Use a fresh disposable directory owned by the account running Git. For the
current Codex candidate:

```powershell
git init artifacts/upstream/codex-candidate
git -C artifacts/upstream/codex-candidate fetch --depth 1 https://github.com/openai/codex.git 3d3ae4965ab370217e871b3a7f0d15589557ee4b
git -C artifacts/upstream/codex-candidate checkout --detach 3d3ae4965ab370217e871b3a7f0d15589557ee4b
node scripts/upstream/inventory.cjs --source artifacts/upstream/codex-candidate --commit 3d3ae4965ab370217e871b3a7f0d15589557ee4b --output artifacts/upstream/codex-inventory.json
```

The inventory destination must be new; reruns cannot overwrite earlier evidence.
The tool reads the exact commit's Git objects, not mutable checkout bytes. It
rejects branch names, gitlinks, unsafe Windows paths, invalid UTF-8, duplicate or
case-colliding files/directories, oversized input, and truncated or mismatched
object responses. Every blob is checked against its Git SHA-1 identity and gets
a separate SHA-256 digest. File kinds and executable modes are retained. It
records symlink target bytes without following them or importing any file.

Schema version 1 sorts paths in JavaScript code-unit order and hashes the UTF-8
`JSON.stringify(files)` representation. The inventory records commit/tree IDs,
encoding, byte-preservation policy, file list digest, original Git object IDs,
sizes and SHA-256 values. The 256 MiB source-byte bound is a tool resource limit,
not a product repository support limit. Dependencies fetched during a build are
outside this Git tree inventory and still require closure/license records.

For Codex revision `3d3ae4965ab370217e871b3a7f0d15589557ee4b`, two independent
inventory invocations produced 8,250 entries, 80,598,610 original bytes, tree
`b54522de663019e332772544ae53054aa3ecb92d` and file-list SHA-256
`0731feb5908c07319e74229723db80320517dd80afbb94bc6cec0768100f8259`.

## Native Windows baseline

Provision the upstream-pinned Rust 1.95.0 toolchain, Visual C++ x64 build tools,
and Visual Studio's CMake/Ninja components. This is the candidate's toolchain,
not yet a supported VCP compiler contract. Then run:

```powershell
pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SourceRoot artifacts/upstream/codex-candidate -Commit 3d3ae4965ab370217e871b3a7f0d15589557ee4b -OutputRoot artifacts/upstream/codex-baseline -Mode Build
pwsh -NoProfile -File scripts/upstream/build-baseline.ps1 -SourceRoot artifacts/upstream/codex-candidate -Commit 3d3ae4965ab370217e871b3a7f0d15589557ee4b -OutputRoot artifacts/upstream/codex-baseline -Mode BoundaryTests
```

`Build` compiles the unmodified `codex-cli` binary; `BoundaryTests` runs the
selected apply-patch and execution-policy package tests. Both use `--locked`,
native `x86_64-pc-windows-msvc`, and four jobs by default (`-Jobs` accepts 1–16).
Neither starts a model session or demonstrates VCP behavior. Ordinary Cargo
dependency provisioning can access the network; deterministic VCP `fast` tests
do not acquire these sources or run this build.

The baseline command checks commit identity and a clean checkout, resolves the
installed native tools, and reports missing prerequisites as `not_run`/exit 3.
It grants Git command-scoped trust only to the explicitly supplied disposable
checkout; it never changes global Git configuration. Outputs stay outside the
source root, in a UUID evidence directory plus a reusable `target/` cache.
`-TargetRoot` can reuse an explicitly chosen cache outside the source checkout.
An output/cache path inside the source is rejected before allocation with exit
2 and diagnostic `BASELINE_OUTPUT_IN_SOURCE`, independent of terminal formatting.
Source, evidence and target paths use the same native full-path normalization,
including Windows 8.3 aliases, before containment checks. A regression exercises
both evidence and target rejection through a short source alias; hosts without
short aliases report that specific case skipped rather than claiming coverage.
Compiler logs, actual command, versions, host description, exit status and log
hash are retained. Build/test failures preserve nonzero exits. A terminated host
may leave a `running` manifest; it is not a pass. Do not delete failed evidence
or use another user's shared target directory.

## Review before selection

The [Munarium local-library command](munarium-baseline.md) extends native
qualification to the kernel, in-memory backend and datastore, with both Tantivy
and DiskANN enabled and an explicit normal/build dependency boundary check.

The current root license declarations for Codex, Gemini CLI and Munarium are
Apache-2.0. This does not license every nested component under Apache. The
Codex inventory includes a symlink at `codex-rs/vendor/bubblewrap/LICENSE`,
Bubblewrap's LGPL-2.0-or-later source and `COPYING`, MIT-licensed WezTerm material,
bundled skill license files, and native voice dependency notices. An eventual
selection must retain applicable notices and explicitly record whether a link
is preserved, materialized or excluded. No import policy is inferred from a
successful inventory or build.

Initial effect-discovery paths at the pinned Codex revision include:

| Candidate boundary | Upstream path | VCP gate |
|---|---|---|
| CLI and headless startup | `codex-rs/cli/`, `codex-rs/exec/`, `codex-rs/tui/` | C06; remove deferred product surfaces from VCP entry points |
| Session submission and interrupt | `codex-rs/core/src/session/mod.rs`, `handlers.rs` | C01/C05; one VCP controller, durable pause and reconciliation |
| Model transport and helpers | `codex-rs/core/src/client.rs`, `codex-rs/model-provider/` | C05; OpenRouter capability/budget admission on every request |
| Credential discovery | `codex-rs/login/src/auth/` | Explicit VCP authority; disable ambient discovery |
| Analytics and telemetry | `codex-rs/analytics/`, `codex-rs/otel/` | Remove or disable implicit telemetry |
| Prepared edits and execution | `codex-rs/apply-patch/`, `codex-rs/exec-server/` | C02/C04; revision checks, broker authority and durable receipts |
| Command policy | `codex-rs/execpolicy/src/policy.rs` | C03; VCP permission ceilings and provenance |

This is a static discovery map, not a complete effect audit. P0-03/P0-08 must
observe actual requests and effects at the retained seams, including helper
calls, retries, review, compaction and maintenance. Upstream interrupt/suspend
APIs alone do not satisfy VCP's in-app `/pause`, child lifetime or durable resume
contract. The [current source selection](codex-source.md) records the imported
baseline; dynamic integration qualification remains outstanding.
