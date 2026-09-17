# P0-07 static source boundary inventory

Date: 2026-09-17. This is source coverage and navigation evidence for P0-07,
not a VCP execution or security qualification. No product session or paid model
request was run for this change. Imported Codex source remains unchanged.

## Inputs and observations

Codex revision `3d3ae4965ab370217e871b3a7f0d15589557ee4b` is the
[committed selection](../../src/third_party/components/codex-selection.json).
Its [boundary catalog](../../src/third_party/components/codex-boundaries.json)
assigns all 154 discovered Cargo packages to 21 groups. Each group identifies
review capabilities, proposed handling, owning plan tasks and the required VCP
gate. Twenty-five source anchors cover controller submission/interruption,
queued child input, main/helper model calls, credentials, telemetry, policy
mutation, MCP transport/tools, process dispatch/termination and local stores.

The package parser follows explicit members and internal path dependencies,
including inherited, target and dev dependencies. Its package names and manifest
paths exactly matched independent `cargo +1.95.0 metadata --locked --offline
--no-deps --format-version 1 --manifest-path
src/third_party/codex/codex-rs/Cargo.toml` output: 154 packages, including four
implicit members. Raw metadata is in ignored
`artifacts/upstream/codex-workspace-metadata.json`. This comparison does not
resolve a platform/feature-specific dependency closure.

The [implementation guide](../development/codex-boundaries.md) explains the
source locations and required adapter experiments. In particular, upstream
interrupt aborts active work; it does not establish VCP's durable in-app pause
fence across root and child admission. Background memory, remote compaction,
review, guardian retries and connection prewarm require separate accounting and
authority treatment from the main model stream.

The [Gemini candidate record](../../src/third_party/components/gemini-cli.md)
now identifies the concrete G01/G02/G03 and G06 inputs at revision
`6a466a7e2fe2b1255752c1e74f69b31f0216084d`. Its 213 prior upstream test passes
are recorded in [native candidate evidence](p0-07-native-candidates.md).
Those source paths are investigation inputs, not a closed Node runtime extraction
or completed Rust port. No Gemini source was imported in this change.

## Checks and limits

`node scripts/upstream/check-boundaries.cjs` checks complete unique package
ownership, valid ledger owners, exact source pin, source path containment and
anchored symbols. Each anchor must belong to its declared package's group.
The checker rejects claims of runtime qualification. Three synthetic regression
tests exercise implicit membership/cycles, missing inheritance, duplicate names,
escaping paths, unsupported exclusions, missing/duplicate/unknown ownership,
wrong anchor ownership and stale symbols/pins.

The `fast` and `upstream` suites run these checks alongside source-byte inventory,
existing harness regressions and documentation/task consistency. The inventory
checker uses Node.js 24.10.0 and the pinned development TOML parser; it does not
require Rust or downloaded candidate checkouts on the Linux CI runner.

`pwsh -NoProfile -File scripts/test.ps1 -Suite fast` passed all six cases and
33 regression tests with exit 0 on native Windows. The evidence manifest is
`artifacts/tests/c5dcdfe4-e8ee-4667-84c1-717be44dd732/manifest.json`.
The source check confirms all 7,937 imported files retain the expected bytes.
Documentation links, 68 task owners and dependency consistency also passed.

Presence of a symbol is not a call trace. Package coverage is not an exhaustive
proof of every effect inside each file. Runtime observation, credential/telemetry
removal, model admission and pause/recovery remain P0-03/P0-05/P0-08 work.
Local inference assets, other component extraction, common compiler qualification
and another Windows environment remain open; P0-07 is still `in_progress`.
