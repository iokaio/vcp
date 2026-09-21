# P6-02 / M1 — Held-out Markov order comparison

Status: implemented and locally verified, September 20, 2026. This is the sixth
independently reviewable M1 increment, not completion of M1 or qualification of a
consumer.

## Behavior

The [selected boundary](../adr/033-heldout-markov-order-comparison.md) compares
first- and second-order task-state candidates on the same held-out next-state
predictions. A supported-parameter penalty resolves complexity and ties in favor
of first order. Unsupported contexts/outcomes and undersampled rows abstain. A
first-order `P²` check reports maximum absolute error against held-out
two-transition frequencies.

The lifecycle adapter partitions complete task identities by a stable digest,
splits every retained gap, and records the source and partition identities,
parameters and cohort sizes in a rebuildable artifact. It writes nothing,
qualifies nothing and cannot feed routing.

## Verification record

Pure fixtures cover a history where second order improves held-out likelihood, a
deterministic history where the complexity tie selects first order, exact
multi-step frequencies, sparse and unseen held-out support, invalid bounds and
combined observation limits. Files and SQLite fixtures rebuild four independent
running-blocked-failed traces, verify stable task separation and artifact identity,
prove the store is unchanged, and exercise sample/partition abstention and errors.

Native Windows MSVC verification used installed stable Rust:

- `cargo +stable test --manifest-path src/crates/vcp-models/Cargo.toml --test markov -- --test-threads=1`: **11 passed**.
- `cargo +stable test --manifest-path src/crates/vcp-lifecycle/Cargo.toml --test routing_state -- --test-threads=1`: **19 passed**, including comparison and prior fit fixtures on Files and SQLite.
- `pwsh -NoProfile -File scripts/test.ps1 -Suite fast`: **9 cases passed**, manifest `2c3dcd5b-37dc-4f4c-a5e9-4e24def9add5`.
- Stable changed-file `rustfmt --check`, repository/link contracts and `git diff --check`: **passed**.

Existing lifecycle/upstream compiler warnings remain. No paid, live-provider or
packaged qualification was run.

## Remaining

M1 still needs reward mapping and uncertainty qualification plus exact
consumed-value persistence/replay. Project/time-separated frozen qualification
and any enabled consumer remain M4 work.
