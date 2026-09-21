# P6-02 / M1 — Exact attempt-visit reward mapping

Status: implemented and locally verified, September 20, 2026. This is the seventh
independently reviewable M1 increment, not completion of M1 or qualification of a
consumer.

## Behavior

The [selected boundary](../adr/034-exact-attempt-reward-mapping.md) maps each
retained attempt once to an exact augmented cohort visit. Exact terminal charges
produce currency-preserving cost samples. Retry, helper, child, verification and
other roles remain separate, as do root identity, endpoint/model, task class,
admitted policy and escalation counters.

A cell publishes an upward-rounded sample mean only when every attempt has a
complete terminal charge. It still reports exact sample count/sum/range and
separate available charged/liability observations when an unknown attempt
withholds the mean. No task/root rollup, inferred completion value or cross-
currency conversion is added. The source-bound artifact is rebuilt, unpersisted
and unqualified.

## Verification record

Files and SQLite fixtures build two exact settled attempts in the same cohort,
verify the exact mean/range, then add an uncertain attempt and prove that the
cohort mean disappears while known charges and reserved liability remain visible.
They also verify empty-history abstention, stable rebuild identity, no store
mutation and reopen parity.

Native Windows MSVC verification used installed stable Rust:

- `cargo +stable test --manifest-path src/crates/vcp-lifecycle/Cargo.toml --test routing_state -- --test-threads=1`: **20 passed**, including reward fixtures on Files and SQLite.
- `pwsh -NoProfile -File scripts/test.ps1 -Suite fast`: **9 cases passed**, manifest `2c2074c7-1302-406f-a561-e8bad1476301`.
- Stable changed-file `rustfmt --check`, repository/link contracts and `git diff --check`: **passed**.

Existing lifecycle/upstream compiler warnings remain. No paid, live-provider or
packaged qualification was run.

## Remaining

Consumed-value persistence and exact replay remain before M1 is complete. M3/M4
still own forecasts, qualification and any enabled consumer.
