# VCP source

This directory is the root for product code, tests, fixtures, evaluation definitions, built-in skill assets, and selected upstream source. It currently contains no runtime implementation or build manifest.

Follow the [code layout](../docs/plan/code-layout.md) and [upstream feasibility work](../docs/plan/01-upstream-feasibility.md) when introducing the Rust workspace. Preserve useful upstream modules and record their actual paths instead of creating empty crates for every logical responsibility.

Contributor setup belongs in `docs/development/`; build and test entry points belong in `scripts/`. See [CONTRIBUTING.md](../CONTRIBUTING.md) for review and attribution requirements.
