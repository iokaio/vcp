# VCP source

This directory owns product code, tests, fixtures, evaluation definitions, built-in skills and selected upstream source. It contains delivery support and contract tests under `tests/`, plus the imported Codex baseline workspace at `third_party/codex/codex-rs/Cargo.toml`. VCP product adapters remain unimplemented. See [harness setup](../docs/development/delivery-harness.md) and [native baseline setup](../docs/development/codex-source.md).

Follow the [code layout](../docs/plan/code-layout.md) and [upstream feasibility work](../docs/plan/01-upstream-feasibility.md) when introducing the Rust workspace. Preserve useful upstream modules and record their actual paths instead of creating empty crates for every logical responsibility.

Codex supplies the selected engine/CLI foundation through committed ordinary files under `src/third_party/codex/`. [ADR-013](../docs/adr/013-upstream-reuse-and-vendoring.md) defines provenance and independent reconstruction. The baseline is imported for qualification; its upstream behavior is not evidence that VCP's authority and accounting contracts are implemented.

Contributor setup belongs in `docs/development/`; build and test entry points belong in `scripts/`. See [CONTRIBUTING.md](../CONTRIBUTING.md) for review and attribution requirements.
