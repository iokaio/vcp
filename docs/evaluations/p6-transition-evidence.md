# P6-02 / M1 — Retained task-transition evidence

Status: implemented and locally verified, September 20, 2026. This is the first independently
reviewable M1 increment, not completion of M1 or live statistical qualification.

## Behavior

`routing_state::transitions` rebuilds versioned task-state observations from
canonical event facts. Local CLI and terminal inspection share the authenticated
canonical routing boundary. Scope, event identity, task revision and time-window
checks prevent cross-task edges and current-state substitution. Gaps, open traces
and unavailable richer observations are explicit. Read-only access suffices; no
saved aggregate, provider, task or reservation is created.

The [decision](../adr/029-retained-transition-evidence.md) records the source,
retention, compatibility and resource boundaries. Existing optimizer reports
retain their schema and current-state meaning.

## Verification record

Native Windows MSVC verification uses installed stable Rust 1.98.1, the repository's
`artifacts/p5-native-env.ps1` setup and the locked offline workspace. The focused suite covers real engine pause and
idempotent command receipts, interleaved task histories, backward wall-clock
timestamps, blocked/resume, same-state revisions, incomplete windows, malformed
and unknown facts, bounded scans, scoped/denied reads, unchanged state, both-store
reopen and actual retention purge. CLI tests cover parsing and read-only dispatch.

Commands and results:

- `cargo +stable test --manifest-path src/third_party/codex/codex-rs/Cargo.toml --locked --offline --target x86_64-pc-windows-msvc -j 2 -p vcp-lifecycle --test routing_state -- --test-threads=1`: **12 passed**, including five new tests exercised against Files and SQLite.
- `cargo +stable test --manifest-path src/third_party/codex/codex-rs/Cargo.toml --locked --offline --target x86_64-pc-windows-msvc -j 2 -p vcp-cli --lib optimize -- --test-threads=1`: **9 passed**.
- `pwsh -NoProfile -File scripts/test.ps1 -Suite fast`: **9 passed**.
- Stable `rustfmt --check --edition 2021 --config skip_children=true` on changed Rust files and `git diff --check`: **passed**.

An initial retention test found that physical redaction checks alone exposed
logically purged snapshots before cleanup. The implementation now checks canonical
purge decisions for tasks and events. The final regression also selects one event
and verifies the existing retention closure removes dependent task evidence.
The test's initial expectation of an isolated edge deletion was corrected after
inspecting that closure contract. Existing upstream/lifecycle warnings remain;
no paid evaluation or packaged-release qualification was run.

## Remaining

M1 still needs turn/action symbols, normalized failure identities, attempt
lineage/counters, endpoint/policy cohorts, complete reward/unknown-liability
attribution, bounded numerical routines and fitted-artifact retention/replay.
No task-state count is consumed as a probability or routing estimate. M2 advisory
integration, M3 forecasts and M4 live qualification remain separate work; the
saved P6-03 persistence branch is not merged by this increment. No paid trials
or native statistical-performance claims are included.
