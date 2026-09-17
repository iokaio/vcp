# VCP source

This directory is the root for product code, tests, fixtures, evaluation definitions, built-in skill assets, and selected upstream source. It contains delivery-harness support and contract tests under `tests/`, but no product runtime implementation or build manifest. See [harness setup](../docs/development/delivery-harness.md).

Follow the [code layout](../docs/plan/code-layout.md) and [upstream feasibility work](../docs/plan/01-upstream-feasibility.md) when introducing the Rust workspace. Preserve useful upstream modules and record their actual paths instead of creating empty crates for every logical responsibility.

Codex will supply the engine/CLI foundation through committed copied source under `src/third_party/codex/`, with reviewed patches already applied. It is part of the planned VCP build, not a reference-only checkout. [ADR-013](../docs/adr/013-upstream-reuse-and-vendoring.md) specifies ordinary Git files rather than a submodule/subtree workflow, the separate reconstruction check, and still-open source selection. No source import or build manifest exists yet.

Contributor setup belongs in `docs/development/`; build and test entry points belong in `scripts/`. See [CONTRIBUTING.md](../CONTRIBUTING.md) for review and attribution requirements.
