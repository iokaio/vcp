# P6-02 / M1 — Causal action observations

Status: implemented and locally verified, September 20, 2026. This is the third independently
reviewable M1 increment, not completion of M1 or qualification of a consumer.

## Behavior

The [selected boundary](../adr/030-causal-action-observations.md) rebuilds separate
turn, tool/effect and attempt traces plus standalone verification observations from currently
retained canonical evidence. It exposes exact admitted endpoint/model cohorts,
available task class, retry lineage and bounded counters without joining sibling,
concurrent or independent work.

New verification events carry the accepted task revision. Exact failure
signatures combine typed input identity with digests of normalized check and
diagnostic data. Older events remain readable and report the signature as
unavailable. The inspector returns no raw objective, check command, diagnostic,
turn reason, accounting uncertainty reason or provider response.

## Verification record

Focused fixtures cover real engine turn and effect facts,
command replay, new and old verification events, exact failure identity, real
budget reservation/submission/settlement, retry lineage, routing-decision cohort
binding, scoped and denied reads, response bounds, read-only state, logical purge
before cleanup, reopen, Files and SQLite.

Native Windows MSVC verification used installed stable Rust 1.98.1, the
repository `artifacts/p5-native-env.ps1` environment and the locked offline
workspace:

- `cargo +stable test --manifest-path src/third_party/codex/codex-rs/Cargo.toml --locked --offline --target x86_64-pc-windows-msvc -j 2 -p vcp-lifecycle --test routing_state -- --test-threads=1`: **16 passed**, including four action-evidence tests exercised against Files and SQLite.
- `cargo +stable test --manifest-path src/third_party/codex/codex-rs/Cargo.toml --locked --offline --target x86_64-pc-windows-msvc -j 2 -p vcp-cli --lib optimize -- --test-threads=1`: **10 passed**.
- The same locked native command with `-p vcp-engine`: **10 passed** across package integration tests; doc/unit targets with zero cases also passed.
- `pwsh -NoProfile -File scripts/test.ps1 -Suite fast`: **9 cases passed**, manifest `ef90adea-6b57-4cf7-bef2-eda247202cf0`.
- Stable changed-file `rustfmt --check`, repository/link contracts and `git diff --check`: **passed**.

Initial fixture runs exposed canonical artifact-reference requirements and active
turn/accounting purge protection. The fixtures now use retained artifacts and
settle turn/task lifecycle before exercising logical purge. Production behavior
was not weakened to make those tests pass. Existing lifecycle/upstream warnings
remain; no paid or packaged qualification was run.

## Remaining

The subsequent [charge-reward increment](p6-charge-rewards.md) adds exact settled
charges and unknown-liability abstention. M1 still needs fit provenance/retention,
uncertainty, held-out comparison and consumed-value replay. These observations do
not feed the numerical kernels or any routing
consumer in this increment. No paid, live-provider or packaged qualification is
included.
