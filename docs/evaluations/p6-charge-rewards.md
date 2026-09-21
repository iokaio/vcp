# P6-02 / M1 — Exact attempt charges and rewards

Status: implemented and locally verified, September 20, 2026. This is the fourth
independently reviewable M1 increment, not completion of M1 or qualification of a
consumer.

## Behavior

The [selected boundary](../adr/031-exact-attempt-charge-attribution.md) advances
the read-only action projection to `canonical-action-observation/2`. New usage
events contain only a typed immutable settlement reference. The projection joins
that reference to its currently retained settlement and owning attempt, then
emits currency, debit/credit/none adjustments, cumulative charge, reservation
liability, completeness and an optional exact terminal charge.

Late cumulative corrections remain separate and reconcile against the prior
retained total. Replayed observation IDs append no duplicate settlement. Main,
retry, helper, child and verification attempts remain separate components; task
and root ledger rollups are not added. A partial prefix, old usage event, pruned
source, open reservation or uncertain provider outcome has no terminal point
estimate. A complete no-send release is explicitly zero.

The event adds no provider response or correction prose. The projection returns
no raw usage artifact, provider response, correction reason or uncertainty reason.

## Verification record

Files and SQLite fixtures cover exact settled currency/total, late debit and
credit corrections, idempotent receipt replay, retry attribution, partial-window
abstention, uncertain liability, exact no-send zero, old event compatibility,
reopen parity and the absence of correction prose from the new event reference.

Native Windows MSVC verification used installed stable Rust 1.98.1 and the
repository `artifacts/p5-native-env.ps1` environment:

- `cargo +stable test --manifest-path src/crates/vcp-lifecycle/Cargo.toml --test routing_state -- --test-threads=1`: **17 passed**, including five action-evidence tests on Files and SQLite.
- `cargo +stable test --manifest-path src/crates/vcp-budget/Cargo.toml -- --test-threads=1`: **10 passed**; zero-case unit/doc targets also passed.
- `cargo +stable test --manifest-path src/crates/vcp-cli/Cargo.toml --lib optimize -- --test-threads=1`: **10 passed**.
- `pwsh -NoProfile -File scripts/test.ps1 -Suite fast`: **9 cases passed**, manifest `8a4ac401-a7aa-499b-916f-f48e2d5f6b40`.
- Stable changed-file `rustfmt --check`, repository/link contracts and `git diff --check`: **passed**.

Existing lifecycle/upstream compiler warnings remain. No paid, live-provider or
packaged qualification was run.

## Remaining

M1 still needs fitted-state/reward mapping and qualification, explicit fit
provenance and uncertainty, retention invalidation, held-out first/second-order
comparison and consumed-value replay. This increment does not feed a numerical
kernel or routing consumer.
