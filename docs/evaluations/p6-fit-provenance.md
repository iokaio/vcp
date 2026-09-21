# P6-02 / M1 — Rebuildable fit provenance

Status: implemented and locally verified, September 20, 2026. This is the fifth
independently reviewable M1 increment, not completion of M1 or qualification of a
consumer.

## Behavior

The [selected boundary](../adr/032-rebuildable-markov-fit-artifacts.md) adds a
read-only first-order task-state fit. Each artifact binds the retained transition
evidence ID/digest, workspace, authority/deletion revisions, cutoff/window/scope,
gap/censoring/pruned coverage, observed alphabet, counts, raw row samples,
algorithm, integer prior, sample gate,
uncertainty method and available policy/catalog identity.

The fixed state order is narrowed to states present on connected retained edges;
the cohort and missing task-class/endpoint dimensions are explicit. Empty,
terminal-free, terminal-only, sparse, nonabsorbing, forbidden, invalid and
numerically unsafe candidates abstain with a closed reason. Every result remains
unqualified, is never persisted and cannot serve routing.

## Verification record

Files and SQLite fixtures build a hand-solvable running-to-failed chain, verify
exact counts/probabilities and stable rebuild identity, confirm source parameters
and unavailable cohort dimensions, prove the store is unchanged, and exercise
sparse, empty-window and invalid-parameter abstention/error paths.

Native Windows MSVC verification used installed stable Rust 1.98.1 and the
repository `artifacts/p5-native-env.ps1` environment:

- `cargo +stable test --manifest-path src/crates/vcp-lifecycle/Cargo.toml --test routing_state -- --test-threads=1`: **18 passed**, including the fit fixture on Files and SQLite.
- `pwsh -NoProfile -File scripts/test.ps1 -Suite fast`: **9 cases passed**, manifest `5f805999-f4aa-47cd-bb78-3bda82941992`.
- Stable changed-file `rustfmt --check`, repository/link contracts and `git diff --check`: **passed**.

Existing lifecycle/upstream compiler warnings remain. No paid, live-provider or
packaged qualification was run.

## Remaining

M1 still needs held-out first/second-order comparison with complexity penalties,
multi-step frequency checks, reward mapping/uncertainty qualification, and
consumed-value persistence/replay. This candidate does not feed a routing
consumer.
