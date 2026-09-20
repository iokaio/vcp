# P6-02 / M1 — Bounded numerical kernels

Status: implemented and locally verified, September 20, 2026. This increment implements the pure
arithmetic portion of M1 step 5; it does not complete M1 or qualify a consumer.

## Contract and evidence

The [kernel contract](../development/markov-kernels.md) defines state/work bounds,
observed-support smoothing, sparse-row gates, numerical tolerances, reward units
and abstention. No dependency, canonical schema, provider call or persisted fit
is introduced.

`vcp-models/tests/markov.rs` includes hand-calculated expected visits and two
terminal outcomes, resumable blocked state, required pivot swapping, arbitrary
terminal index order, fully terminal inputs, segment isolation, illegal/sparse
counts, overflow, non-finite/unnormalized/nonabsorbing inputs, near-singular
rejection, unknown rewards and log-space long-sequence likelihood.

A fixed seed (`0x5eed`) generates 32 invented five-state matrices. An independent
100-step probability-mass propagation checks visits, absorption and rewards from
all three transient starts. Every transient row has at least one-half terminal
mass, so truncation error is bounded without using the solver as its own oracle.
This starts M9's numeric fixtures; it does not claim controller, restore or
packaged-release campaign coverage. No real/private history trains these tests.

## Verification

Native Windows MSVC, installed stable Rust 1.98.1, using the existing
`artifacts/p5-native-env.ps1` environment:

- `cargo +stable test --manifest-path src/third_party/codex/codex-rs/Cargo.toml --locked --offline --target x86_64-pc-windows-msvc -j 2 -p vcp-models --test markov`: **9 passed**.
- The same command with `-p vcp-models` and no test filter: **60 passed**, including existing provider, routing, decision and escalation tests; doc tests passed (zero cases).
- `pwsh -NoProfile -File scripts/test.ps1 -Suite fast`: **9 passed**, manifest `49f54cf0-b46c-4d7a-9190-84f976d29ef3`.
- Stable `rustfmt --check --edition 2021 --config skip_children=true` on the three changed Rust files, repository/link contracts and `git diff --check`: **passed**.

The initial test build needed an explicit empty-slice type annotation; after
that test-only correction all numerical cases passed. No production numerical
failure was hidden by relaxing an expected outcome. Packaged and cross-platform
qualification remain not run for this increment.

## Remaining

Rich action/attempt/cohort/reward capture, fitted-artifact provenance/retention,
uncertainty, held-out first/second-order comparison and consumer replay remain
open under M1. M2/M3 consumers and M4 live qualification remain separate. No
calibration, task-cost improvement, policy default or paid trial is claimed.
